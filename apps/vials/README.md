# Vials

Tidy desktop containers for Windows, in the spirit of Stardock Fences. A **vial** is a named,
semi-transparent box on your desktop that holds shortcuts, files and folders. Items you drop into a
vial disappear from the loose desktop and live in the vial instead; take them out (or quit Vials)
and they come back exactly where they were.

## What it does

- **Make one right on the desktop.** Drag a selection box over empty desktop space (the same
  gesture you use to select icons). If it selects nothing, a small menu pops up at the cursor:
  *Create vial here*. The vial appears exactly where you dragged. Turn this off in Settings.
- **Or right-click the desktop.** *Create vial here* in the desktop's right-click menu makes a vial
  with its corner where you clicked (Windows 11: under *Show more options*). The entry exists only
  while Vials runs.
- **Drag icons in and out like Explorer.** Drag shortcuts, files and folders from the desktop onto a
  vial. Drag one out of a vial and drop it on the desktop (it lands where you drop it), on another
  vial, or in a folder: it is a normal Windows drag with the icon under the cursor; Ctrl, Shift and
  Alt choose copy, move or shortcut, Esc cancels. Click a vial's name on the desktop to rename it.
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

## Looks

Settings has a *Look of vials* choice: **Glass** (default; blurred desktop behind the vial, rounded
corners, light edges) or **Classic** (a plain dark panel). Glass has two knobs: *Glass blur* (Frosted =
Windows acrylic, Clear = plain blur without grain) and *Glass tint* (0 clearest to 100 nearly solid,
default 20). Changing any of them restyles the open vials in place.

## Known limits

- Vials start empty: nothing is created until you make a vial.
- The drag-a-box gesture needs a system-wide mouse hook that only looks at left-button presses and
  releases that start on the desktop icon view. It works only while Vials is on.

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
