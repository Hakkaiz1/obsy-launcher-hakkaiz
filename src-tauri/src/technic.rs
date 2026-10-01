use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Seek, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use zip::ZipArchive;

pub const TECHNIC_PACK_ID: u64 = 1_132_904;
pub const TECHNIC_PACK_SLUG: &str = "dbc-super-oficial";
pub const TECHNIC_MINECRAFT_VERSION: &str = "1.7.10";
pub const TECHNIC_LAUNCH_ID: &str = "technic-1132904";
const TECHNIC_LATEST_URL: &str =
    "https://api.technicpack.net/modpack/dbc-super-oficial?build=latest&token=2";
const MAX_ARCHIVE_BYTES: u64 = 1_073_741_824;
const MAX_UNCOMPRESSED_BYTES: u64 = 1_073_741_824;
const API_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
const MANIFEST_FILE: &str = "manifest.json";
const MAX_METADATA_BYTES: u64 = 1_048_576;

#[derive(Debug, Clone)]
pub struct TechnicPackMetadata {
    pub id: u64,
    pub name: String,
    pub version: String,
    pub minecraft: String,
    pub url: Url,
}

fn is_windows_reserved_name(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
        }))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledPackManifest {
    pub pack_id: u64,
    pub pack_version: String,
    pub minecraft_version: String,
    pub forge_version: String,
    pub managed_files: Vec<ManagedFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateDecision {
    Install,
    Update,
    AlreadyCurrent,
    UseCachedWithWarning,
}

pub fn validate_launch_id(id: &str) -> Result<(), String> {
    if id == TECHNIC_LAUNCH_ID {
        Ok(())
    } else {
        Err("Only the DBC Super Technic pack can be launched".to_string())
    }
}

pub fn managed_game_root() -> PathBuf {
    managed_pack_root().join("game")
}

pub fn managed_runtime_root() -> PathBuf {
    managed_pack_root().join("runtime")
}

pub fn get_pack_status() -> Result<Option<InstalledPackManifest>, String> {
    read_pack_status(&managed_pack_root())
}

fn read_pack_status(root: &Path) -> Result<Option<InstalledPackManifest>, String> {
    let root_metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Could not inspect managed Technic directory: {error}"
            ))
        }
    };
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err("Managed Technic path is not a safe directory".to_string());
    }

    let manifest_path = root.join(MANIFEST_FILE);
    let bytes = match fs::read(&manifest_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Could not read installed Technic manifest: {error}"
            ))
        }
    };
    let manifest: InstalledPackManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Installed Technic manifest is invalid: {error}"))?;
    validate_installed_manifest(root, &manifest)?;
    Ok(Some(manifest))
}

#[derive(Debug)]
enum TechnicApiError {
    Unavailable(String),
    Invalid(String),
}

pub fn parse_latest_metadata(body: &str) -> Result<TechnicPackMetadata, String> {
    let value: Value =
        serde_json::from_str(body).map_err(|error| format!("Invalid Technic metadata: {error}"))?;
    let id = value
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| "Technic metadata is missing a valid pack ID".to_string())?;
    if id != TECHNIC_PACK_ID {
        return Err(format!(
            "Unexpected Technic pack ID: expected {TECHNIC_PACK_ID}, received {id}"
        ));
    }

    let name = required_string(&value, "name")?;
    if name != TECHNIC_PACK_SLUG {
        return Err(format!("Unexpected Technic pack name: {name}"));
    }
    let version = required_string(&value, "version")?;
    parse_version_parts(&version)?;
    let minecraft = required_string(&value, "minecraft")?;
    if minecraft != TECHNIC_MINECRAFT_VERSION {
        return Err(format!(
            "Unsupported Minecraft version for this launcher: {minecraft}"
        ));
    }

    let raw_url = required_string(&value, "url")?;
    let url =
        Url::parse(&raw_url).map_err(|error| format!("Invalid Technic archive URL: {error}"))?;
    validate_archive_url(&url)?;

    Ok(TechnicPackMetadata {
        id,
        name,
        version,
        minecraft,
        url,
    })
}

fn required_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("Technic metadata is missing required field '{field}'"))
}

async fn fetch_latest_metadata() -> Result<TechnicPackMetadata, TechnicApiError> {
    let response = crate::open_launcher::utils::get_http_client()
        .get(TECHNIC_LATEST_URL)
        .timeout(API_TIMEOUT)
        .send()
        .await
        .map_err(|error| {
            TechnicApiError::Unavailable(format!("Could not reach the Technic API: {error}"))
        })?;
    if !response.status().is_success() {
        let message = format!("Technic API returned HTTP {}", response.status());
        return if response.status().is_server_error() || response.status().as_u16() == 429 {
            Err(TechnicApiError::Unavailable(message))
        } else {
            Err(TechnicApiError::Invalid(message))
        };
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_METADATA_BYTES)
    {
        return Err(TechnicApiError::Invalid(
            "Technic API metadata exceeds the size limit".to_string(),
        ));
    }
    let body = response.bytes().await.map_err(|error| {
        TechnicApiError::Unavailable(format!("Could not read Technic metadata: {error}"))
    })?;
    if body.len() as u64 > MAX_METADATA_BYTES {
        return Err(TechnicApiError::Invalid(
            "Technic API metadata exceeds the size limit".to_string(),
        ));
    }
    let body = std::str::from_utf8(&body).map_err(|error| {
        TechnicApiError::Invalid(format!("Invalid Technic metadata encoding: {error}"))
    })?;

    parse_latest_metadata(body).map_err(TechnicApiError::Invalid)
}

fn parse_version_parts(version: &str) -> Result<Vec<u64>, String> {
    version
        .split('.')
        .map(|part| {
            if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(format!("Unsupported Technic build version: {version}"));
            }
            part.parse::<u64>()
                .map_err(|_| format!("Unsupported Technic build version: {version}"))
        })
        .collect()
}

fn compare_pack_versions(installed: &str, latest: &str) -> Result<Ordering, String> {
    let installed_parts = parse_version_parts(installed)?;
    let latest_parts = parse_version_parts(latest)?;
    let length = installed_parts.len().max(latest_parts.len());
    for index in 0..length {
        let installed_part = installed_parts.get(index).copied().unwrap_or(0);
        let latest_part = latest_parts.get(index).copied().unwrap_or(0);
        match latest_part.cmp(&installed_part) {
            Ordering::Equal => {}
            difference => return Ok(difference),
        }
    }
    Ok(Ordering::Equal)
}

pub fn decide_update(
    installed: Option<&InstalledPackManifest>,
    latest: Result<&TechnicPackMetadata, &str>,
) -> Result<UpdateDecision, String> {
    if let Some(installed) = installed {
        if installed.pack_id != TECHNIC_PACK_ID
            || installed.minecraft_version != TECHNIC_MINECRAFT_VERSION
            || installed.pack_version.trim().is_empty()
        {
            return Err(
                "Installed manifest does not match the configured DBC Super pack".to_string(),
            );
        }
        parse_version_parts(&installed.pack_version)?;
    }
    let latest = match latest {
        Ok(latest) => latest,
        Err(error) => {
            return if installed.is_some() {
                Ok(UpdateDecision::UseCachedWithWarning)
            } else {
                Err(format!(
                    "No valid DBC Super installation is available: {error}"
                ))
            }
        }
    };

    if latest.id != TECHNIC_PACK_ID
        || latest.name != TECHNIC_PACK_SLUG
        || latest.minecraft != TECHNIC_MINECRAFT_VERSION
    {
        return Err("Technic metadata does not match the configured DBC Super pack".to_string());
    }
    parse_version_parts(&latest.version)?;

    let Some(installed) = installed else {
        return Ok(UpdateDecision::Install);
    };
    match compare_pack_versions(&installed.pack_version, &latest.version)? {
        Ordering::Greater => Ok(UpdateDecision::Update),
        Ordering::Equal => Ok(UpdateDecision::AlreadyCurrent),
        Ordering::Less => Err(format!(
            "Refusing to downgrade DBC Super from {} to {}",
            installed.pack_version, latest.version
        )),
    }
}

#[cfg(test)]
fn install_archive_transaction(
    root: &Path,
    archive: &[u8],
    pack_version: &str,
    previous: Option<&InstalledPackManifest>,
) -> Result<InstalledPackManifest, String> {
    install_archive_reader_transaction(root, Cursor::new(archive), pack_version, previous)
}

fn apply_staged_update(
    root: &Path,
    staged_root: &Path,
    manifest: &InstalledPackManifest,
) -> Result<(), String> {
    let parent = root
        .parent()
        .ok_or_else(|| "Managed Technic directory has no parent".to_string())?;
    if staged_root.parent() != Some(parent) {
        return Err("Technic staging directory must be on the managed pack volume".to_string());
    }

    let backup = root.with_file_name(format!(".dbc-super-backup-{}", uuid::Uuid::new_v4()));
    let had_previous = root.exists();
    if had_previous {
        fs::rename(root, &backup).map_err(|error| {
            format!("Could not back up the existing DBC Super installation: {error}")
        })?;
    }

    if let Err(error) = fs::rename(staged_root, root) {
        if had_previous {
            fs::rename(&backup, root).map_err(|restore_error| {
                format!(
                    "Could not activate the DBC Super update ({error}) or restore the previous installation ({restore_error})"
                )
            })?;
        }
        return Err(format!("Could not activate the DBC Super update: {error}"));
    }

    let manifest_path = root.join(MANIFEST_FILE);
    let write_result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&manifest_path)
            .map_err(|error| format!("Could not create the installed pack manifest: {error}"))?;
        let bytes = serde_json::to_vec_pretty(manifest)
            .map_err(|error| format!("Could not serialize the installed pack manifest: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("Could not persist the installed pack manifest: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Could not sync the installed pack manifest: {error}"))
    })();

    if let Err(error) = write_result {
        let cleanup_result = fs::remove_dir_all(root);
        let restore_result = if had_previous {
            fs::rename(&backup, root)
        } else {
            Ok(())
        };
        if let Err(rollback_error) = cleanup_result {
            return Err(format!(
                "{error}; failed to remove the incomplete installation during rollback: {rollback_error}"
            ));
        }
        if let Err(rollback_error) = restore_result {
            return Err(format!(
                "{error}; failed to restore the previous installation: {rollback_error}"
            ));
        }
        return Err(error);
    }

    if had_previous {
        fs::remove_dir_all(&backup).map_err(|error| {
            format!("Installed DBC Super, but could not remove backup: {error}")
        })?;
    }
    Ok(())
}

fn install_archive_reader_transaction<R: Read + Seek>(
    root: &Path,
    reader: R,
    pack_version: &str,
    previous: Option<&InstalledPackManifest>,
) -> Result<InstalledPackManifest, String> {
    if pack_version.trim().is_empty() {
        return Err("Technic build version is empty".to_string());
    }
    let parent = root
        .parent()
        .ok_or_else(|| "Managed Technic directory has no parent".to_string())?;
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("Managed Technic directory cannot be a symlink".to_string())
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Could not inspect managed Technic directory: {error}"
            ))
        }
    }
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create Technic installation directory: {error}"))?;
    let staged_root = root.with_file_name(format!(".dbc-super-staging-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&staged_root)
        .map_err(|error| format!("Could not create Technic staging directory: {error}"))?;

    let prepared = (|| {
        let mut managed_paths = extract_pack_archive(reader, &staged_root)?;
        reconcile_previous_files(root, &staged_root, previous, &managed_paths)?;
        managed_paths.sort();
        managed_paths.dedup();
        let managed_files = managed_paths
            .into_iter()
            .map(|path| {
                let full_path = managed_path(&staged_root, &path)?;
                Ok(ManagedFile {
                    path,
                    sha256: hash_file(&full_path)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let forge_version = installed_forge_version(&staged_root, &managed_files)?;

        Ok(InstalledPackManifest {
            pack_id: TECHNIC_PACK_ID,
            pack_version: pack_version.to_string(),
            minecraft_version: TECHNIC_MINECRAFT_VERSION.to_string(),
            forge_version,
            managed_files,
        })
    })();

    let manifest = match prepared {
        Ok(manifest) => manifest,
        Err(error) => {
            fs::remove_dir_all(&staged_root).map_err(|cleanup_error| {
                format!("{error}; could not remove staging directory: {cleanup_error}")
            })?;
            return Err(error);
        }
    };

    if let Err(error) = apply_staged_update(root, &staged_root, &manifest) {
        if staged_root.exists() {
            fs::remove_dir_all(&staged_root).map_err(|cleanup_error| {
                format!("{error}; could not remove staging directory: {cleanup_error}")
            })?;
        }
        return Err(error);
    }

    Ok(manifest)
}

fn extract_pack_archive<R: Read + Seek>(
    reader: R,
    staged_root: &Path,
) -> Result<Vec<String>, String> {
    let mut archive = ZipArchive::new(reader)
        .map_err(|error| format!("Technic download is not a valid ZIP archive: {error}"))?;
    validate_archive_entries(&mut archive)?;

    let game_root = staged_root.join("game");
    let runtime_root = staged_root.join("runtime");
    fs::create_dir_all(&game_root)
        .map_err(|error| format!("Could not create staged game directory: {error}"))?;
    fs::create_dir_all(&runtime_root)
        .map_err(|error| format!("Could not create staged runtime directory: {error}"))?;

    let mut forge_profile_bytes = None;
    let mut forge_jar_bytes = None;
    let mut managed_paths = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Could not read Technic archive entry: {error}"))?;
        let entry_name = entry.name().to_string();
        let is_directory = entry.is_dir();
        let normalized = normalize_archive_path(&entry_name, is_directory)?;

        if entry_name.starts_with("bin/") {
            match entry_name.as_str() {
                "bin/version.json" => {
                    let mut contents = Vec::new();
                    entry.read_to_end(&mut contents).map_err(|error| {
                        format!("Could not read Technic Forge profile: {error}")
                    })?;
                    forge_profile_bytes = Some(contents);
                }
                "bin/modpack.jar" => {
                    let mut contents = Vec::new();
                    entry.read_to_end(&mut contents).map_err(|error| {
                        format!("Could not read Technic Forge universal JAR: {error}")
                    })?;
                    forge_jar_bytes = Some(contents);
                }
                _ => {}
            }
            continue;
        }

        let output = crate::fs_utils::safe_zip_extract_path(&game_root, &normalized)
            .map_err(|error| error.to_string())?;
        if is_directory {
            fs::create_dir_all(&output)
                .map_err(|error| format!("Could not create game directory: {error}"))?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create game file parent: {error}"))?;
        }
        let mut file = File::create(&output)
            .map_err(|error| format!("Could not create staged game file: {error}"))?;
        std::io::copy(&mut entry, &mut file)
            .map_err(|error| format!("Could not extract Technic game file: {error}"))?;

        if !is_personal_data_path(&normalized) {
            managed_paths.push(format!("game/{normalized}"));
        }
    }

    let forge_profile_bytes = forge_profile_bytes
        .ok_or_else(|| "Technic archive is missing bin/version.json".to_string())?;
    let forge_jar_bytes =
        forge_jar_bytes.ok_or_else(|| "Technic archive is missing bin/modpack.jar".to_string())?;
    let forge_profile: Value = serde_json::from_slice(&forge_profile_bytes)
        .map_err(|error| format!("Invalid Technic Forge profile: {error}"))?;
    let forge_version = verify_forge_profile(&forge_profile)?;
    verify_forge_jar(&forge_jar_bytes, &forge_profile)?;

    let combined = format!("forge-{TECHNIC_MINECRAFT_VERSION}-{forge_version}");
    let forge_dir = runtime_root.join("versions").join(&combined);
    fs::create_dir_all(&forge_dir)
        .map_err(|error| format!("Could not create managed Forge profile directory: {error}"))?;
    let profile_path = forge_dir.join(format!("{combined}.json"));
    let jar_path = forge_dir.join(format!("{combined}.jar"));
    fs::write(&profile_path, forge_profile_bytes)
        .map_err(|error| format!("Could not stage the verified Forge profile: {error}"))?;
    fs::write(&jar_path, forge_jar_bytes)
        .map_err(|error| format!("Could not stage the verified Forge JAR: {error}"))?;
    managed_paths.push(relative_to_root(staged_root, &profile_path)?);
    managed_paths.push(relative_to_root(staged_root, &jar_path)?);

    Ok(managed_paths)
}

fn validate_archive_entries<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<(), String> {
    let mut seen = HashSet::new();
    let mut files = HashSet::new();
    let mut total_size = 0_u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("Could not inspect Technic archive: {error}"))?;
        let normalized = normalize_archive_path(entry.name(), entry.is_dir())?;
        let key = normalized.to_lowercase();
        if !seen.insert(key.clone()) {
            return Err(format!(
                "Technic archive contains duplicate path: {}",
                entry.name()
            ));
        }

        let mode = entry.unix_mode().unwrap_or(0);
        let file_type = mode & 0o170000;
        if file_type == 0o120000 {
            return Err(format!(
                "Technic archive contains a symlink: {}",
                entry.name()
            ));
        }
        if file_type != 0 && file_type != 0o100000 && file_type != 0o040000 {
            return Err(format!(
                "Technic archive contains an unsupported filesystem entry: {}",
                entry.name()
            ));
        }

        if entry.is_dir() {
            if files.contains(&key) {
                return Err(format!(
                    "Technic archive has conflicting file and directory paths: {}",
                    entry.name()
                ));
            }
        } else {
            let mut parent = key.as_str();
            while let Some((ancestor, _)) = parent.rsplit_once('/') {
                if files.contains(ancestor) {
                    return Err(format!(
                        "Technic archive has a file/directory path conflict: {}",
                        entry.name()
                    ));
                }
                parent = ancestor;
            }
            if seen.iter().any(|path| path.starts_with(&format!("{key}/"))) {
                return Err(format!(
                    "Technic archive has a file/directory path conflict: {}",
                    entry.name()
                ));
            }
            files.insert(key);
        }

        total_size = total_size
            .checked_add(entry.size())
            .ok_or_else(|| "Technic archive uncompressed size overflow".to_string())?;
        if total_size > MAX_UNCOMPRESSED_BYTES {
            return Err("Technic archive exceeds the uncompressed size limit".to_string());
        }
    }
    Ok(())
}

fn normalize_archive_path(name: &str, is_directory: bool) -> Result<String, String> {
    if name.is_empty()
        || name.contains('\\')
        || name.contains(':')
        || name.contains('\0')
        || name.starts_with('/')
    {
        return Err(format!("Unsafe Technic archive path: {name}"));
    }
    let path = if is_directory {
        name.strip_suffix('/').unwrap_or(name)
    } else {
        name
    };
    if path.is_empty()
        || path.split('/').any(|component| {
            component.is_empty()
                || component == "."
                || component == ".."
                || component.ends_with('.')
                || component.ends_with(' ')
                || component
                    .chars()
                    .any(|character| character.is_control() || "<>\"|?*".contains(character))
                || is_windows_reserved_name(component)
        })
    {
        return Err(format!("Unsafe Technic archive path: {name}"));
    }
    crate::fs_utils::safe_zip_extract_path(Path::new("."), path)?;
    Ok(path.to_string())
}

fn verify_forge_profile(profile: &Value) -> Result<String, String> {
    let id = required_string(profile, "id")?;
    if profile.get("inheritsFrom").and_then(Value::as_str) != Some(TECHNIC_MINECRAFT_VERSION)
        || profile.get("jar").and_then(Value::as_str) != Some(TECHNIC_MINECRAFT_VERSION)
        || profile.get("mainClass").and_then(Value::as_str)
            != Some("net.minecraft.launchwrapper.Launch")
        || !profile
            .get("minecraftArguments")
            .and_then(Value::as_str)
            .is_some_and(|arguments| arguments.contains("cpw.mods.fml.common.launcher.FMLTweaker"))
    {
        return Err("Technic archive contains an unsupported Forge launch profile".to_string());
    }

    let expected_id_prefix = format!("{TECHNIC_MINECRAFT_VERSION}-Forge");
    let Some(forge_id_version) = id
        .strip_prefix(&expected_id_prefix)
        .and_then(|suffix| suffix.strip_suffix(&format!("-{TECHNIC_MINECRAFT_VERSION}")))
    else {
        return Err(format!("Unexpected Technic Forge profile ID: {id}"));
    };

    let libraries = profile
        .get("libraries")
        .and_then(Value::as_array)
        .ok_or_else(|| "Technic Forge profile has no libraries list".to_string())?;
    if libraries
        .iter()
        .any(|library| library.get("name").and_then(Value::as_str).is_none())
    {
        return Err("Technic Forge profile contains an invalid library entry".to_string());
    }
    let forge_library = libraries
        .iter()
        .filter_map(|library| library.get("name").and_then(Value::as_str))
        .find(|name| name.starts_with("net.minecraftforge:forge:"))
        .ok_or_else(|| "Technic Forge profile has no Forge library".to_string())?;
    let artifact = forge_library
        .strip_prefix("net.minecraftforge:forge:")
        .ok_or_else(|| "Invalid Forge library coordinate".to_string())?;
    let forge_version = artifact
        .strip_prefix(&format!("{TECHNIC_MINECRAFT_VERSION}-"))
        .and_then(|version| version.strip_suffix(&format!("-{TECHNIC_MINECRAFT_VERSION}")))
        .ok_or_else(|| "Forge library version does not match Minecraft 1.7.10".to_string())?;
    if forge_version != forge_id_version
        || forge_version.is_empty()
        || !forge_version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err("Technic Forge profile ID and library version do not match".to_string());
    }
    Ok(forge_version.to_string())
}

fn verify_forge_jar(bytes: &[u8], expected_profile: &Value) -> Result<(), String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("Technic modpack.jar is not a valid JAR: {error}"))?;
    validate_archive_entries(&mut archive)?;
    let mut embedded_profile = None;
    let mut has_forge_classes = false;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Could not inspect Technic modpack.jar: {error}"))?;
        if entry.name() == "version.json" {
            let mut contents = Vec::new();
            entry
                .read_to_end(&mut contents)
                .map_err(|error| format!("Could not read embedded Forge profile: {error}"))?;
            embedded_profile = Some(
                serde_json::from_slice::<Value>(&contents)
                    .map_err(|error| format!("Invalid embedded Forge profile: {error}"))?,
            );
        }
        if entry.name().starts_with("cpw/mods/fml/") {
            has_forge_classes = true;
        }
        if !entry.is_dir() && entry.name() != "version.json" {
            std::io::copy(&mut entry, &mut std::io::sink())
                .map_err(|error| format!("Could not validate Technic Forge JAR entry: {error}"))?;
        }
    }
    if embedded_profile.as_ref() != Some(expected_profile) || !has_forge_classes {
        return Err("Technic modpack.jar does not match its Forge profile".to_string());
    }
    Ok(())
}

fn is_personal_data_path(path: &str) -> bool {
    matches!(
        path.split('/').next(),
        Some("saves" | "screenshots" | "shaderpacks" | "crash-reports" | "logs")
    ) || matches!(path, "options.txt" | "servers.dat")
}

fn reconcile_previous_files(
    previous_root: &Path,
    staged_root: &Path,
    previous_manifest: Option<&InstalledPackManifest>,
    new_managed_paths: &[String],
) -> Result<(), String> {
    if !previous_root.exists() {
        return Ok(());
    }
    let previous_owned: HashMap<String, String> = previous_manifest
        .into_iter()
        .flat_map(|manifest| manifest.managed_files.iter())
        .map(|file| (file.path.to_lowercase(), file.sha256.clone()))
        .collect();
    let new_managed: HashSet<String> = new_managed_paths
        .iter()
        .map(|path| path.to_lowercase())
        .collect();
    copy_preserved_files(
        previous_root,
        previous_root,
        staged_root,
        Path::new(""),
        &previous_owned,
        &new_managed,
    )
}

fn copy_preserved_files(
    root: &Path,
    current: &Path,
    staged_root: &Path,
    relative: &Path,
    previous_owned: &HashMap<String, String>,
    new_managed: &HashSet<String>,
) -> Result<(), String> {
    let entries = fs::read_dir(current)
        .map_err(|error| format!("Could not inspect previous Technic files: {error}"))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("Could not inspect previous Technic file: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Could not inspect previous Technic file type: {error}"))?;
        if file_type.is_symlink() {
            return Err(format!(
                "Cannot safely update because a managed game path is a symlink: {}",
                entry.path().display()
            ));
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Previous Technic installation contains a non-UTF-8 path".to_string())?;
        if relative.as_os_str().is_empty() && name == MANIFEST_FILE {
            continue;
        }
        let child_relative = relative.join(name);
        if file_type.is_dir() {
            copy_preserved_files(
                root,
                &entry.path(),
                staged_root,
                &child_relative,
                previous_owned,
                new_managed,
            )?;
            continue;
        }
        if !file_type.is_file() {
            return Err(format!(
                "Cannot safely update unsupported filesystem entry: {}",
                entry.path().display()
            ));
        }

        let relative_string = path_to_slashes(&child_relative)?;
        let normalized_path = relative_string.to_lowercase();
        let source = root.join(&child_relative);
        let target = managed_path(staged_root, &relative_string)?;
        let is_personal = relative_string.starts_with("game/")
            && is_personal_data_path(relative_string.trim_start_matches("game/"));
        if !is_personal {
            if new_managed.contains(&normalized_path) {
                continue;
            }
            if let Some(expected_hash) = previous_owned.get(&normalized_path) {
                if hash_file(&source)? == *expected_hash {
                    continue;
                }
            }
        }

        if target.exists() && !is_personal {
            return Err(format!(
                "Cannot preserve existing player file because the new Technic archive uses the same path: {}",
                target.display()
            ));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not preserve player data: {error}"))?;
        }
        fs::copy(&source, &target).map_err(|error| {
            format!(
                "Could not preserve existing player file {}: {error}",
                source.display()
            )
        })?;
    }
    Ok(())
}

fn path_to_slashes(path: &Path) -> Result<String, String> {
    path.components()
        .map(|component| match component {
            std::path::Component::Normal(part) => part
                .to_str()
                .map(str::to_string)
                .ok_or_else(|| "Managed Technic path is not valid UTF-8".to_string()),
            _ => Err("Managed Technic path contains an unsafe component".to_string()),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join("/"))
}

fn managed_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.contains('\\')
        || relative.contains(':')
        || relative.contains('\0')
        || relative.starts_with('/')
        || relative
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(format!("Unsafe managed Technic path: {relative}"));
    }
    crate::fs_utils::safe_zip_extract_path(root, relative)
}

fn relative_to_root(root: &Path, path: &Path) -> Result<String, String> {
    path.strip_prefix(root)
        .map_err(|_| "Managed Technic file is outside the installation root".to_string())
        .and_then(path_to_slashes)
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("Could not hash managed file: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not hash managed file: {error}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn installed_forge_version(root: &Path, managed_files: &[ManagedFile]) -> Result<String, String> {
    let profile = managed_files
        .iter()
        .find(|file| {
            file.path.starts_with("runtime/versions/forge-") && file.path.ends_with(".json")
        })
        .ok_or_else(|| "Installed Technic pack is missing its Forge profile".to_string())?;
    let path = managed_path(root, &profile.path)?;
    let value: Value = serde_json::from_slice(
        &fs::read(path)
            .map_err(|error| format!("Could not verify installed Forge profile: {error}"))?,
    )
    .map_err(|error| format!("Invalid installed Forge profile: {error}"))?;
    verify_forge_profile(&value)
}

fn validate_installed_manifest(
    root: &Path,
    manifest: &InstalledPackManifest,
) -> Result<(), String> {
    let root_metadata = fs::symlink_metadata(root)
        .map_err(|error| format!("Could not inspect installed Technic directory: {error}"))?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err("Installed Technic path is not a safe directory".to_string());
    }
    if manifest.pack_id != TECHNIC_PACK_ID
        || manifest.minecraft_version != TECHNIC_MINECRAFT_VERSION
        || manifest.pack_version.trim().is_empty()
    {
        return Err("Installed Technic manifest does not match DBC Super".to_string());
    }
    parse_version_parts(&manifest.pack_version)?;
    let mut seen = HashSet::new();
    for managed_file in &manifest.managed_files {
        if !managed_file.path.starts_with("game/")
            && !managed_file.path.starts_with("runtime/versions/forge-")
        {
            return Err(format!(
                "Installed Technic manifest has an unexpected managed path: {}",
                managed_file.path
            ));
        }
        if !seen.insert(managed_file.path.to_lowercase()) {
            return Err("Installed Technic manifest contains duplicate paths".to_string());
        }
        if managed_file.sha256.len() != 64
            || !managed_file
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("Installed Technic manifest contains an invalid SHA-256 hash".to_string());
        }
        let path = managed_path(root, &managed_file.path)?;
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "Installed Technic file is missing: {}: {error}",
                path.display()
            )
        })?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(format!(
                "Installed Technic path is not a regular file: {}",
                path.display()
            ));
        }
        if hash_file(&path)? != managed_file.sha256 {
            return Err(format!(
                "Installed Technic file failed hash validation: {}",
                path.display()
            ));
        }
    }

    let forge_version = installed_forge_version(root, &manifest.managed_files)?;
    if forge_version != manifest.forge_version {
        return Err("Installed Technic Forge version does not match its manifest".to_string());
    }
    let combined = format!("forge-{TECHNIC_MINECRAFT_VERSION}-{forge_version}");
    for suffix in [".json", ".jar"] {
        let expected = format!("runtime/versions/{combined}/{combined}{suffix}");
        if !manifest
            .managed_files
            .iter()
            .any(|file| file.path == expected)
        {
            return Err(format!(
                "Installed Technic manifest is missing required Forge file: {expected}"
            ));
        }
    }
    Ok(())
}

fn load_valid_installed_manifest(root: &Path) -> Option<InstalledPackManifest> {
    let manifest_path = root.join(MANIFEST_FILE);
    let bytes = match fs::read(&manifest_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            eprintln!(
                "[TECHNIC] Could not read installed pack manifest {}: {error}",
                manifest_path.display()
            );
            return None;
        }
    };
    let manifest: InstalledPackManifest = match serde_json::from_slice(&bytes) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("[TECHNIC] Ignoring malformed installed pack manifest: {error}");
            return None;
        }
    };
    if let Err(error) = validate_installed_manifest(root, &manifest) {
        eprintln!("[TECHNIC] Ignoring invalid installed pack: {error}");
        return None;
    }
    Some(manifest)
}

fn managed_pack_root() -> PathBuf {
    crate::minecraft::versions::get_minecraft_dir()
        .join("technic")
        .join(TECHNIC_PACK_ID.to_string())
}

#[derive(Serialize, Clone)]
struct TechnicProgressPayload {
    status: String,
    progress: f32,
    detail: Option<String>,
}

fn emit_progress(
    app: &AppHandle,
    status: &str,
    progress: f32,
    detail: Option<String>,
) -> Result<(), String> {
    let payload = TechnicProgressPayload {
        status: status.to_string(),
        progress,
        detail,
    };
    app.emit("technic-progress", payload.clone())
        .map_err(|error| format!("Could not report Technic progress: {error}"))?;
    app.emit("launch-progress", payload)
        .map_err(|error| format!("Could not report launch progress: {error}"))
}

pub async fn ensure_pack_current(app: &AppHandle) -> Result<InstalledPackManifest, String> {
    let root = managed_pack_root();
    let current = load_valid_installed_manifest(&root);
    emit_progress(app, "checking_technic", 0.0, None)?;

    let metadata = match fetch_latest_metadata().await {
        Ok(metadata) => metadata,
        Err(TechnicApiError::Unavailable(error)) => {
            if decide_update(current.as_ref(), Err(&error))? != UpdateDecision::UseCachedWithWarning
            {
                return Err(format!(
                    "No valid DBC Super installation is available: {error}"
                ));
            }
            let manifest = current.ok_or_else(|| {
                "Offline fallback requires a valid cached installation".to_string()
            })?;
            let warning = format!(
            "Não foi possível consultar o Technic ({error}); iniciando a última instalação válida."
            );
            app.emit("technic-offline-warning", warning)
                .map_err(|error| format!("Could not report offline fallback: {error}"))?;
            emit_progress(
                app,
                "cached_offline",
                0.6,
                Some(format!("Versão {}", manifest.pack_version)),
            )?;
            return Ok(manifest);
        }
        Err(TechnicApiError::Invalid(error)) => return Err(error),
    };

    let decision = decide_update(current.as_ref(), Ok(&metadata))?;
    if decision == UpdateDecision::AlreadyCurrent {
        emit_progress(
            app,
            "ready",
            0.6,
            Some(format!("Versão {}", metadata.version)),
        )?;
        return current.ok_or_else(|| "Valid installed Technic pack disappeared".to_string());
    }

    emit_progress(
        app,
        "downloading_technic",
        0.05,
        Some(format!("Versão {}", metadata.version)),
    )?;
    let archive_path = download_archive(app, &metadata.url, &root).await?;
    emit_progress(app, "applying_technic", 0.55, None)?;
    let root_for_install = root.clone();
    let archive_for_install = archive_path.clone();
    let version = metadata.version.clone();
    let previous = current.clone();
    let install_result = tokio::task::spawn_blocking(move || {
        let archive = File::open(&archive_for_install)
            .map_err(|error| format!("Could not open downloaded Technic archive: {error}"))?;
        install_archive_reader_transaction(&root_for_install, archive, &version, previous.as_ref())
    })
    .await
    .map_err(|error| format!("Technic installation task failed: {error}"))
    .and_then(|result| result);
    let cleanup_result = tokio::fs::remove_file(&archive_path).await;
    let installed = match (install_result, cleanup_result) {
        (Ok(installed), Ok(())) => installed,
        (Err(error), Ok(())) => return Err(error),
        (Err(error), Err(cleanup_error)) => {
            return Err(format!(
                "{error}; could not remove temporary Technic archive: {cleanup_error}"
            ))
        }
        (Ok(_), Err(error)) => {
            return Err(format!(
                "Installed DBC Super, but could not remove temporary Technic archive: {error}"
            ))
        }
    };
    emit_progress(
        app,
        "ready",
        0.6,
        Some(format!("Versão {}", installed.pack_version)),
    )?;
    Ok(installed)
}

async fn download_archive(app: &AppHandle, url: &Url, root: &Path) -> Result<PathBuf, String> {
    let parent = root
        .parent()
        .ok_or_else(|| "Managed Technic directory has no parent".to_string())?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| format!("Could not create Technic download directory: {error}"))?;
    let client = crate::open_launcher::utils::get_http_client();
    let mut response = client
        .get(url.clone())
        .timeout(DOWNLOAD_TIMEOUT)
        .send()
        .await
        .map_err(|error| {
            format!(
                "Could not download DBC Super from Technic: {}",
                error.without_url()
            )
        })?
        .error_for_status()
        .map_err(|error| {
            format!(
                "Technic archive download returned an error: {}",
                error.without_url()
            )
        })?;
    validate_archive_url(response.url())?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_ARCHIVE_BYTES)
    {
        return Err("Technic archive exceeds the 1 GiB download limit".to_string());
    }

    let archive_path = parent.join(format!(".dbc-super-download-{}.zip", uuid::Uuid::new_v4()));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&archive_path)
        .await
        .map_err(|error| format!("Could not create temporary Technic archive: {error}"))?;
    let content_length = response.content_length().unwrap_or(0);
    let download_result = async {
        let mut downloaded = 0_u64;
        let mut last_progress = std::time::Instant::now();
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            format!(
                "Could not read Technic archive response: {}",
                error.without_url()
            )
        })? {
            downloaded = downloaded
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| "Technic archive size overflow".to_string())?;
            if downloaded > MAX_ARCHIVE_BYTES {
                return Err("Technic archive exceeds the 1 GiB download limit".to_string());
            }
            file.write_all(&chunk)
                .await
                .map_err(|error| format!("Could not write Technic archive: {error}"))?;
            if last_progress.elapsed() >= Duration::from_millis(150)
                || (content_length > 0 && downloaded >= content_length)
            {
                let progress = if content_length > 0 {
                    0.05 + (downloaded as f32 / content_length as f32).min(1.0) * 0.45
                } else {
                    0.05
                };
                let detail = if content_length > 0 {
                    format!(
                        "{:.1}/{:.1} MB",
                        downloaded as f32 / 1_048_576.0,
                        content_length as f32 / 1_048_576.0
                    )
                } else {
                    format!("{:.1} MB", downloaded as f32 / 1_048_576.0)
                };
                emit_progress(app, "downloading_technic", progress, Some(detail))?;
                last_progress = std::time::Instant::now();
            }
        }
        file.flush()
            .await
            .map_err(|error| format!("Could not flush Technic archive: {error}"))?;
        file.sync_all()
            .await
            .map_err(|error| format!("Could not sync Technic archive: {error}"))
    }
    .await;
    drop(file);
    if let Err(error) = download_result {
        match tokio::fs::remove_file(&archive_path).await {
            Ok(()) => return Err(error),
            Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {
                return Err(error)
            }
            Err(cleanup_error) => {
                return Err(format!(
                    "{error}; could not remove partial Technic archive: {cleanup_error}"
                ))
            }
        }
    }
    Ok(archive_path)
}

fn validate_archive_url(url: &Url) -> Result<(), String> {
    let host = url
        .host_str()
        .ok_or_else(|| "Technic archive URL is missing a host".to_string())?;
    let supported_host = host == "dropbox.com"
        || host.ends_with(".dropbox.com")
        || host == "dropboxusercontent.com"
        || host.ends_with(".dropboxusercontent.com")
        || host == "technicpack.net"
        || host.ends_with(".technicpack.net");
    if url.scheme() != "https"
        || !supported_host
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some_and(|port| port != 443)
    {
        return Err("Unsupported Technic archive URL".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        decide_update, install_archive_transaction, parse_latest_metadata, InstalledPackManifest,
        ManagedFile, TechnicPackMetadata, UpdateDecision, TECHNIC_MINECRAFT_VERSION,
        TECHNIC_PACK_ID,
    };
    use reqwest::Url;
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        io::{Cursor, Write},
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };
    use zip::{write::SimpleFileOptions, ZipWriter};

    const VALID_METADATA: &str = r#"{
        "id": 1132904,
        "name": "dbc-super-oficial",
        "version": "10.8",
        "minecraft": "1.7.10",
        "url": "https://www.dropbox.com/scl/fi/example/ATT58.zip"
    }"#;

    static TEMP_DIR_ID: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let id = TEMP_DIR_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("obsy-technic-test-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn build_zip(entries: Vec<(String, Vec<u8>, Option<u32>)>) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes, mode) in entries {
            let options = mode
                .map(|mode| SimpleFileOptions::default().unix_permissions(mode))
                .unwrap_or_default();
            writer.start_file(name, options).unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn mark_zip_entry_as_symlink(mut bytes: Vec<u8>, name: &str) -> Vec<u8> {
        let mut found = false;
        for offset in 0..bytes.len().saturating_sub(46) {
            if &bytes[offset..offset + 4] != b"PK\x01\x02" {
                continue;
            }
            let name_len = u16::from_le_bytes([bytes[offset + 28], bytes[offset + 29]]) as usize;
            let name_start = offset + 46;
            if name_start + name_len > bytes.len() {
                continue;
            }
            if &bytes[name_start..name_start + name_len] == name.as_bytes() {
                bytes[offset + 5] = 3;
                let unix_mode = (0o120777_u32 << 16).to_le_bytes();
                bytes[offset + 38..offset + 42].copy_from_slice(&unix_mode);
                found = true;
                break;
            }
        }
        assert!(found, "test ZIP entry {name} was not written");
        bytes
    }

    fn profile_entries() -> Vec<(String, Vec<u8>, Option<u32>)> {
        let base_profile = serde_json::json!({
            "id": "1.7.10",
            "type": "release",
            "assets": "1.7.10",
            "mainClass": "net.minecraft.client.main.Main",
            "minecraftArguments": "--username ${auth_player_name}",
            "libraries": []
        });
        let forge_profile = serde_json::json!({
            "id": "1.7.10-Forge10.13.4.1558-1.7.10",
            "inheritsFrom": "1.7.10",
            "jar": "1.7.10",
            "assets": "1.7.10",
            "mainClass": "net.minecraft.launchwrapper.Launch",
            "minecraftArguments": "--username ${auth_player_name} --tweakClass cpw.mods.fml.common.launcher.FMLTweaker",
            "libraries": [
                { "name": "net.minecraftforge:forge:1.7.10-10.13.4.1558-1.7.10" }
            ]
        });
        let minecraft_jar = build_zip(vec![(
            "net/minecraft/client/main/Main.class".to_string(),
            b"minecraft".to_vec(),
            None,
        )]);
        let forge_jar = build_zip(vec![
            (
                "cpw/mods/fml/common/Loader.class".to_string(),
                b"forge".to_vec(),
                None,
            ),
            (
                "version.json".to_string(),
                serde_json::to_vec(&forge_profile).unwrap(),
                None,
            ),
        ]);

        vec![
            (
                "bin/1.7.10.json".to_string(),
                serde_json::to_vec(&base_profile).unwrap(),
                None,
            ),
            (
                "bin/version.json".to_string(),
                serde_json::to_vec(&forge_profile).unwrap(),
                None,
            ),
            ("bin/minecraft.jar".to_string(), minecraft_jar, None),
            ("bin/modpack.jar".to_string(), forge_jar, None),
        ]
    }

    fn pack_archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut entries = profile_entries();
        entries.extend(
            files
                .iter()
                .map(|(path, bytes)| (path.to_string(), bytes.to_vec(), None)),
        );
        build_zip(entries)
    }

    fn installed_manifest(version: &str) -> InstalledPackManifest {
        InstalledPackManifest {
            pack_id: TECHNIC_PACK_ID,
            pack_version: version.to_string(),
            minecraft_version: TECHNIC_MINECRAFT_VERSION.to_string(),
            forge_version: "10.13.4.1558".to_string(),
            managed_files: Vec::new(),
        }
    }

    fn metadata(version: &str) -> TechnicPackMetadata {
        TechnicPackMetadata {
            id: TECHNIC_PACK_ID,
            name: "dbc-super-oficial".to_string(),
            version: version.to_string(),
            minecraft: TECHNIC_MINECRAFT_VERSION.to_string(),
            url: Url::parse("https://www.dropbox.com/archive.zip").unwrap(),
        }
    }

    #[test]
    fn parse_latest_metadata_accepts_verified_pack_response() {
        let metadata = parse_latest_metadata(VALID_METADATA).unwrap();

        assert_eq!(metadata.id, 1132904);
        assert_eq!(metadata.name, "dbc-super-oficial");
        assert_eq!(metadata.version, "10.8");
        assert_eq!(metadata.minecraft, "1.7.10");
        assert_eq!(metadata.url.scheme(), "https");
    }

    #[test]
    fn parse_latest_metadata_rejects_another_pack_id() {
        let body = VALID_METADATA.replace("1132904", "42");

        assert!(parse_latest_metadata(&body).is_err());
    }

    #[test]
    fn parse_latest_metadata_rejects_another_pack_name() {
        let body = VALID_METADATA.replace("dbc-super-oficial", "another-pack");

        assert!(parse_latest_metadata(&body).is_err());
    }

    #[test]
    fn parse_latest_metadata_rejects_missing_or_empty_required_fields() {
        for field in ["version", "minecraft", "url"] {
            let mut body: serde_json::Value = serde_json::from_str(VALID_METADATA).unwrap();
            body[field] = serde_json::json!("");

            assert!(
                parse_latest_metadata(&body.to_string()).is_err(),
                "expected empty {field} to be rejected"
            );

            body.as_object_mut().unwrap().remove(field);
            assert!(
                parse_latest_metadata(&body.to_string()).is_err(),
                "expected missing {field} to be rejected"
            );
        }
    }

    #[test]
    fn parse_latest_metadata_rejects_invalid_json_and_non_https_urls() {
        assert!(parse_latest_metadata("{").is_err());

        let body = VALID_METADATA.replace("https://", "http://");
        assert!(parse_latest_metadata(&body).is_err());

        let body = VALID_METADATA.replace("www.dropbox.com", "untrusted.example");
        assert!(parse_latest_metadata(&body).is_err());

        let body = VALID_METADATA.replace("1.7.10", "1.20.1");
        assert!(parse_latest_metadata(&body).is_err());
    }

    #[test]
    fn install_manifest_persists_pack_profile_and_sha256_inventory() {
        let temp = TempDir::new();
        let archive = pack_archive(&[("mods/example.jar", b"mod bytes")]);
        let manifest = install_archive_transaction(temp.path(), &archive, "10.8", None).unwrap();
        let persisted: InstalledPackManifest =
            serde_json::from_slice(&fs::read(temp.path().join("manifest.json")).unwrap()).unwrap();
        let expected_hash = format!("{:x}", Sha256::digest(b"mod bytes"));

        assert_eq!(manifest, persisted);
        assert_eq!(persisted.pack_id, TECHNIC_PACK_ID);
        assert_eq!(persisted.pack_version, "10.8");
        assert_eq!(persisted.minecraft_version, "1.7.10");
        assert_eq!(persisted.forge_version, "10.13.4.1558");
        assert!(persisted.managed_files.contains(&ManagedFile {
            path: "game/mods/example.jar".to_string(),
            sha256: expected_hash,
        }));
        assert!(persisted.managed_files.iter().any(|file| {
            file.path == "runtime/versions/forge-1.7.10-10.13.4.1558/forge-1.7.10-10.13.4.1558.json"
        }));
        assert!(persisted.managed_files.iter().any(|file| {
            file.path == "runtime/versions/forge-1.7.10-10.13.4.1558/forge-1.7.10-10.13.4.1558.jar"
        }));
        let forge_profile: serde_json::Value =
            serde_json::from_slice(
                &fs::read(temp.path().join(
                    "runtime/versions/forge-1.7.10-10.13.4.1558/forge-1.7.10-10.13.4.1558.json",
                ))
                .unwrap(),
            )
            .unwrap();
        assert_eq!(forge_profile["id"], "1.7.10-Forge10.13.4.1558-1.7.10");
    }

    #[test]
    fn install_manifest_rejects_html_invalid_paths_duplicates_and_symlinks() {
        let temp = TempDir::new();
        for (label, archive) in [
            ("HTML response", b"<html>error</html>".to_vec()),
            (
                "traversal path",
                pack_archive(&[("../outside.txt", b"bad")]),
            ),
            ("absolute path", pack_archive(&[("/outside.txt", b"bad")])),
            (
                "Windows drive path",
                pack_archive(&[("C:/outside.txt", b"bad")]),
            ),
            (
                "Windows reserved name",
                pack_archive(&[("mods/CON.txt", b"bad")]),
            ),
            (
                "Windows trailing dot",
                pack_archive(&[("mods/invalid./file.jar", b"bad")]),
            ),
            (
                "duplicate paths",
                pack_archive(&[
                    ("mods/Duplicate.jar", b"first"),
                    ("mods/duplicate.jar", b"second"),
                ]),
            ),
            ("symlink entry", {
                let mut entries = profile_entries();
                entries.push((
                    "mods/link.jar".to_string(),
                    b"target".to_vec(),
                    Some(0o120777),
                ));
                mark_zip_entry_as_symlink(build_zip(entries), "mods/link.jar")
            }),
        ] {
            let root = temp.path().join(label.replace(' ', "_"));
            assert!(
                install_archive_transaction(&root, &archive, "10.8", None).is_err(),
                "{label} must be rejected"
            );
            assert!(!root.exists(), "{label} must not leave an installation");
        }
    }

    #[test]
    fn install_manifest_preserves_personal_and_untracked_files() {
        let temp = TempDir::new();
        let root = temp.path();
        let first = pack_archive(&[
            ("mods/remove.jar", b"old pack file"),
            ("mods/current.jar", b"old current file"),
            ("options.txt", b"pack defaults"),
            ("servers.dat", b"pack servers"),
        ]);
        let first_manifest = install_archive_transaction(root, &first, "10.8", None).unwrap();
        fs::write(root.join("game/options.txt"), b"player settings").unwrap();
        fs::write(root.join("game/servers.dat"), b"player servers").unwrap();
        fs::write(root.join("game/mods/remove.jar"), b"player-edited mod").unwrap();
        let preserved_paths = [
            ("game/saves/world/level.dat", b"world".as_slice()),
            ("game/screenshots/image.png", b"screen".as_slice()),
            (
                "game/resourcepacks/CustomPack/pack.txt",
                b"resource pack".as_slice(),
            ),
            ("game/shaderpacks/custom.zip", b"shader".as_slice()),
            ("game/untracked.txt", b"untracked".as_slice()),
            ("runtime/libraries/cached.jar", b"cached library".as_slice()),
        ];
        for (path, bytes) in preserved_paths {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }

        let second = pack_archive(&[
            ("mods/current.jar", b"current pack file"),
            ("mods/new.jar", b"new pack file"),
            ("options.txt", b"new pack defaults"),
            ("servers.dat", b"new pack servers"),
        ]);
        let manifest =
            install_archive_transaction(root, &second, "10.9", Some(&first_manifest)).unwrap();

        assert_eq!(
            fs::read(root.join("game/options.txt")).unwrap(),
            b"player settings"
        );
        assert_eq!(
            fs::read(root.join("game/servers.dat")).unwrap(),
            b"player servers"
        );
        assert_eq!(
            fs::read(root.join("game/mods/remove.jar")).unwrap(),
            b"player-edited mod"
        );
        assert_eq!(
            fs::read(root.join("game/mods/current.jar")).unwrap(),
            b"current pack file"
        );
        for (path, bytes) in preserved_paths {
            assert_eq!(fs::read(root.join(path)).unwrap(), bytes, "{path}");
        }
        assert!(!manifest
            .managed_files
            .iter()
            .any(|file| file.path == "game/options.txt"));
    }

    #[test]
    fn install_manifest_removes_only_unchanged_obsolete_pack_files() {
        let temp = TempDir::new();
        let root = temp.path();
        let first = pack_archive(&[
            ("mods/obsolete.jar", b"obsolete"),
            ("mods/edited.jar", b"original"),
        ]);
        let first_manifest = install_archive_transaction(root, &first, "10.8", None).unwrap();
        fs::write(root.join("game/mods/edited.jar"), b"local edit").unwrap();
        let second = pack_archive(&[("mods/current.jar", b"current")]);

        install_archive_transaction(root, &second, "10.9", Some(&first_manifest)).unwrap();

        assert!(!root.join("game/mods/obsolete.jar").exists());
        assert_eq!(
            fs::read(root.join("game/mods/edited.jar")).unwrap(),
            b"local edit"
        );
        assert!(root.join("game/mods/current.jar").exists());
    }

    #[test]
    fn install_manifest_aborts_on_untracked_file_directory_conflict() {
        let temp = TempDir::new();
        let root = temp.path();
        let first = pack_archive(&[("mods/old.jar", b"old")]);
        install_archive_transaction(root, &first, "10.8", None).unwrap();
        fs::write(root.join("game/conflict"), b"player file").unwrap();
        let second = pack_archive(&[("conflict/nested.txt", b"new pack file")]);

        assert!(install_archive_transaction(root, &second, "10.9", None).is_err());

        assert_eq!(
            fs::read(root.join("game/conflict")).unwrap(),
            b"player file"
        );
        let manifest: InstalledPackManifest =
            serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest.pack_version, "10.8");
    }

    #[test]
    fn install_manifest_restores_old_root_when_manifest_persistence_fails() {
        let temp = TempDir::new();
        let root = temp.path().join("pack");
        let staged_root = temp.path().join("staged");
        fs::create_dir_all(root.join("game")).unwrap();
        fs::write(root.join("manifest.json"), b"old manifest").unwrap();
        fs::write(root.join("game/old.txt"), b"old files").unwrap();
        fs::create_dir_all(staged_root.join("manifest.json")).unwrap();
        fs::create_dir_all(staged_root.join("game")).unwrap();
        fs::write(staged_root.join("game/new.txt"), b"new files").unwrap();

        let error = super::apply_staged_update(&root, &staged_root, &installed_manifest("10.9"))
            .unwrap_err();

        assert!(error.contains("manifest"), "unexpected error: {error}");
        assert_eq!(
            fs::read(root.join("manifest.json")).unwrap(),
            b"old manifest"
        );
        assert_eq!(fs::read(root.join("game/old.txt")).unwrap(), b"old files");
        assert!(!root.join("game/new.txt").exists());
    }

    #[test]
    fn install_manifest_decides_install_update_current_and_offline_fallback() {
        let installed = installed_manifest("10.8");

        assert_eq!(
            decide_update(None, Ok(&metadata("10.8"))).unwrap(),
            UpdateDecision::Install
        );
        assert_eq!(
            decide_update(Some(&installed), Ok(&metadata("10.8"))).unwrap(),
            UpdateDecision::AlreadyCurrent
        );
        assert_eq!(
            decide_update(Some(&installed), Ok(&metadata("10.9"))).unwrap(),
            UpdateDecision::Update
        );
        assert_eq!(
            decide_update(Some(&installed), Err("offline")).unwrap(),
            UpdateDecision::UseCachedWithWarning
        );
        assert!(decide_update(None, Err("offline")).is_err());
        assert!(decide_update(Some(&installed), Ok(&metadata("10.7"))).is_err());
    }

    #[test]
    fn launch_id_accepts_only_the_fixed_technic_pack() {
        assert!(super::validate_launch_id("technic-1132904").is_ok());
        for id in ["1.20.1", "23w13a", "alternate-instance"] {
            assert!(super::validate_launch_id(id).is_err(), "{id}");
        }
    }

    #[test]
    fn launch_status_returns_only_a_valid_installed_manifest() {
        let temp = TempDir::new();
        let archive = pack_archive(&[("mods/example.jar", b"mod bytes")]);
        let installed = install_archive_transaction(temp.path(), &archive, "10.8", None).unwrap();

        assert_eq!(
            super::read_pack_status(temp.path()).unwrap(),
            Some(installed)
        );

        fs::write(temp.path().join("game/mods/example.jar"), b"changed").unwrap();
        assert!(super::read_pack_status(temp.path()).is_err());
        assert_eq!(
            super::read_pack_status(&temp.path().join("not-installed")).unwrap(),
            None
        );
    }
}
