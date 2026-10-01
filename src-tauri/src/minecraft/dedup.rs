use std::fs;
use std::path::{Path, PathBuf};

pub fn get_objects_dir_for_layout(layout: &crate::minecraft::versions::StorageLayout) -> PathBuf {
    layout.launcher_root().join("objects")
}

pub fn get_objects_dir() -> Result<PathBuf, String> {
    let layout = crate::minecraft::versions::StorageLayout::current()?;
    Ok(get_objects_dir_for_layout(&layout))
}

/// Links `src` to `dst` using hard links if possible, falling back to copying.
pub fn link_or_copy(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }

    if dst.exists() {
        let _ = fs::remove_file(dst);
    }

    if fs::hard_link(src, dst).is_ok() {
        return Ok(());
    }

    // Hard links cannot span across different filesystem volumes; fall back to copy.
    fs::copy(src, dst)?;
    Ok(())
}

pub fn store_and_link(src_file: &Path, dst_file: &Path, sha1: &str) -> std::io::Result<PathBuf> {
    if sha1.len() < 4 {
        link_or_copy(src_file, dst_file)?;
        return Ok(dst_file.to_path_buf());
    }

    let prefix = &sha1[0..2];
    let cas_dir = get_objects_dir()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?
        .join(prefix);
    fs::create_dir_all(&cas_dir)?;

    let cas_file = cas_dir.join(sha1);
    if !cas_file.exists() {
        if let Err(link_error) = fs::hard_link(src_file, &cas_file) {
            fs::copy(src_file, &cas_file).map_err(|copy_error| {
                std::io::Error::new(
                    copy_error.kind(),
                    format!("Could not link ({link_error}) or copy file into object storage: {copy_error}"),
                )
            })?;
        }
    }

    link_or_copy(&cas_file, dst_file)?;
    Ok(cas_file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minecraft::versions::StorageLayout;
    use std::io::Write;

    #[test]
    fn test_link_or_copy() {
        let tmp = std::env::temp_dir().join("obsy_test_dedup");
        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::create_dir_all(&tmp);

        let src = tmp.join("source.txt");
        let dst = tmp.join("target.txt");

        let mut f = fs::File::create(&src).unwrap();
        f.write_all(b"hello obsy dedup").unwrap();
        drop(f);

        link_or_copy(&src, &dst).unwrap();
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "hello obsy dedup");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn cas_objects_are_stored_under_launcher_metadata_root() {
        let layout = StorageLayout::from_app_root(std::path::PathBuf::from(
            "C:/Users/test/AppData/Roaming/DBC Super Launcher",
        ));

        assert_eq!(
            get_objects_dir_for_layout(&layout),
            layout.launcher_root().join("objects")
        );
    }
}
