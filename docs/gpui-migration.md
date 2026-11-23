# Moving Vials (then Desqu) from Tauri to GPUI

Branch `spike/gpui-vials`. Why: see [spikes/gpui-vials/RESULTS.md](../spikes/gpui-vials/RESULTS.md) (about 170 MB -> 30 MB RAM, 10 processes -> 1).

## How it is cut

The UI is the only thing that changes. Everything that is not UI stays Rust and is shared:

| Piece | Where | State |
| --- | --- | --- |
| Model, rules, layout, sort (pure) | `crates/vials-core` | extracted, 49 tests, used by both UIs |
| Desktop layer (items, hide, icons) | `crates/desqu-desktop` | unchanged; new `icon_png()` needs no lock |
| Native Windows pieces (item menu, drag out, clipboard, mouse hook) | `crates/vials-platform` | moved out of the Tauri module, used by both |
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
| 5 | Settings window, rules window, themes (Glass, Classic, Fluent) | todo |
| 6 | Liquid glass: wallpaper shader needs a custom wgpu pass; GPUI does not expose one | todo, hardest |
| 7 | Installer, updater, autostart, then Screenbound / Workspaces / the Desqu shell | todo |

Do not delete the Tauri Vials before phase 5 is at parity. Both can run from the same crates.

## Known risks

- GPUI is pre-1.0 (0.2.x) and developed for Zed: expect API changes on upgrade, pin the version.
- Icons for `.url` shortcuts only load in a single-threaded COM apartment, so they are fetched on their own thread, not GPUI's pool.
- Text input (rename, search, settings) has no ready-made widget in GPUI.
