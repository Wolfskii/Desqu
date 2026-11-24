# Vials compared with Stardock Fences 7, and the plan to close the gap

Source: stardock.com product page, trial page and the Fences 7 release note (18 Nov 2026), read on
24 Nov 2026. The page does not cover everything Fences does. Items marked **(older Fences)** are from
earlier versions I know of and were **not confirmed** on the current page; check them in the trial
before building them.

## What Fences 7 is

- Groups of desktop items ("Fence groups") with their own size, colour, transparency, tabs, icon tint
  and corner style.
- Rules that sort new files into groups (by type, name, date or destination).
- **Folder Portals**: a group that shows any folder (local, network or cloud), with live search and
  filtering (new in 7).
- **Peek**: Win+Space brings the groups above open windows.
- **Quick-hide**: double-click empty desktop to hide or show all icons and groups.
- **Privacy mode** (new in 7): masks file names and icons until the desktop is in focus.
- Roll-up of groups; light and dark mode that follows Windows; Mica on Windows 11 (new in 7).
- Layout management, locked groups and positions, deployment and central activation for businesses.
- 11 languages; Windows 10, 11 and ARM; subscription from $10 (a year, it seems); free trial.

## Where Vials stands

| Area | Fences 7 | Vials today |
|---|---|---|
| Groups of desktop items | Yes | Yes (vials) |
| Create a group | Draw on the desktop | Box-drag popup, desktop menu, tray |
| Rules | Type, name, date, destination | Extension, kind, name (glob, contains, starts, ends), any/all combined. **No date, size or folder** |
| Sort inside a group | Yes | Name, type, modified, size; folders first |
| Roll-up | Yes, opens on hover **(older Fences)** | Collapse button; **no hover to open** |
| Per-group colour, transparency, icon tint, corners | Yes | **One look for all vials** (theme + tint) |
| Tabs (several groups in one) | Yes | **No** |
| Icon size | Per Folder Portal (per group **(older Fences)**) | **Fixed 32 px** |
| Light / dark follows Windows | Yes | **Manual** (Fluent light or dark) |
| Mica | Yes | **No** (acrylic blur, Liquid shader) |
| Folder Portals with search | Yes | **No** |
| Search / filter in a group | Yes (portals) | **No** |
| Peek (groups above windows) | Win+Space | **No** |
| Quick-hide (double-click desktop) | Yes | **No** |
| Privacy mode | Yes | **No** |
| Lock a group's position | Yes | **No** |
| Layouts, snapshots, per-screen-setup layouts | Layout management; snapshots and per-resolution layouts **(older Fences)** | Per-monitor placement only; **no saved layouts** |
| Desktop pages **(older Fences)** | Swipe between pages | Workspaces: icons per virtual desktop (covers the need) |
| Thumbnails of images | Windows' own icons | Yes |
| Languages | 11 | **English only** |
| ARM64 | Yes | **Not built** |
| Business deployment, central config | Yes | No (out of scope for now) |

What Vials has that Fences does not: icons per **virtual desktop** (Workspaces), Screenbound in the
same suite, the Liquid glass look, rules that combine conditions, and no subscription.

## Plan

Sizes: **S** = a day or less, **M** = a few days, **L** = a week or more. Each item lists where it goes
and how it is checked. Every item keeps the rules learned so far: no code inside Explorer, no
cross-process calls into Explorer from inside a drag or drop, and a test (unit or headless GPUI)
for any logic.

### Phase 1: quick wins (all S–M)

1. **Quick-hide** (S). Double-click on empty desktop hides all icons and vials; again shows them.
   The mouse hook already reports desktop presses and `item_at` tells empty from icon; hiding icons
   reuses the layer's `veil`, vials hide their windows. Setting on/off (default on). Check: a unit test
   for the double-click timing; by hand on the desktop.
2. **Lock a vial** (S). Menu entry "Lock" (checked): no move, no resize, no drop out, no rename.
   `Vial.locked` in vials-core. Check: unit test that a locked vial refuses moves in the engine.
3. **Hover to open a collapsed vial** (S). A collapsed vial opens while the pointer is over it and
   closes after it leaves (short delay). Setting on/off. Check: by hand.
4. **Follow Windows light/dark** (S). New theme choice "Fluent (follow Windows)": read and watch
   `AppsUseLightTheme`. Check: unit test for the choice; by hand by flipping the Windows setting.
5. **Per-vial look** (M). Colour (a few presets and a custom one), opacity, icon size (small, medium,
   large: 24, 32, 48 px) and title alignment, from the vial menu ("Look..."). Fields on `Vial`, all
   optional so old configs load. Check: config round-trip tests; headless GPUI test that icon size
   changes the item width.
6. **More rule conditions** (M). Modified or created older/newer than N days, size bigger/smaller
   than, and "came from folder" (Downloads, Screenshots). Plus "Run rules on existing items now".
   Lives in vials-core `ItemMatcher` (pure, tested); the file facts come from vials-platform.
7. **Keyboard in vials** (M). Arrow keys move the selection, Enter opens, F2 renames, Shift+F10 or
   the menu key opens the item menu, Home/End. Headless GPUI tests like the box-select ones.

### Phase 2: the big Fences features (M–L)

8. **Folder Portals** (L). A vial that shows a folder instead of desktop items: any local, network or
   OneDrive folder, with a breadcrumb to open subfolders and go back. Nothing is hidden on the desktop.
   Live updates through a folder watcher; drag out copies or moves like Explorer. `Vial.source:
   Option<PathBuf>` in vials-core; a portal module in vials-platform for listing and watching.
   "New portal..." in the desktop popup and vial menu. Check: unit tests for listing and paths; by hand
   with a network share and OneDrive.
9. **Search and filter** (M). Typing in a focused vial (or a search icon in its title) filters its
   items by name; portals also search subfolders (off the UI thread, cancellable). Filter by kind
   (images, documents, shortcuts). Check: tests for the filter logic; headless GPUI test for the box.
10. **Peek** (M). A hotkey brings all vials above open windows until Esc or a click outside. Win+Space
    is Windows' keyboard-layout switch, so the default is **Win+Shift+Space**, configurable, with
    Win+Space as an option. `RegisterHotKey`, then topmost and back. Check: by hand.
11. **Privacy mode** (M). Names and pictures in vials are blurred or replaced by placeholders until the
    pointer is over a vial (or always while a screen is shared). Extra beyond Fences: vials can be
    **left out of screen recordings and shares** (`SetWindowDisplayAffinity` with
    `WDA_EXCLUDEFROMCAPTURE`). Check: by hand with a Teams or OBS capture.
12. **Layouts** (M). Save the current vials (places, sizes, items) under a name, restore one, and keep
    one layout per screen setup (monitors and resolutions), switched on automatically when you dock or
    undock. Stored next to the config; restore goes through the engine so items hide and show
    correctly. Check: unit tests for the screen-setup key and restore plan.

### Phase 3: polish and reach

13. **Tabs** (L). Drag a vial onto another's title to merge them into one window with tabs; drag a tab
    out to split. `Vial.tab_group` in vials-core; one window renders several vials. Check: tests for
    merge and split; by hand.
14. **Mica** (M). A "Mica" theme on Windows 11 (`DWMWA_SYSTEMBACKDROP_TYPE`), falling back to Fluent on
    Windows 10. Needs a check that Mica works on the borderless vial windows at all.
15. **Languages** (L). Move every user-facing string of the four apps into one table per language and
    pick the Windows display language. Start with Swedish and Polish, then German, French, Spanish.
16. **ARM64 builds** (M). Add `aarch64-pc-windows-msvc` to packaging and CI; the GPUI apps have no
    x64-only code that we know of, but the desktop layer must be tried on an ARM device.
17. **Accessibility** (L). Screen-reader names for vials and items. GPUI 0.2 has little accessibility
    support, so this starts with finding out what is possible.

### Not planned

- Business deployment, central activation and network layouts: no business users yet.
- Desktop pages: Workspaces already gives each virtual desktop its own icons.
- Subscriptions and licensing.

### Still open from the GPUI rewrite (do alongside Phase 1)

- Signed installers and update feeds are not published.
- Dragging several desktop items into a vial shows one bare icon.
- Remove the old Tauri app folders and their docs.

## Order of work

Phase 1 items 1, 2, 3 and 4 first (each small and visible), then 6 and 5. In Phase 2, Folder Portals
(8) and search (9) together, then Peek (10), Layouts (12) and Privacy (11). Phase 3 after a release
with Phase 1 and 2.

## Beyond Fences: more ideas for the suite

Ideas that build on what the apps already do (the desktop layer, the window engine, virtual desktops,
the browser bridge, the tray). Not part of the phases above yet; pick from them after Phase 1.

### Vials

| Idea | What it does | Size |
|---|---|---|
| Undo | Ctrl+Z puts back the last move, removal, rename or delete of a vial. | M |
| Put selection in a new vial | Select desktop icons, then the popup offers "New vial with these". | S |
| Recent files vial | A built-in vial that lists the files you opened lately (Windows' Recent list). | M |
| Cleanup rule | Items untouched for N days move to an "Archive" vial or folder (the Downloads mess). | M |
| Note items | A plain text note or a web link as an item in a vial, without a file on the desktop. | M |
| Vial actions | "Open all", "Copy paths", "Zip these" and "Move files to folder..." on a vial or a selection. | S |
| Wallpaper colour | Vials take their tint from the wallpaper behind them (Fences calls it Chameleon). | M |

### Workspaces

| Idea | What it does | Size |
|---|---|---|
| Desktop name on switch | A short overlay with the desktop's name when you switch. | S |
| Jump hotkeys | Win+Ctrl+1..9 goes straight to desktop N. | S |
| Startup sets | A desktop can open a set of apps and folders the first time you go to it ("Work": VS Code, terminal, project folder). | M |
| Wallpaper per desktop | Different wallpaper per desktop on Windows 10 too (Windows 11 has it built in). | M |
| App always on desktop N | Moving another program's window between desktops needs Windows' undocumented COM interface, which changes between Windows builds. Only with a fallback that does nothing when the interface is not there. | L, risky |

### Screenbound

| Idea | What it does | Size |
|---|---|---|
| Zone per app or group | Different zones for different apps (games left, videos right), and more than one zone per monitor. | M |
| Zone hotkey | A hotkey puts the focused window into the zone (or makes it borderless) without a rule. | S |
| Zones per screen setup | Zones change by themselves when you dock, undock or plug in a monitor. | M |
| Pause for games | Leave exclusive-fullscreen games alone automatically. | S |

### Desqu itself

| Idea | What it does | Size |
|---|---|---|
| Command palette | Alt+Space (configurable) finds vial items, apps, desktops and Desqu actions ("new vial", "go to Work desktop", "release all windows"). Ties the apps together. | L |
| Hotkey manager | One page with every hotkey of every app, and warnings when two clash or Windows already uses one (PowerToys shows "shortcut conflicts"). | M |
| Backup and sync | Export and import all settings as one file; optional sync through a OneDrive folder. | M |
| One update for all | One update check and one installer run for the whole suite, with the changelog. | M |
| Diagnostics bundle | "Save a report": logs, versions and settings (no file names unless asked) in a zip to attach to a bug. | S |
| First-run tour | A short walk through the apps the first time Desqu opens. | M |

### New tools for Desqu

Small utilities in the same spirit (the desktop and its windows), each one an app Desqu can turn on
and off like the others.

| Tool | What it does | Reuses | Size |
|---|---|---|---|
| Window layouts | Remembers where every window was and puts them back after you dock, undock, wake the PC or a monitor reconnects. The most common multi-monitor annoyance. | Screenbound's window engine | M |
| Always on top | A hotkey pins the focused window above all others, with a thin coloured border so you see it. | Screenbound's window engine | S |
| Folder rules | The Vials rules for any folder: Downloads sorts itself into subfolders by type, name, age or size. | vials-core rules | M |
| Monitor brightness | Brightness of every monitor from the tray (DDC/CI for external ones), with a schedule. | Tray, monitor list | M |
| Auto dark mode | Switches Windows between light and dark at set times or sunset; vials follow. | Settings plumbing | S |
| Keep awake | Keeps the PC from sleeping for a set time or while an app runs. | Tray | S |
| Focus dimmer | Dims everything except the active window (or the Screenbound zone) while you work. | Screenbound overlays | M |
| Desktop notes | Sticky notes on the desktop that live with the vials, follow Workspaces and survive restarts. | Vial windows | M |

### Recommended next five

1. **Window layouts**: most daily value for multi-monitor users, and the engine already exists.
2. **Command palette**: makes the suite feel like one product.
3. **Folder rules**: the Vials rules are already written and tested; Downloads is where people need them.
4. **Always on top**: small, often asked for, and easy to show off.
5. **Hotkey manager**: needed anyway once Peek, the palette and the new tools add hotkeys.
