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

/// A bundled app preset: the config files worth theming, and how to make the app
/// re-read them. Files that do not exist on this machine are skipped, so a preset
/// that does not match your setup costs nothing.
struct Preset {
    app: &'static str,
    files: &'static [&'static str],
    /// `None` when the app needs no nudge (it watches its config, or reads it on
    /// the next launch). Only commands that are safe to repeat belong here.
    reload: Option<&'static str>,
}

const SIGUSR1_KITTY: &str = "pkill -SIGUSR1 -x kitty || true";
const SIGUSR2_WAYBAR: &str = "pkill -SIGUSR2 -x waybar || true";
// A reload only makes sense while the app runs, and some clients block forever
// waiting for a daemon that is not there (swaync-client does), which would hang
// the whole Apply — so each is gated on the process existing.
const RELOAD_EWW: &str = "pgrep -x eww >/dev/null && eww reload || true";
const RELOAD_POLYBAR: &str = "pgrep -x polybar >/dev/null && polybar-msg cmd restart || true";
const RELOAD_DUNST: &str = "pgrep -x dunst >/dev/null && dunstctl reload || true";
const RELOAD_MAKO: &str = "pgrep -x mako >/dev/null && makoctl reload || true";
const RELOAD_SWAYNC: &str = "pgrep -x swaync >/dev/null && swaync-client -rs || true";
const RELOAD_HYPR: &str = "pgrep -x Hyprland >/dev/null && hyprctl reload || true";
const RELOAD_SWAY: &str = "pgrep -x sway >/dev/null && swaymsg reload || true";
const RELOAD_I3: &str = "pgrep -x i3 >/dev/null && i3-msg reload || true";

const PRESETS: &[Preset] = &[
    // ---- terminals ----
    Preset {
        app: "kitty",
        files: &["~/.config/kitty/current-theme.conf"],
        reload: Some(SIGUSR1_KITTY),
    },
    // alacritty and wezterm watch their own config, so there is nothing to run
    Preset {
        app: "alacritty",
        files: &[
            "~/.config/alacritty/alacritty.toml",
            "~/.config/alacritty/alacritty.yml",
        ],
        reload: None,
    },
    Preset {
        app: "wezterm",
        files: &["~/.config/wezterm/wezterm.lua"],
        reload: None,
    },
    Preset {
        app: "foot",
        files: &["~/.config/foot/foot.ini"],
        reload: None,
    },
    Preset {
        app: "ghostty",
        files: &["~/.config/ghostty/config"],
        reload: None,
    },
    // ---- bars and shells ----
    Preset {
        app: "waybar",
        files: &[
            "~/.config/waybar/style.css",
            "~/.config/waybar/config.jsonc",
            "~/.config/waybar/config",
        ],
        reload: Some(SIGUSR2_WAYBAR),
    },
    Preset {
        app: "eww",
        files: &[
            "~/.config/eww/eww.scss",
            "~/.config/eww/eww.css",
            "~/.config/eww/bar/eww.scss",
        ],
        reload: Some(RELOAD_EWW),
    },
    Preset {
        app: "polybar",
        files: &["~/.config/polybar/config.ini", "~/.config/polybar/config"],
        reload: Some(RELOAD_POLYBAR),
    },
    Preset {
        app: "ags",
        files: &["~/.config/ags/style.scss"],
        reload: None,
    },
    // ---- launchers ----
    Preset {
        app: "rofi",
        files: &["~/.config/rofi/config.rasi"],
        reload: None,
    },
    Preset {
        app: "wofi",
        files: &["~/.config/wofi/style.css"],
        reload: None,
    },
    Preset {
        app: "fuzzel",
        files: &["~/.config/fuzzel/fuzzel.ini"],
        reload: None,
    },
    Preset {
        app: "tofi",
        files: &["~/.config/tofi/config"],
        reload: None,
    },
    Preset {
        app: "anyrun",
        files: &["~/.config/anyrun/style.css"],
        reload: None,
    },
    // ---- notifications ----
    Preset {
        app: "dunst",
        files: &["~/.config/dunst/dunstrc"],
        reload: Some(RELOAD_DUNST),
    },
    Preset {
        app: "mako",
        files: &["~/.config/mako/config"],
        reload: Some(RELOAD_MAKO),
    },
    Preset {
        app: "swaync",
        files: &["~/.config/swaync/style.css"],
        reload: Some(RELOAD_SWAYNC),
    },
    // ---- compositors ----
    Preset {
        app: "hypr",
        files: &[
            "~/.config/hypr/hyprland.conf",
            "~/.config/hypr/hyprland.lua",
        ],
        reload: Some(RELOAD_HYPR),
    },
    // niri reloads a saved config by itself
    Preset {
        app: "niri",
        files: &["~/.config/niri/config.kdl"],
        reload: None,
    },
    Preset {
        app: "sway",
        files: &["~/.config/sway/config"],
        reload: Some(RELOAD_SWAY),
    },
    Preset {
        app: "i3",
        files: &["~/.config/i3/config"],
        reload: Some(RELOAD_I3),
    },
    Preset {
        app: "hyprlock",
        files: &["~/.config/hypr/hyprlock.conf"],
        reload: None,
    },
    Preset {
        app: "swaylock",
        files: &["~/.config/swaylock/config"],
        reload: None,
    },
    Preset {
        app: "wlogout",
        files: &["~/.config/wlogout/style.css"],
        reload: None,
    },
    // No GTK preset on purpose: gtk.css is usually a whole generated theme with
    // hundreds of distinct colours, and collapsing those onto one theme's ~20
    // turns every accent into the background colour. Point themr at a small
    // hand-written gtk.css yourself if that is not the case for you.
    Preset {
        app: "qt",
        files: &["~/.config/Trolltech.conf"],
        reload: None,
    },
    // The applied palette KDE apps read; `plasma-apply-colorscheme` would
    // overwrite it again from the scheme file, so theme the scheme itself too
    // if you switch palettes that way. Reloading a KDE app means restarting it.
    Preset {
        app: "kde",
        files: &["~/.config/kdeglobals"],
        reload: None,
    },
    Preset {
        app: "konsole",
        files: &["~/.local/share/konsole/*.colorscheme"],
        reload: None,
    },
    Preset {
        app: "xresources",
        files: &["~/.Xresources"],
        reload: Some("xrdb -merge ~/.Xresources || true"),
    },
    // ---- system monitors, editors, CLIs ----
    Preset {
        app: "btop",
        files: &["~/.config/btop/btop.conf"],
        reload: None,
    },
    Preset {
        app: "cava",
        files: &["~/.config/cava/config"],
        reload: None,
    },
    Preset {
        app: "starship",
        files: &["~/.config/starship.toml"],
        reload: None,
    },
    Preset {
        app: "yazi",
        files: &["~/.config/yazi/theme.toml"],
        reload: None,
    },
    Preset {
        app: "zellij",
        files: &["~/.config/zellij/config.kdl"],
        reload: None,
    },
    Preset {
        app: "tmux",
        files: &["~/.config/tmux/tmux.conf", "~/.tmux.conf"],
        reload: None,
    },
    Preset {
        app: "lsd",
        files: &["~/.config/lsd/colors.yaml"],
        reload: None,
    },
    Preset {
        app: "lf",
        files: &["~/.config/lf/colors"],
        reload: None,
    },
    Preset {
        app: "bat",
        files: &["~/.config/bat/config"],
        reload: None,
    },
    Preset {
        app: "eza",
        files: &["~/.config/eza/theme.yml"],
        reload: None,
    },
    Preset {
        app: "zathura",
        files: &["~/.config/zathura/zathurarc"],
        reload: None,
    },
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

/// Expand a single `*` wildcard in the last path component of `path`, with
/// `home` resolving `~/`. A path without a wildcard is returned as is. Sorted,
/// so an Apply touches files in a stable order.
fn glob_paths(path: &str, home: &Path) -> Vec<PathBuf> {
    let rel = path.strip_prefix("~/").unwrap_or(path);
    let full = home.join(rel);
    let Some(name) = full.file_name().and_then(|n| n.to_str()) else {
        return vec![full];
    };
    let Some((prefix, suffix)) = name.split_once('*') else {
        return vec![full];
    };
    if suffix.contains('*') {
        return vec![full]; // shortcut: one wildcard per path, enough for scheme dirs
    }
    let Ok(entries) = fs::read_dir(full.parent().unwrap_or(home)) else {
        return vec![];
    };
    let mut out: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| {
            let Some(n) = p.file_name().and_then(|n| n.to_str()) else {
                return false;
            };
            n.len() >= prefix.len() + suffix.len() && n.starts_with(prefix) && n.ends_with(suffix)
        })
        .collect();
    out.sort();
    out
}

/// A preset's files that exist under `home`, in preset order. A wildcard is
/// kept as written when it matches something, so new scheme files are picked
/// up without re-detecting.
fn existing_files(preset: &Preset, home: &Path) -> Vec<String> {
    preset
        .files
        .iter()
        .filter(|rel| {
            if rel.contains('*') {
                return !glob_paths(rel, home).is_empty();
            }
            let rel_path = rel.strip_prefix("~/").unwrap_or(rel);
            home.join(rel_path).exists()
        })
        .map(|rel| rel.to_string())
        .collect()
}

/// The config for a preset, or `None` when none of its files exist here.
fn preset_config(preset: &Preset, home: &Path) -> Option<AppConfig> {
    let files = existing_files(preset, home);
    (!files.is_empty()).then(|| AppConfig {
        theme: None,
        files,
        reload: preset.reload.map(str::to_string),
    })
}

/// Preset apps whose config file actually exists under `home`.
fn seed_apps(home: &Path) -> BTreeMap<String, AppConfig> {
    let mut apps = BTreeMap::new();
    for preset in PRESETS {
        if let Some(app) = preset_config(preset, home) {
            apps.insert(preset.app.to_string(), app);
        }
    }
    apps
}

fn detect_apps_in(home: &Path, cfg: &ThemrConfig) -> Vec<(String, AppConfig)> {
    PRESETS
        .iter()
        .filter(|p| !cfg.apps.contains_key(p.app))
        .filter_map(|p| preset_config(p, home).map(|app| (p.app.to_string(), app)))
        .collect()
}

/// Apps themr knows about that have a config file on this machine but are not in
/// `cfg` yet. Seeding only happens for a brand new config, so this is how an
/// existing one picks up presets added later.
pub fn detect_apps(cfg: &ThemrConfig) -> Vec<(String, AppConfig)> {
    detect_apps_in(&home_dir(), cfg)
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
    let home = home_dir();
    for file in &app.files {
        let paths = if file.contains('*') {
            glob_paths(file, &home)
        } else {
            vec![expand(file)]
        };
        for path in paths {
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
    }
    // Only nudge the app when a file actually changed, so a repeat Apply is a no-op.
    if report.files.iter().any(|f| f.status == FileStatus::Updated)
        && let Some(cmd) = app.reload.as_deref()
        && let Err(e) = run_reload(cmd)
    {
        report.errors.push(format!("reload: {e}"));
    }
    report
}

/// Run one app's reload command through the shell, so pipes and `|| true` work.
fn run_reload(cmd: &str) -> Result<(), String> {
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .map_err(|e| format!("{cmd}: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let detail = stderr.trim();
    Err(if detail.is_empty() {
        format!("{cmd}: {e}", e = out.status)
    } else {
        format!("{cmd}: {detail}")
    })
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
        // kitty is nudged after a write so the running terminal re-reads the file
        assert_eq!(
            apps["kitty"].reload.as_deref(),
            Some("pkill -SIGUSR1 -x kitty || true")
        );
        // the preset table carries a broad set of apps
        assert!(PRESETS.len() >= 30, "expected a broad preset table");
    }

    #[test]
    fn wildcard_files_expand_to_matching_files_and_seed_once() {
        let home = tmpdir("glob");
        fs::create_dir_all(home.join(".local/share/konsole")).unwrap();
        fs::write(home.join(".local/share/konsole/A.colorscheme"), "[Color1]\nColor=1,1,1\n")
            .unwrap();
        fs::write(home.join(".local/share/konsole/B.colorscheme"), "[Color1]\nColor=2,2,2\n")
            .unwrap();
        fs::write(home.join(".local/share/konsole/notes.txt"), "x\n").unwrap();

        // the config keeps the wildcard, so new scheme files are picked up later
        let apps = seed_apps(&home);
        assert_eq!(
            apps["konsole"].files,
            vec!["~/.local/share/konsole/*.colorscheme".to_string()]
        );

        let all = glob_paths("~/.local/share/konsole/*.colorscheme", &home);
        let names: Vec<String> = all
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["A.colorscheme", "B.colorscheme"]);

        // no match is not an error and seeds nothing
        assert!(glob_paths("~/.local/share/konsole/*.nope", &home).is_empty());
        let empty = tmpdir("glob-empty");
        assert!(!seed_apps(&empty).contains_key("konsole"));
    }

    #[test]
    fn collects_every_preset_file_that_exists() {
        let home = tmpdir("seed-multi");
        fs::create_dir_all(home.join(".config/waybar")).unwrap();
        fs::write(home.join(".config/waybar/style.css"), "a { color: #000; }\n").unwrap();
        fs::write(home.join(".config/waybar/config.jsonc"), "{}\n").unwrap();

        let apps = seed_apps(&home);
        assert_eq!(
            apps["waybar"].files,
            vec![
                "~/.config/waybar/style.css".to_string(),
                "~/.config/waybar/config.jsonc".to_string()
            ],
            "existing files of an app are listed, missing ones are dropped"
        );
    }

    #[test]
    fn preset_reloads_can_not_hang_or_fail_when_the_app_is_absent() {
        for p in PRESETS {
            let Some(cmd) = p.reload else { continue };
            assert!(
                cmd.contains("|| true"),
                "{} reload must tolerate the app not running: {cmd}",
                p.app
            );
            // A daemon client (swaync-client, eww, …) blocks forever waiting for a
            // daemon that is not there, which would hang the whole Apply. pkill and
            // pgrep either find the process or exit immediately. xrdb needs no daemon.
            assert!(
                cmd.starts_with("pkill ")
                    || cmd.starts_with("pgrep ")
                    || cmd.starts_with("xrdb "),
                "{} reload must not call a daemon client unguarded: {cmd}",
                p.app
            );
        }
    }

    #[test]
    fn detect_apps_adds_only_what_is_missing() {
        let home = tmpdir("detect");
        fs::create_dir_all(home.join(".config/kitty")).unwrap();
        fs::create_dir_all(home.join(".config/waybar")).unwrap();
        fs::create_dir_all(home.join(".config/nvim")).unwrap();
        fs::write(home.join(".config/kitty/current-theme.conf"), "b #000000\n").unwrap();
        fs::write(home.join(".config/waybar/style.css"), "a { color: #000; }\n").unwrap();
        fs::write(home.join(".config/nvim/init.lua"), "-- nothing to theme\n").unwrap();

        // kitty already handled, waybar not; nvim is not a preset
        let mut cfg = ThemrConfig::default();
        cfg.apps.insert(
            "kitty".into(),
            AppConfig {
                theme: None,
                files: vec!["~/.config/kitty/current-theme.conf".into()],
                reload: None,
            },
        );

        let found = detect_apps_in(&home, &cfg);
        let names: Vec<&str> = found.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["waybar"], "only the missing preset with a file here");
        assert_eq!(found[0].1.files, vec!["~/.config/waybar/style.css"]);
        assert_eq!(
            found[0].1.reload.as_deref(),
            Some("pkill -SIGUSR2 -x waybar || true")
        );

        // running it again after adding finds nothing new
        cfg.apps.insert(found[0].0.clone(), found[0].1.clone());
        assert!(detect_apps_in(&home, &cfg).is_empty());
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
                reload: None,
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
    fn apply_app_writes_every_file_a_wildcard_matches() {
        let dir = tmpdir("apply-glob");
        fs::write(dir.join("A.colorscheme"), "[Background]\nColor=1,1,1\n").unwrap();
        fs::write(dir.join("B.colorscheme"), "[Background]\nColor=2,2,2\n").unwrap();
        fs::write(dir.join("keep.txt"), "untouched\n").unwrap();

        let mut cfg = ThemrConfig {
            theme: "a".into(),
            apps: BTreeMap::new(),
        };
        cfg.apps.insert(
            "konsole".into(),
            AppConfig {
                theme: None,
                files: vec![format!("{}/*.colorscheme", dir.display())],
                reload: None,
            },
        );
        let mut themes = BTreeMap::new();
        themes.insert("a".into(), theme("a", "#222222"));
        let mut state = State::default();
        let report = apply_all(&cfg, &themes, &mut state);

        assert!(report[0].errors.is_empty(), "{:?}", report[0].errors);
        assert_eq!(report[0].files.len(), 2, "both schemes, not the .txt");
        for name in ["A.colorscheme", "B.colorscheme"] {
            assert_eq!(
                fs::read_to_string(dir.join(name)).unwrap(),
                "[Background]\nColor=34,34,34\n"
            );
        }
        assert_eq!(fs::read_to_string(dir.join("keep.txt")).unwrap(), "untouched\n");
        assert_eq!(state.files.len(), 2, "one state entry per matched file");
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
                reload: None,
            },
        );

        let mut state = State::default();
        let report = apply_app("x", &cfg, &BTreeMap::new(), &mut state);
        assert!(report.errors[0].contains("not found"));
        assert_eq!(fs::read_to_string(&file).unwrap(), "bg {{background}}\n");
    }

    #[test]
    fn reload_runs_only_when_a_file_changed() {
        let dir = tmpdir("reload");
        let file = dir.join("colors.conf");
        let flag = dir.join("flag");
        fs::write(&file, "bg {{background}}\n").unwrap();

        let mut cfg = ThemrConfig {
            theme: "a".into(),
            apps: BTreeMap::new(),
        };
        cfg.apps.insert(
            "kitty".into(),
            AppConfig {
                theme: None,
                files: vec![file.to_string_lossy().into_owned()],
                reload: Some(format!("printf x >> {}", flag.display())),
            },
        );
        let mut themes = BTreeMap::new();
        themes.insert("a".into(), theme("a", "#111111"));

        let mut state = State::default();
        let first = apply_app("kitty", &cfg, &themes, &mut state);
        assert!(first.errors.is_empty(), "{:?}", first.errors);
        assert_eq!(fs::read_to_string(&flag).unwrap(), "x");

        // nothing changed this time, so the app must not be reloaded again
        let second = apply_app("kitty", &cfg, &themes, &mut state);
        assert!(second.errors.is_empty(), "{:?}", second.errors);
        assert_eq!(fs::read_to_string(&flag).unwrap(), "x");
    }

    #[test]
    fn failed_reload_is_reported() {
        let dir = tmpdir("reload-fail");
        let file = dir.join("colors.conf");
        fs::write(&file, "bg {{background}}\n").unwrap();
        let mut cfg = ThemrConfig {
            theme: "a".into(),
            apps: BTreeMap::new(),
        };
        cfg.apps.insert(
            "x".into(),
            AppConfig {
                theme: None,
                files: vec![file.to_string_lossy().into_owned()],
                reload: Some("echo boom >&2; exit 3".into()),
            },
        );
        let mut themes = BTreeMap::new();
        themes.insert("a".into(), theme("a", "#111111"));

        let mut state = State::default();
        let report = apply_app("x", &cfg, &themes, &mut state);
        assert!(report.errors[0].contains("boom"), "{:?}", report.errors);
    }
}
