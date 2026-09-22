use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub capture_hotkey: String,
    pub dictate_hotkey: String,
    pub microphone: String,
    pub speech_model: String,
    pub tidy: bool,
    pub vocabulary: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            capture_hotkey: "Ctrl+Shift+S".into(),
            dictate_hotkey: "RightAlt".into(),
            microphone: String::new(),
            speech_model: "base-en-q5".into(),
            tidy: true,
            vocabulary: String::new(),
        }
    }
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("settings.json"))
        .map_err(|e| e.to_string())
}

pub fn load(app: &AppHandle) -> Settings {
    let mut value: Settings = path(app)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    // Migrate the original Windows default while preserving shortcuts the user
    // deliberately customized to anything else.
    if value.dictate_hotkey.eq_ignore_ascii_case("Ctrl+Shift+D") {
        value.dictate_hotkey = "RightAlt".into();
    }
    value
}

pub fn save(app: &AppHandle, value: &Settings) -> Result<(), String> {
    let path = path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}
