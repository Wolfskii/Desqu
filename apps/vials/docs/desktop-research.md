# Desktop icon layer: research (P8.1)

How Vials can show desktop items in its own windows and keep them off the desktop, answered by
**observation on one machine**. Everything below was measured with the throwaway examples in
[`crates/desqu-desktop/examples/`](../../../crates/desqu-desktop/examples/) (`spike_desktop`,
`spike_desktop_2`). They only ever move, hide and delete **a temporary file they create
themselves** on the desktop; the user's own items are read, never changed.

- **Machine:** Windows 11 26H2, build 10.0.26300.9550, four monitors forming an 8960 x 2160 virtual
  screen, all at 100 % DPI (system DPI 96), process set to Per-Monitor-V2.
- **Desktop:** 394 items (393 in the user's Desktop folder, 2 in the Public Desktop), align-to-grid
  **on**, auto-arrange **off**, "show hidden files" **off**. Wallpaper Engine is running (its windows
  are children of `Progman`).
- **Not covered:** Windows 10 22H2, Windows 11 23H2/24H2 and other DPI setups. Nothing here says
  those behave the same.

## Answers

### 1. Enumerating desktop items

`SHGetKnownFolderPath` gives `C:\Users\<user>\Desktop` and `C:\Users\Public\Desktop` (not
OneDrive-redirected here, but the API follows redirection). The desktop *view* shows both folders
merged plus virtual items (This PC, Recycle Bin), so the **view is the source of truth**, not the two
folders. An item's stable identity is its absolute parsing name (a file path, or `::{CLSID}` for a
virtual item); display names are not unique and hide extensions (`Steam ROM Manager` for a `.lnk`).

### 2. Positions through `IFolderView`

Reachable with `IShellWindows::FindWindowSW(CSIDL_DESKTOP, SWC_DESKTOP)` → `IServiceProvider` →
`IShellBrowser` → `IShellView` → `IFolderView2`. Item count, index → PIDL, `GetItemPosition`,
`GetCurrentFolderFlags` all work (flags here: `SNAPTOGRID | DESKTOP`, no `AUTOARRANGE`).

Writing with `SelectAndPositionItems(SVSI_POSITIONITEM)` **works but snaps**: asked for (316, 493),
got (320, 584), the nearest *free grid cell*. It is also clamped to the grid area: asking for
(30000, 30000) gave (5032, 1263), and (-6000, -6000) gave (16, 2). With auto-arrange on, expect the
write to be ignored (not tested).

### 3. Hiding options

| Option | Result |
| --- | --- |
| Move icons under the vial (`SelectAndPositionItems`) | Unreliable: snaps to free cells, and on a dense desktop the area under the vial is occupied by other icons. Rejected. |
| Park off-screen with `IFolderView` | Impossible: positions are clamped onto the grid (see 2). Rejected. |
| `FILE_ATTRIBUTE_HIDDEN` | Works (item left the view), but it edits the user's files on disk, and the view took ~4.4 s to react. Rejected unless nothing else works. |
| **Park off-screen with `LVM_SETITEMPOSITION` sent to the desktop `SysListView32`** | **Works.** The message packs x/y as signed 16-bit values in `lParam` (no pointers, so it works cross-process). Asked for (-3000, -3000) and (-20000, -20000); `IFolderView::GetItemPosition` read back exactly those values. Restoring the original position worked and read back correctly. The list-view index equals the `IFolderView` index. No file is touched. **Chosen.** |

Not observed: whether parked positions survive Explorer restart, sign-out or Win+D, and whether
Explorer persists a parked layout (it saves icon layouts in the registry). The design therefore
journals every park and restores on stop, on start and after a crash.

### 4. Where the vial window sits

Tested on a patch of desktop that no application window covered (found by `WindowFromPoint`):

| Placement | Result |
| --- | --- |
| **Bottom-most normal popup** (`WS_EX_TOOLWINDOW \| WS_EX_NOACTIVATE`, `HWND_BOTTOM`) | **Works.** Topmost window at the test point was the popup, 4/4 sampled pixels were its colour, only `Progman` is below it in z-order, and the foreground window did not change (no focus stolen). **Chosen.** |
| Child of `Progman` (the window hosting `SHELLDLL_DefView`), `HWND_TOP` | Hit-testable (the child was topmost at the point), but 0/4 sampled pixels were its colour: Wallpaper Engine's windows, also children of `Progman`, paint over it. Rejected. |

Not observed: behaviour under **Win+D** ("show desktop") and across virtual desktops. (My first
attempt used pixel sampling over an arbitrary rect and got a false negative because application
windows covered it; `WindowFromPoint` on a bare patch is what the table reports.)

### 5. Icon extraction cost

`SHCreateItemFromIDList` + `IShellItemImageFactory::GetImage(48 x 48, SIIGBF_ICONONLY)`: **200 icons
in 1154 ms (about 5.8 ms each; the first took 148 ms)**, 394 distinct items cycled. Fine if done off
the UI thread and cached by path and modification time; too slow to do synchronously for a whole
desktop on every refresh.

### 6. Change notifications

Event-driven options compared by creating a file on the desktop:

- **`ReadDirectoryChangesW` on the Desktop and Public Desktop folders: reported immediately (about
  0 ms).** Event-driven, independent of Explorer. **Chosen.**
- `SHChangeNotifyRegister` on the desktop folder (shell-level, all events, new delivery): registered
  fine but **no message arrived within 8 s**. Not used.

Important: the desktop *view* itself lagged the file system by **about 4 s** for both new and
hidden items (4074 ms and 4081 ms to appear, 4465 ms to disappear). After a folder notification the
layer must therefore retry resolving an item in the view a few times (a bounded, timer-based retry
for that one item, not polling of windows) before concluding it is not there.

### 7. Per-monitor DPI

All four monitors were at 100 % here, so **scaling effects were not observed**. What was seen: in a
Per-Monitor-V2 process `IFolderView` positions ran from x = 16..4728 and y = 2..2024 across the 8960 x
2160 virtual screen, i.e. they are plain desktop coordinates, and the list-view message and the
folder-view read-back agree. Non-100 % monitors need a manual check (see the Vials manual tests).

### 8. Explorer restart

`RegisterWindowMessage("TaskbarCreated")` returns a usable message id. Explorer was **not**
restarted (it would disturb the session). Design: on that message, re-resolve the view and list view
and re-apply the journaled parks; verified by hand before release.

## Decision

Vials implements decision D5 ("references + hide on desktop") like this:

1. **Items stay in the Desktop folders.** A vial stores references (absolute parsing names).
2. **Hide = park off-screen** with `LVM_SETITEMPOSITION` to a negative position, after writing the
   item's original position to `<data_dir>\desktop-journal.json` (write-ahead). **Show = restore**
   that position and remove the journal entry. No file attribute is ever changed.
3. **Recovery:** on start, stop, and after a crash, every journaled item that still exists is restored.
4. **The vial window** is a bottom-most, non-activating tool window per vial, placed at its rect.
5. **Events:** `ReadDirectoryChangesW` on both desktop folders, coalesced, with bounded retries for
   the lagging view; no periodic scanning.
6. **Icons:** extracted lazily off the UI thread, cached by path and modification time, sent to the UI
   as PNG data URLs.
7. **One owner:** a cross-process lock (`Local\Desqu.DesktopLayer`) so Vials and Workspaces never
   rearrange the desktop from two processes at once; inside one process (Desqu) they share the layer.

Known limits to put in front of users: if the desktop uses *auto-arrange*, positions cannot be
controlled and the layer reports that it cannot hide items (not tested here); a desktop with
thousands of items needs more time to list.

## Update 2026-10-15: parking no longer works, Vials takes over the desktop

**Observation.** On the same machine (Windows 11 26H2, build 10.0.26300.9550) the `LVM_SETITEMPOSITION`
parking measured above stopped working. Three updates were installed on 2026-10-15 (KB5121794,
KB5127647, KB5127648); I suspect one of them, but did not confirm that. `spike_desktop_2`, part D, now
reads: asked (-6000, -6000) -> reads back (14, 2); asked (11120, 8160) and (30000, 30000) -> reads back
(5030, 1226). The call still returns success, and `IFolderView` writes were already clamped before, so
there is no way left to place an icon off-screen. The app logs showed it as "N of N desktop items are
still on screen after 3 passes"; the failed attempts also left the icons pushed into the screen corners.
`FILE_ATTRIBUTE_HIDDEN` was not used: it edits the user's files.

**What PecoFence does** ([github.com/DayuanJiang/PecoFence](https://github.com/DayuanJiang/PecoFence),
`crates/platform/src/shell_icons.rs`): it never hides single icons. It switches off the whole icon layer
with the command Explorer's own "View, Show desktop icons" sends (`WM_COMMAND 0x7402` to
`SHELLDLL_DefView`; Explorer persists it) and draws every icon itself, with a built-in "Desktop" fence
for the items that are in no other fence. The desktop icons come back on exit and after a crash.

**Measured here** (`cargo run -p desqu-desktop --example surface_probe`): with the icon layer switched
off, `IFolderView` still lists every item with its position (4 before, 4 while off), the selection count
is 0, and switching back restores the list view. The system items (Recycle Bin, This PC) are listed too.

**Why not just PecoFence's way.** It switches the whole layer off and re-implements the desktop
(menus, drag and drop, selection, placement) in its own windows. The owner wanted the native desktop
as Stardock Fences leaves it, so that is an option here, not the default.

**Measured: the hidden attribute with an explicit shell notification** (`attr_probe` example, temp file
only, "show hidden files" off): `SetFileAttributesW(+HIDDEN)` then `SHChangeNotify(SHCNE_ATTRIBUTES,
SHCNF_PATHW | SHCNF_FLUSH)` removed the item from the desktop view after 0.46 s; clearing it brought it
back after 65 ms at the same position (14, 2). Without the notification the view needed about 4.4 s.

**Decision, default (native desktop).** Items in a vial (and items Workspaces hides on other virtual
desktops) get the hidden attribute and nothing else about the file changes. The journal records every
file before it is touched and whether it already was hidden (then its attribute is left alone); stop,
exit and the next start clear the marks. Explorer no longer lists a hidden item, so the layer lists it
from its journal (flagged `parked`) for the owners. Dropping an item on the desktop unhides it and moves
it to the drop point. System items (`::{CLSID}`, e.g. the Recycle Bin) have no file and cannot be hidden
(reported in `HideReport::failed`); the Public Desktop needs administrator rights, so those fail too. If
Windows is set to show hidden files (`HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced\Hidden = 1`)
hiding does nothing and `can_hide()` says so.

**Decision, optional (`drawDesktop`).** `desqu-desktop` has a *surface mode* (`DesktopLayer::set_surface`):

1. Vials (`drawDesktop`, default off) switches the icon layer off and draws the desktop. The built-in
   "Desktop" vial (`Vial.inbox`, id `desktop`) lists every item in no other vial, in the desktop's
   own column order; a vial's items show in the vial only.
2. In surface mode `hide`/`show` move nothing. They record per-owner *claims*; an item claimed by any
   owner is reported as `parked` and no surface draws it. Workspaces uses this to hide the items of
   other virtual desktops. Items parked before the switch are put back and keep their claims.
3. The journal records `iconsHidden` when *we* switched the layer off, so stop, exit and the next start
   switch it back on. If the user had the icons off already, nothing is recorded and nothing is undone.
4. If someone switches the icons back on while Vials takes over, Vials switches them off again: the
   Settings switch decides, not the View menu.
5. The mouse hook treats a press on the desktop's top-level window (`Progman`/`WorkerW`) as "on the
   desktop", since the hidden list view no longer receives it.
6. Parking icons off-screen was removed; journals it wrote are still understood and restored.

**Not observed.** Behaviour of the vial windows across virtual desktops (they are tool windows the
shell does not manage per desktop; whether they show on every desktop is unverified), Explorer restart
and sign-out while the layer is off, other Windows builds, and DPI other than 100 %. No marquee is
drawn while the icons are off (a dragged box still offers "Create vial here").
