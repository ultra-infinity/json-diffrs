//! What json-diff remembers between runs, in `settings.json` in the OS
//! config directory (`~/.config/io.github.ultra-infinity.json-diff` on
//! Linux, `%APPDATA%\io.github.ultra-infinity.json-diff` on Windows).
//!
//! API keys are never stored here; see [`crate::secrets`].

use crate::state::{AppState, Command, PanelKind, Theme};
use json_diff_core::{DiffOptions, IgnoreRule};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: Theme,
    pub zoom: f32,
    pub show_explorer: bool,
    pub show_agent: bool,
    pub options: DiffOptions,
    /// Keys whose changes are ignored, kept for every later comparison.
    pub ignored: Vec<IgnoreRule>,
    /// Kept for the frontend: panel widths, the chosen provider and model.
    pub ui: Value,
    /// The version that last ran, so the next one knows what's new to it.
    pub last_version: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self::of(&AppState::default(), Value::Null)
    }
}

impl Settings {
    /// The settings `state` is using, plus the frontend's.
    pub fn of(state: &AppState, ui: Value) -> Self {
        Self {
            theme: state.theme,
            zoom: state.zoom,
            show_explorer: state.show_explorer,
            show_agent: state.show_agent,
            options: state.options,
            ignored: state.ignored.clone(),
            ui,
            last_version: None,
        }
    }

    /// Puts these settings into `state` through the usual commands, so
    /// out-of-range values (a hand-edited zoom) are refused, not trusted.
    pub fn apply_to(&self, state: &mut AppState) {
        let commands = [
            Command::SetTheme { theme: self.theme },
            Command::SetZoom { zoom: self.zoom },
            Command::SetPanelVisible {
                panel: PanelKind::Explorer,
                visible: Some(self.show_explorer),
            },
            Command::SetPanelVisible {
                panel: PanelKind::Agent,
                visible: Some(self.show_agent),
            },
            Command::SetOptions {
                format: Some(self.options.format),
                sort_keys: Some(self.options.sort_keys),
                ignore_whitespace: Some(self.options.ignore_whitespace),
            },
        ];
        for command in commands {
            let _ = state.apply(command);
        }
        state.ignored.clone_from(&self.ignored);
        state.recompute(false);
        state.status = None;
    }

    /// The settings saved at `path`; `None` if there are none yet or the
    /// file is unreadable (the defaults are used and it is rewritten).
    pub fn load(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        match serde_json::from_str(&text) {
            Ok(settings) => Some(settings),
            Err(error) => {
                eprintln!("ignoring {}: {error}", path.display());
                None
            }
        }
    }

    /// Writes the settings to `path`, replacing the old file in one step so
    /// a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory)?;
        }
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&temporary, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn saved_settings_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/settings.json");
        assert_eq!(Settings::load(&path), None, "nothing saved yet");

        let mut state = AppState::default();
        state
            .apply(Command::SetTheme {
                theme: Theme::Light,
            })
            .unwrap();
        state.apply(Command::SetZoom { zoom: 1.5 }).unwrap();
        state
            .apply(Command::IgnoreChanges {
                side: crate::state::Side::Left,
                pointer: None,
                key: Some("updatedAt".into()),
            })
            .unwrap();
        let settings = Settings::of(&state, json!({ "provider": "openai" }));
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), Some(settings.clone()));

        let mut restored = AppState::default();
        settings.apply_to(&mut restored);
        assert_eq!(restored.theme, Theme::Light);
        assert_eq!(restored.zoom, 1.5);
        assert_eq!(restored.ignored, state.ignored);
        assert!(restored.status.is_none(), "restoring is silent");
    }

    #[test]
    fn bad_or_partial_files_fall_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(Settings::load(&path), None);

        // Missing fields take their defaults; unusable values are refused.
        std::fs::write(&path, r#"{ "theme": "light", "zoom": 40 }"#).unwrap();
        let settings = Settings::load(&path).unwrap();
        assert_eq!(settings.theme, Theme::Light);
        assert!(settings.show_explorer);
        let mut state = AppState::default();
        settings.apply_to(&mut state);
        assert_eq!(state.zoom, 1.0, "zoom 40 is out of range");
    }
}
