# Workspaces

Give each Windows virtual desktop its own desktop icons. Tie shortcuts, files and folders to a
virtual desktop and they show up only there; on your other desktops they are hidden.

## What it does

- **Icons per desktop.** An icon tied to a virtual desktop is shown on that desktop and hidden on the
  others. Icons nobody owns show everywhere. The Desktops tab shows how many items each desktop has.
- **Move icons with Ctrl+X and Ctrl+V.** Select icons on the desktop, press Ctrl+X, switch to another
  virtual desktop and press Ctrl+V on its desktop: the icons now belong to that desktop. (Explorer
  itself does nothing for a cut-and-paste into the folder the files are already in.) "Show on every
  desktop" on a card releases its items.
- **First run asks once.** The first time Workspaces opens it asks which virtual desktop all the
  items already on your desktop belong to (or to leave them on every desktop). Nothing to set up
  item by item.
- **New files join the desktop you are on.** A file that appears on the desktop while Workspaces is
  running is tied to the virtual desktop you are on (switch it off in Settings). Files that were
  already there stay on every desktop until you choose.
- **Nothing is moved or deleted.** Items stay in your Desktop folders. Hidden icons are parked
  off-screen and always put back: when you turn Workspaces off, quit, or after a crash on the next
  start. If a virtual desktop is removed, its items are shown everywhere again.
- **Works alone or inside Desqu.** The standalone app and [Desqu](../desqu/README.md) share the same
  settings; only one of them runs at a time. With [Vials](../vials/README.md) also on, an icon both
  want hidden stays hidden until both let go of it.

## Limits

- Windows lists virtual desktops only once you have more than one (Win+Tab, then New desktop).
- Switching is detected from the focused window moving to another virtual desktop (public
  `IVirtualDesktopManager` plus a WinEvent hook) and from the virtual desktop registry key. That this fires on a real switch
  has been observed only on one Windows build (see
  [docs/virtual-desktop-research.md](docs/virtual-desktop-research.md)); other builds are untested.
- Windows only for now.

## Develop

```powershell
task workspaces:dev                           # run locally
task workspaces:ci                            # checks, tests, clippy
pwsh apps/workspaces/scripts/smoke-test.ps1   # real-desktop smoke test (temp files only)
```

Quit any running Desqu or Workspaces first: only one process may run the module. Manual checks that
need a human: [docs/manual-testing.md](docs/manual-testing.md).
