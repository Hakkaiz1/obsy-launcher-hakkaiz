# Remove the Add-on System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the launcher's add-on/plugin feature from the UI, runtime, backend, bundled assets, and active documentation while preserving instance archive extraction and user data outside the repository.

**Architecture:** Remove the frontend add-on entry points, runtime API, stores, UI, and translations. Delete the Tauri add-on commands, moving the safe path and ZIP entry helpers they currently share with instance extraction into `src-tauri/src/fs_utils.rs`. Remove the built-in add-on build pipeline and repository artifacts, then update active documentation and validate the integrated repository.

**Tech Stack:** React 19, TypeScript, Vite, Tauri 2, Rust, Cargo, Prettier.

**Spec:** `docs/superpowers/specs/2026-09-30-remove-addon-system-design.md`

## Global Constraints

- Do not delete or modify add-on files in a user's Minecraft data directory.
- Do not remove Minecraft mods, mod loaders, instance archive installation, or unrelated archive handling.
- Retain path validation and safe ZIP extraction with rejection of absolute paths and traversal.
- Keep legacy migration behavior for existing `addons` data; it copies user files but does not load or execute them.
- Preserve unrelated working-tree changes; edit existing dirty files surgically and do not revert them.
- Do not remove dependencies or Tauri plugins unless code inspection confirms they are exclusively used by the add-on feature. `sha2` is also used in `src-tauri/src/lib.rs`, so retain it.
- Do not start a development server; the user runs it externally.
- Leave implementation changes uncommitted because multiple files already contain unrelated user changes.

## Review Focus

- **ZIP path traversal and absolute paths:** existing safety behavior must remain pinned by unit tests in `fs_utils.rs`.
- **Instance ZIP extraction with an empty destination or nested folder prefix:** keep using the extracted helper functions in `extract_instance_zip_folder`.
- **Normal launcher startup and launch flow:** removing addon initialization and launch hooks must not interfere with state fetches, launch invocation, progress, or error handling.
- **Tauri command surface:** add-on-only commands must disappear while instance, profile, wardrobe, and updater commands remain registered.
- **Legacy user data:** migration must not delete or modify existing add-on directories; it may continue copying legacy data without loading it.

---

### Task 1: Remove Frontend Add-on UI and Runtime

**Files:**

- Modify: `src/App.tsx`
- Modify: `src/components/launcher/Header.tsx`
- Modify: `src/components/launcher/LaunchButton.tsx`
- Modify: `src/components/launcher/VersionSelector.tsx`
- Modify: `src/index.tsx`
- Modify: `src/locales/pt-BR.json`
- Delete: `src/components/addons/AddonsDialog.tsx`
- Delete: `src/components/addons/AddonsDialogHeader.tsx`
- Delete: `src/components/addons/AddonSecurityReviewModal.tsx`
- Delete: `src/components/addons/AddonSettingsModal.tsx`
- Delete: `src/components/addons/AddonTrustBadge.tsx`
- Delete: `src/components/addons/AddonsModalDialogContent.tsx`
- Delete: `src/components/addons/AddonsTabsHeader.tsx`
- Delete: `src/components/addons/ConfigFieldRow.tsx`
- Delete: `src/components/addons/InstalledAddonCard.tsx`
- Delete: `src/components/addons/InstalledTabContent.tsx`
- Delete: `src/components/addons/PluginErrorBoundary.tsx`
- Delete: `src/components/addons/PluginSlot.tsx`
- Delete: `src/components/addons/StoreAddonCard.tsx`
- Delete: `src/components/addons/StoreTabContent.tsx`
- Delete: `src/components/addons/UrlInstallModal.tsx`
- Delete: `src/lib/addons/addonStore.ts`
- Delete: `src/lib/addons/catalog.ts`
- Delete: `src/lib/addons/loader.ts`
- Delete: `src/lib/addons/registry.ts`
- Delete: `src/lib/addons/types.ts`
- Delete: `src/types/obsy.d.ts`

**Interfaces:**

- Consumes: Existing launcher state and Tauri `invoke("launch_game", ...)` flow.
- Produces: A frontend with no add-on entry points, plugin slots, add-on initialization, or dynamically exposed `window.React`, `window.ReactDOM`, and `window.Obsy` extension runtime.

- [ ] **Step 1: Remove UI entry points and slots**

  In `Header.tsx`, remove the `AddonsDialog` and `PluginSlot` imports and elements while retaining the console and settings controls. In `App.tsx`, remove add-on store/slot imports, the `initAddons()` startup call, and the dashboard widget slot. In `VersionSelector.tsx`, remove its `PluginSlot` import and `version.footer` slot.

- [ ] **Step 2: Remove launch hooks without changing game launch behavior**

  In `LaunchButton.tsx`, remove the add-on registry import and the `runBeforeLaunchHooks`/`game:launching` calls. Keep the existing validation, launch progress handling, `invoke("launch_game", ...)`, refresh, and error handling unchanged.

- [ ] **Step 3: Remove the dynamically exposed add-on runtime**

  In `index.tsx`, remove the imports used only to build `window.Obsy`, the add-on store import, and the `window.React`, `window.ReactDOM`, and `window.Obsy` assignments. Retain React StrictMode, `createRoot`, and the normal app render.

- [ ] **Step 4: Remove add-on translation keys and exclusively used frontend modules**

  Remove the `addons` translation subtree from `src/locales/pt-BR.json`; delete the listed UI, store, catalog, loader, registry, types, and runtime type declaration files.

- [ ] **Step 5: Run the frontend build and search for frontend references**

  Run: `npm run build`

  Expected: TypeScript and Vite build succeed. Search `src` for `PluginSlot`, `AddonsDialog`, `useAddonStore`, `addonRegistry`, `window.Obsy`, and add-on imports; no active frontend references remain.

### Task 2: Remove Tauri Add-on Commands and Preserve Safe Instance Extraction

**Files:**

- Create: `src-tauri/src/fs_utils.rs`
- Modify: `src-tauri/src/lib.rs`
- Delete: `src-tauri/src/addons.rs`

**Interfaces:**

- Consumes: Current helper implementations `addons::sanitize_path(base: &Path, relative: &Path) -> Result<PathBuf, String>` and `addons::safe_zip_extract_path(target_dir: &Path, rel_name: &str) -> Result<PathBuf, String>`.
- Produces: `crate::fs_utils::sanitize_path(base: &Path, relative: &Path) -> Result<PathBuf, String>` and `crate::fs_utils::safe_zip_extract_path(target_dir: &Path, rel_name: &str) -> Result<PathBuf, String>`, used by instance ZIP extraction. The module owns unit tests for safe and rejected paths.

- [ ] **Step 1: Move safe path helpers and tests to a neutral module**

  Create `fs_utils.rs` with the existing implementations of `sanitize_path` and `safe_zip_extract_path`, plus tests covering safe file paths, safe nested paths, parent traversal, nested traversal, and absolute paths. Keep their current return types and error behavior.

- [ ] **Step 2: Switch instance extraction to the neutral helpers**

  In `lib.rs`, declare `mod fs_utils;` and update both helper call sites in `extract_instance_zip_folder` to use `crate::fs_utils`. Preserve the current ZIP folder-prefix, destination, and best-effort extraction behavior.

- [ ] **Step 3: Remove add-on command registrations and add-on-only tests**

  Remove `pub mod addons;`, all add-on commands from `generate_handler!`, add-on path test coverage now owned by `fs_utils.rs`, and add-on verification tests. Keep unrelated launcher tests, including `validate_safe_id` tests used by instance operations.

- [ ] **Step 4: Delete the add-on backend module and validate Rust**

  Delete `src-tauri/src/addons.rs`.

  Run from `src-tauri`: `cargo fmt --check`

  Run from `src-tauri`: `cargo test`

  Expected: formatting and tests pass; path traversal tests execute. If Cargo is unavailable, report that limitation and use targeted source searches to confirm there are no `crate::addons` references or add-on Tauri handler registrations.

### Task 3: Remove Built-In Add-on Assets, Build Pipeline, and Active Documentation

**Files:**

- Modify: `package.json`
- Modify: `.gitignore`
- Modify: `README.md`
- Modify: `CONTRIBUTING.md`
- Modify: `scripts/capture-screenshots.ts`
- Delete: `scripts/build-addons.ts`
- Delete: `addons/README.md`
- Delete: `addons/catalog.json`
- Delete: `addons/dist/discord-rpc.zip`
- Delete: `addons/dist/modrinth-browser.zip`
- Delete: `addons/dist/skin-3d-viewer.zip`
- Delete: `docs/images/obsy-addons.png`

**Interfaces:**

- Consumes: The removal scope in the approved spec and remaining launcher features/documentation.
- Produces: No add-on build target, bundled add-on catalog/archives, screenshot capture, development guide, or active README/contribution instructions for add-ons.

- [ ] **Step 1: Remove the add-on build command and ignore exceptions**

  Remove only the `build:addons` script from `package.json`; remove only the add-on source/dist/catalog exception lines from `.gitignore`. Do not regenerate or replace the already-dirty `package-lock.json`.

- [ ] **Step 2: Remove add-on screenshot capture and repository artifacts**

  Remove the `obsy-addons.png` capture from `scripts/capture-screenshots.ts`. Delete the exact add-on guide, catalog, three bundled ZIP archives, screenshot, and build script listed above. Do not delete directories or files under the user's Minecraft data directory.

- [ ] **Step 3: Update active documentation in both README languages**

  In `README.md`, remove the add-on guide links, feature bullets, marketplace tour sections/images, build-addons setup steps, and add-on development guide sections. Keep the English and Russian setup instructions accurate and preserve descriptions of supported Minecraft mod loaders and mods.

- [ ] **Step 4: Remove add-on contribution guide sections**

  In `CONTRIBUTING.md`, remove the English and Russian add-on development sections and links, retaining general contribution, setup, and code-style guidance.

- [ ] **Step 5: Validate documentation and build configuration**

  Run: `npm run build`

  Search active files (excluding the removal spec/plan and git history) for `build:addons`, `addons/README.md`, `obsy-addons.png`, and user-facing add-on development/marketplace references.

  Expected: build succeeds; no active instructions or scripts refer to the deleted feature.

### Task 4: Integrated Acceptance Check

**Files:**

- Verify only: all frontend, Rust, build, and documentation files from Tasks 1–3.

**Interfaces:**

- Consumes: The completed frontend, backend, and repository cleanup from Tasks 1–3.
- Produces: Evidence that add-on functionality is absent and existing Minecraft instance archive operations remain intact.

- [ ] **Step 1: Search for remaining active add-on implementation references**

  Search `src`, `src-tauri/src`, `scripts`, `package.json`, `README.md`, `CONTRIBUTING.md`, `.gitignore`, and `docs/images` for add-on UI/runtime imports, command names, catalog/build references, and slots. Ignore historical Git data and the specification/plan describing their removal. Retain only the legacy data migration reference that copies old user files without loading them.

- [ ] **Step 2: Run final frontend and Rust verification**

  Run: `npm run build`

  Run from `src-tauri`: `cargo fmt --check`

  Run from `src-tauri`: `cargo test`

  Expected: frontend build succeeds; Rust formatter and tests pass if Cargo is installed. Confirm `extract_instance_zip_folder` still calls `crate::fs_utils` and its path traversal tests pass.

- [ ] **Step 3: Check working-tree scope**

  Run: `git diff --check`

  Review `git status --short` and the changed-file list. Preserve unrelated pre-existing user changes and leave all implementation changes uncommitted.
