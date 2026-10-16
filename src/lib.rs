mod apply;
mod config;
mod theme;

pub use apply::*;
pub use config::*;
pub use theme::*;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const BUILTIN_THEMES: [(&str, &str); 4] = [
    (
        "tokyonight-moon",
        include_str!("../themes/tokyonight-moon.json"),
    ),
    (
        "catppuccin-mocha",
        include_str!("../themes/catppuccin-mocha.json"),
    ),
    ("gruvbox-dark", include_str!("../themes/gruvbox-dark.json")),
    ("nord", include_str!("../themes/nord.json")),
];

/// Which real config files each bundled app points at. Only seeded if the file exists.
const PRESET_APPS: [(&str, &str); 6] = [
    ("kitty", "~/.config/kitty/current-theme.conf"),
    ("waybar", "~/.config/waybar/style.css"),
    ("hypr", "~/.config/hypr/hyprland.lua"),
    ("btop", "~/.config/btop/btop.conf"),
    ("cava", "~/.config/cava/config"),
    ("rofi", "~/.config/rofi/config.rasi"),
];

pub fn config_home() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"));
    base.join("themr")
}

pub fn config_path() -> PathBuf {
    config_home().join("config.toml")
}
pub fn state_path() -> PathBuf {
    config_home().join("state.json")
}
pub fn themes_dir() -> PathBuf {
    config_home().join("themes")
}

/// `~/x` -> `$HOME/x`, everything else untouched.
pub fn expand(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    if path == "~"
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home);
    }
    PathBuf::from(path)
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Preset apps whose config file actually exists under `home`.
fn seed_apps(home: &Path) -> BTreeMap<String, AppConfig> {
    let mut apps = BTreeMap::new();
    for (app, rel) in PRESET_APPS {
        let rel_path = rel.strip_prefix("~/").unwrap_or(rel);
        if home.join(rel_path).exists() {
            apps.insert(
                app.to_string(),
                AppConfig {
                    theme: None,
                    files: vec![rel.to_string()],
                },
            );
        }
    }
    apps
}

/// First launch: themes, config with app presets, empty state.
pub fn first_run() -> Result<(), String> {
    fs::create_dir_all(themes_dir()).map_err(|e| e.to_string())?;
    for (id, json) in BUILTIN_THEMES {
        let dest = themes_dir().join(format!("{id}.json"));
        if !dest.exists() {
            fs::write(dest, json).map_err(|e| e.to_string())?;
        }
    }
    if !config_path().exists() {
        let cfg = ThemrConfig {
            apps: seed_apps(&home_dir()),
            ..ThemrConfig::default()
        };
        save_config(&cfg)?;
    }
    if !state_path().exists() {
        save_state(&State::default())?;
    }
    Ok(())
}

pub fn load_config() -> Result<ThemrConfig, String> {
    match fs::read_to_string(config_path()) {
        Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", config_path().display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ThemrConfig::default()),
        Err(e) => Err(format!("{}: {e}", config_path().display())),
    }
}

pub fn save_config(cfg: &ThemrConfig) -> Result<(), String> {
    let text = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(config_path(), text).map_err(|e| format!("{}: {e}", config_path().display()))
}

pub fn load_state() -> Result<State, String> {
    match fs::read_to_string(state_path()) {
        Ok(text) => {
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", state_path().display()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
        Err(e) => Err(format!("{}: {e}", state_path().display())),
    }
}

pub fn save_state(state: &State) -> Result<(), String> {
    let text = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(state_path(), text).map_err(|e| format!("{}: {e}", state_path().display()))
}

/// theme id -> Theme, from `<config_home>/themes/*.json`. Broken files are reported, not fatal.
pub fn load_themes() -> (BTreeMap<String, Theme>, Vec<String>) {
    let mut themes = BTreeMap::new();
    let mut warnings = Vec::new();
    let dir = themes_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return (
            themes,
            vec![format!("themes dir {} missing", dir.display())],
        );
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        match fs::read_to_string(&path) {
            Ok(text) => match parse_theme(&text, &id) {
                Ok(t) => {
                    themes.insert(id, t);
                }
                Err(e) => warnings.push(format!("{}: {e}", path.display())),
            },
            Err(e) => warnings.push(format!("{}: {e}", path.display())),
        }
    }
    (themes, warnings)
}

/// Save a community theme JSON under a new id. Returns the id used.
pub fn import_theme(json: &str, fallback_id: &str) -> Result<String, String> {
    let theme = parse_theme(json, fallback_id).map_err(|e| e.to_string())?;
    let mut id = slugify(&theme.name);
    if id == "theme" {
        id = slugify(fallback_id);
    }
    let mut n = 2;
    while themes_dir().join(format!("{id}.json")).exists() {
        id = format!("{}-{n}", slugify(&theme.name));
        n += 1;
    }
    let mut stored = theme;
    stored.id = id.clone();
    let text = serde_json::to_string_pretty(&stored).map_err(|e| e.to_string())?;
    fs::write(themes_dir().join(format!("{id}.json")), text).map_err(|e| e.to_string())?;
    Ok(id)
}

/// Create a new theme by copying `base`'s colors under a new name. Returns the id used.
pub fn duplicate_theme(base: &Theme, name: &str) -> Result<String, String> {
    let copy = Theme {
        id: String::new(),
        name: name.to_string(),
        colors: base.colors.clone(),
    };
    let json = serde_json::to_string(&copy).map_err(|e| e.to_string())?;
    import_theme(&json, name)
}

pub struct AppReport {
    pub app: String,
    pub theme: String,
    pub files: Vec<FileReport>,
    pub errors: Vec<String>,
}

/// Apply one app's effective theme to all of its files, updating the tracking state.
pub fn apply_app(
    name: &str,
    cfg: &ThemrConfig,
    themes: &BTreeMap<String, Theme>,
    state: &mut State,
) -> AppReport {
    let theme_id = cfg.app_theme(name);
    let mut report = AppReport {
        app: name.to_string(),
        theme: theme_id.clone(),
        files: vec![],
        errors: vec![],
    };
    let Some(theme) = themes.get(&theme_id) else {
        report.errors.push(format!("theme '{theme_id}' not found"));
        return report;
    };
    let Some(app) = cfg.apps.get(name) else {
        report.errors.push(format!("unknown app '{name}'"));
        return report;
    };
    for file in &app.files {
        let path = expand(file);
        let previous = state
            .files
            .get(path.to_string_lossy().as_ref())
            .cloned()
            .unwrap_or_default();
        match apply_file(&path, theme, &previous) {
            Ok(fr) => {
                if fr.status != FileStatus::MissingFile {
                    let key = path.to_string_lossy().into_owned();
                    let entry = state.files.entry(key).or_default();
                    entry.extend(fr.written.clone());
                }
                report.files.push(fr);
            }
            Err(e) => report.errors.push(e),
        }
    }
    report
}

/// Apply every app. Returns one report per app, in config order.
pub fn apply_all(
    cfg: &ThemrConfig,
    themes: &BTreeMap<String, Theme>,
    state: &mut State,
) -> Vec<AppReport> {
    cfg.apps
        .keys()
        .map(|name| apply_app(name, cfg, themes, state))
        .collect()
}

/// Put an app's files back to their pre-themr state and stop tracking them.
pub fn restore_app(name: &str, cfg: &ThemrConfig, state: &mut State) -> Vec<String> {
    let mut out = Vec::new();
    let Some(app) = cfg.apps.get(name) else {
        return vec![format!("unknown app '{name}'")];
    };
    for file in &app.files {
        let path = expand(file);
        state.files.remove(path.to_string_lossy().as_ref());
        match restore_file(&path) {
            Ok(msg) => out.push(msg),
            Err(e) => out.push(e),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("themr-lib-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn theme(id: &str, bg: &str) -> Theme {
        parse_theme(
            &format!(r##"{{"colors":{{"background":"{bg}","foreground":"#eeeeee"}}}}"##),
            id,
        )
        .unwrap()
    }

    #[test]
    fn seeds_only_apps_whose_config_exists() {
        let home = tmpdir("seed");
        fs::create_dir_all(home.join(".config/kitty")).unwrap();
        fs::write(
            home.join(".config/kitty/current-theme.conf"),
            "background #000000\n",
        )
        .unwrap();

        let apps = seed_apps(&home);
        assert!(apps.contains_key("kitty"));
        assert!(
            !apps.contains_key("cava"),
            "apps without a config file must not be seeded"
        );
        // stored path stays portable instead of expanding to this machine's home
        assert_eq!(apps["kitty"].files[0], "~/.config/kitty/current-theme.conf");
    }

    #[test]
    fn applies_per_app_override_then_global_then_restores() {
        let dir = tmpdir("flow");
        let file = dir.join("colors.conf");
        fs::write(&file, "bg {{background}}\nfg #000000\n").unwrap();

        let mut cfg = ThemrConfig {
            theme: "a".into(),
            apps: BTreeMap::new(),
        };
        cfg.apps.insert(
            "kitty".into(),
            AppConfig {
                theme: None,
                files: vec![file.to_string_lossy().into_owned()],
            },
        );
        let mut themes = BTreeMap::new();
        themes.insert("a".into(), theme("a", "#111111"));
        themes.insert("b".into(), theme("b", "#222222"));

        // per-app override wins over the global theme
        cfg.apps.get_mut("kitty").unwrap().theme = Some("b".into());
        let mut state = State::default();
        let reports = apply_all(&cfg, &themes, &mut state);
        assert_eq!(reports[0].theme, "b");
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "bg #222222\nfg #000000\n"
        );

        // dropping the override rewrites what themr wrote before; other hex is untouched
        cfg.apps.get_mut("kitty").unwrap().theme = None;
        apply_all(&cfg, &themes, &mut state);
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "bg #111111\nfg #000000\n"
        );

        // restore puts the original file (markers and all) back
        let msgs = restore_app("kitty", &cfg, &mut state);
        assert!(msgs[0].contains("restored"), "{msgs:?}");
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "bg {{background}}\nfg #000000\n"
        );
        assert!(!state.files.contains_key(file.to_string_lossy().as_ref()));
    }

    #[test]
    fn missing_theme_is_reported_not_applied() {
        let dir = tmpdir("missing-theme");
        let file = dir.join("f.conf");
        fs::write(&file, "bg {{background}}\n").unwrap();
        let mut cfg = ThemrConfig {
            theme: "nope".into(),
            apps: BTreeMap::new(),
        };
        cfg.apps.insert(
            "x".into(),
            AppConfig {
                theme: None,
                files: vec![file.to_string_lossy().into_owned()],
            },
        );

        let mut state = State::default();
        let report = apply_app("x", &cfg, &BTreeMap::new(), &mut state);
        assert!(report.errors[0].contains("not found"));
        assert_eq!(fs::read_to_string(&file).unwrap(), "bg {{background}}\n");
    }
}
