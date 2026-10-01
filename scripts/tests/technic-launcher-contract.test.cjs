const assert = require("node:assert/strict");
const fs = require("node:fs");
const test = require("node:test");

const read = (relativePath) =>
  fs.readFileSync(`${process.cwd()}/${relativePath}`, "utf8");

const app = read("src/App.tsx");
const launchButton = read("src/components/launcher/LaunchButton.tsx");
const launcherState = read("src/state.ts");
const frontend = [app, launchButton, launcherState].join("\n");

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

test("tauri_handler_does_not_register_version_or_instance_crud", () => {
  const source = read("src-tauri/src/lib.rs");
  const handler = source.match(
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
