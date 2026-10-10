# Vials

Tidy desktop containers for Windows. A **vial** is a small window on your desktop that holds
shortcuts, apps and folders. Items you put in a vial disappear from the loose desktop and live in
the vial instead; take them out (or quit Vials) and they come back exactly where they were.

## What it does

- **Vials on the desktop.** Drag the title bar to move one, collapse it to just its title, double
  click an item to open it, press × to take an item out. Vials sit below your other windows and
  never take focus or show up in Alt+Tab.
- **Your files are never moved.** Items stay in your Desktop folders. A vial only refers to them
  and hides them on the desktop view (parked off-screen, always put back on exit, also after a
  crash: the next start restores them).
- **Auto-sort rules.** Optional rules (by extension, kind or name pattern) send new desktop items
  to a vial. Items you took out by hand are remembered and not sorted again.
- **Works alone or inside Desqu.** The standalone app and [Desqu](../desqu/README.md) share the
  same settings; only one of them runs at a time.

Windows 10/11 with WebView2. Windows only for now.

## Known limits

- Hiding uses the desktop's own icon view. It is verified on this project's test machine only;
  see [docs/desktop-research.md](docs/desktop-research.md) for what was observed and what was not.
- The desktop view takes about 4 seconds to notice a new file, so a new item can flash on the
  desktop before it moves into its vial.

## Develop

```powershell
task vials:dev      # run locally
task vials:ci       # checks, tests, clippy
pwsh apps/vials/scripts/smoke-test.ps1   # real-desktop smoke test (creates one temp file)
```

Manual checks that need a human are in [docs/manual-testing.md](docs/manual-testing.md).
