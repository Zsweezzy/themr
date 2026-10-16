use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// Theme id for this app only. `None` means "follow the global theme".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
    /// Shell command run after this app's files are written, to make it re-read them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reload: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemrConfig {
    pub theme: String,
    #[serde(default)]
    pub apps: BTreeMap<String, AppConfig>,
}

impl Default for ThemrConfig {
    fn default() -> Self {
        Self {
            theme: "tokyonight-moon".into(),
            apps: BTreeMap::new(),
        }
    }
}

impl ThemrConfig {
    pub fn app_theme(&self, app: &str) -> String {
        self.apps
            .get(app)
            .and_then(|a| a.theme.clone())
            .unwrap_or_else(|| self.theme.clone())
    }
}

/// What themr wrote into each file last time: file -> token -> hex.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub files: BTreeMap<String, BTreeMap<String, String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_app_override_beats_global() {
        let mut cfg = ThemrConfig {
            theme: "nord".into(),
            apps: BTreeMap::new(),
        };
        cfg.apps.insert(
            "kitty".into(),
            AppConfig {
                theme: Some("gruvbox-dark".into()),
                files: vec![],
                reload: None,
            },
        );
        cfg.apps.insert("waybar".into(), AppConfig::default());
        assert_eq!(cfg.app_theme("kitty"), "gruvbox-dark");
        assert_eq!(cfg.app_theme("waybar"), "nord");
        assert_eq!(cfg.app_theme("unknown"), "nord");
    }

    #[test]
    fn reload_command_is_optional_and_parses() {
        let cfg: ThemrConfig = toml::from_str(
            r#"
theme = "nord"

[apps.kitty]
files = ["~/.config/kitty/current-theme.conf"]
reload = "pkill -SIGUSR1 -x kitty"

[apps.btop]
files = ["~/.config/btop/btop.conf"]
"#,
        )
        .unwrap();
        assert_eq!(
            cfg.apps["kitty"].reload.as_deref(),
            Some("pkill -SIGUSR1 -x kitty")
        );
        assert!(cfg.apps["btop"].reload.is_none());

        let back: ThemrConfig = toml::from_str(&toml::to_string_pretty(&cfg).unwrap()).unwrap();
        assert_eq!(back.apps["kitty"].reload, cfg.apps["kitty"].reload);
        assert!(back.apps["btop"].reload.is_none());
    }

    #[test]
    fn config_roundtrips() {
        let mut cfg = ThemrConfig::default();
        cfg.apps.insert(
            "kitty".into(),
            AppConfig {
                theme: None,
                files: vec!["~/.config/kitty/current-theme.conf".into()],
                reload: None,
            },
        );
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: ThemrConfig = toml::from_str(&text).unwrap();
        assert_eq!(back.theme, "tokyonight-moon");
        assert!(back.apps["kitty"].theme.is_none());
    }
}
