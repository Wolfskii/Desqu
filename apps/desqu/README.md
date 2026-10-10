# Desqu

One app for a growing set of small Windows utilities. Turn each one on or off from a single place,
choose which ones start with Windows, and keep everything in one tray icon.

## What it does

- **One window, many apps.** The Home page has a card per utility with an on/off switch, a
  *Start with system* switch and an *Open* button. A master switch starts all of them with Windows.
- **Truly off means off.** Turning an app off stops it completely: no background threads, no hooks,
  and anything it changed (for example constrained windows) is put back.
- **Shares settings with the standalone apps.** If you also have an app installed on its own
  (for example Screenbound), both read the same settings. Only one of them runs at a time; if the
  standalone copy is running, Desqu offers to take over from it.
- **One tray icon.** Show Desqu, switch apps on and off, pause or release Screenbound's windows,
  check for updates, exit.
- **Updates** are checked shortly after start and then daily; installing is one click.

## Apps

| App | What it does | Status |
| --- | --- | --- |
| [Screenbound](../screenbound/README.md) | Keeps fullscreen windows inside a zone of your monitor | Included |
| Vials | Desktop containers for shortcuts, apps and folders | Planned |
| Workspaces | Saves and restores a desktop layout per virtual desktop | Planned |

Windows 10/11 with WebView2 (preinstalled on Windows 11). Windows only for now.

## Install

Download one of these from a release:

| File | Use it when |
| --- | --- |
| `Desqu_*_x64-setup.exe` | You want a normal install (recommended) |
| `Desqu_*_x64.msi` | You deploy with MSI tooling |
| `Desqu_*_x64-portable.exe` | You want to run it without installing |

Windows SmartScreen may warn on first run because the installers are not code-signed
(*More info → Run anyway*).

## Where things are stored

| What | Where |
| --- | --- |
| Which apps are on, and which start with Windows | `%APPDATA%\io.github.wolfskii.desqu\modules.json` |
| Desqu's own settings (start minimized, close to tray, updates) | `%APPDATA%\io.github.wolfskii.desqu\shell.json` |
| An app's own settings (e.g. Screenbound's zones) | the standalone app's folder, e.g. `%APPDATA%\io.github.wolfskii.screenbound` |

Uninstalling a standalone app with **Delete app data** checked removes the settings Desqu shares
with it.

## Build from source

Prerequisites: Rust (stable, MSVC), Node 20+, [Task](https://taskfile.dev), WebView2.

```powershell
task install          # once, from the repository root
task desqu:dev        # run locally
task desqu:release    # NSIS + MSI + portable exe into dist/desqu/
```

See [../../docs/architecture.md](../../docs/architecture.md) for how the pieces fit together and
[../../docs/releasing.md](../../docs/releasing.md) for releases.
