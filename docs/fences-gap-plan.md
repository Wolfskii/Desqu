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
