use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::theme::Theme;

pub const BACKUP_EXT: &str = "themr.bak";

#[derive(Debug, Clone, PartialEq)]
pub enum FileStatus {
    Updated,
    Unchanged,
    MissingFile,
}

#[derive(Debug, Clone)]
pub struct FileReport {
    pub path: PathBuf,
    pub status: FileStatus,
    pub replaced: usize,
    /// colors found already equal to this theme's palette and now tracked
    pub adopted: usize,
    pub warnings: Vec<String>,
    /// tokens confirmed in this file and their new values, for the tracking state
    pub written: BTreeMap<String, String>,
}

impl FileReport {
    pub fn label(&self) -> String {
        let name = self.path.display();
        match self.status {
            FileStatus::MissingFile => format!("{name}: not found, skipped"),
            FileStatus::Unchanged if self.adopted > 0 => format!(
                "{name}: {} colors adopted from this theme's palette (tracked, values unchanged)",
                self.adopted
            ),
            FileStatus::Unchanged => format!("{name}: already up to date"),
            FileStatus::Updated => format!("{name}: {} colors rewritten", self.replaced),
        }
    }
}

pub fn backup_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.{BACKUP_EXT}", path.display()))
}

/// A colour literal in the text, with enough info to rewrite just the channel
/// digits and leave the syntax, separators and alpha untouched.
struct ColorLit {
    start: usize,
    end: usize,
    /// normalised "#rrggbb", lowercased
    rgb: String,
    /// absolute byte ranges of the channel digits: one for hex syntaxes, three
    /// for CSS `rgb()`/`rgba()`.
    digits: Vec<(usize, usize)>,
    decimal: bool,
    upper: bool,
}

fn take_hex(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_hexdigit() {
        i += 1;
    }
    i
}

fn expand3(h: &str) -> String {
    h.chars().flat_map(|c| [c, c]).collect()
}

/// `#rgb` / `#rrggbb`. The 8-digit `#rrggbbaa` is a distinct colour and is
/// deliberately not recognised, so a binding to `#rrggbb` never touches it.
fn hash_lit(text: &str, i: usize) -> Option<ColorLit> {
    let b = text.as_bytes();
    let j = i + 1;
    let end = take_hex(b, j);
    let (rgb, upper) = match end - j {
        3 => (format!("#{}", expand3(&text[j..end])), false),
        6 => (
            format!("#{}", text[j..end].to_ascii_lowercase()),
            text[j..end].bytes().any(|c| c.is_ascii_uppercase()),
        ),
        _ => return None,
    };
    Some(ColorLit {
        start: i,
        end,
        rgb,
        digits: vec![(j, end)],
        decimal: false,
        upper,
    })
}

/// hypr: `0xRRGGBB` and `0xAARRGGBB`.
fn hex_lit(text: &str, i: usize) -> Option<ColorLit> {
    let b = text.as_bytes();
    let j = i + 2;
    let end = take_hex(b, j);
    let (rgb, rs) = match end - j {
        6 => (format!("#{}", text[j..end].to_ascii_lowercase()), j),
        8 => (format!("#{}", text[j + 2..end].to_ascii_lowercase()), j + 2),
        _ => return None,
    };
    Some(ColorLit {
        start: i,
        end,
        rgb,
        digits: vec![(rs, end)],
        decimal: false,
        upper: text[rs..end].bytes().any(|c| c.is_ascii_uppercase()),
    })
}

/// `rgb(rrggbb)` / `rgba(rrggbbaa)` (hypr) and `rgb(r, g, b)` / `rgba(r, g, b, a)` (CSS).
fn css_lit(text: &str, i: usize) -> Option<ColorLit> {
    let b = text.as_bytes();
    let mut k = i + 3;
    if k < b.len() && b[k] == b'a' {
        k += 1;
    }
    if k >= b.len() || b[k] != b'(' {
        return None;
    }
    let open = k + 1;
    let close = open + text[open..].find(')')?;
    let content = &text[open..close];
    let stripped: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    if !stripped.contains(',')
        && matches!(stripped.len(), 6 | 8)
        && stripped.bytes().all(|c| c.is_ascii_hexdigit())
    {
        let alpha_form = stripped.len() == 8;
        let rgb = if alpha_form {
            format!("#{}", stripped[..6].to_ascii_lowercase())
        } else {
            format!("#{}", stripped.to_ascii_lowercase())
        };
        let re = if alpha_form { open + 6 } else { close };
        return Some(ColorLit {
            start: i,
            end: close + 1,
            rgb,
            digits: vec![(open, re)],
            decimal: false,
            upper: stripped.bytes().any(|c| c.is_ascii_uppercase()),
        });
    }
    let mut comps: Vec<(usize, usize, u8)> = Vec::new();
    let mut off = 0;
    for part in content.split(',') {
        let lead = part.len() - part.trim_start().len();
        let tok = part.trim_start();
        let n = tok.bytes().take_while(|c| c.is_ascii_digit()).count();
        if n == 0 {
            return None;
        }
        let val: u8 = tok[..n].parse().ok()?;
        comps.push((open + off + lead, open + off + lead + n, val));
        off += part.len() + 1;
    }
    if comps.len() < 3 {
        return None;
    }
    Some(ColorLit {
        start: i,
        end: close + 1,
        rgb: format!("#{:02x}{:02x}{:02x}", comps[0].2, comps[1].2, comps[2].2),
        digits: vec![
            (comps[0].0, comps[0].1),
            (comps[1].0, comps[1].1),
            (comps[2].0, comps[2].1),
        ],
        decimal: true,
        upper: false,
    })
}

/// Every colour literal in `text`, in order.
fn scan_lits(text: &str) -> Vec<ColorLit> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let found = if c == b'#' {
            hash_lit(text, i)
        } else if c == b'0' && i + 1 < b.len() && (b[i + 1] | 0x20) == b'x' {
            hex_lit(text, i)
        } else if c == b'r' && text[i..].starts_with("rgb") {
            css_lit(text, i)
        } else {
            None
        };
        match found {
            Some(l) => {
                i = l.end;
                out.push(l);
            }
            None => {
                i += if c < 0x80 {
                    1
                } else {
                    text[i..].chars().next().unwrap().len_utf8()
                }
            }
        }
    }
    out
}

/// Rewrite `lit`'s channels to `new_rgb` ("#rrggbb"), keeping its syntax and alpha.
fn rebuild(text: &str, lit: &ColorLit, new_rgb: &str) -> String {
    let hex = &new_rgb[1..];
    let comps = [
        u8::from_str_radix(&hex[0..2], 16).unwrap(),
        u8::from_str_radix(&hex[2..4], 16).unwrap(),
        u8::from_str_radix(&hex[4..6], 16).unwrap(),
    ];
    let mut s = String::new();
    let mut cur = lit.start;
    for (idx, (ds, de)) in lit.digits.iter().enumerate() {
        s.push_str(&text[cur..*ds]);
        if lit.decimal {
            s.push_str(&comps[idx].to_string());
        } else {
            s.push_str(&if lit.upper {
                hex.to_ascii_uppercase()
            } else {
                hex.to_string()
            });
        }
        cur = *de;
    }
    s.push_str(&text[cur..lit.end]);
    s
}

/// Replace every literal whose RGB is `old` with `new`. Two literals of the
/// same colour both change, whatever syntax each uses.
fn replace_color(text: &str, old: &str, new: &str) -> (String, usize) {
    let mut out = text.to_string();
    let mut n = 0;
    for lit in scan_lits(text).iter().rev() {
        if lit.rgb == old {
            let rebuilt = rebuild(text, lit, new);
            out.replace_range(lit.start..lit.end, &rebuilt);
            n += 1;
        }
    }
    (out, n)
}

/// Replace `{{token}}` markers, which is how a config refers to a color by name.
fn replace_markers(
    text: &str,
    theme: &Theme,
) -> (String, BTreeMap<String, String>, Vec<String>, usize) {
    let mut out = String::with_capacity(text.len());
    let mut written = BTreeMap::new();
    let mut unknown = Vec::new();
    let mut n = 0;
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let key = after[..end].trim();
                match theme.colors.get(key) {
                    Some(value) => {
                        out.push_str(value);
                        written.insert(key.to_string(), value.clone());
                        n += 1;
                    }
                    None => {
                        unknown.push(key.to_string());
                        out.push_str(&rest[start..start + 2 + end + 2]);
                    }
                }
                rest = &after[end + 2..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    (out, written, unknown, n)
}

pub struct RenderResult {
    pub text: String,
    pub written: BTreeMap<String, String>,
    pub replaced: usize,
    pub adopted: usize,
    pub warnings: Vec<String>,
}

/// The RGB of every colour literal in the text, lowercased "#rrggbb".
fn scan_colors(text: &str) -> Vec<String> {
    scan_lits(text).into_iter().map(|l| l.rgb).collect()
}

/// Names that are only synonyms in the bundled themes. When a value has more
/// than one of them, the palette name is adopted instead, so a config follows
/// `foreground`/`blue`/`magenta`/`overlay` and not a synonym.
const ALIASES: [&str; 4] = ["primary", "accent", "black", "white"];

/// A file that already uses this theme's exact colors needs no markers: adopt
/// those values so the next Apply can move the file to another theme.
fn adopt_hexes(text: &str, theme: &Theme) -> BTreeMap<String, String> {
    let mut by_value: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (token, value) in &theme.colors {
        by_value.entry(value.as_str()).or_default().push(token);
    }
    let mut adopted = BTreeMap::new();
    for hex in scan_colors(text) {
        let Some(tokens) = by_value.get(hex.as_str()) else {
            continue;
        };
        let pick = tokens
            .iter()
            .find(|t| !ALIASES.contains(&**t))
            .or(tokens.first());
        if let Some(token) = pick {
            adopted.insert((*token).to_string(), hex);
        }
    }
    adopted
}

/// Config keys that name a palette token but are not spelled like one, so a
/// keyed line still binds: btop's `theme[main_bg]`, rofi's `text-color`.
const KEY_ALIASES: [(&str, &str); 48] = [
    ("main_bg", "background"),
    ("main_fg", "foreground"),
    ("title", "foreground"),
    ("hi_fg", "cyan"),
    ("selected_bg", "surface"),
    ("selected_fg", "foreground"),
    ("inactive_fg", "muted"),
    ("proc_misc", "cyan"),
    ("cpu_box", "muted"),
    ("mem_box", "muted"),
    ("net_box", "muted"),
    ("proc_box", "muted"),
    ("div_line", "muted"),
    ("temp_start", "cyan"),
    ("temp_mid", "yellow"),
    ("temp_end", "red"),
    ("cpu_start", "blue"),
    ("cpu_mid", "purple"),
    ("cpu_end", "red"),
    ("free_start", "cyan"),
    ("free_mid", "blue"),
    ("free_end", "foreground"),
    ("cached_start", "cyan"),
    ("cached_mid", "cyan"),
    ("cached_end", "blue"),
    ("available_start", "cyan"),
    ("available_mid", "cyan"),
    ("available_end", "blue"),
    ("used_start", "green"),
    ("used_mid", "yellow"),
    ("used_end", "red"),
    ("download_start", "blue"),
    ("download_mid", "cyan"),
    ("download_end", "cyan"),
    ("upload_start", "magenta"),
    ("upload_mid", "purple"),
    ("upload_end", "blue"),
    ("text-color", "foreground"),
    ("background-color", "background"),
    ("selected-background-color", "surface"),
    ("selected-text-color", "foreground"),
    ("active-background-color", "blue"),
    ("active-text-color", "overlay"),
    ("urgent-background-color", "red"),
    ("urgent-text-color", "overlay"),
    ("border-color", "surface"),
    ("placeholder-color", "muted"),
    ("handle-color", "surface"),
];

/// `main_bg` for `theme[main_bg]`, and any configured alias -> palette token.
fn resolve_key<'a>(key: &'a str, theme: &'a Theme) -> Option<&'a str> {
    let bare = key
        .strip_prefix("theme[")
        .and_then(|k| k.strip_suffix(']'))
        .unwrap_or(key);
    if theme.colors.contains_key(bare) {
        return Some(bare);
    }
    KEY_ALIASES
        .iter()
        .find(|(alias, token)| *alias == bare && theme.colors.contains_key(*token))
        .map(|(_, token)| *token)
}

/// A line that names a color by key (`background #222436`, `foreground = "#c8d3f5"`)
/// ties that line to the token of the same name, even when the value is not in
/// the palette. Without it, a value-only match can bind a token to any line that
/// happens to share the color (kitty's `active_tab_foreground` instead of
/// `background`), so editing the token never reaches the line the user means.
fn key_hexes(text: &str, theme: &Theme) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim_start();
        if line.starts_with('#')
            || line.starts_with("//")
            || line.starts_with(';')
            || line.starts_with('!')
        {
            continue; // comment
        }
        let end = line
            .find(|c: char| c == '=' || c == ':' || c.is_whitespace())
            .unwrap_or(line.len());
        let Some(token) = resolve_key(&line[..end], theme) else {
            continue;
        };
        // the first colour on the line is the one it means, alpha or not
        if let Some(lit) = scan_lits(&line[end..]).into_iter().next() {
            out.entry(token.to_string()).or_insert(lit.rgb);
        }
    }
    out
}

/// If `line` names a colour by key (`theme[main_bg]=`, `text-color:`), rewrite
/// that line's colour to the theme's value and report the token it belongs to.
/// Syntax and any alpha are kept.
fn rewrite_key_line(line: &str, theme: &Theme) -> (String, Option<(String, String)>) {
    let trimmed = line.trim_start();
    let lead = line.len() - trimmed.len();
    if trimmed.starts_with('#')
        || trimmed.starts_with("//")
        || trimmed.starts_with(';')
        || trimmed.starts_with('!')
    {
        return (line.to_string(), None);
    }
    let end = trimmed
        .find(|c: char| c == '=' || c == ':' || c.is_whitespace())
        .unwrap_or(trimmed.len());
    let Some(token) = resolve_key(&trimmed[..end], theme) else {
        return (line.to_string(), None);
    };
    let body = &trimmed[end..];
    let Some(lit) = scan_lits(body).into_iter().next() else {
        return (line.to_string(), None);
    };
    let new = theme.colors[token].to_ascii_lowercase();
    let rebuilt = rebuild(body, &lit, &new);
    let out = format!(
        "{}{}{}{}",
        &line[..lead + end],
        &body[..lit.start],
        rebuilt,
        &body[lit.end..]
    );
    (out, Some((token.to_string(), new)))
}

/// Set every key-named line to its token's value. Run last: a line that names a
/// colour is authoritative, so two tokens that share one colour (Nord gives
/// `magenta` and `purple` the same hex) can never fight over it — without this
/// they alternately overwrite each other and never settle.
fn apply_key_lines(text: &str, theme: &Theme) -> (String, BTreeMap<String, String>, usize) {
    let mut out = String::with_capacity(text.len());
    let mut written = BTreeMap::new();
    let mut hits = 0;
    let mut rest = text;
    while !rest.is_empty() {
        let (line, tail) = match rest.find('\n') {
            Some(i) => (&rest[..i + 1], &rest[i + 1..]),
            None => (rest, ""),
        };
        let (new_line, bound) = rewrite_key_line(line, theme);
        if let Some((token, value)) = bound {
            if new_line != line {
                hits += 1;
            }
            written.insert(token, value);
        }
        out.push_str(&new_line);
        rest = tail;
    }
    (out, written, hits)
}

/// Rewrite every color themr tracks in this file to the theme's value.
/// `previous` is what themr wrote last time (token -> hex). On the first look
/// at a file, palette-matching hex values are adopted instead of rewritten.
pub fn render(text: &str, theme: &Theme, previous: &BTreeMap<String, String>) -> RenderResult {
    let mut warnings = Vec::new();
    let (mut out, mut written, unknown, mut replaced) = replace_markers(text, theme);
    for key in unknown {
        warnings.push(format!(
            "token {{{{{key}}}}} is not in this theme, left as is"
        ));
    }

    let adopted = if previous.is_empty() {
        adopt_hexes(&out, theme)
    } else {
        BTreeMap::new()
    };
    let mut tracked = if previous.is_empty() {
        adopted.clone()
    } else {
        previous.clone()
    };
    // A line that names a color by key is authoritative: it wins over a stale
    // value-only match that pointed the token at some other line.
    for (token, hex) in key_hexes(&out, theme) {
        tracked.insert(token, hex);
    }
    for (token, value) in &adopted {
        written
            .entry(token.clone())
            .or_insert_with(|| value.clone());
    }
    let previous = &tracked;

    // shortcut: same hex used for two tokens can only resolve to one new value; the first wins.
    let mut claimed: BTreeMap<String, String> = BTreeMap::new();
    for (token, old_raw) in previous {
        let Some(new_raw) = theme.colors.get(token) else {
            warnings.push(format!(
                "token '{token}' missing from this theme, left as is"
            ));
            continue;
        };
        let old = old_raw.to_ascii_lowercase();
        let new = new_raw.to_ascii_lowercase();
        if let Some(other) = claimed.get(&old)
            && other != token
        {
            warnings.push(format!(
                "'{old}' is shared by '{other}' and '{token}', used '{other}'"
            ));
            continue;
        }
        if old == new {
            continue;
        }
        claimed.insert(old.clone(), token.clone());
        let (next, hits) = replace_color(&out, &old, &new);
        if hits == 0 {
            if !written.contains_key(token) {
                warnings.push(format!(
                    "value {old} for '{token}' not found in file, left as is"
                ));
            }
            continue;
        }
        out = next;
        replaced += hits;
        written.insert(token.clone(), new.clone());
    }

    // Key-named lines are authoritative and go last, so a colour two tokens
    // share can never bind a token to the wrong line.
    let (keyed, key_written, key_hits) = apply_key_lines(&out, theme);
    out = keyed;
    replaced += key_hits;
    for (token, value) in key_written {
        written.insert(token, value);
    }

    if written.is_empty() && previous.is_empty() && out == text {
        warnings.push("nothing themeable here: no {{token}} markers, and no hex value matching this theme's palette to adopt".into());
    }

    RenderResult {
        text: out,
        written,
        replaced,
        adopted: adopted.len(),
        warnings,
    }
}

/// Apply `theme` to one file, backing the original up once before the first edit.
pub fn apply_file(
    path: &Path,
    theme: &Theme,
    previous: &BTreeMap<String, String>,
) -> Result<FileReport, String> {
    let original = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(FileReport {
                path: path.to_path_buf(),
                status: FileStatus::MissingFile,
                replaced: 0,
                adopted: 0,
                warnings: vec![],
                written: BTreeMap::new(),
            });
        }
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };

    let result = render(&original, theme, previous);
    if result.text != original {
        let backup = backup_path(path);
        if !backup.exists() {
            fs::copy(path, &backup).map_err(|e| format!("backup {}: {e}", backup.display()))?;
        }
        fs::write(path, &result.text).map_err(|e| format!("write {}: {e}", path.display()))?;
    }

    let status = if result.text != original {
        FileStatus::Updated
    } else {
        FileStatus::Unchanged
    };
    Ok(FileReport {
        path: path.to_path_buf(),
        status,
        replaced: result.replaced,
        adopted: result.adopted,
        warnings: result.warnings,
        written: result.written,
    })
}

/// Put the file back the way it was before themr touched it.
pub fn restore_file(path: &Path) -> Result<String, String> {
    let backup = backup_path(path);
    if !backup.exists() {
        return Err(format!("{}: no backup", backup.display()));
    }
    fs::copy(&backup, path).map_err(|e| format!("restore {}: {e}", path.display()))?;
    Ok(format!("{}: restored from backup", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::parse_theme;
    use std::env;

    fn theme() -> Theme {
        parse_theme(r##"{"colors":{"bg":"#111111","fg":"#eeeeee"}}"##, "t").unwrap()
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("themr-test-{}-{}", std::process::id(), name));
        fs::create_dir_all(&dir).unwrap();
        dir.join("config.conf")
    }

    #[test]
    fn markers_are_replaced() {
        let r = render("background {{bg}}\n", &theme(), &BTreeMap::new());
        assert_eq!(r.text, "background #111111\n");
        assert_eq!(r.written["bg"], "#111111");
        assert_eq!(r.replaced, 1);
    }

    #[test]
    fn unknown_marker_is_kept_and_warned() {
        let r = render("x {{nope}} y\n", &theme(), &BTreeMap::new());
        assert_eq!(r.text, "x {{nope}} y\n");
        assert!(r.warnings[0].contains("nope"));
    }

    #[test]
    fn tracked_hex_is_rewritten_on_theme_change() {
        let mut prev = BTreeMap::new();
        prev.insert("bg".to_string(), "#000000".to_string());
        let r = render("bg #000000\n", &theme(), &prev);
        assert_eq!(r.text, "bg #111111\n");
        assert_eq!(r.replaced, 1);
    }

    #[test]
    fn hex_inside_longer_hex_is_not_touched() {
        let mut prev = BTreeMap::new();
        prev.insert("bg".to_string(), "#000000".to_string());
        let r = render("bg #000000ff\n", &theme(), &prev);
        assert_eq!(r.text, "bg #000000ff\n");
        assert_eq!(r.replaced, 0);
    }

    #[test]
    fn untracked_hex_is_never_touched() {
        let r = render("border #ff00ff\n", &theme(), &BTreeMap::new());
        assert_eq!(r.text, "border #ff00ff\n");
    }

    #[test]
    fn apply_then_change_theme_then_restore() {
        let path = tmp("roundtrip");
        fs::write(&path, "window_bg #ffffff\n# comment\n").unwrap();

        // first apply only touches markers (and key lines), so nothing changes
        let r1 = apply_file(&path, &theme(), &BTreeMap::new()).unwrap();
        assert_eq!(r1.status, FileStatus::Unchanged);
        assert!(!backup_path(&path).exists());
        assert!(r1.warnings.iter().any(|w| w.contains("nothing themeable")));

        // after the file holds a tracked value, a theme change rewrites it and backs up
        let mut prev = BTreeMap::new();
        prev.insert("bg".to_string(), "#ffffff".to_string());
        let r2 = apply_file(&path, &theme(), &prev).unwrap();
        assert_eq!(r2.status, FileStatus::Updated);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "window_bg #111111\n# comment\n"
        );
        assert!(backup_path(&path).exists());

        // backup stays the pristine original, restore brings it back
        let r3 = restore_file(&path).unwrap();
        assert!(r3.contains("restored"));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "window_bg #ffffff\n# comment\n"
        );
    }

    #[test]
    fn missing_file_is_reported_not_created() {
        let dir = tmp("missing");
        let path = dir.join("nope.conf");
        let r = apply_file(&path, &theme(), &BTreeMap::new()).unwrap();
        assert_eq!(r.status, FileStatus::MissingFile);
        assert!(!path.exists());
    }

    #[test]
    fn apply_is_idempotent() {
        let path = tmp("idempotent");
        fs::write(&path, "fg #000000\n").unwrap();
        let mut prev = BTreeMap::new();
        prev.insert("fg".to_string(), "#000000".to_string());
        let first = apply_file(&path, &theme(), &prev).unwrap();
        assert_eq!(first.status, FileStatus::Updated);
        let second = apply_file(&path, &theme(), &prev).unwrap();
        assert_eq!(second.status, FileStatus::Unchanged);
    }

    #[test]
    fn plain_hex_matching_the_theme_is_adopted() {
        let r = render("bg #111111\nfg #eeeeee\n", &theme(), &BTreeMap::new());
        assert_eq!(r.text, "bg #111111\nfg #eeeeee\n");
        assert_eq!(r.adopted, 2);
        assert_eq!(r.written["bg"], "#111111");
        assert_eq!(r.written["fg"], "#eeeeee");
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn adopt_is_case_insensitive_and_ignores_longer_hex() {
        let r = render("a #111111FF\nb #111111\n", &theme(), &BTreeMap::new());
        assert_eq!(r.adopted, 1);
        assert_eq!(r.text, "a #111111FF\nb #111111\n");
    }

    #[test]
    fn shared_value_adopts_the_palette_name_not_the_alias() {
        // blue and primary hold the same color: the config must follow `blue`
        let t = parse_theme(
            r##"{"colors":{"blue":"#111111","primary":"#111111"}}"##,
            "alias",
        )
        .unwrap();
        let r = render("x #111111\n", &t, &BTreeMap::new());
        assert_eq!(r.adopted, 1);
        assert_eq!(r.written.keys().collect::<Vec<_>>(), vec!["blue"]);
        assert_eq!(r.text, "x #111111\n");
    }

    #[test]
    fn adopted_colors_are_rewritten_on_the_next_apply() {
        let path = tmp("adopt-then-switch");
        fs::write(&path, "bg #111111\n").unwrap();

        // first apply adopts, file untouched, no backup
        let r1 = apply_file(&path, &theme(), &BTreeMap::new()).unwrap();
        assert_eq!(r1.status, FileStatus::Unchanged);
        assert_eq!(r1.adopted, 1);
        assert_eq!(fs::read_to_string(&path).unwrap(), "bg #111111\n");

        // switching theme now rewrites the adopted value
        let other = parse_theme(r##"{"colors":{"bg":"#222222"}}"##, "other").unwrap();
        let r2 = apply_file(&path, &other, &r1.written).unwrap();
        assert_eq!(r2.status, FileStatus::Updated);
        assert_eq!(fs::read_to_string(&path).unwrap(), "bg #222222\n");
        assert_eq!(
            fs::read_to_string(backup_path(&path)).unwrap(),
            "bg #111111\n"
        );
    }

    #[test]
    fn keyed_line_follows_its_token_even_when_value_is_not_in_the_palette() {
        // kitty: `background #222436` must follow the `background` token, though
        // the palette's background (#1e2030) is a different color
        let t = parse_theme(r##"{"colors":{"background":"#1e2030"}}"##, "k").unwrap();
        let r = render("background #222436\n", &t, &BTreeMap::new());
        assert_eq!(r.text, "background #1e2030\n");
        assert_eq!(r.written["background"], "#1e2030");
        assert_eq!(r.replaced, 1);
    }

    #[test]
    fn keyed_line_overrides_a_stale_value_binding() {
        // regression: state held background -> #26a269 (another line's color),
        // so editing `background` recolored the wrong line and never the real one
        let t = parse_theme(r##"{"colors":{"background":"#1e2030"}}"##, "k").unwrap();
        let mut prev = BTreeMap::new();
        prev.insert("background".to_string(), "#26a269".to_string());
        let r = render("background #222436\nactive_tab_foreground #26a269\n", &t, &prev);
        assert_eq!(r.text, "background #1e2030\nactive_tab_foreground #26a269\n");
    }

    #[test]
    fn keyed_line_with_alpha_is_left_alone() {
        let t = parse_theme(r##"{"colors":{"bg":"#111111"}}"##, "k").unwrap();
        let r = render("bg #000000ff\n", &t, &BTreeMap::new());
        assert_eq!(r.text, "bg #000000ff\n");
        assert_eq!(r.replaced, 0);
    }

    #[test]
    fn hypr_rgba_keeps_its_alpha() {
        // active_border = rgba(RRGGBBaa)
        let t = parse_theme(r##"{"colors":{"blue":"#81a1c1"}}"##, "k").unwrap();
        let mut prev = BTreeMap::new();
        prev.insert("blue".to_string(), "#82aaff".to_string());
        let r = render(r#"active = rgba(82aaffee)"#, &t, &prev);
        assert_eq!(r.text, "active = rgba(81a1c1ee)");
        assert_eq!(r.replaced, 1);
    }

    #[test]
    fn hypr_rgba_hex_is_adopted_by_value() {
        let t = parse_theme(r##"{"colors":{"blue":"#82aaff"}}"##, "k").unwrap();
        let r = render(r#"active = rgba(82aaffee)"#, &t, &BTreeMap::new());
        assert_eq!(r.text, "active = rgba(82aaffee)");
        assert_eq!(r.written["blue"], "#82aaff");
    }

    #[test]
    fn hypr_0x_keeps_its_alpha() {
        // color = 0xAARRGGBB
        let t = parse_theme(r##"{"colors":{"overlay":"#434c5e"}}"##, "k").unwrap();
        let mut prev = BTreeMap::new();
        prev.insert("overlay".to_string(), "#16161e".to_string());
        let r = render("color = 0xee16161e", &t, &prev);
        assert_eq!(r.text, "color = 0xee434c5e");
    }

    #[test]
    fn css_rgb_and_rgba_are_rewritten() {
        let t = parse_theme(r##"{"colors":{"blue":"#81a1c1","background":"#2e3440"}}"##, "k").unwrap();
        let mut prev = BTreeMap::new();
        prev.insert("blue".to_string(), "#82aaff".to_string());
        prev.insert("background".to_string(), "#1e2030".to_string());
        let r = render(
            "color: rgb(130, 170, 255);\nbackground-color: rgba(30, 32, 48, 0.9);",
            &t,
            &prev,
        );
        assert_eq!(
            r.text,
            "color: rgb(129, 161, 193);\nbackground-color: rgba(46, 52, 64, 0.9);"
        );
    }

    #[test]
    fn btop_theme_keys_bind_through_aliases() {
        // theme[main_bg]="#222436" follows the `background` token
        let t = parse_theme(r##"{"colors":{"background":"#1e2030"}}"##, "k").unwrap();
        let r = render("theme[main_bg]=\"#222436\"\n", &t, &BTreeMap::new());
        assert_eq!(r.text, "theme[main_bg]=\"#1e2030\"\n");
        assert_eq!(r.replaced, 1);
    }

    #[test]
    fn rofi_colors_bind_through_aliases() {
        let t = parse_theme(r##"{"colors":{"foreground":"#eeeeee"}}"##, "k").unwrap();
        let r = render("text-color: #ff0000;\n", &t, &BTreeMap::new());
        assert_eq!(r.text, "text-color: #eeeeee;\n");
        assert_eq!(r.replaced, 1);
    }

    #[test]
    fn tokens_sharing_a_colour_do_not_fight_over_one_value() {
        // regression: Nord gives magenta and purple the same #b48ead, so btop's
        // cpu_mid (purple) and upload_start (magenta) shared an old value and
        // alternately overwrote each other on every apply, never settling
        let t = parse_theme(
            r##"{"colors":{"magenta":"#bb9af7","purple":"#9d7cd8"}}"##,
            "k",
        )
        .unwrap();
        let text = "theme[cpu_mid]=\"#b48ead\"\ntheme[upload_start]=\"#b48ead\"\ntheme[upload_mid]=\"#b48ead\"\n";
        let mut prev = BTreeMap::new();
        prev.insert("magenta".to_string(), "#b48ead".to_string());
        prev.insert("purple".to_string(), "#b48ead".to_string());

        let r1 = render(text, &t, &prev);
        assert_eq!(
            r1.text,
            "theme[cpu_mid]=\"#9d7cd8\"\ntheme[upload_start]=\"#bb9af7\"\ntheme[upload_mid]=\"#9d7cd8\"\n"
        );

        let r2 = render(&r1.text, &t, &r1.written);
        assert_eq!(r2.text, r1.text, "second apply must be a no-op");
        assert_eq!(r2.replaced, 0);
    }
}
