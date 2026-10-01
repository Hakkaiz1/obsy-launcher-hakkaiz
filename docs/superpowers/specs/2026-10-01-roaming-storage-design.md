# Roaming Storage Design

## Goal

Keep the installed launcher executable in its current installation location,
but store the DBC Super Minecraft files in a short, user-accessible directory:
`%APPDATA%\DBC Super Launcher` on Windows. Game files such as `mods`, `config`,
`saves`, and `resourcepacks` must be directly under that directory rather than
under `.obsy\technic\1132904\game`.

## Storage layout

The application data root is `dirs::data_dir()/DBC Super Launcher` (Roaming
AppData on Windows). The game root is the application data root itself.

```text
%APPDATA%\DBC Super Launcher\
  mods\
  config\
  saves\
  resourcepacks\
  options.txt
  java\                         # Managed Java installations
  launcher\
    launcher_state.json
    profiles.json
    profiles.key
    playtime.json
    wardrobe.json
    technic\
      1132904\
        manifest.json
        runtime\                # Managed Forge profile and JAR
        staging/backup data
```

All release-build game launches use the game root above as their working
directory. The launcher state, encrypted profiles and their encryption key,
playtime, wardrobe, Technic manifest, managed Forge runtime, and transactional
update files stay under `launcher/` so they do not pollute the visible game
directory. The managed Java runtime is stored in `java/`. The executable is not
moved.

On non-Windows platforms, `dirs::data_dir()` remains the platform data
directory. Debug builds retain the current repository-local `.obsy` location
to avoid making development builds operate on the installed user's data.

## Technic installation and update behavior

The Technic archive's game entries are installed directly into the game root.
The existing managed-file manifest continues to identify managed game entries
and internal Forge runtime entries; path resolution maps these two classes to
their respective roots. Personal files, including saves, screenshots,
shaderpacks, crash reports, logs, `options.txt`, and `servers.dat`, remain
excluded from pack ownership and are preserved across updates. The pack
manifest, staging, backup, and rollback files live under
`launcher/technic/1132904/`, not in the visible game directory.

Launch, pack-status, update, resource-pack-directory creation, Forge classpath,
Java discovery, and the "open folder" action must all use the new path
contracts consistently. "Open folder" opens the game root.

## Migration

At startup, migrate the existing release data from
`%LOCALAPPDATA%\DBC Super Launcher\.obsy` into the Roaming layout. In debug
builds, keep the existing local `.obsy` data location.

Migration copies and merges data; it never deletes the old source. It moves the
contents of `technic/1132904/game/` to the game root, the old pack runtime and
manifest to `launcher/technic/1132904/`, `jre/` to `java/`, and launcher-owned
JSON and `profiles.key` into `launcher/`. Both `profiles.json` and
`profiles.key` must be retained together so existing credentials remain
decryptable. Other game data already in the old root is preserved in its
corresponding destination.

Migration does not overwrite existing destination files. When a destination
file already exists, it remains authoritative; the migration proceeds with
other files and validates the resulting managed pack manifest. If the manifest
cannot safely describe the resulting pack, normal Technic update/install
reconciliation must restore managed pack files without deleting personal data.
The migration-complete marker is written only after the copy/merge succeeds.
Filesystem failures are surfaced and must not be converted into a successful
migration or leave the source deleted.

## Error handling and compatibility

If the platform data directory cannot be resolved or required directories
cannot be created, startup or the affected operation reports an actionable
error; it must not silently fall back to the executable directory. Existing
legacy data stays intact after any failed or partial migration, and the next
startup can safely retry. Repeated migrations are idempotent.

The launcher remains dedicated to Technic pack ID `1132904`; this change does
not add arbitrary install locations or move the launcher executable.

## Verification and acceptance criteria

- Unit tests verify the release data root, debug data root, game root, launcher
  data root, Java root, and managed Technic runtime/manifest paths.
- Migration tests verify legacy layout transformation, preservation of saves
  and resource packs, joint preservation of profiles and key, no overwrite of
  destination data, retry after failure, idempotency, and retention of the
  legacy source.
- Technic installation/update tests verify archive game entries are installed
  directly at the game root, runtime entries and metadata stay under
  `launcher/`, and personal files survive updates.
- Launch-contract tests verify the Minecraft working directory, Java path,
  Forge classpath, and open-folder action use the new roots.
- Existing Rust tests/doctests, frontend contract tests, formatting, and
  production build continue to pass.
