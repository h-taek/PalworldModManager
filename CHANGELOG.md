# Changelog

<p align="center"><a href="CHANGELOG.ko.md">한국어</a></p>

Notable changes to this project. Versions follow the manager app version (`src-tauri/tauri.conf.json`) and are separate from the bundled UE4SS runtime version.

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
