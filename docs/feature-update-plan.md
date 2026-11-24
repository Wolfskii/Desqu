# Feature update plan

The work list from [fences-gap-plan.md](fences-gap-plan.md) (what Stardock Fences 7 has that Vials lacks)
and its "Beyond Fences" ideas, as one tracked list. Follow it top to bottom; update the status and the
progress log in the same commit as the work.

## How to work this list

- **Status:** `[ ]` todo, `[~]` in progress, `[x]` done and checked as far as it can be without a person,
  `[!]` blocked (say why in the notes), `[-]` dropped (say why).
- **Done means:** it compiles, `cargo clippy` is clean, there are tests for the logic (unit tests, or a
  headless GPUI test for anything about layout and input), and the commit is made. Anything that needs
  a real desktop, a real drag or a second monitor is marked **hand test** and listed in the log until
  the owner has tried it.
- **Rules that stay:** no code loaded into Explorer; no calls into Explorer from inside a drag or drop
  (do the work after the drop returns); no launching Vials, Workspaces or Desqu with apps on against the
  real desktop from a session; a short one-line conventional commit per step; never push without being
  asked.
- **Where things live:** pure logic in `crates/*-core`, Windows calls in `crates/*-platform`, windows and
  views in `apps/*-gpui`. New logic goes in the pure crate first, with tests.
- Sizes: S = a day or less, M = a few days, L = a week or more.

## Phase 1: quick wins

| ID | Status | Item | Size | Notes |
|---|---|---|---|---|
| P1-01 | [x] | Quick-hide: double-click empty desktop hides or shows all icons and vials | S | Setting on/off; reuse `veil` and the desktop-press event. Hand test |
| P1-02 | [x] | Lock a vial (no move, resize, drop-out or rename) | S | `Vial.locked`; menu entry with check mark |
| P1-03 | [x] | Collapsed vial opens while the pointer is over it | S | Setting; short close delay |
| P1-04 | [x] | Theme "follow Windows" (light or dark by the Windows setting) | S | Watch `AppsUseLightTheme` |
| P1-05 | [x] | Per-vial look: colour, opacity, icon size, title alignment | M | Optional fields on `Vial`; "Look..." in the vial menu |
| P1-06 | [x] | More rule conditions: modified/created age, size, source folder; run rules now | M | `ItemMatcher` in vials-core, file facts from vials-platform |
| P1-07 | [x] | Keyboard in vials: arrows, Enter, F2, menu key, Home/End | M | Headless GPUI tests |
| P1-08 | [x] | Undo for vial changes (Ctrl+Z) | M | Engine keeps a short history |
| P1-09 | [x] | "New vial with these" for selected desktop icons | S | Offer in the create popup |
| P1-10 | [x] | Vial actions: open all, copy paths, zip, move to folder | S | On a vial or selection |
| P1-11 | [x] | Workspaces: desktop name overlay on switch | S | |
| P1-12 | [x] | Workspaces: jump hotkeys Win+Ctrl+1..9 | S | Check clashes with Windows |
| P1-13 | [x] | Screenbound: hotkey puts the focused window in the zone | S | |
| P1-14 | [x] | Screenbound: pause for exclusive-fullscreen games | S | |
| P1-15 | [x] | Desqu: diagnostics bundle ("Save a report") | S | No file names unless asked |

## Phase 2: the big Fences features

| ID | Status | Item | Size | Notes |
|---|---|---|---|---|
| P2-01 | [x] | Folder Portals: a vial that shows a folder (local, network, OneDrive) | L | `Vial.source`; watcher; breadcrumb; drag out copies/moves. Hand test with a share |
| P2-02 | [x] | Search and filter in a vial and in portals | M | Off the UI thread, cancellable |
| P2-03 | [x] | Peek: hotkey brings all vials above windows | M | Default Win+Shift+Space, configurable |
| P2-04 | [x] | Privacy mode: masked names and pictures; exclude vials from screen capture | M | `WDA_EXCLUDEFROMCAPTURE`. Hand test |
| P2-05 | [x] | Saved layouts, one per screen setup, switched on dock/undock | M | |
| P2-06 | [ ] | Recent files vial | M | Windows Recent list |
| P2-07 | [ ] | Cleanup rule: untouched items move to an Archive vial or folder | M | |
| P2-08 | [ ] | Note and link items | M | |
| P2-09 | [ ] | Wallpaper-derived vial colour | M | |
| P2-10 | [ ] | Workspaces: startup sets per desktop | M | |
| P2-11 | [ ] | Workspaces: wallpaper per desktop (Windows 10) | M | |
| P2-12 | [ ] | Screenbound: zone per app or group, several zones per monitor | M | |
| P2-13 | [ ] | Screenbound: zones per screen setup | M | |
| P2-14 | [ ] | Desqu: hotkey manager with clash warnings | M | |
| P2-15 | [ ] | Desqu: backup, restore and OneDrive sync of all settings | M | |
| P2-16 | [ ] | Desqu: one update check and installer for the suite | M | Needs signed feeds first |

## Phase 3: polish, reach, command palette

| ID | Status | Item | Size | Notes |
|---|---|---|---|---|
| P3-01 | [ ] | Tabs: merge vials into one window | L | |
| P3-02 | [ ] | Mica theme (Windows 11), fallback to Fluent | M | Check it works on borderless vial windows |
| P3-03 | [ ] | Languages: string tables, Swedish and Polish first | L | |
| P3-04 | [ ] | ARM64 builds | M | Needs an ARM device to try |
| P3-05 | [ ] | Accessibility: names for screen readers | L | Find out what GPUI 0.2 allows first |
| P3-06 | [ ] | Desqu: command palette (Alt+Space) | L | |
| P3-07 | [ ] | Desqu: first-run tour | M | |
| P3-08 | [!] | Workspaces: pin another program's window to a desktop | L | Undocumented Windows interface; only with a safe fallback |

## Phase 4: new tools in Desqu

Each is a new app Desqu can turn on and off, with its own page.

| ID | Status | Item | Size | Reuses |
|---|---|---|---|---|
| P4-01 | [ ] | Window layouts: remember and restore window places after dock, undock, wake | M | Screenbound engine |
| P4-02 | [ ] | Always on top (hotkey, border) | S | Screenbound engine |
| P4-03 | [ ] | Folder rules: Downloads and other folders sort themselves | M | vials-core rules |
| P4-04 | [ ] | Auto dark mode on a schedule or at sunset | S | |
| P4-05 | [ ] | Keep awake | S | Tray |
| P4-06 | [ ] | Monitor brightness (DDC/CI) | M | |
| P4-07 | [ ] | Focus dimmer | M | |
| P4-08 | [ ] | Desktop notes | M | Vial windows |

## Also open from the GPUI rewrite

| ID | Status | Item | Notes |
|---|---|---|---|
| R-01 | [ ] | Signed installers and update feeds | Needed before P2-16 |
| R-02 | [ ] | Dragging several desktop items into a vial shows one icon | Own overlay instead of Explorer's picture |
| R-03 | [ ] | Right-click selected desktop items to pick a workspace | Needs a shell extension: only on the owner's go-ahead |
| R-04 | [ ] | Remove the old Tauri app folders and their docs | On the owner's go-ahead |
| R-05 | [ ] | Try on a real desktop: Task View icon hiding, "Paste on this workspace", dark item menus, Desqu settings pages | Hand test |

## Progress log

Newest first. One line per finished item: the ID, the commit, what was checked, what is left for a hand test.

- P2-05 saved layouts: vial menu "Layouts..." saves where the vials sit ("Layout N") and brings back or deletes a saved one; the program also remembers the places per screen setup (monitor names, sizes, arrangement) and, when you dock or undock, puts the vials where they were the last time you had that setup. Locked vials stay. Tests: the setup key, save/restore/delete, docking back and forth. Hand test: really docking and undocking.
- P2-04 privacy mode: setting "Privacy mode" hides names and pictures of items until the pointer is over the vial; setting "Hide vials from screen sharing" keeps vials out of shares, recordings and screenshots (SetWindowDisplayAffinity, Windows 10 2004+). Tests for the setting and the masking rule. Hand test: with Teams, OBS or the Snipping Tool.
- P2-03 peek: a hotkey (off by default; Win+Shift+Space or Ctrl+Alt+V on the Vials page and in settings; Win+Space is Windows' keyboard layout key so it is not the default) brings every vial above the open windows; a click outside every vial or Esc puts them back; the hotkey again toggles. Tests for the hotkey text, the setting and the point-in-vials test; the topmost switch itself needs a hand test.
- P2-02 search in a vial: Ctrl+F or "Search..." in the vial menu opens a bar; words, .ext and kind:images/documents/videos/audio/archives/programs/shortcuts/folders filter at once; in a portal the folders below are searched too (off the UI thread, up to 300 hits, 6 levels, cancelled when you type on). Esc or x closes. Tests: query parsing and matching, the folder walk (depth, limit, cancel), a headless GPUI test of typing and Esc. Hand test: a large network folder.
- P2-01 folder portals: "New folder portal here..." (desktop box popup and vial menu) makes a vial that shows any folder (local, network, OneDrive): folders first, double-click or Enter goes in, < or Backspace goes up (never above its folder), the folder is re-read every 2 s, drops copy into it, dragging to the empty desktop copies there, Explorer's menu works, nothing is hidden or deleted. Tests: folder listing, never-above-root, a headless GPUI test that walks a portal. Hand test: a network share and OneDrive; very large folders are listed in one go (no paging yet).
- P1-15 "Save a report" on Desqu's Settings page: a zip on the desktop with versions, what runs, a summary of settings (options and counts, no file names) and the last 400 lines of each log, with the user folder hidden. Tests for hiding the folder, the tail, that summaries hold no names, and a real zip. Hand test: the button and the Explorer window that opens. Phase 1 is done.
- P1-14 games in exclusive fullscreen (Windows' D3D full screen state) are left alone (setting on by default, on the Screenbound page). Engine test with the fake platform: a fullscreen window is managed without a game and untouched with one. Hand test: with a real exclusive-fullscreen game.
- P1-13 zone hotkey: a hotkey (off, Ctrl+Alt+Z, Win+Shift+Z or Ctrl+Alt+Space, chosen on the Screenbound page) puts the window in front into the zone, once. New shared desqu-native-shell hotkey module (text to key combination, tested) for the later hotkey items; engine test with the fake platform. Hand test: the key on real windows.
- P1-11 desktop name note when you switch (setting "Show the desktop's name", on by default): a dark panel in the middle of the screen for 1.3 s that takes no focus and lets clicks through. Hand test: the look and position, on more than one monitor.
- P1-12 Win+Ctrl+1..9 jumps to desktop N (setting, off by default): the jump is Windows' own Win+Ctrl+Left/Right pressed the right number of times (plan tested in workspaces-core). Hand test: the keys on a PC with several desktops; check no other tool uses Win+Ctrl+number.
- P1-10 vial actions in the vial menu: Open all (up to 20), Copy paths (text), Zip to the desktop (PowerShell Compress-Archive, name made safe and unique) and Move to folder... (folder picker, then Explorer's own move, items leave the vial). Tests for the zip (real PowerShell on temp files) and the name helpers. Hand test: the folder picker and the move.
- P1-09 "New vial with these": a box dragged over desktop icons can offer a vial that takes them (setting "Offer a vial for selected icons", off by default so selecting icons stays quiet). New read-only desqu-desktop::selected_items (checked against the real desktop, ignored test). Hand test: the popup and the vial that takes the icons.
- P1-08 undo for vials: Ctrl+Z in a vial and "Undo" in the vial menu take back the last 30 things you did (drops, drags out, new/deleted/renamed vials, sorts, rules, look, lock); not window moves, settings, collapse or automatic sorting. Unit tests for the history and for restoring content while keeping places and settings. Hand test: undo of a drag into a vial and of a deleted vial.
- P1-07 keyboard in vials: arrows, Home/End, Shift+arrows, Enter opens, F2 renames the vial, Esc clears, menu key or Shift+F10 opens the item menu. Unit tests for the movement (vials-core nav) and a headless GPUI test of the keys. Hand test: the item menu from the keyboard.
- P1-06 more rule conditions: changed in the last N days, not changed for N days, created in the last N days, bigger/smaller than N MB (in the rules window), plus "Sort desktop items now". Unit tests with a fake clock and file facts. 'Came from folder' dropped: desktop items always sit in the Desktop folder. Hand test: a rule on real files.
- P1-05 per-vial look: vial menu "Look..." sets icon size (24/32/48), panel colour (6 presets), how see-through (5/default/45/90) and the title in the middle. Unit tests for the model fields, fallbacks and the colour on a theme; old configs load. Hand test: how each looks on the desktop, esp. colour on Fluent and Liquid (Liquid ignores the colour).
- P1-04 theme "Follow Windows": Fluent dark or light by the Windows app setting, re-read about once a second while chosen. Tests for the setting name and that the theme resolves. The old Tauri Vials treats it as dark. Hand test: flip the Windows light/dark setting.
- P1-03 collapsed vials open on hover (setting "Open collapsed vials on hover", off by default; closes 350 ms after the pointer leaves). Setting covered by a unit test. Hand test: the opening and closing.
- P1-02 lock a vial: menu entry "Lock" (checked); a locked vial snaps back when moved or resized, cannot be renamed, dragged out or saved at a new place. Unit tests for the model (and old configs load unlocked). Hand test: moving, resizing and dragging out of a locked vial.
- P1-01 quick-hide: double-click empty desktop hides or shows vials and desktop icons (setting on by default, on the Vials page in Desqu and in Vials settings). Unit tests for the double-click timing and the setting. Hand test: the real double-click and the icons coming back.
- 2026-11-24: plan written.
