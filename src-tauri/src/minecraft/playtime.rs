use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlaytimeStore {
    pub total_seconds: u64,
    pub versions: HashMap<String, u64>,
    pub last_played: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaytimeSummary {
    pub total_seconds: u64,
    pub formatted_total: String,
    pub versions: HashMap<String, u64>,
    pub formatted_versions: HashMap<String, String>,
}

impl PlaytimeStore {
    pub fn path_for(layout: &crate::minecraft::versions::StorageLayout) -> PathBuf {
        layout.launcher_root().join("playtime.json")
    }

    pub fn get_path(_app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
        let layout = crate::minecraft::versions::StorageLayout::current()?;
        Ok(Self::path_for(&layout))
    }

    pub fn load(app_handle: &tauri::AppHandle) -> Result<Self, String> {
        let path = Self::get_path(app_handle)?;
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str::<Self>(&content)
                .map_err(|error| format!("Could not parse playtime data: {error}")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(format!(
                "Could not read playtime data {}: {error}",
                path.display()
            )),
        }
    }

    pub fn save(&self, app_handle: &tauri::AppHandle) -> Result<(), String> {
        let path = Self::get_path(app_handle)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Could not create playtime directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        let content = serde_json::to_string_pretty(self)
            .map_err(|error| format!("Could not serialize playtime data: {error}"))?;
        fs::write(&path, content)
            .map_err(|error| format!("Could not save playtime data {}: {error}", path.display()))?;
        Ok(())
    }

    pub fn add_session(
        &mut self,
        version_id: &str,
        duration_secs: u64,
        app_handle: &tauri::AppHandle,
    ) -> Result<(), String> {
        if duration_secs == 0 {
            return Ok(());
        }
        self.total_seconds = self.total_seconds.saturating_add(duration_secs);
        let entry = self.versions.entry(version_id.to_string()).or_insert(0);
        *entry = entry.saturating_add(duration_secs);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("System clock is before the Unix epoch: {error}"))?
            .as_secs();
        self.last_played.insert(version_id.to_string(), now);

        self.save(app_handle)
    }
}

pub fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let mins = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{}h {}m", hours, mins)
    } else if mins > 0 {
        format!("{}m", mins)
    } else {
        format!("{}s", seconds)
    }
}

pub fn get_summary(app_handle: &tauri::AppHandle) -> Result<PlaytimeSummary, String> {
    let store = PlaytimeStore::load(app_handle)?;
    let mut formatted_versions = HashMap::new();
    for (ver, secs) in &store.versions {
        formatted_versions.insert(ver.clone(), format_duration(*secs));
    }

    Ok(PlaytimeSummary {
        total_seconds: store.total_seconds,
        formatted_total: format_duration(store.total_seconds),
        versions: store.versions,
        formatted_versions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minecraft::versions::StorageLayout;
    use std::path::PathBuf;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(45), "45s");
        assert_eq!(format_duration(125), "2m");
        assert_eq!(format_duration(3665), "1h 1m");
    }

    #[test]
    fn playtime_file_is_under_launcher_metadata_root() {
        let layout = StorageLayout::from_app_root(PathBuf::from(
            "C:/Users/test/AppData/Roaming/DBC Super Launcher",
        ));

        assert_eq!(
            PlaytimeStore::path_for(&layout),
            layout.launcher_root().join("playtime.json")
        );
    }
}
