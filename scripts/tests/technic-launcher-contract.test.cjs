const assert = require("node:assert/strict");
const fs = require("node:fs");
const test = require("node:test");

const read = (relativePath) =>
  fs.readFileSync(`${process.cwd()}/${relativePath}`, "utf8");

const app = read("src/App.tsx");
const launchButton = read("src/components/launcher/LaunchButton.tsx");
const profileSelector = read("src/components/launcher/ProfileSelector.tsx");
const technicPackStatus = read("src/components/launcher/TechnicPackStatus.tsx");
const launcherState = read("src/state.ts");
const backend = read("src-tauri/src/lib.rs");
const styles = read("src/index.css");
const frontend = [app, launchButton, launcherState].join("\n");
const backendStores = [
  read("src-tauri/src/state.rs"),
  read("src-tauri/src/auth.rs"),
  read("src-tauri/src/wardrobe.rs"),
  read("src-tauri/src/minecraft/playtime.rs"),
];
const javaStore = read("src-tauri/src/minecraft/java.rs");
const dedupStore = read("src-tauri/src/minecraft/dedup.rs");
const technicBackend = read("src-tauri/src/technic.rs");

test("launcher_opens_without_first_run_onboarding", () => {
  assert.doesNotMatch(app, /Onboarding|showOnboarding|hasCompletedOnboarding/);
});

test("does_not_render_generic_version_controls", () => {
  assert.doesNotMatch(app, /VersionSelector|CreateInstanceModal/);
  for (const component of [
    "VersionSelector.tsx",
    "CreateInstanceModal.tsx",
    "InstanceControls.tsx",
  ]) {
    assert.equal(
      fs.existsSync(`src/components/launcher/${component}`),
      false,
      `${component} should be removed`,
    );
  }
});

test("frontend_does_not_invoke_version_or_instance_crud", () => {
  for (const command of [
    "get_versions",
    "select_version",
    "create_instance",
    "delete_instance",
    "open_version_folder",
  ]) {
    assert.ok(
      !frontend.includes(`"${command}"`),
      `frontend still uses ${command}`,
    );
  }
});

test("launch_uses_fixed_technic_id", () => {
  assert.match(launchButton, /versionId:\s*["']technic-1132904["']/);
  assert.doesNotMatch(launchButton, /selectedVersionId/);
});

test("launch_button_keeps_play_label_during_install", () => {
  assert.match(launchButton, /t\("launch\.play"\)/);
  assert.doesNotMatch(launchButton, /t\("launch\.installAndPlay"\)/);
  assert.doesNotMatch(launchButton, /isPackInstalled\s*\?/);
});

test("only_the_play_button_renders_launch_progress", () => {
  assert.match(launchButton, /<Progress/);
  assert.doesNotMatch(technicPackStatus, /<Progress/);
});

test("add_profile_dialog_uses_compact_width_and_spacing", () => {
  assert.match(profileSelector, /<DialogContent[^>]*data-profile-dialog/);
  assert.match(
    styles,
    /\[data-profile-dialog\]\s*\{[^}]*max-width:\s*min\(420px,\s*calc\(100vw\s*-\s*2\.5rem\)\)\s*!important;[^}]*padding:\s*1rem\s*!important;[^}]*gap:\s*0?\.75rem\s*!important;/s,
  );
});

test("minecraft_runs_without_a_console_and_keeps_capturing_logs", () => {
  assert.match(backend, /creation_flags\(0x08000000\)/);
  assert.match(backend, /command\.stdout\(std::process::Stdio::piped\(\)\)/);
  assert.match(backend, /command\.stderr\(std::process::Stdio::piped\(\)\)/);
});

test("launcher_owned_stores_use_launcher_metadata_not_the_game_root", () => {
  for (const store of backendStores) {
    assert.match(store, /StorageLayout::current\(\)/);
    assert.match(store, /launcher_root\(\)/);
    assert.doesNotMatch(store, /get_minecraft_dir\(/);
  }
});

test("managed_java_and_dedup_caches_use_launcher_owned_roots", () => {
  assert.match(javaStore, /StorageLayout::current\(\)/);
  assert.match(javaStore, /java_root\(\)/);
  assert.doesNotMatch(javaStore, /get_minecraft_dir\(/);
  assert.match(dedupStore, /StorageLayout::current\(\)/);
  assert.match(dedupStore, /launcher_root\(\)/);
  assert.doesNotMatch(dedupStore, /get_minecraft_dir\(/);
});

test("launch_runtime_and_open_folder_use_their_designated_roots", () => {
  assert.doesNotMatch(backend, /get_minecraft_dir\(/);
  assert.match(backend, /managed_game_root\(\)\?/);
  assert.match(backend, /managed_runtime_root\(\)\?/);
  assert.match(
    backend,
    /"open-folder"\s*=>\s*\{[\s\S]*?managed_game_root\(\)\?/,
  );
  assert.match(technicBackend, /StorageLayout::current\(\)/);
});

test("tauri_handler_does_not_register_version_or_instance_crud", () => {
  const handler = backend.match(
    /invoke_handler\(tauri::generate_handler!\[([\s\S]*?)\]\)/,
  );
  assert.ok(handler, "Tauri invoke handler was not found");

  for (const command of [
    "get_versions",
    "select_version",
    "create_instance",
    "delete_instance",
    "open_version_folder",
    "download_instance_file",
    "read_instance_zip_entry",
    "extract_instance_zip_folder",
    "delete_instance_file",
  ]) {
    assert.ok(
      !handler[1].includes(command),
      `handler still registers ${command}`,
    );
  }
});
