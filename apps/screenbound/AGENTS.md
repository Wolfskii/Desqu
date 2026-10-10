# AGENTS.md — engineering rules for Screenbound

Read this and [PLAN.md](PLAN.md) before changing code. PLAN.md is the source of truth for status.
Repo-wide rules are in [../../AGENTS.md](../../AGENTS.md).

## What Screenbound is

A native window-behavior manager. MVP: when a window enters fullscreen, constrain the *real native
window* to a user-defined zone of the monitor. Not browser PiP, not an overlay, not fake fullscreen.

## Architecture (keep these boundaries)

```
Svelte UI (src/)              config, visualization, diagnostics — no window logic
  │ invoke / events (src/lib/api.ts)
Tauri commands (src-tauri/src/commands.rs)   thin forwarding only
  │ EngineHandle::call (channel → single engine thread)
Engine (src-tauri/src/engine/)               state machine: detect, apply, restore, journal
  │ PlatformWindowManager trait
Core (src-tauri/src/core/)                   pure logic: geometry, zones, rules, heuristics, config
Platform (src-tauri/src/platform/windows/)   Win32 only: monitors, window state/styles, WinEvent hooks
```

- `core/` must never call native APIs. Anything decidable from data belongs here, with unit tests.
- The engine is single-threaded and owns its state; talk to it through `EngineHandle`.
- No app-specific branches (`if chrome`). Apps are targeted by `WindowRule` matchers in config.
  A native workaround is acceptable only when generic (e.g. `SWP_NOSENDCHANGING`) and documented.

(Paths above are from before the module split; they move under `module/` in Phase 2.)

## Native rules

- Event-driven only (WinEvent hooks + display broadcasts). No periodic polling of windows.
- Physical pixels everywhere (process is Per-Monitor-V2 DPI aware). Never assume monitor origin,
  ordering, sign of coordinates, equal DPI, or that fullscreen means the primary monitor.
- Capture state before modifying a window; journal it to disk; restore on release/exit/next start.
- External windows vanish at any time: return `PlatformError`, never panic.
- Never modify a window you cannot re-validate (pid + process start time).

## Done means tested on real Windows

A feature is not "done" because it compiles or has unit tests. Native behavior must be exercised
against real applications (see [docs/manual-testing.md](docs/manual-testing.md)) and findings
recorded in PLAN.md / [docs/fullscreen-research.md](docs/fullscreen-research.md).

## Commands

Run from the repo root as `task screenbound:<name>`, or from this folder as `task <name>`.
`task --list` prints these.

| Task | Command |
| --- | --- |
| Install npm dependencies (root workspace) | `task install` (repo root) |
| Run app (dev) | `task screenbound:dev` |
| Verbose logs | `task screenbound:dev:logs` |
| Rust unit tests | `task screenbound:test` |
| Desktop probe (read-only, real monitors/windows) | `task screenbound:probe` |
| Resize one window and print bounds | `task screenbound:probe:resize PID=<pid>` |
| Lint | `task screenbound:lint` |
| Frontend type check | `task screenbound:check` |
| Typecheck, test, and lint | `task screenbound:ci` |
| Release build (NSIS + MSI + portable → `dist/screenbound/`) | `task screenbound:release` |
| Install that release for the current user | `task screenbound:deploy` |

`scripts/probe-resize.ps1 -ProcessId <pid> [-NoSendChanging]` resizes one process's main window and
prints how its bounds evolve — useful for checking whether an app vetoes external resizes.

## Conventions

- Keep comments short; explain *why*, not *what*.
- Config lives in `%APPDATA%\io.github.wolfskii.screenbound\config.json` (zones, rules, seen apps); journal in
  `managed-windows.json`. The NSIS uninstaller leaves that folder in place unless **Delete app data**
  is checked.
