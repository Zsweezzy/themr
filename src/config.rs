use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// Theme id for this app only. `None` means "follow the global theme".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
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
            },
        );
        cfg.apps.insert("waybar".into(), AppConfig::default());
        assert_eq!(cfg.app_theme("kitty"), "gruvbox-dark");
        assert_eq!(cfg.app_theme("waybar"), "nord");
        assert_eq!(cfg.app_theme("unknown"), "nord");
    }

    #[test]
    fn config_roundtrips() {
        let mut cfg = ThemrConfig::default();
        cfg.apps.insert(
            "kitty".into(),
            AppConfig {
                theme: None,
                files: vec!["~/.config/kitty/current-theme.conf".into()],
            },
        );
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: ThemrConfig = toml::from_str(&text).unwrap();
        assert_eq!(back.theme, "tokyonight-moon");
        assert!(back.apps["kitty"].theme.is_none());
    }
}
