<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="128" alt="PalworldModManager icon" />
</p>

# PalworldModManager

> A native macOS (Apple Silicon) mod manager for Palworld. Detects the game, injects the UE4SS loader, and manages mod import · install · enable/disable · update · profiles in one app.

<p align="center">
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/License-MIT-green.svg"></a>
  <img alt="Version" src="https://img.shields.io/badge/version-0.2.0-orange.svg">
  <img alt="Desktop" src="https://img.shields.io/badge/desktop-Apple%20Silicon-lightgrey.svg">
  <img alt="UE4SS" src="https://img.shields.io/badge/UE4SS-v0.2.0%20bundled-blue.svg">
</p>

<p align="center"><a href="README.ko.md">한국어</a></p>

---

## Requirements

- macOS, Apple Silicon (arm64)
- Palworld (Mac App Store / sandboxed build) installed
- Mods are supplied by you (this app is not a mod host)

## Install

1. Download `PalworldModManager.app` (or the DMG) from the release and move it to Applications.
2. The app is ad-hoc signed (not notarized), so the first launch is blocked by Gatekeeper. Allow it once:
   - Try to open the app; a block notice appears.
   - Go to System Settings > Privacy & Security.
   - Next to "PalworldModManager was blocked", click **Open Anyway**.
3. On launch the game is auto-detected. If it isn't found, set the game binary path in the app.

## Using mods

- Import mods with their original folder structure (`LogicMods` / `~mods` / `Scripts`) intact — after unzipping. Placement is decided from that structure.
- Prefer mods cooked as **IoStore** (`.pak` + `.utoc` + `.ucas`). GamePass-version mods are usually in this format. A single legacy `.pak` is auto-converted on import, but an IoStore original is more reliable.
- Auto-update only checks mods whose manifest carries an `updateURL`. Most distributed mods have none, so update them by re-importing.
- After a game update, the first Play may prompt for your admin password again to set folder permissions.

## Limitations

- macOS Apple Silicon (arm64) only.
- Some Windows-cooked cosmetic paks may not render due to shader-format differences (a property of how the mod was cooked, not the manager).
- The manager checks for updates (itself, the UE4SS runtime, and mods) and shows them in a panel, but the app itself can't self-install (ad-hoc signed) — it links to the release page, so update the app by re-downloading.

## License

[MIT](LICENSE) © h-taek
