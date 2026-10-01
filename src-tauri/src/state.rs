use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Language {
    #[serde(alias = "ENGLISH", alias = "RUSSIAN")]
    Portuguese,
}

impl Default for Language {
    fn default() -> Self {
        Language::Portuguese
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Theme {
    Light,
    Dark,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::Light
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherState {
    pub scale: i32,
    pub language: Language,
    pub theme: Theme,

    pub memory_amount: i32,
    pub auto_memory: bool,

    pub screen_width: i32,
    pub screen_height: i32,
    pub fullscreen: bool,
    pub jvm_arguments: String,
    pub java_path: Option<String>,
    pub close_after_launch: bool,

    pub selected_profile_id: Option<String>,
}

impl Default for LauncherState {
    fn default() -> Self {
        Self {
            scale: 1,
            language: Language::Portuguese,
            theme: Theme::Light,
            memory_amount: 4096,
            auto_memory: true,
            screen_width: 854,
            screen_height: 480,
            fullscreen: false,
            jvm_arguments: String::new(),
            java_path: None,
            close_after_launch: false,
            selected_profile_id: None,
        }
    }
}

impl LauncherState {
    pub fn normalize(&mut self) {
        self.scale = self.scale.max(1);
        self.memory_amount = self.memory_amount.clamp(512, 65536);
        if self.auto_memory {
            let sys = sysinfo::System::new_with_specifics(
                sysinfo::RefreshKind::nothing()
                    .with_memory(sysinfo::MemoryRefreshKind::everything()),
            );
            let total_mb = sys.total_memory() / 1024 / 1024;
            let half_ram = (total_mb / 2) as i32;
            self.memory_amount = half_ram.clamp(2048, 8192);
        }
        if self.screen_width < 1 {
            self.screen_width = 854;
        }
        if self.screen_height < 1 {
            self.screen_height = 480;
        }
    }

    pub fn path_for(layout: &crate::minecraft::versions::StorageLayout) -> PathBuf {
        layout.launcher_root().join("launcher_state.json")
    }

    pub fn get_path(_app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
        let layout = crate::minecraft::versions::StorageLayout::current()?;
        Ok(Self::path_for(&layout))
    }

    pub fn load(app_handle: &tauri::AppHandle) -> Result<Self, String> {
        let path = Self::get_path(app_handle)?;
        let mut state = match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents)
                .map_err(|error| format!("Could not parse launcher state: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(error) => {
                return Err(format!(
                    "Could not read launcher state {}: {error}",
                    path.display()
                ))
            }
        };
        state.normalize();
        state.save(app_handle)?;
        Ok(state)
    }

    pub fn save(&self, app_handle: &tauri::AppHandle) -> Result<(), String> {
        let path = Self::get_path(app_handle)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Could not create launcher state directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        let contents = serde_json::to_string_pretty(self)
            .map_err(|error| format!("Could not serialize launcher state: {error}"))?;
        fs::write(&path, contents).map_err(|error| {
            format!("Could not save launcher state {}: {error}", path.display())
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Language, LauncherState};
    use crate::minecraft::versions::StorageLayout;
    use std::path::PathBuf;

    #[test]
    fn launcher_state_file_is_under_launcher_metadata_root() {
        let layout = StorageLayout::from_app_root(PathBuf::from(
            "C:/Users/test/AppData/Roaming/DBC Super Launcher",
        ));

        assert_eq!(
            LauncherState::path_for(&layout),
            layout.launcher_root().join("launcher_state.json")
        );
    }

    #[test]
    fn language_defaults_to_portuguese_in_saved_state() {
        let language = Language::default();

        assert_eq!(serde_json::to_string(&language).unwrap(), "\"PORTUGUESE\"");
    }

    #[test]
    fn legacy_language_values_load_as_portuguese() {
        for legacy_value in ["\"ENGLISH\"", "\"RUSSIAN\""] {
            let language: Language = serde_json::from_str(legacy_value).unwrap();

            assert_eq!(language, Language::default());
        }
    }

    #[test]
    fn legacy_state_omits_removed_version_fields() {
        let legacy: LauncherState = serde_json::from_value(serde_json::json!({
            "scale": 1,
            "language": "PORTUGUESE",
            "theme": "LIGHT",
            "memoryAmount": 4096,
            "autoMemory": false,
            "screenWidth": 1280,
            "screenHeight": 720,
            "fullscreen": true,
            "jvmArguments": "-Xmx4G",
            "javaPath": null,
            "closeAfterLaunch": true,
            "releaseFilter": true,
            "moddedFilter": false,
            "snapshotFilter": true,
            "legacyFilter": false,
            "selectedProfileId": "profile-id",
            "selectedVersionId": "1.7.10"
        }))
        .unwrap();
        let saved = serde_json::to_value(legacy).unwrap();

        for key in [
            "releaseFilter",
            "moddedFilter",
            "snapshotFilter",
            "legacyFilter",
            "selectedVersionId",
        ] {
            assert!(saved.get(key).is_none(), "legacy key {key} was serialized");
        }
        assert_eq!(saved["selectedProfileId"], "profile-id");
        assert_eq!(saved["language"], "PORTUGUESE");
        assert_eq!(saved["memoryAmount"], 4096);
        assert_eq!(saved["screenWidth"], 1280);
        assert_eq!(saved["screenHeight"], 720);
    }
}
