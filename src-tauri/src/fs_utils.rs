use std::path::{Path, PathBuf};

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
    use std::path::PathBuf;

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
