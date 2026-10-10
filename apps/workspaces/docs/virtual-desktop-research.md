# Virtual desktops: what an unelevated app can do (P9.1)

Spike: [`crates/desqu-desktop/examples/spike_vdesktop.rs`](../../../crates/desqu-desktop/examples/spike_vdesktop.rs).
Run it with `cargo run -p desqu-desktop --example spike_vdesktop`. It only reads, except for two
windows it creates itself (a test window and a `winver.exe`); it never creates, removes, renames or
switches the user's desktops and never moves a window it did not start.

## Where this was observed

| Build | Result |
| --- | --- |
| Windows 11 Pro, build 26300 (this machine, 2 virtual desktops) | Everything below |
| Windows 10 22H2, Windows 11 23H2, Windows 11 24H2 | **Not tested.** Needs a human on those builds; run the spike and compare |

Nothing here is claimed for any other build. The undocumented COM interfaces
(`IVirtualDesktopManagerInternal` and friends) change GUIDs between builds and were deliberately
**not used**, so the design below depends only on the registry layout and the public
`IVirtualDesktopManager`.

## Answers (build 26300)

| # | Question | Observed |
| --- | --- | --- |
| 1 | List desktop ids and names | Registry `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops`: `VirtualDesktopIDs` is a REG_BINARY of consecutive 16-byte GUIDs (GUID byte layout: first three fields little-endian). Names are in `...\VirtualDesktops\Desktops\{GUID}\Name` (REG_SZ). A desktop with no `Name` value is unnamed ("Desktop N"). **The `Desktops` subkey also keeps entries for desktops that no longer exist** (8 entries here vs. 2 live ids), so the live list must always come from `VirtualDesktopIDs`, never from the subkeys. Works unelevated, no undocumented API |
| 2 | Current desktop and switches, event-driven | `CurrentVirtualDesktop` (REG_BINARY, one GUID) sits in the **same key** on this build and always matched one of the listed ids. `RegNotifyChangeKeyValue` on the key (subtree, `REG_NOTIFY_CHANGE_LAST_SET`, async with an event) registers fine and stayed quiet while nothing happened. **Not observed: an event actually firing on a switch** (switching the owner's desktop was deliberately not automated). Needs a human to switch desktops with the spike's watcher or the Workspaces app. Where the value lives on other builds is untested |
| 3 | `IVirtualDesktopManager::GetWindowDesktopId` for other processes' windows | Works for ordinary top-level windows of other processes: 16 of 46 visible top-level windows answered, 0 errors from `IsWindowOnCurrentVirtualDesktop`. Reported ids were the two real desktop GUIDs plus `{00000000-0000-0000-0000-000000000000}` (a window shown on all desktops / not tied to one). The other windows answer `TYPE_E_ELEMENTNOTFOUND` (0x8002802B): tool windows and similar are unknown to the shell. **A window created as `WS_EX_TOOLWINDOW` stayed unknown for 3 s**, i.e. the shell does not manage it per desktop (relevant for Vials' windows, which are tool windows) |
| 4 | Moving a window to a desktop | Public `MoveWindowToDesktop` **works for a window owned by the calling process** (own window moved to the other desktop and back, `IsWindowOnCurrentVirtualDesktop` flipped to false and back) and **fails for a foreign window with `E_ACCESSDENIED` (0x80070005)**, as documented. The undocumented internal API was not used |
| 5 | Launch info of a running app | `QueryFullProcessImageNameW` (with `PROCESS_QUERY_LIMITED_INFORMATION`) gives the exe path of a same-user, unelevated process. `NtQueryInformationProcess(ProcessCommandLineInformation = 60)` returned the command line for the same kind of process (`"winver.exe"`). Elevated or other-user processes were not tested and are expected to fail |
| 6 | Open Explorer folders | `IShellWindows` + `IWebBrowser2::LocationURL` returns `file:///…` URLs of open folder windows (read from a window the spike opened and closed itself). Their desktop is available through `GetWindowDesktopId` on `IWebBrowser2::HWND`. Not tested: virtual folders (This PC, Recycle Bin) and tabbed Explorer on Windows 11 23H2+ (one HWND may host several tabs, so tabs may be invisible to this API) |

## Decision

The product idea is: desktop items (shortcuts, files, folders) belong to a virtual desktop and are
hidden on the others. That needs only questions 1 and 2, which work without any undocumented API.

1. **Registry only:** desktop ids and names from `VirtualDesktopIDs` and `Desktops\{GUID}\Name`,
   the current desktop from `CurrentVirtualDesktop`. Hiding goes through `desqu-desktop`.
2. **Switch detection is event-driven** through `RegNotifyChangeKeyValue`, re-reading
   `CurrentVirtualDesktop` after each signal. No polling. If the value cannot be read (unknown
   layout on another build) nothing is hidden and every item is shown, never guessed.
3. **Apps and windows are out of scope.** Answers 3 to 6 above are kept as research: moving other
   apps' windows is refused by Windows (`E_ACCESSDENIED`), so Workspaces does not try, and it does
   not save or relaunch apps or folders.
4. **Supported-builds list for anything beyond the registry: empty.**
