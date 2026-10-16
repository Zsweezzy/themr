use std::collections::BTreeMap;
use std::fmt;

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub colors: BTreeMap<String, String>,
}

#[derive(Debug)]
pub struct ThemeError(pub String);

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for ThemeError {}

/// Accepts `{"name": .., "colors": {..}}` or a flat map of token -> hex.
/// Non-hex values in the flat form (author, url, ...) are ignored.
pub fn parse_theme(json: &str, fallback_id: &str) -> Result<Theme, ThemeError> {
    let v: Value =
        serde_json::from_str(json).map_err(|e| ThemeError(format!("invalid JSON: {e}")))?;
    let obj = v
        .as_object()
        .ok_or_else(|| ThemeError("theme must be a JSON object".into()))?;

    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(fallback_id)
        .to_string();

    let colors_src: &serde_json::Map<String, Value> = match obj.get("colors") {
        Some(Value::Object(m)) => m,
        Some(_) => return Err(theme_err("\"colors\" must be an object")),
        None => obj,
    };

    let mut colors = BTreeMap::new();
    for (k, v) in colors_src {
        if let Some(s) = v.as_str()
            && let Some(hex) = parse_hex(s)
        {
            colors.insert(k.clone(), hex);
        }
    }
    if colors.is_empty() {
        return Err(theme_err("no valid hex colors found"));
    }
    Ok(Theme {
        id: fallback_id.to_string(),
        name,
        colors,
    })
}

fn theme_err(msg: &str) -> ThemeError {
    ThemeError(msg.to_string())
}

/// `#rgb`, `#rrggbb`, `#rrggbbaa`, with or without `#`. Normalized to lowercase with `#`.
pub fn parse_hex(s: &str) -> Option<String> {
    let t = s.trim();
    let t = t.strip_prefix('#').unwrap_or(t);
    if !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let t = t.to_ascii_lowercase();
    match t.len() {
        3 => Some(format!(
            "#{}",
            t.chars().flat_map(|c| [c, c]).collect::<String>()
        )),
        6 | 8 => Some(format!("#{t}")),
        _ => None,
    }
}

pub fn hex_to_rgba(hex: &str) -> Option<(f32, f32, f32, f32)> {
    let h = parse_hex(hex)?;
    let b: Vec<u8> = (1..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    let (r, g, b, a) = (b[0], b[1], b[2], b.get(3).copied().unwrap_or(255));
    Some((
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ))
}

pub fn rgba_to_hex(r: f32, g: f32, b: f32, a: f32) -> String {
    let c = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (r, g, b) = (c(r), c(g), c(b));
    if a >= 1.0 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{:02x}", c(a))
    }
}

pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "theme".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_theme() {
        let t = parse_theme(
            r##"{"name":"T","colors":{"primary":"#ABC","bg":"#1e1e2e"}}"##,
            "x",
        )
        .unwrap();
        assert_eq!(t.name, "T");
        assert_eq!(t.colors["primary"], "#aabbcc");
        assert_eq!(t.colors["bg"], "#1e1e2e");
    }

    #[test]
    fn parses_flat_community_theme_and_ignores_meta() {
        let t = parse_theme(
            r##"{"name":"Cool","author":"someone","primary":"#ff0000","note":"not a color"}"##,
            "cool",
        )
        .unwrap();
        assert_eq!(t.name, "Cool");
        assert_eq!(t.colors.len(), 1);
        assert_eq!(t.colors["primary"], "#ff0000");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_theme("[]", "x").is_err());
        assert!(parse_theme(r#"{"colors":{}}"#, "x").is_err());
        assert!(parse_theme(r#"{"colors":{"a":"nope"}}"#, "x").is_err());
        assert!(parse_theme("not json", "x").is_err());
    }

    #[test]
    fn hex_forms() {
        assert_eq!(parse_hex("#FFF"), Some("#ffffff".into()));
        assert_eq!(parse_hex("ff00aa"), Some("#ff00aa".into()));
        assert_eq!(parse_hex("#11223344"), Some("#11223344".into()));
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("hello"), None);
    }

    #[test]
    fn rgba_roundtrip() {
        let (r, g, b, a) = hex_to_rgba("#82aaff").unwrap();
        assert_eq!(rgba_to_hex(r, g, b, a), "#82aaff");
    }

    #[test]
    fn slug() {
        assert_eq!(slugify("Tokyo Night Moon!"), "tokyo-night-moon");
        assert_eq!(slugify("  "), "theme");
    }
}
