use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::Manager;

/// Settings that must survive `clear_all_browsing_data` / logout.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SidebarSettings {
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub collapsed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SettingsFile {
    #[serde(default)]
    sidebar: SidebarSettings,
    #[serde(default, alias = "standard_theme")]
    global_theme: bool,
    #[serde(default = "default_true")]
    auto_updates: bool,
    #[serde(default)]
    tray_mode: bool,
}

fn default_true() -> bool {
    true
}

pub struct SettingsStore {
    path: Option<PathBuf>,
    inner: Mutex<SettingsFile>,
}

impl SettingsStore {
    pub fn load(app: &tauri::AppHandle) -> Self {
        let path = app
            .path()
            .app_config_dir()
            .ok()
            .map(|directory| directory.join("settings.json"));

        let inner = path
            .as_ref()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<SettingsFile>(&text).ok())
            .unwrap_or_default();

        Self {
            path,
            inner: Mutex::new(inner),
        }
    }

    pub fn sidebar(&self) -> SidebarSettings {
        self.inner
            .lock()
            .map(|settings| settings.sidebar.clone())
            .unwrap_or_default()
    }

    pub fn update_sidebar(&self, width: Option<u32>, collapsed: bool) -> bool {
        let Ok(mut settings) = self.inner.lock() else {
            return false;
        };

        if settings.sidebar.width == width && settings.sidebar.collapsed == collapsed {
            return false;
        }

        settings.sidebar.width = width;
        settings.sidebar.collapsed = collapsed;
        self.save(&settings);
        true
    }

    pub fn global_theme(&self) -> bool {
        self.inner
            .lock()
            .map(|settings| settings.global_theme)
            .unwrap_or(false)
    }

    pub fn update_global_theme(&self, enabled: bool) -> bool {
        let Ok(mut settings) = self.inner.lock() else {
            return false;
        };

        if settings.global_theme == enabled {
            return false;
        }

        settings.global_theme = enabled;
        self.save(&settings);
        true
    }

    pub fn auto_updates(&self) -> bool {
        self.inner
            .lock()
            .map(|settings| settings.auto_updates)
            .unwrap_or(true)
    }

    pub fn update_auto_updates(&self, enabled: bool) -> bool {
        let Ok(mut settings) = self.inner.lock() else {
            return false;
        };

        if settings.auto_updates == enabled {
            return false;
        }

        settings.auto_updates = enabled;
        self.save(&settings);
        true
    }

    pub fn tray_mode(&self) -> bool {
        self.inner
            .lock()
            .map(|settings| settings.tray_mode)
            .unwrap_or(false)
    }

    pub fn update_tray_mode(&self, enabled: bool) -> bool {
        let Ok(mut settings) = self.inner.lock() else {
            return false;
        };

        if settings.tray_mode == enabled {
            return false;
        }

        settings.tray_mode = enabled;
        self.save(&settings);
        true
    }

    fn save(&self, settings: &SettingsFile) {
        let Some(path) = &self.path else {
            return;
        };

        if let Some(directory) = path.parent()
            && let Err(error) = fs::create_dir_all(directory)
        {
            eprintln!("[mesa] failed to create settings directory: {error}");
            return;
        }

        match serde_json::to_string_pretty(settings) {
            Ok(json) => {
                if let Err(error) = fs::write(path, json) {
                    eprintln!("[mesa] failed to write settings: {error}");
                }
            }
            Err(error) => eprintln!("[mesa] failed to serialize settings: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let settings = SettingsFile {
            sidebar: SidebarSettings {
                width: Some(280),
                collapsed: true,
            },
            global_theme: true,
            auto_updates: false,
            tray_mode: true,
        };

        let json = serde_json::to_string(&settings).expect("serialize settings");
        let parsed: SettingsFile = serde_json::from_str(&json).expect("deserialize settings");

        assert_eq!(parsed.sidebar.width, Some(280));
        assert!(parsed.sidebar.collapsed);
        assert!(parsed.global_theme);
        assert!(!parsed.auto_updates);
        assert!(parsed.tray_mode);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let parsed: SettingsFile = serde_json::from_str("{}").expect("deserialize empty settings");
        assert_eq!(parsed.sidebar.width, None);
        assert!(!parsed.sidebar.collapsed);
        assert!(!parsed.global_theme);
        // Auto-updates opt in rather than opt out.
        assert!(parsed.auto_updates);
    }

    #[test]
    fn legacy_standard_theme_field_is_read() {
        let parsed: SettingsFile = serde_json::from_str(r#"{"standard_theme":true}"#)
            .expect("deserialize legacy settings");
        assert!(parsed.global_theme);
    }
}
