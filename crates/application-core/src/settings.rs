//! User settings and the recent-files list, stored as one JSON file in the
//! user's configuration folder.
//!
//! Loading never fails: a missing file gives defaults, and a damaged file is
//! moved aside (`settings.json.damaged`) so the user starts from defaults
//! instead of a crash. Saving writes a temporary file and renames it over the
//! old one, so a crash mid-save leaves the previous settings intact.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How many recent files are remembered.
pub const MAX_RECENT: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LanguagePreference {
    #[default]
    #[serde(rename = "system")]
    System,
    #[serde(rename = "zh-Hant")]
    ZhHant,
    #[serde(rename = "en")]
    En,
}

/// Beginner mode shows the most-used adjustments; advanced shows all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum EditorMode {
    #[default]
    Beginner,
    Advanced,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentFile {
    pub path: String,
    /// Seconds since the Unix epoch.
    pub opened_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: ThemePreference,
    pub language: LanguagePreference,
    pub editor_mode: EditorMode,
    pub recent: Vec<RecentFile>,
}

/// Result of [`load`]: the settings, plus whether a damaged file was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub settings: Settings,
    pub recovered_from_damage: bool,
}

/// Read settings from `path`.
pub fn load(path: &Path) -> Loaded {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => {
            return Loaded {
                settings: Settings::default(),
                recovered_from_damage: false,
            }
        }
    };
    match serde_json::from_str::<Settings>(&text) {
        Ok(settings) => Loaded {
            settings,
            recovered_from_damage: false,
        },
        Err(_) => {
            let _ = fs::rename(path, damaged_path(path));
            Loaded {
                settings: Settings::default(),
                recovered_from_damage: true,
            }
        }
    }
}

/// Write settings to `path`, creating its folder if needed.
pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(&tmp, path)
}

fn damaged_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".damaged");
    path.with_file_name(name)
}

/// Windows paths are case-insensitive; compare them that way everywhere so
/// the same file opened as `C:\A.jpg` and `c:\a.jpg` is one entry.
fn same_path(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

impl Settings {
    /// Put `path` at the top of the recent list.
    pub fn add_recent(&mut self, path: &str, now: u64) {
        self.recent.retain(|entry| !same_path(&entry.path, path));
        self.recent.insert(
            0,
            RecentFile {
                path: path.to_string(),
                opened_at: now,
            },
        );
        self.recent.truncate(MAX_RECENT);
    }

    pub fn remove_recent(&mut self, path: &str) {
        self.recent.retain(|entry| !same_path(&entry.path, path));
    }

    pub fn clear_recent(&mut self) {
        self.recent.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "aibs-settings-{}-{name}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = TempDir::new("missing");
        let loaded = load(&dir.0.join("settings.json"));
        assert_eq!(loaded.settings, Settings::default());
        assert!(!loaded.recovered_from_damage);
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = TempDir::new("roundtrip");
        let path = dir.0.join("nested").join("settings.json");
        let mut settings = Settings {
            theme: ThemePreference::Dark,
            language: LanguagePreference::ZhHant,
            editor_mode: EditorMode::Advanced,
            recent: Vec::new(),
        };
        settings.add_recent("C:\\照片\\一.jpg", 100);
        save(&path, &settings).unwrap();
        assert_eq!(load(&path).settings, settings);
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn damaged_file_is_moved_aside_and_defaults_are_used() {
        let dir = TempDir::new("damaged");
        let path = dir.0.join("settings.json");
        fs::write(&path, "{ this is not json").unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.settings, Settings::default());
        assert!(loaded.recovered_from_damage);
        assert!(!path.exists());
        assert!(dir.0.join("settings.json.damaged").exists());
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let dir = TempDir::new("partial");
        let path = dir.0.join("settings.json");
        fs::write(&path, r#"{"theme":"light","futureOption":true}"#).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.settings.theme, ThemePreference::Light);
        assert_eq!(loaded.settings.language, LanguagePreference::System);
        assert_eq!(loaded.settings.editor_mode, EditorMode::Beginner);
        assert!(!loaded.recovered_from_damage);
    }

    #[test]
    fn json_uses_the_names_the_ui_expects() {
        let settings = Settings {
            theme: ThemePreference::Dark,
            language: LanguagePreference::ZhHant,
            editor_mode: EditorMode::Advanced,
            recent: vec![RecentFile {
                path: "a.jpg".into(),
                opened_at: 5,
            }],
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(
            json,
            r#"{"theme":"dark","language":"zh-Hant","editorMode":"advanced","recent":[{"path":"a.jpg","openedAt":5}]}"#
        );
    }

    #[test]
    fn recent_list_moves_reopened_files_to_the_top() {
        let mut settings = Settings::default();
        settings.add_recent("C:\\a.jpg", 1);
        settings.add_recent("C:\\b.jpg", 2);
        settings.add_recent("c:\\A.JPG", 3);
        let paths: Vec<_> = settings.recent.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, ["c:\\A.JPG", "C:\\b.jpg"]);
        assert_eq!(settings.recent[0].opened_at, 3);
    }

    #[test]
    fn recent_list_is_capped() {
        let mut settings = Settings::default();
        for i in 0..(MAX_RECENT + 5) {
            settings.add_recent(&format!("{i}.jpg"), i as u64);
        }
        assert_eq!(settings.recent.len(), MAX_RECENT);
        assert_eq!(settings.recent[0].path, format!("{}.jpg", MAX_RECENT + 4));
    }

    #[test]
    fn remove_and_clear_recent() {
        let mut settings = Settings::default();
        settings.add_recent("a.jpg", 1);
        settings.add_recent("b.jpg", 2);
        settings.remove_recent("A.JPG");
        assert_eq!(settings.recent.len(), 1);
        settings.clear_recent();
        assert!(settings.recent.is_empty());
    }
}
