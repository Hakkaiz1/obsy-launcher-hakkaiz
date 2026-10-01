use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageLayout {
    app_root: PathBuf,
}

impl StorageLayout {
    pub fn from_app_root(app_root: PathBuf) -> Self {
        Self { app_root }
    }

    pub fn resolve(
        is_debug: bool,
        current_dir: Option<PathBuf>,
        data_dir: Option<PathBuf>,
    ) -> Result<Self, String> {
        let app_root = if is_debug {
            current_dir
                .ok_or_else(|| "Could not resolve the launcher working directory".to_string())?
                .join(".obsy")
        } else {
            data_dir
                .ok_or_else(|| "Could not resolve the user data directory".to_string())?
                .join("DBC Super Launcher")
        };
        Ok(Self::from_app_root(app_root))
    }

    pub fn current() -> Result<Self, String> {
        Self::resolve(
            cfg!(debug_assertions),
            std::env::current_dir().ok(),
            dirs::data_dir(),
        )
    }

    pub fn app_root(&self) -> &std::path::Path {
        &self.app_root
    }

    pub fn game_root(&self) -> &std::path::Path {
        &self.app_root
    }

    pub fn launcher_root(&self) -> PathBuf {
        self.app_root.join("launcher")
    }

    pub fn java_root(&self) -> PathBuf {
        self.app_root.join("java")
    }

    pub fn technic_pack_root(&self, pack_id: u64) -> PathBuf {
        self.launcher_root()
            .join("technic")
            .join(pack_id.to_string())
    }
}

pub fn get_minecraft_dir() -> PathBuf {
    #[cfg(debug_assertions)]
    {
        if let Ok(cwd) = std::env::current_dir() {
            return cwd.join(".obsy");
        }
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            #[cfg(target_os = "macos")]
            {
                let path_str = exe_dir.to_string_lossy();
                if path_str.contains(".app/Contents/MacOS") {
                    let mut path = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
                    path.push("obsy");
                    return path;
                }
            }
            return exe_dir.join(".obsy");
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        return cwd.join(".obsy");
    }

    PathBuf::from(".obsy")
}

#[cfg(test)]
mod tests {
    use super::StorageLayout;
    use std::path::PathBuf;

    #[test]
    fn storage_layout_uses_app_root_for_game_and_separates_launcher_data() {
        let layout = StorageLayout::from_app_root(PathBuf::from(
            "C:/Users/test/AppData/Roaming/DBC Super Launcher",
        ));

        assert_eq!(
            layout.game_root(),
            PathBuf::from("C:/Users/test/AppData/Roaming/DBC Super Launcher")
        );
        assert_eq!(
            layout.launcher_root(),
            PathBuf::from("C:/Users/test/AppData/Roaming/DBC Super Launcher/launcher")
        );
        assert_eq!(
            layout.java_root(),
            PathBuf::from("C:/Users/test/AppData/Roaming/DBC Super Launcher/java")
        );
        assert_eq!(
            layout.technic_pack_root(1_132_904),
            PathBuf::from(
                "C:/Users/test/AppData/Roaming/DBC Super Launcher/launcher/technic/1132904"
            )
        );
    }

    #[test]
    fn storage_layout_resolves_debug_root_from_current_directory() {
        let layout =
            StorageLayout::resolve(true, Some(PathBuf::from("C:/projects/obsy-launcher")), None)
                .unwrap();

        assert_eq!(
            layout.app_root(),
            PathBuf::from("C:/projects/obsy-launcher/.obsy")
        );
    }

    #[test]
    fn storage_layout_resolves_release_root_from_roaming_data_directory() {
        let layout = StorageLayout::resolve(
            false,
            None,
            Some(PathBuf::from("C:/Users/test/AppData/Roaming")),
        )
        .unwrap();

        assert_eq!(
            layout.app_root(),
            PathBuf::from("C:/Users/test/AppData/Roaming/DBC Super Launcher")
        );
    }

    #[test]
    fn storage_layout_rejects_missing_base_paths_without_fallback() {
        assert!(StorageLayout::resolve(false, Some(PathBuf::from("C:/cwd")), None).is_err());
        assert!(StorageLayout::resolve(true, None, Some(PathBuf::from("C:/data"))).is_err());
    }
}
