# Changelog

<p align="center"><a href="CHANGELOG.ko.md">한국어</a></p>

Notable changes to this project. Versions follow the manager app version (`src-tauri/tauri.conf.json`) and are separate from the bundled UE4SS runtime version.

## [0.2.0] - 2026-07-16

### Added

- Unified update notifications. On launch the app checks for updates to itself, the UE4SS runtime, and installed mods in parallel (each check is failure-isolated), and surfaces any available update as a persistent card in the bottom-right of the Mods tab. Mods and the UE4SS runtime can be updated in place from their card; the app update opens its GitHub release page instead of auto-installing (the app is ad-hoc signed, so it cannot self-install). Re-check manually from the sidebar.

### Changed

- The mod list no longer shows a per-row "Update" badge — update notices are consolidated into the unified panel.
- Play screen: added two background clips and lightened the readability overlay so darker footage is no longer over-dimmed.

## [0.1.1] - 2026-07-05

### Fixed

- Game launch hang (black screen / "not responding") with log-heavy mods. The manager spawned the game with inherited stdio, so once the game's log output filled the unread pipe buffer (~64 KB) its `write()` blocked and the game froze — reproducible with a verbose mod (e.g. MinimapWidget) enabled. The manager now redirects the game's stdout/stderr to a log file (`~/Library/Caches/ue4ss-mac/palworld-launch.log`, same location as the terminal launcher), so log volume can no longer stall the game.

### Changed

- Bundled UE4SS runtime updated to **v0.2.1** (was v0.2.0). Fixes the FName setter guard so Blueprint logic mods (e.g. ModConfigMenu) actually load instead of failing at `GetAsset`. Manager app version and bundled UE4SS version remain independent.

## [0.1.0] - 2026-07-05

Initial public release. A native macOS (Apple Silicon) mod manager for Palworld.

### Added

- Auto-detect the game and run it with the UE4SS loader via DYLD injection (Play).
- Mod import: both Lua and pak mods. A single legacy pak is converted to IoStore (.pak/.ucas/.utoc) on import.
- Enable/disable/retract mods and manage profiles (working sets). Retract removes only manager-placed files; files placed by the user are preserved.
- Bundle direct staging: active mods are distributed into the game bundle's three folders (`Content/Paks/~mods`, `Content/Paks/LogicMods`, `Binaries/Win64/Mods`) at Play time. First run sets folder ownership to the user via one admin prompt.
- UE4SS runtime auto-update: checks GitHub releases and swaps the whole runtime folder — dylib, Lua infra (BPModLoaderMod, shared), and settings — as one unit. Bundles v0.2.0.
- Mod auto-update: only for mods whose manifest carries an `updateURL` (opt-in).
- ModConfig settings handling: the `.modconfig.json` original is kept in the writable container with only a symlink in the read-only bundle, so in-game saves succeed. User-saved values in the container are preserved across retract/redeploy.

### Known limitations

- macOS Apple Silicon (arm64) only.
- The app is ad-hoc signed (not notarized), so the first launch must be allowed via System Settings > Privacy & Security > "Open Anyway".
- Some Windows-cooked cosmetic paks may not render due to shader-format differences (a property of how the mod was cooked).
- The manager app itself has no self-update; a new version is installed by re-downloading.
