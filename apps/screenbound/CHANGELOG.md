# Changelog

All notable changes to Screenbound. The release workflow publishes the section that matches the
version being released as the release notes and as the in-app update description.

## Unreleased

- Browser integration (optional): a Browsers tab for the Desqu Fullscreen browser extension, with per-site
  fullscreen rules, install links, and the connection status. Off by default until the extension is installed.
- Zone mode for browsers (experimental until verified on real browsers): a browser's page can be placed in
  a zone, with the tabs and toolbar clipped away.

### Added

- **Borderless windows**: a rule can make a windowed app (for example a game running in a window)
  borderless and fit it to a monitor or zone, then put everything back on release, exit or crash.
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
