# Manual test procedures

Run with `task dev` and keep the **Diagnostics** tab open. For isolated browser tests use a
throwaway profile so your real session is untouched, e.g.
`msedge.exe --user-data-dir=%TEMP%\sb-test --no-first-run https://www.youtube.com`.

For each case record result + date in PLAN.md / docs/fullscreen-research.md.

## Core flow (repeat for Chrome, Edge, Firefox)

1. Zone "75% Left", rule "Browsers" enabled, chrome mode Keep.
2. Play a video, enter fullscreen (video button, then separately F11).
3. Expect: activity log "Constrained to zone…", window occupies left 75%, remaining 25% usable.
4. Click a window in the remaining 25%, then click back on the video. Expect the zone to hold (no flicker loop, no "app refused").
5. Exit fullscreen (Esc). Expect "Fullscreen exited; released", browser back at its previous size/position/maximized state.
6. Repeat with chrome mode Hide.

## Companion panels (VLC)

1. VLC enabled on the Apps tab, fullscreen a video.
2. Move the mouse so the fullscreen controls appear. Expect them inside the zone, at the same
   relative spot they use on the full monitor (bottom-centre-ish), not on the rest of the screen.
3. Drag the controls out of the zone; expect them to stay there, also after they hide and
   reappear. Leave fullscreen and enter it again; expect them back inside the zone.
4. In a browser, open a menu or bubble near the zone edge while fullscreen; expect it untouched.
5. With a second browser window on the same monitor, expect that window untouched.

## Borderless gaming (windowed games and apps)

Setup: Rules tab → **+ Borderless rule**, set *Applies to* the game (turn it on in the Apps tab
first), *Fit to* **Whole monitor** or a zone, *Keep on top* off. Keep Diagnostics open: the
constrained table shows a **borderless** tag and the activity log explains every action.

A quick no-game check (also what `task screenbound:probe` automates with `winver.exe`): a rule for
`winver.exe`, open it → it loses its title bar and fills the monitor; **Release** → title bar and
exact position come back.

For each case record result + date in PLAN.md.

1. **Windowed game.** Start the game in windowed mode. Expect "Made borderless on monitor…", no
   title bar, the window fills the monitor, the taskbar is not painted over it once the game is
   focused. Quit the game from its menu; expect the entry to disappear ("Window closed while
   borderless") and no leftover change.
2. **Game that recreates its swapchain on resize.** The picture must stay correct after the move
   (no black frame or stretched image). If it re-asserts its own window size, expect at most four
   re-fits in three seconds and then "Application keeps changing its window; leaving it as it is".
3. **Zone target.** Use *Fit to* a zone. Expect the window to fill exactly that zone. Edit the zone
   size in the Zones tab; expect the window to re-fit immediately.
4. **Keep on top.** Turn it on; expect the game above other windows (alt-tab still works). Turn the
   rule off; expect it to drop back.
5. **Elevated game** (run as administrator while Screenbound is not). Expect a clear
   "Could not make window borderless: access denied…" entry and no change to the window.
6. **Exclusive fullscreen game.** Expect nothing to happen (it is not a window we can style) and no
   error loop.
7. **Release.** Press **Release** (or **Release all**): the frame and the original position
   return, and the window is not grabbed again until you edit the config or restart the game.
8. **Pause.** Untick **Enabled**: expect borderless windows to be released.
9. **Quit Screenbound** while a game is borderless: expect the game window restored to framed.
10. **Kill Screenbound** (`taskkill /f /im screenbound.exe`) while borderless, then start it again:
    expect "Restored borderless window left by a previous session".
11. **Both rule kinds.** A browser with a fullscreen rule and a game with a borderless rule at the
    same time: each follows its own rule; a borderless rule listed above a fullscreen rule never
    hides the fullscreen rule.
12. **Display change.** Unplug or change the resolution of the monitor while a game is borderless
    on *Whole monitor*: expect it to re-fit the new monitor size.

## Lifecycle

- Close the browser while constrained → "Window closed while managed", journal cleared.
- Close ScreenBound while constrained → browser returns to full-monitor fullscreen.
- Kill ScreenBound (Task Manager) while constrained, restart → "Restored window left constrained by a previous session", then re-constrained.
- Change the zone while constrained → window moves immediately.
- Disable ScreenBound (header toggle) while constrained → window released.
- "Release" button in Diagnostics → window released and not re-captured until it re-enters fullscreen.

## Monitors

- Two monitors, fullscreen on each; zone unpinned → applies on the fullscreen monitor.
- Secondary monitor left of / above primary (negative coordinates).
- Mixed DPI (100% + 150%).
- Zone pinned to monitor B while fullscreen happens on A.
- Unplug / change resolution while constrained → re-applied on the new geometry.
- Taskbar auto-hide on/off; zone "Relative to: Work area".

## Idle cost

- With no fullscreen windows, ScreenBound CPU in Task Manager should read ~0%.
- Drag windows around rapidly; CPU should stay low (events are coalesced per window).
