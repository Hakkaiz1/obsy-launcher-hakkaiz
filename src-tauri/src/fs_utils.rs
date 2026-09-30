use std::path::{Path, PathBuf};

pub fn sanitize_path(base: &Path, relative: &Path) -> Result<PathBuf, String> {
    if relative.is_absolute() {
        return Err("Invalid file path: absolute path not allowed".to_string());
    }
    for component in relative.components() {
        match component {
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err("Invalid file path: path traversal detected".to_string());
            }
            _ => {}
        }
    }
    let full = base.join(relative);
    if full.starts_with(base) {
        Ok(full)
    } else {
        Err("Invalid file path: path traversal detected".to_string())
    }
}

pub fn safe_zip_extract_path(target_dir: &Path, rel_name: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(rel_name);
    if rel_path.is_absolute() {
        return Err(format!(
            "Invalid archive entry: absolute path '{}'",
            rel_name
        ));
    }
    for component in rel_path.components() {
        match component {
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(format!(
                    "Path traversal detected in archive entry: '{}'",
                    rel_name
                ));
            }
            _ => {}
        }
    }
    let outpath = target_dir.join(rel_path);
    if outpath.starts_with(target_dir) {
        Ok(outpath)
    } else {
        Err(format!(
            "Archive entry '{}' escapes target directory",
            rel_name
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn safe_file_and_nested_paths_stay_under_base() {
        let base = PathBuf::from("/tmp/obsy_test_base");
        assert_eq!(
            sanitize_path(&base, Path::new("index.js")).unwrap(),
            base.join("index.js")
        );
        assert_eq!(
            sanitize_path(&base, Path::new("assets/icon.png")).unwrap(),
            base.join("assets/icon.png")
        );
    }

    #[test]
    fn sanitize_path_rejects_parent_traversal_and_absolute_paths() {
        let base = PathBuf::from("/tmp/obsy_test_base");
        assert!(sanitize_path(&base, Path::new("../evil.js")).is_err());
        assert!(sanitize_path(&base, Path::new("nested/../../evil")).is_err());
        assert!(sanitize_path(&base, Path::new("/etc/passwd")).is_err());
    }

    #[test]
    fn safe_zip_entries_stay_under_target() {
        let base = PathBuf::from("/tmp/obsy_test_base");
        assert_eq!(
            safe_zip_extract_path(&base, "index.js").unwrap(),
            base.join("index.js")
        );
        assert_eq!(
            safe_zip_extract_path(&base, "sub/dir/file.txt").unwrap(),
            base.join("sub/dir/file.txt")
        );
    }

    #[test]
    fn safe_zip_extract_path_rejects_traversal_and_absolute_paths() {
        let base = PathBuf::from("/tmp/obsy_test_base");
        assert!(safe_zip_extract_path(&base, "../evil.js").is_err());
        assert!(safe_zip_extract_path(&base, "../../root.txt").is_err());
        assert!(safe_zip_extract_path(&base, "/absolute/path").is_err());
    }
}
