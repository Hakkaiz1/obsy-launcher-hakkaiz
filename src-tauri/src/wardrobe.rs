use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WardrobeSkin {
    pub id: String,
    pub name: String,
    pub base64_data: String,
    pub slim: bool,
    pub profile_id: Option<String>,
}

pub struct WardrobeStore {
    path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::WardrobeStore;
    use crate::minecraft::versions::StorageLayout;
    use std::path::PathBuf;

    #[test]
    fn wardrobe_file_is_under_launcher_metadata_root() {
        let layout = StorageLayout::from_app_root(PathBuf::from(
            "C:/Users/test/AppData/Roaming/DBC Super Launcher",
        ));

        assert_eq!(
            WardrobeStore::path_for(&layout),
            layout.launcher_root().join("wardrobe.json")
        );
    }
}

impl WardrobeStore {
    pub fn path_for(layout: &crate::minecraft::versions::StorageLayout) -> PathBuf {
        layout.launcher_root().join("wardrobe.json")
    }

    pub fn new(_app_handle: &tauri::AppHandle) -> Result<Self, String> {
        let layout = crate::minecraft::versions::StorageLayout::current()?;
        Ok(Self {
            path: Self::path_for(&layout),
        })
    }

    pub fn load(&self) -> Result<Vec<WardrobeSkin>, String> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(error) => {
                return Err(format!(
                    "Could not read wardrobe {}: {error}",
                    self.path.display()
                ))
            }
        };
        serde_json::from_str(&contents)
            .map_err(|error| format!("Could not parse wardrobe data: {error}"))
    }

    pub fn save(&self, skins: &[WardrobeSkin]) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Could not create wardrobe directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        let json_contents = serde_json::to_string_pretty(skins)
            .map_err(|error| format!("Could not serialize wardrobe data: {error}"))?;
        fs::write(&self.path, json_contents)
            .map_err(|error| format!("Could not save wardrobe {}: {error}", self.path.display()))?;
        Ok(())
    }

    pub fn add_skin(
        &self,
        file_bytes: Vec<u8>,
        name: String,
        slim: bool,
        profile_id: String,
    ) -> Result<WardrobeSkin, String> {
        let mut skins = self.load()?;

        let id = Uuid::new_v4().to_string();
        use base64::{engine::general_purpose, Engine as _};
        let base64_data = format!(
            "data:image/png;base64,{}",
            general_purpose::STANDARD.encode(&file_bytes)
        );

        let skin = WardrobeSkin {
            id,
            name,
            base64_data,
            slim,
            profile_id: Some(profile_id),
        };

        skins.push(skin.clone());
        self.save(&skins)?;

        Ok(skin)
    }

    pub fn remove_skin(&self, id: &str) -> Result<(), String> {
        let mut skins = self.load()?;
        if let Some(index) = skins.iter().position(|s| s.id == id) {
            skins.remove(index);
            self.save(&skins)?;
        }
        Ok(())
    }
}
