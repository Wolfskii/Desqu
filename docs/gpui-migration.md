# Moving Vials (then Desqu) from Tauri to GPUI

Branch `spike/gpui-vials`. Why: see [spikes/gpui-vials/RESULTS.md](../spikes/gpui-vials/RESULTS.md) (about 170 MB -> 30 MB RAM, 10 processes -> 1).

## How it is cut

The UI is the only thing that changes. Everything that is not UI stays Rust and is shared:

| Piece | Where | State |
| --- | --- | --- |
| Model, rules, layout, sort (pure) | `crates/vials-core` | extracted, 49 tests, used by both UIs |
| Desktop layer (items, hide, icons) | `crates/desqu-desktop` | unchanged; new `icon_png()` needs no lock |
| Native Windows pieces (item menu, drag out, clipboard, mouse hook) | `crates/vials-platform` | moved out of the Tauri module, used by both |
| Workspaces (core, native pieces) | `crates/workspaces-core`, `crates/workspaces-platform` | moved out of the Tauri module, used by both |
| Tauri + Svelte Vials (current) | `apps/vials`, `apps/vials/module` | still shipping, now uses the shared crates |
| GPUI Vials | `apps/vials-gpui` (own workspace, so Tauri CI never builds GPUI) | phase 1 |

## Phases

| # | What | Status |
| --- | --- | --- |
| 1 | Read-only viewer: real config, real positions, real icons, blur, rounded, tool window, drag by header, collapse, double-click opens | done |
| 2 | Own engine for the GPUI app (not a refactor of the Tauri one, to keep it shipping safely): `--live` owns the desktop layer, hides items of vials, restores on exit or from the journal after a crash, saves window moves. Test: `spikes/gpui-vials/live-test.ps1` | done |
| 3a | Editing, first half: drop files in, take an item out (x on hover, Delete), drag out to Explorer and the desktop, Explorer's item menu (right-click), Ctrl+C / Ctrl+X, window takes the keyboard only while used | done, drop and drag-out still need a hand test |
| 3b | Editing, second half: the vial menu (right-click title or empty space: rename, sort by name/type/date/size, folders first, new vial, delete vial), rename by clicking the title, move by dragging the title, Ctrl+V paste, auto-sort by rules on desktop changes. Tested with real windows: `spikes/gpui-vials/ui-smoke.ps1` (moves your pointer for a few seconds) and the ignored engine tests | done |
| 4 | One running copy (a second launch hands `--action=...` to the first), tray icon with New vial / Quit, the desktop's "Create vial here" right-click entry, and dragging a box on empty desktop offers a vial there. Tested: `spikes/gpui-vials/action-test.ps1` (registry entry, tray window, create-vial and quit actions). The box-drag gesture needs the real mouse hook, so it needs a hand test; the tray icon is the stock Windows application icon for now | done, box-drag and icon look need a hand test |
| 5 | Settings window (theme, glass tint, four on/off options, opens the rules window), rules window (list, on/off, reorder, delete, add one condition; a small text field of our own), themes Glass / Classic / Fluent dark / Fluent light applied live. Tested: `settings-test.ps1`, `rules-test.ps1`; rules with several conditions made in the Tauri app are listed and can be reordered, switched off or deleted, but not edited | done |
| 6 | Liquid glass: GPUI has no shader hook, so the glass is computed on the CPU (`liquid.rs`): the wallpaper is read, and for each vial's rectangle the refraction at the rim, colour fringing, blur, tint and rim light are drawn per pixel (about 20 ms for a 480 x 320 vial) and shown as an image behind the content; redrawn after a move or resize, and when the wallpaper file changes. It does not show icons or windows behind a vial, only the wallpaper. Look: `spikes/gpui-vials/theme-shot.ps1` | done |
| 7 | Vials: installer (per-user NSIS, closes a running copy, clean uninstall), start with Windows (own Run entry, refused for debug builds), update check (reads a Tauri-style manifest and links to the download; it does not install), icon and version info in the exe. Shared by every GPUI app in `crates/desqu-native-shell` (single instance, tray with menu, actions from second launches, autostart, update check). Tested: `installer-test.ps1 -App vials`, `action-test.ps1` | done |
| 8 | Workspaces in GPUI (`apps/workspaces-gpui`): engine ported with injectable desktops, window with one card per virtual desktop, first-run question, auto-assign, tray, installer. Core and platform extracted to `crates/workspaces-core` and `crates/workspaces-platform`. Tested: engine test with two fake desktops on the real desktop, `workspaces-test.ps1`, `installer-test.ps1 -App workspaces` | done |
| 9 | Screenbound in GPUI (`apps/screenbound-gpui`): the engine is used as it is (the Tauri plugin is now an optional cargo feature of `screenbound-module`), with a runtime wrapper, a window with on/off, the zone drawn on a preview of the monitors or from presets, the Apps list, what is managed now and recent activity, tray, installer. Not in this window yet (the engine still applies them when they are in the config): groups, rules editing, the browser integration (extension bridge and native host), diagnostics, borderless-window settings, the old-install migration. Tested: 112 module tests, `screenbound-test.ps1` (paused temp config, so no real window is moved), `installer-test.ps1 -App screenbound` | done, minus the panels above |
| 10 | The Desqu collection shell that hosts Vials, Workspaces and Screenbound in one process with one tray icon and central settings | todo |

Do not delete the Tauri Vials before phase 5 is at parity. Both can run from the same crates.

## Known risks

- GPUI is pre-1.0 (0.2.x) and developed for Zed: expect API changes on upgrade, pin the version.
- Icons for `.url` shortcuts only load in a single-threaded COM apartment, so they are fetched on their own thread, not GPUI's pool.
- Text input (rename, search, settings) has no ready-made widget in GPUI.
