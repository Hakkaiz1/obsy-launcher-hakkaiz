use std::path::PathBuf;

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
