use crate::minecraft::versions::StorageLayout;
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

const LAUNCHER_FILES: &[&str] = &[
    "launcher_state.json",
    "wardrobe.json",
    "playtime.json",
    "version_manifest.json",
];
const PROFILE_FILES: &[&str] = &["profiles.json", "profiles.key"];
const GAME_DIRECTORIES: &[&str] = &[
    "config",
    "mods",
    "resourcepacks",
    "saves",
    "screenshots",
    "shaderpacks",
    "crash-reports",
    "logs",
];
const GAME_FILES: &[&str] = &["options.txt", "servers.dat"];
const LAUNCHER_DIRECTORIES: &[&str] = &[
    "instances",
    "versions",
    "addons",
    "assets",
    "libraries",
    "obsy_objects",
];

pub fn current_legacy_root() -> Result<PathBuf, String> {
    if cfg!(debug_assertions) {
        return std::env::current_dir()
            .map(|directory| directory.join(".obsy"))
            .map_err(|error| format!("Could not resolve the legacy launcher directory: {error}"));
    }

    std::env::current_exe()
        .map_err(|error| format!("Could not resolve the launcher executable path: {error}"))?
        .parent()
        .map(|directory| directory.join(".obsy"))
        .ok_or_else(|| "The launcher executable has no parent directory".to_string())
}

pub fn migrate_legacy_data_if_needed(
    layout: &StorageLayout,
    legacy_root: &Path,
) -> Result<(), String> {
    let launcher_root = layout.launcher_root();
    let marker_file = launcher_root.join(".migration_completed");
    if marker_is_complete(&marker_file)? {
        return Ok(());
    }

    ensure_directory(layout.app_root())?;
    ensure_directory(&launcher_root)?;
    let source_metadata = match fs::symlink_metadata(legacy_root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("Legacy launcher directory cannot be a symlink".to_string())
        }
        Ok(metadata) if metadata.is_dir() => Some(metadata),
        Ok(_) => return Err("Legacy launcher path is not a directory".to_string()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "Could not inspect legacy launcher directory: {error}"
            ))
        }
    };

    if source_metadata.is_some() {
        migrate_files(layout, legacy_root)?;
    }

    write_completion_marker(&marker_file)
}

fn marker_is_complete(marker: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(marker) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err("Migration marker is not a regular file".to_string())
        }
        Ok(_) => fs::read(marker)
            .map(|contents| Ok(contents == b"migration_completed\n"))
            .map_err(|error| format!("Could not read migration marker: {error}"))?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Could not inspect migration marker: {error}")),
    }
}

fn migrate_files(layout: &StorageLayout, source: &Path) -> Result<(), String> {
    let launcher_root = layout.launcher_root();
    for filename in LAUNCHER_FILES {
        copy_legacy_path(&source.join(filename), &launcher_root.join(filename))?;
    }
    migrate_profile_pair(source, &launcher_root)?;

    copy_legacy_path(&source.join("jre"), &layout.java_root())?;

    for dirname in GAME_DIRECTORIES {
        copy_legacy_path(&source.join(dirname), &layout.game_root().join(dirname))?;
    }
    for filename in GAME_FILES {
        copy_legacy_path(&source.join(filename), &layout.game_root().join(filename))?;
    }

    migrate_technic_data(layout, source)?;

    for dirname in LAUNCHER_DIRECTORIES {
        copy_legacy_path(&source.join(dirname), &launcher_root.join(dirname))?;
    }

    let reserved = [
        "launcher",
        "java",
        "technic",
        "jre",
        ".migration_completed",
        "profiles.json",
        "profiles.key",
        "launcher_state.json",
        "wardrobe.json",
        "playtime.json",
        "version_manifest.json",
        "options.txt",
        "servers.dat",
        "config",
        "mods",
        "resourcepacks",
        "saves",
        "screenshots",
        "shaderpacks",
        "crash-reports",
        "logs",
        "instances",
        "versions",
        "addons",
        "assets",
        "libraries",
        "obsy_objects",
    ];
    let legacy_archive = launcher_root.join("legacy");
    for entry in read_directory(source)? {
        let name = entry.file_name();
        if reserved.iter().any(|reserved| name == *reserved) {
            continue;
        }
        copy_legacy_path(&entry.path(), &legacy_archive.join(name))?;
    }
    Ok(())
}

fn migrate_profile_pair(source: &Path, launcher_root: &Path) -> Result<(), String> {
    let source_profile = source.join(PROFILE_FILES[0]);
    let source_key = source.join(PROFILE_FILES[1]);
    let target_profile = launcher_root.join(PROFILE_FILES[0]);
    let target_key = launcher_root.join(PROFILE_FILES[1]);
    let target_profile_exists = regular_file_exists(&target_profile)?;
    let target_key_exists = regular_file_exists(&target_key)?;

    if target_profile_exists || target_key_exists {
        if target_profile_exists
            && !target_key_exists
            && files_match(&source_profile, &target_profile)?
        {
            copy_legacy_path(&source_key, &target_key)?;
        } else if target_key_exists
            && !target_profile_exists
            && files_match(&source_key, &target_key)?
        {
            copy_legacy_path(&source_profile, &target_profile)?;
        }
        return Ok(());
    }

    copy_legacy_path(&source_key, &target_key)?;
    copy_legacy_path(&source_profile, &target_profile)
}

fn files_match(source: &Path, destination: &Path) -> Result<bool, String> {
    let source_bytes = match fs::read(source) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("Could not read profile source file: {error}")),
    };
    let destination_bytes = fs::read(destination)
        .map_err(|error| format!("Could not read destination profile file: {error}"))?;
    Ok(source_bytes == destination_bytes)
}

fn regular_file_exists(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(format!(
            "Profile migration target is not a regular file: {}",
            path.display()
        )),
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "Could not inspect profile migration target {}: {error}",
            path.display()
        )),
    }
}

fn migrate_technic_data(layout: &StorageLayout, source: &Path) -> Result<(), String> {
    let old_technic_root = source.join("technic");
    let old_pack_root = old_technic_root.join(crate::technic::TECHNIC_PACK_ID.to_string());
    let new_technic_root = layout.launcher_root().join("technic");
    let new_pack_root = layout.technic_pack_root(crate::technic::TECHNIC_PACK_ID);

    copy_legacy_path(&old_pack_root.join("game"), layout.game_root())?;
    copy_directory_contents_excluding(&old_pack_root, &new_pack_root, &["game"])?;

    let current_pack_id = crate::technic::TECHNIC_PACK_ID.to_string();
    for entry in read_directory(&old_technic_root)? {
        if entry.file_name() == std::ffi::OsStr::new(&current_pack_id) {
            continue;
        }
        copy_legacy_path(&entry.path(), &new_technic_root.join(entry.file_name()))?;
    }
    Ok(())
}

fn copy_directory_contents_excluding(
    source: &Path,
    destination: &Path,
    excluded: &[&str],
) -> Result<(), String> {
    let source_metadata = match fs::symlink_metadata(source) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(format!(
                "Legacy migration source is not a safe directory: {}",
                source.display()
            ))
        }
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "Could not inspect legacy migration directory {}: {error}",
                source.display()
            ))
        }
    };
    if source_metadata.is_none() {
        return Ok(());
    }

    ensure_source_path_safe(source)?;
    ensure_directory(destination)?;
    for entry in read_directory(source)? {
        let name = entry.file_name();
        if excluded.iter().any(|excluded| name == *excluded) {
            continue;
        }
        copy_legacy_path(&entry.path(), &destination.join(name))?;
    }
    Ok(())
}

fn copy_legacy_path(source: &Path, destination: &Path) -> Result<(), String> {
    if source == destination {
        return Ok(());
    }
    let source_metadata = match fs::symlink_metadata(source) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "Legacy migration does not follow symlinks: {}",
                source.display()
            ))
        }
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Could not inspect legacy migration path {}: {error}",
                source.display()
            ))
        }
    };
    ensure_source_path_safe(source)?;

    if source_metadata.is_dir() {
        ensure_directory(destination)?;
        for entry in read_directory(source)? {
            copy_legacy_path(&entry.path(), &destination.join(entry.file_name()))?;
        }
        return Ok(());
    }
    if !source_metadata.is_file() {
        return Err(format!(
            "Legacy migration only accepts regular files and directories: {}",
            source.display()
        ));
    }

    copy_file_if_missing(source, destination)
}

fn copy_file_if_missing(source: &Path, destination: &Path) -> Result<(), String> {
    ensure_directory(
        destination
            .parent()
            .ok_or_else(|| "Migration target has no parent directory".to_string())?,
    )?;
    if regular_file_exists(destination)? {
        return Ok(());
    }

    let file_name = destination
        .file_name()
        .ok_or_else(|| "Migration target has no file name".to_string())?
        .to_string_lossy();
    let temporary = destination.with_file_name(format!(
        ".{file_name}.migration-{}.tmp",
        uuid::Uuid::new_v4()
    ));
    let result = (|| {
        let mut input = File::open(source)
            .map_err(|error| format!("Could not open legacy file {}: {error}", source.display()))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("Could not stage migrated file: {error}"))?;
        io::copy(&mut input, &mut output)
            .map_err(|error| format!("Could not copy legacy file {}: {error}", source.display()))?;
        output
            .sync_all()
            .map_err(|error| format!("Could not sync migrated file: {error}"))?;
        drop(output);
        match fs::hard_link(&temporary, destination) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(format!(
                "Could not publish migrated file {}: {error}",
                destination.display()
            )),
        }
    })();
    let cleanup = match fs::remove_file(&temporary) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not remove migration staging file: {error}")),
    };
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(cleanup_error)) => Err(format!("{error}; {cleanup_error}")),
    }
}

fn ensure_directory(path: &Path) -> Result<(), String> {
    let mut ancestors: Vec<_> = path.ancestors().collect();
    ancestors.reverse();
    for ancestor in ancestors {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "Migration directory cannot be a symlink: {}",
                    ancestor.display()
                ))
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(format!(
                    "Migration path component is not a directory: {}",
                    ancestor.display()
                ))
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Could not inspect migration directory {}: {error}",
                    ancestor.display()
                ))
            }
        }
    }
    fs::create_dir_all(path).map_err(|error| {
        format!(
            "Could not create migration directory {}: {error}",
            path.display()
        )
    })
}

fn ensure_source_path_safe(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "Legacy migration does not follow symlinks: {}",
                    ancestor.display()
                ))
            }
            Ok(metadata) if ancestor != path && !metadata.is_dir() => {
                return Err(format!(
                    "Legacy migration path component is not a directory: {}",
                    ancestor.display()
                ))
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Could not inspect legacy migration path {}: {error}",
                    ancestor.display()
                ))
            }
        }
    }
    Ok(())
}

fn read_directory(path: &Path) -> Result<Vec<fs::DirEntry>, String> {
    ensure_source_path_safe(path)?;
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "Could not read migration directory {}: {error}",
                path.display()
            ))
        }
    };
    entries
        .map(|entry| {
            entry.map_err(|error| {
                format!(
                    "Could not read an entry in migration directory {}: {error}",
                    path.display()
                )
            })
        })
        .collect()
}

fn write_completion_marker(marker: &Path) -> Result<(), String> {
    ensure_directory(
        marker
            .parent()
            .ok_or_else(|| "Migration marker has no parent directory".to_string())?,
    )?;
    let temporary =
        marker.with_file_name(format!(".migration_completed-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("Could not stage migration marker: {error}"))?;
        use std::io::Write;
        file.write_all(b"migration_completed\n")
            .map_err(|error| format!("Could not write migration marker: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Could not sync migration marker: {error}"))?;
        fs::rename(&temporary, marker)
            .map_err(|error| format!("Could not publish migration marker: {error}"))
    })();
    if temporary.exists() {
        fs::remove_file(&temporary)
            .map_err(|error| format!("Could not remove staged migration marker: {error}"))?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minecraft::versions::StorageLayout;
    use std::path::PathBuf;

    struct MigrationFixture {
        root: PathBuf,
        source: PathBuf,
        layout: StorageLayout,
    }

    impl MigrationFixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "obsy-migration-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            let source = root.join("Local/DBC Super Launcher/.obsy");
            let layout = StorageLayout::from_app_root(root.join("Roaming/DBC Super Launcher"));
            fs::create_dir_all(&source).unwrap();
            Self {
                root,
                source,
                layout,
            }
        }
    }

    impl Drop for MigrationFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn write(path: &Path, contents: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn migrates_legacy_game_runtime_java_and_launcher_data_without_deleting_source() {
        let fixture = MigrationFixture::new();
        let old_pack = fixture.source.join("technic/1132904");
        write(&old_pack.join("game/saves/world/level.dat"), b"world");
        write(&old_pack.join("game/resourcepacks/custom.zip"), b"pack");
        write(&old_pack.join("game/config/custom.cfg"), b"config");
        write(
            &old_pack.join("runtime/versions/forge/profile.json"),
            b"profile",
        );
        write(&old_pack.join("manifest.json"), b"old manifest");
        write(&fixture.source.join("jre/bin/java.exe"), b"java");
        write(&fixture.source.join("launcher_state.json"), b"state");
        write(&fixture.source.join("profiles.json"), b"encrypted profiles");
        write(&fixture.source.join("profiles.key"), b"matching key");
        write(&fixture.source.join("assets/cache.bin"), b"cache");
        write(&fixture.source.join("libraries/lib.jar"), b"library");
        write(
            &fixture.source.join("instances/legacy/instance.json"),
            b"instance",
        );
        write(
            &fixture.source.join("versions/legacy/legacy.json"),
            b"version",
        );

        migrate_legacy_data_if_needed(&fixture.layout, &fixture.source).unwrap();

        let app_root = fixture.layout.app_root();
        let launcher_root = fixture.layout.launcher_root();
        let new_pack = fixture.layout.technic_pack_root(1_132_904);
        assert_eq!(
            fs::read(app_root.join("saves/world/level.dat")).unwrap(),
            b"world"
        );
        assert_eq!(
            fs::read(app_root.join("resourcepacks/custom.zip")).unwrap(),
            b"pack"
        );
        assert_eq!(
            fs::read(app_root.join("config/custom.cfg")).unwrap(),
            b"config"
        );
        assert_eq!(
            fs::read(new_pack.join("runtime/versions/forge/profile.json")).unwrap(),
            b"profile"
        );
        assert_eq!(
            fs::read(new_pack.join("manifest.json")).unwrap(),
            b"old manifest"
        );
        assert_eq!(
            fs::read(fixture.layout.java_root().join("bin/java.exe")).unwrap(),
            b"java"
        );
        assert_eq!(
            fs::read(launcher_root.join("launcher_state.json")).unwrap(),
            b"state"
        );
        assert_eq!(
            fs::read(launcher_root.join("profiles.json")).unwrap(),
            b"encrypted profiles"
        );
        assert_eq!(
            fs::read(launcher_root.join("profiles.key")).unwrap(),
            b"matching key"
        );
        assert_eq!(
            fs::read(launcher_root.join("assets/cache.bin")).unwrap(),
            b"cache"
        );
        assert_eq!(
            fs::read(launcher_root.join("libraries/lib.jar")).unwrap(),
            b"library"
        );
        assert_eq!(
            fs::read(launcher_root.join("instances/legacy/instance.json")).unwrap(),
            b"instance"
        );
        assert_eq!(
            fs::read(launcher_root.join("versions/legacy/legacy.json")).unwrap(),
            b"version"
        );
        assert!(fixture.source.join("profiles.json").exists());
        assert!(fixture
            .source
            .join("technic/1132904/game/saves/world/level.dat")
            .exists());
        assert!(launcher_root.join(".migration_completed").exists());
    }

    #[test]
    fn migration_keeps_existing_destination_data_and_does_not_mix_profile_key_pairs() {
        let fixture = MigrationFixture::new();
        write(&fixture.source.join("launcher_state.json"), b"old state");
        write(
            &fixture.source.join("profiles.json"),
            b"old encrypted profiles",
        );
        write(&fixture.source.join("profiles.key"), b"old key");
        write(
            &fixture.source.join("technic/1132904/game/config/user.cfg"),
            b"old config",
        );
        write(
            &fixture.layout.launcher_root().join("launcher_state.json"),
            b"new state",
        );
        write(
            &fixture.layout.launcher_root().join("profiles.json"),
            b"new encrypted profiles",
        );
        write(
            &fixture.layout.launcher_root().join("profiles.key"),
            b"new key",
        );
        write(
            &fixture.layout.game_root().join("config/user.cfg"),
            b"user config",
        );

        migrate_legacy_data_if_needed(&fixture.layout, &fixture.source).unwrap();

        let launcher_root = fixture.layout.launcher_root();
        assert_eq!(
            fs::read(launcher_root.join("launcher_state.json")).unwrap(),
            b"new state"
        );
        assert_eq!(
            fs::read(launcher_root.join("profiles.json")).unwrap(),
            b"new encrypted profiles"
        );
        assert_eq!(
            fs::read(launcher_root.join("profiles.key")).unwrap(),
            b"new key"
        );
        assert_eq!(
            fs::read(fixture.layout.game_root().join("config/user.cfg")).unwrap(),
            b"user config"
        );
    }

    #[test]
    fn failed_migration_writes_no_marker_and_can_safely_retry() {
        let fixture = MigrationFixture::new();
        write(
            &fixture
                .source
                .join("technic/1132904/game/saves/world/level.dat"),
            b"world",
        );
        write(
            &fixture.layout.game_root().join("saves"),
            b"blocks directory",
        );

        assert!(migrate_legacy_data_if_needed(&fixture.layout, &fixture.source).is_err());
        assert!(!fixture
            .layout
            .launcher_root()
            .join(".migration_completed")
            .exists());
        assert!(fixture
            .source
            .join("technic/1132904/game/saves/world/level.dat")
            .exists());

        fs::remove_file(fixture.layout.game_root().join("saves")).unwrap();
        migrate_legacy_data_if_needed(&fixture.layout, &fixture.source).unwrap();
        assert_eq!(
            fs::read(fixture.layout.game_root().join("saves/world/level.dat")).unwrap(),
            b"world"
        );
        assert!(fixture
            .layout
            .launcher_root()
            .join(".migration_completed")
            .exists());
    }

    #[test]
    fn completed_migration_is_idempotent() {
        let fixture = MigrationFixture::new();
        write(&fixture.source.join("launcher_state.json"), b"first");
        migrate_legacy_data_if_needed(&fixture.layout, &fixture.source).unwrap();

        write(&fixture.source.join("launcher_state.json"), b"second");
        migrate_legacy_data_if_needed(&fixture.layout, &fixture.source).unwrap();

        assert_eq!(
            fs::read(fixture.layout.launcher_root().join("launcher_state.json")).unwrap(),
            b"first"
        );
    }

    #[test]
    fn migration_rejects_symlinks_without_marking_complete() {
        let fixture = MigrationFixture::new();
        let outside_root = fixture.root.join("outside");
        let outside = outside_root.join("1132904/game/saves/world/level.dat");
        write(&outside, b"outside world");
        let link = fixture.source.join("technic");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside_root, &link).unwrap();
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;

            let status = std::process::Command::new("cmd")
                .raw_arg(format!(
                    "/C mklink /J \"{}\" \"{}\"",
                    link.display(),
                    outside_root.display()
                ))
                .status()
                .unwrap();
            assert!(status.success(), "could not create test junction");
        }

        assert!(migrate_legacy_data_if_needed(&fixture.layout, &fixture.source).is_err());
        assert!(!fixture
            .layout
            .game_root()
            .join("saves/world/level.dat")
            .exists());
        assert!(!fixture
            .layout
            .launcher_root()
            .join(".migration_completed")
            .exists());
    }
}
