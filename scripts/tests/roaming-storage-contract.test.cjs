const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const read = (relativePath) =>
  fs.readFileSync(path.join(__dirname, "..", "..", relativePath), "utf8");

const backend = read("src-tauri/src/lib.rs");
const storageLayout = read("src-tauri/src/minecraft/versions.rs");

test("release storage is resolved from Roaming, not the executable directory", () => {
  assert.match(storageLayout, /dirs::data_dir\(\)/);
  assert.doesNotMatch(storageLayout, /current_exe/);
  assert.doesNotMatch(storageLayout, /get_minecraft_dir\(/);
});

test("launcher-owned data uses the launcher metadata root", () => {
  for (const relativePath of [
    "src-tauri/src/state.rs",
    "src-tauri/src/auth.rs",
    "src-tauri/src/wardrobe.rs",
    "src-tauri/src/minecraft/playtime.rs",
  ]) {
    const source = read(relativePath);
    assert.match(source, /StorageLayout::current\(\)/, relativePath);
    assert.match(source, /launcher_root\(\)/, relativePath);
    assert.doesNotMatch(source, /get_minecraft_dir\(/, relativePath);
  }
});

test("managed Java and deduplication use their launcher-owned roots", () => {
  const javaStore = read("src-tauri/src/minecraft/java.rs");
  const dedupStore = read("src-tauri/src/minecraft/dedup.rs");

  assert.match(javaStore, /StorageLayout::current\(\)/);
  assert.match(javaStore, /java_root\(\)/);
  assert.doesNotMatch(javaStore, /get_minecraft_dir\(/);
  assert.match(dedupStore, /StorageLayout::current\(\)/);
  assert.match(dedupStore, /launcher_root\(\)/);
  assert.doesNotMatch(dedupStore, /get_minecraft_dir\(/);
});

test("launch and folder actions use their designated game and runtime roots", () => {
  assert.doesNotMatch(backend, /get_minecraft_dir\(/);
  assert.match(backend, /managed_game_root\(\)\?/);
  assert.match(backend, /managed_runtime_root\(\)\?/);
  assert.match(backend, /Launcher::new\(\s*&runtime_dir_str/);
  assert.match(backend, /set_execution_directory\(game_dir\)/);
  assert.match(backend, /ensure_resourcepacks_directory\(&game_dir\)/);
  assert.match(backend, /if let Some\(path\) = &launcher_state\.java_path/);
  assert.match(
    backend,
    /"open-folder"\s*=>\s*\{[\s\S]*?managed_game_root\(\)\?/,
  );
});
