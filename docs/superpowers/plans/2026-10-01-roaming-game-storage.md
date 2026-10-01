# Roaming Game Storage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move release-build DBC Super game files to `%APPDATA%\DBC Super Launcher`, separate launcher-owned metadata and managed Java, and safely migrate existing Local data.

**Architecture:** Add an injectable `StorageLayout` that resolves the application root, game root, launcher root, Java root, and Technic metadata root. Release builds derive the application root from `dirs::data_dir()`; debug builds retain the existing repository-local `.obsy` root. Migrate old installations by retryable, non-destructive merging, then refactor Technic installs so game-owned files are transactionally reconciled in the game root while manifest/runtime/staging remain under launcher metadata.

**Tech Stack:** Rust 2021, Tauri 2, `dirs`, `std::fs`, existing Rust unit tests and Node.js contract tests.

**Spec:** `docs/superpowers/specs/2026-10-01-roaming-storage-design.md`

## Global Constraints

- Store release-build DBC Super game files directly in `%APPDATA%\DBC Super Launcher`.
- Store the launcher executable in its existing installation location; do not move it.
- Keep launcher-owned JSON, `profiles.key`, Technic manifest, Forge runtime, and transaction staging under `launcher/`.
- Store managed Java under `java/`.
- Preserve debug builds' existing repository-local `.obsy` location.
- Migration copies and merges data; it never deletes the old source.
- Migration does not overwrite existing destination files.
- Preserve `profiles.json` and `profiles.key` together.
- Preserve saves, screenshots, shaderpacks, crash reports, logs, `options.txt`, and `servers.dat` across pack updates.
- If a data directory cannot be resolved or created, report an actionable error instead of silently falling back to the executable directory.
- Keep the launcher dedicated to Technic pack ID `1132904`.

## Review Focus

- Existing `%APPDATA%` files conflict with migrated Local files: destination files remain authoritative, but missing profile/key partner files must still migrate together.
- Interruption during a large game/runtime copy: no completion marker is written; a retry safely continues without deleting the source.
- The legacy manifest describes the old nested `game/` and `runtime/` layout: validate/reconcile it without treating user files as launcher-owned.
- A Technic update fails between preparing game files and updating metadata: restore the previous playable installation and retain all personal files.
- Malformed or symlinked legacy entries and unavailable data directories: fail explicitly, do not follow unsafe entries, and do not mark migration complete.

---

### Task 1: Define and test the application storage layout

**Files:**

- Modify: `src-tauri/src/minecraft/versions.rs`
- Modify: `src-tauri/src/minecraft/mod.rs`
- Test: unit tests in `src-tauri/src/minecraft/versions.rs`

**Interfaces:**

- Produces `StorageLayout::from_app_root(app_root: PathBuf) -> StorageLayout`.
- Produces `StorageLayout::resolve(is_debug: bool, current_dir: Option<PathBuf>, data_dir: Option<PathBuf>) -> Result<StorageLayout, String>` as a deterministic, testable resolver, plus `StorageLayout::current() -> Result<StorageLayout, String>` as its production wrapper.
- `StorageLayout` exposes `app_root()`, `game_root()`, `launcher_root()`, `java_root()`, and `technic_pack_root(pack_id: u64)`, each returning `&Path` or `PathBuf` consistently.
- The game root is the application root; launcher metadata is `<app_root>/launcher`; Java is `<app_root>/java`; Technic metadata is `<app_root>/launcher/technic/<pack_id>`.
- In debug builds the application root is `current_dir()/.obsy`; in release it is `dirs::data_dir()/DBC Super Launcher`. Missing base paths return `Err`, not an executable/current-directory fallback; callers create required directories and propagate creation errors.
- Replace or deprecate `get_minecraft_dir()` only after all callers have an explicit layout path in dependent tasks.

- [ ] **Step 1: Write failing layout tests** for an injected root, debug root, release root, and unavailable data directory. Assert exact derived paths and an error rather than fallback when a required base cannot be resolved; test `resolve()` with explicit paths/options rather than mutating process environment.
- [ ] **Step 2: Run the focused tests** with `Push-Location src-tauri; cargo test minecraft::versions::tests; Pop-Location`. Expected: the new layout API/tests fail to compile or fail assertions because the API is not implemented.
- [ ] **Step 3: Implement `StorageLayout` and `StorageLayout::current()`** in `versions.rs`, export the module API through `minecraft/mod.rs`, and keep any temporary compatibility wrapper needed by later tasks.
- [ ] **Step 4: Rerun the focused tests**. Expected: exact game, launcher, Java, and Technic paths pass in debug/release-specific configurations.
- [ ] **Step 5: Commit** as `feat: define launcher storage layout`.

### Task 2: Make legacy migration safe, complete, and retryable

**Files:**

- Modify: `src-tauri/src/minecraft/migration.rs`
- Modify: `src-tauri/src/lib.rs` startup setup
- Test: unit tests in `src-tauri/src/minecraft/migration.rs`

**Interfaces:**

- Consumes `StorageLayout` from Task 1.
- Replace the current infallible `migrate_legacy_data_if_needed(target_dir, app_handle)` with `migrate_legacy_data_if_needed(layout: &StorageLayout, legacy_root: &Path) -> Result<(), String>`; resolve the release legacy root as the existing executable-adjacent `.obsy` directory and the debug legacy root as the existing repository-local `.obsy`.
- Migration must be unit-testable with explicit source and destination paths; no test may depend on the developer's actual AppData.
- A successful migration writes the completion marker under `<launcher_root>/.migration_completed`; failures propagate and do not write it.

- [ ] **Step 1: Write failing migration tests** for moving legacy `technic/1132904/game/*` into the game root, runtime/manifest into the new Technic root, `jre/` into `java/`, launcher JSON/key into `launcher/`, legacy caches such as `assets/`, `libraries/`, `instances/`, and `versions/` into launcher-owned storage, and preserving `profiles.json` with `profiles.key`.
- [ ] **Step 2: Add failing merge/retry tests** for destination-wins conflicts, missing profile/key partner migration, personal files, interrupted copy followed by retry, idempotent second run, retained source, malformed entries, and filesystem errors leaving no completion marker.
- [ ] **Step 3: Run the focused migration tests** with `Push-Location src-tauri; cargo test minecraft::migration::tests; Pop-Location`. Expected: tests fail against the current migration's flat, best-effort copy behavior.
- [ ] **Step 4: Implement fallible, non-destructive migration** using temporary source/destination fixtures, safe file-type checks, and per-file copy/merge; map old launcher-owned files and managed subtrees to the layout specified in the design.
- [ ] **Step 5: Wire startup to resolve the layout, run migration before loading stores, and propagate migration/path errors** rather than silently continuing with an empty store.
- [ ] **Step 6: Rerun focused migration tests**. Expected: all transformation, preservation, conflict, retry, and error tests pass.
- [ ] **Step 7: Commit** as `feat: migrate launcher data to Roaming layout`.

### Task 3: Separate Technic game files from launcher metadata transactionally

**Files:**

- Modify: `src-tauri/src/technic.rs`
- Test: unit tests in `src-tauri/src/technic.rs`

**Interfaces:**

- Consumes `StorageLayout` from Task 1.
- Keep `managed_game_root() -> Result<PathBuf, String>` as the game root and add/use `managed_pack_root() -> Result<PathBuf, String>` for `<launcher_root>/technic/1132904`.
- Define a `TechnicRoots` value (or equivalent explicit arguments) carrying `game_root` and `pack_root`; all status, install, update, manifest validation, and rollback operations use these roots rather than inferring a single directory.
- Keep the manifest's relative managed paths rooted at the pack metadata root for runtime entries and at the game root for game entries; include an explicit, validated discriminator/prefix so a manifest entry cannot escape either root. Preserve compatibility by safely translating old `game/...` and `runtime/...` manifest entries during migration.

- [ ] **Step 1: Write failing archive-install tests** asserting archive game files land directly under `game_root`, while manifest, Forge JSON/JAR, and staging/backup files remain under `pack_root`.
- [ ] **Step 2: Write failing update tests** proving personal files survive an update, modified user files are not overwritten, stale launcher-owned files are reconciled, and a failed activation restores the previous installation in both roots.
- [ ] **Step 3: Write failing path-validation tests** rejecting traversal, symlink, ambiguous, or unknown-root manifest entries.
- [ ] **Step 4: Run focused Technic tests** with `Push-Location src-tauri; cargo test technic::tests; Pop-Location`. Expected: the new root-separation assertions fail with the current single-root installer.
- [ ] **Step 5: Refactor archive staging and managed-file reconciliation** to prepare game and runtime outputs independently, retain a rollback copy/record under `pack_root`, and only publish the new manifest after game/runtime activation succeeds. On failure, restore previous managed files without deleting personal data.
- [ ] **Step 6: Rerun focused Technic tests**. Expected: separated roots, manifest validation, preservation, and rollback tests pass.
- [ ] **Step 7: Commit** as `feat: install Technic game files in compact root`.

### Task 4: Route launcher stores, Java, launch, and folder actions through the layout

**Files:**

- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/auth.rs`
- Modify: `src-tauri/src/wardrobe.rs`
- Modify: `src-tauri/src/minecraft/java.rs`
- Modify: `src-tauri/src/minecraft/playtime.rs`
- Modify: `src-tauri/src/minecraft/dedup.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/open_launcher/libraries.rs` only if path resolution requires it
- Test: Rust unit tests in the affected modules and launch contract tests in `scripts/tests/technic-launcher-contract.test.cjs`

**Interfaces:**

- Consumes `StorageLayout::current()` and migration behavior from Tasks 1–2, plus `managed_game_root()` / `managed_pack_root()` from Task 3.
- Persist launcher state, profiles and key, wardrobe, and playtime under `launcher_root()`.
- Download/find managed Java under `java_root()`; preserve a user-selected Java executable path.
- Set Minecraft execution directory and open-folder action to `game_root()`.
- Set Forge profile/JAR and its library base to the managed runtime under `launcher/technic/1132904/runtime`.
- Store installation caches/dedup data under launcher-owned paths, never in the visible game root unless it is Minecraft game content.

- [ ] **Step 1: Write failing tests/contracts** for every store path, managed Java path, launch working directory, Forge runtime/classpath root, resource-pack directory, and open-folder target. Include assertions that launcher JSON is not created in the game root.
- [ ] **Step 2: Run focused tests/contracts** with `Push-Location src-tauri; cargo test; Pop-Location` and `node --test scripts/tests/technic-launcher-contract.test.cjs`. Expected: the new path contracts fail against legacy `get_minecraft_dir()` consumers.
- [ ] **Step 3: Update store APIs to return path/load/save errors explicitly** where layout resolution can fail; ensure Tauri startup and commands propagate actionable errors rather than replacing inaccessible data with defaults.
- [ ] **Step 4: Route Java, launch setup, Forge runtime, dedup/cache paths, resource-pack creation, and folder opening through the correct layout roots.**
- [ ] **Step 5: Rerun focused Rust tests and launcher contracts.** Expected: all state/data/launch paths match the layout and existing launch behavior remains intact.
- [ ] **Step 6: Commit** as `refactor: route launcher operations through storage layout`.

### Task 5: Verify end-to-end behavior and remove obsolete path assumptions

**Files:**

- Modify: `scripts/tests/technic-launcher-contract.test.cjs`
- Modify: relevant Rust tests in `src-tauri/src/minecraft/versions.rs`, `migration.rs`, `technic.rs`, and store/launch modules
- Modify: directly related documentation if existing path instructions reference the old directory

**Interfaces:**

- Consumes the completed storage APIs and behavior from Tasks 1–4.
- No new storage roots or fallback behavior may be introduced in this task.

- [ ] **Step 1: Add final regression assertions** that no release code derives data from the executable directory, game content is rooted directly at the app data folder, and the legacy `.obsy` source remains present after migration.
- [ ] **Step 2: Run Rust tests and doctests** with `Push-Location src-tauri; cargo test; Pop-Location`. Expected: all unit tests and doctests pass.
- [ ] **Step 3: Run frontend contract tests** with `node --test scripts/tests/technic-launcher-contract.test.cjs`. Expected: all launcher path contracts pass.
- [ ] **Step 4: Run formatting and frontend validation** with `Push-Location src-tauri; cargo fmt --check; Pop-Location`, `npx prettier --check` on modified frontend/test/docs files, and `npm run build`. Expected: all checks pass; report any pre-existing bundle-size warning separately.
- [ ] **Step 5: Run `git diff --check` and inspect the final diff** to confirm prior unrelated/local changes were not reverted and no old path behavior remains in the touched flow.
- [ ] **Step 6: Commit** as `test: verify Roaming storage migration and launch`.
