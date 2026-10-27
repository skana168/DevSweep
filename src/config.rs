use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::delete::DeleteMode;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub roots: Vec<PathBuf>,
    pub delete_mode: DeleteMode,
    pub theme: ThemeChoice,
    pub language: LanguageChoice,
    pub window: Option<Window>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    System,
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguageChoice {
    #[default]
    System,
    Zh,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub width: f32,
    pub height: f32,
}

/// macOS `~/Library/Application Support/DevSweep`, Windows `%APPDATA%\DevSweep`.
pub fn dir() -> Option<PathBuf> {
    // `data_dir` is Application Support on macOS and roaming AppData on Windows.
    directories::BaseDirs::new().map(|base| base.data_dir().join("DevSweep"))
}

pub fn config_path() -> Option<PathBuf> {
    dir().map(|dir| dir.join("config.toml"))
}

pub fn rules_path() -> Option<PathBuf> {
    dir().map(|dir| dir.join("rules.toml"))
}

/// A missing or unreadable config yields the defaults; the app still starts.
pub fn load() -> Config {
    load_from(config_path())
}

fn load_from(path: Option<PathBuf>) -> Config {
    path.and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

/// Best effort: a failed write must not take the app down with it.
pub fn save(config: &Config) {
    save_to(config_path(), config);
}

fn save_to(path: Option<PathBuf>, config: &Config) {
    let Some(path) = path else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = toml::to_string_pretty(config) {
        let _ = fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_lives_under_the_app_name() {
        let dir = dir().expect("a config directory on this platform");
        assert_eq!(dir.file_name().unwrap(), "DevSweep");
    }

    #[test]
    fn a_saved_config_loads_back() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("nested/config.toml");
        let config = Config {
            roots: vec![PathBuf::from("/work/app"), PathBuf::from("/work/other")],
            delete_mode: DeleteMode::Direct,
            theme: ThemeChoice::Dark,
            language: LanguageChoice::Zh,
            window: Some(Window {
                width: 1280.0,
                height: 800.0,
            }),
        };

        save_to(Some(path.clone()), &config);

        assert_eq!(load_from(Some(path)), config);
    }

    #[test]
    fn a_missing_or_broken_config_falls_back_to_defaults() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            load_from(Some(temp.path().join("missing.toml"))),
            Config::default()
        );

        let broken = temp.path().join("config.toml");
        fs::write(&broken, "roots = [").unwrap();
        assert_eq!(load_from(Some(broken)), Config::default());
    }
}
