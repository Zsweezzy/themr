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
    pub warnings: Vec<String>,
    /// tokens confirmed in this file and their new values, for the tracking state
    pub written: BTreeMap<String, String>,
}

impl FileReport {
    pub fn label(&self) -> String {
        let name = self.path.display();
        match self.status {
            FileStatus::MissingFile => format!("{name}: not found, skipped"),
            FileStatus::Unchanged => format!("{name}: already up to date"),
            FileStatus::Updated => format!("{name}: {} colors rewritten", self.replaced),
        }
    }
}

pub fn backup_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.{BACKUP_EXT}", path.display()))
}

fn is_hex(c: char) -> bool {
    c.is_ascii_hexdigit()
}

/// Replace whole hex tokens only: `#ff0000` must not match inside `#ff0000ff`.
fn replace_hex(text: &str, old: &str, new: &str) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut n = 0;
    while let Some(i) = rest.find(old) {
        let before_ok = rest[..i]
            .chars()
            .next_back()
            .map(|c| !is_hex(c))
            .unwrap_or(true);
        let after_ok = rest[i + old.len()..]
            .chars()
            .next()
            .map(|c| !is_hex(c))
            .unwrap_or(true);
        if before_ok && after_ok {
            out.push_str(&rest[..i]);
            out.push_str(new);
            n += 1;
        } else {
            out.push_str(&rest[..i + old.len()]);
        }
        rest = &rest[i + old.len()..];
    }
    out.push_str(rest);
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
    pub warnings: Vec<String>,
}

/// Rewrite every color themr tracks in this file to the theme's value.
/// `previous` is what themr wrote last time (token -> hex).
pub fn render(text: &str, theme: &Theme, previous: &BTreeMap<String, String>) -> RenderResult {
    let mut warnings = Vec::new();
    let (mut out, mut written, unknown, mut replaced) = replace_markers(text, theme);
    for key in unknown {
        warnings.push(format!(
            "token {{{{{key}}}}} is not in this theme, left as is"
        ));
    }

    // shortcut: same hex used for two tokens can only resolve to one new value; the first wins.
    let mut claimed: BTreeMap<&str, &str> = BTreeMap::new();
    for (token, old) in previous {
        let Some(new) = theme.colors.get(token) else {
            warnings.push(format!(
                "token '{token}' missing from this theme, left as is"
            ));
            continue;
        };
        if let Some(other) = claimed.get(old.as_str())
            && other != &token.as_str()
        {
            warnings.push(format!(
                "'{old}' is shared by '{other}' and '{token}', used '{other}'"
            ));
            continue;
        }
        if old == new {
            continue;
        }
        claimed.insert(old, token);
        let (next, hits) = replace_hex(&out, old, new);
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

    if written.is_empty() && previous.is_empty() && out == text {
        warnings.push("no colors tracked yet: put {{primary}} and friends in this file, or point themr at a theme file".into());
    }

    RenderResult {
        text: out,
        written,
        replaced,
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
        fs::write(&path, "bg #ffffff\n# comment\n").unwrap();

        // first apply only touches markers, so nothing changes
        let r1 = apply_file(&path, &theme(), &BTreeMap::new()).unwrap();
        assert_eq!(r1.status, FileStatus::Unchanged);
        assert!(!backup_path(&path).exists());
        assert!(r1.warnings.iter().any(|w| w.contains("no colors tracked")));

        // after the file holds a tracked value, a theme change rewrites it and backs up
        let mut prev = BTreeMap::new();
        prev.insert("bg".to_string(), "#ffffff".to_string());
        let r2 = apply_file(&path, &theme(), &prev).unwrap();
        assert_eq!(r2.status, FileStatus::Updated);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "bg #111111\n# comment\n"
        );
        assert!(backup_path(&path).exists());

        // backup stays the pristine original, restore brings it back
        let r3 = restore_file(&path).unwrap();
        assert!(r3.contains("restored"));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "bg #ffffff\n# comment\n"
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
}
