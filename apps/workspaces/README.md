# Workspaces

A different desktop setup for each Windows virtual desktop. Give a desktop its own set of visible
desktop icons, and save the apps and folders you use there so you can open them again in one click.

## What it does

- **Icons per desktop.** For each virtual desktop you choose which desktop icons to hide while it
  is the current one. Switch desktops and the icons follow. Files are never moved or deleted; hidden
  icons are parked off-screen and always put back (on exit, and after a crash on the next start).
  Files you add later stay visible.
- **Save what is open.** "Save open windows" remembers the apps and Explorer folders open on a
  desktop. "Launch here" starts the ones that are not open yet.
- **Works alone or inside Desqu.** The standalone app and [Desqu](../desqu/README.md) share the same
  settings; only one of them runs at a time. With [Vials](../vials/README.md) also on, a shared icon
  stays hidden until both let go of it.

## Limits (by design)

- **Windows does not let an app move other apps' windows between virtual desktops**, so Workspaces
  cannot gather windows for you. "Launch here" opens things on the desktop you are on: switch to the
  desktop first. See [docs/virtual-desktop-research.md](docs/virtual-desktop-research.md).
- Windows lists virtual desktops only once you have more than one (Win+Tab → New desktop).
- Saved command lines come from running processes of your own user; elevated apps cannot be read.
- Verified on Windows 11 build 26300 only. Other builds are untested.

Windows only for now.

## Develop

```powershell
task workspaces:dev                                   # run locally
task workspaces:ci                                    # checks, tests, clippy
pwsh apps/workspaces/scripts/smoke-test.ps1           # real-desktop smoke test (one temp file)
cargo test -p workspaces-module -- --ignored real_running   # real-window test
```

Manual checks that need a human: [docs/manual-testing.md](docs/manual-testing.md).
