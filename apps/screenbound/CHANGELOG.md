# Changelog

All notable changes to Screenbound. The release workflow publishes the section that matches the
version being released as the release notes and as the in-app update description.

## Unreleased

### Added

- In-app updates: Settings shows the current version, checks for updates (shortly after start and
  daily) and installs them. Portable builds link to the download page instead.
- **Start with system**, **Start minimized to tray** and **Close to tray** settings.
- Tray menu item to check for and install updates.

### Changed

- Renamed from ScreenBound to Screenbound with a new settings location
  (`%APPDATA%\io.github.wolfskii.screenbound`). Existing settings are copied automatically; see the
  [migration guide](../../docs/migration-screenbound.md).
- Only one copy can control windows at a time; an older running copy is detected and never fought.

## 0.1.0

- First release: constrain fullscreen windows to a zone, per-app rules and groups, animations,
  tray icon with close-to-tray, start at boot.
