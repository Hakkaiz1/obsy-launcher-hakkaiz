# Remove the Add-on System

## Goal

Remove the launcher add-on/plugin feature completely from the application and
repository. The launcher will no longer display, discover, load, install,
configure, update, or uninstall add-ons.

This change does not remove Minecraft mods, mod loaders, or instance archive
installation.

## Data preservation

Do not delete add-on directories or files already present in a user's Minecraft
data directory. They will no longer be read or executed by the launcher. This
change removes feature code and bundled repository assets only; it does not
perform cleanup of user data.

## Frontend

- Remove the add-ons button and plugin slots from the launcher header and
  dashboard.
- Remove add-on initialization from application startup and remove the
  dynamically exposed `window.Obsy` add-on runtime API.
- Delete add-on UI, store, catalog, loader, registry, types, and runtime type
  declarations that are exclusively used by the add-on system.
- Remove add-on-specific translation keys while preserving translations used
  by other launcher features.

## Native backend

- Remove the add-on module and its Tauri commands for catalog access, archive
  inspection, downloading, installation, local add-on storage, and
  uninstallation.
- Remove those commands from Tauri's invoke handler.
- Move path validation and safe ZIP extraction helpers, plus their relevant
  tests, into a neutral filesystem utility module. They are still needed by
  instance archive installation and must keep rejecting absolute paths and
  traversal.
- Keep instance installation, Minecraft migration, and unrelated archive
  handling unchanged.

## Build and documentation

- Remove the add-on build command and its build script.
- Remove the add-on catalog, bundled add-on archives, add-on development guide,
  and add-on screenshot from the repository.
- Update the English and Russian README feature lists, visual tour, developer
  setup, and links, and remove add-on contribution instructions.
- Preserve unrelated package dependencies and Tauri plugins unless code
  inspection confirms they are used exclusively by add-ons.

## Acceptance criteria

1. The launcher UI contains no add-on controls or plugin-provided slots.
2. Application startup does not initialize add-ons or expose a plugin runtime.
3. The Tauri invoke handler no longer exposes add-on-specific commands.
4. No add-on feature code, build script, bundled catalog/archives, or active
   documentation remains in the repository.
5. Safe path and ZIP entry validation continue to be exercised by tests and
   instance archive extraction retains its existing behavior.
6. No user data outside the repository is deleted or modified.
7. Frontend production build and Rust tests/build pass where the toolchains are
   available.

## Validation

- Search the repository for add-on runtime imports, commands, build targets,
  and active documentation references.
- Run the frontend production build.
- Run Rust formatting and tests/build for the Tauri crate when Cargo is
  available; otherwise report the toolchain limitation and validate the
  unaffected Rust call sites by search and review.
