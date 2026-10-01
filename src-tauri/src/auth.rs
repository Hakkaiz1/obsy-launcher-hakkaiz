use keyring::Entry;
use magic_crypt::{new_magic_crypt, MagicCryptTrait};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

fn get_encryption_key(store_path: &std::path::Path) -> Result<String, String> {
    let key_path = store_path.with_file_name("profiles.key");
    match fs::read_to_string(&key_path) {
        Ok(key) if !key.is_empty() => return Ok(key),
        Ok(_) => return Err("Profile encryption key file is empty".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Could not read profile encryption key {}: {error}",
                key_path.display()
            ))
        }
    }

    if let Ok(entry) = Entry::new("obsy-launcher", "profile-encryption-key") {
        if let Ok(password) = entry.get_password() {
            return Ok(password);
        }
        let key = Uuid::new_v4().to_string() + &Uuid::new_v4().to_string();
        if entry.set_password(&key).is_ok() {
            return Ok(key);
        }
    }

    let key = Uuid::new_v4().to_string() + &Uuid::new_v4().to_string();
    if let Some(parent) = key_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Could not create profile storage directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(&key_path, &key).map_err(|error| {
        format!(
            "Could not save profile encryption key {}: {error}",
            key_path.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(&key_path).map_err(|error| {
            format!("Could not inspect profile encryption key permissions: {error}")
        })?;
        let mut perms = metadata.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&key_path, perms)
            .map_err(|error| format!("Could not protect profile encryption key: {error}"))?;
    }
    Ok(key)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftCape {
    pub id: String,
    pub url: String,
    pub alias: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub username: String,
    pub microsoft: bool,
    pub skin_png: Option<String>,
    pub cape_png: Option<String>,
    pub slim: bool,
    pub capes: Vec<MinecraftCape>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
}

impl Profile {
    pub fn new_offline(username: String) -> Self {
        let id = Uuid::new_v3(
            &Uuid::NAMESPACE_OID,
            format!("OfflinePlayer:{}", username).as_bytes(),
        )
        .to_string();
        let skin_png = Some(format!("https://minotar.net/skin/{}", username));
        Self {
            id,
            username,
            microsoft: false,
            skin_png,
            cape_png: None,
            slim: false,
            capes: vec![],
            access_token: None,
            refresh_token: None,
        }
    }
}

pub struct ProfileStore {
    path: PathBuf,
}

impl ProfileStore {
    pub fn path_for(layout: &crate::minecraft::versions::StorageLayout) -> PathBuf {
        layout.launcher_root().join("profiles.json")
    }

    pub fn new(_app_handle: &tauri::AppHandle) -> Result<Self, String> {
        let layout = crate::minecraft::versions::StorageLayout::current()?;
        Ok(Self {
            path: Self::path_for(&layout),
        })
    }

    pub fn load(&self) -> Result<Vec<Profile>, String> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(error) => {
                return Err(format!(
                    "Could not read profiles {}: {error}",
                    self.path.display()
                ))
            }
        };
        let key = get_encryption_key(&self.path)?;
        let mc = new_magic_crypt!(key, 256);
        let decrypted = mc
            .decrypt_base64_to_string(&contents)
            .map_err(|error| format!("Could not decrypt saved profiles: {error}"))?;
        serde_json::from_str(&decrypted)
            .map_err(|error| format!("Could not parse saved profiles: {error}"))
    }

    pub fn save(&self, profiles: &[Profile]) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "Could not create profile storage directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        let json_contents = serde_json::to_string_pretty(profiles)
            .map_err(|error| format!("Could not serialize profiles: {error}"))?;

        let key = get_encryption_key(&self.path)?;
        let mc = new_magic_crypt!(key, 256);
        let encrypted = mc.encrypt_str_to_base64(json_contents);

        fs::write(&self.path, encrypted)
            .map_err(|error| format!("Could not save profiles {}: {error}", self.path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&self.path)
                .map_err(|error| format!("Could not inspect profile file permissions: {error}"))?;
            let mut perms = metadata.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&self.path, perms)
                .map_err(|error| format!("Could not protect saved profiles: {error}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ProfileStore;
    use crate::minecraft::versions::StorageLayout;
    use std::path::PathBuf;

    #[test]
    fn profile_store_and_encryption_key_are_under_launcher_metadata_root() {
        let layout = StorageLayout::from_app_root(PathBuf::from(
            "C:/Users/test/AppData/Roaming/DBC Super Launcher",
        ));
        let profiles = ProfileStore::path_for(&layout);

        assert_eq!(profiles, layout.launcher_root().join("profiles.json"));
        assert_eq!(
            profiles.with_file_name("profiles.key").parent(),
            Some(layout.launcher_root().as_path())
        );
    }
}
