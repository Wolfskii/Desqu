# Architecture

Desqu is a repository of desktop utilities. Each utility is a library (a *module*) that can run on
its own inside a thin app, or together with the others inside the **Desqu** collection app. The code
is the same either way: the only difference is which process hosts the module.

```
apps/desqu          collection app: UI, settings, tray, take-over; hosts every module in one process
apps/<x>/src-tauri  standalone app for one module (thin shell)
apps/<x>/module     the module: Rust library + Tauri plugin (core → engine → platform)
apps/<x>/ui         the module's Svelte UI (a package both apps render)
crates/desqu-shell  shared shell: settings, args, single instance, autostart, tray, window
                    lifecycle, module lock, updater
packages/ui         shared UI kit: tokens, switches, settings and update cards
```

## Dependency rules

Enforced by `scripts/check-standalone-deps.ps1` in CI:

- A standalone app (`apps/<x>/src-tauri`) depends on its own module and on `crates/*`, never on `apps/desqu`.
- Module crates never depend on each other; shared code goes in `crates/`.
- Nothing depends on `apps/desqu`.

## A module

A module crate exports a Tauri **plugin** (`init(ModuleOptions)`) and a type implementing the shell's
`Module` trait (`id`, `start`, `stop`, `status`). Its Svelte UI talks to the plugin through
`invoke('plugin:<module>|<command>')`. The plugin name must equal the Cargo package name because the
permission namespace is derived from it (`screenbound-module:default`).

Inside a module, layers stay strict (see each app's AGENTS.md): UI → plugin commands → engine
(single-threaded state machine) → core (pure, unit-tested logic) and platform (native code only).

Module status is one of `off`, `paused`, `on`, `unsupported`, `ownedElsewhere`.

## One engine per module

Only one process may run a module's engine, or two copies would fight over the same windows. A named
mutex per module (`Local\Desqu.Module.<id>`) enforces that. A process that cannot get it reports
`ownedElsewhere` and does not start.

Because both the standalone app and Desqu read and write the **same settings folder**
(`%APPDATA%\<standalone identifier>`), switching between them loses nothing.

**Take-over.** When Desqu is asked to run a module that its standalone app is running, it asks the
user, runs `<standalone exe> --quit` (the standalone app restores its managed windows and exits),
starts the module itself and removes the standalone app's start-with-system entry.

## What "off" means

| App | Off |
| --- | --- |
| Screenbound | Engine stopped (windows restored, journal cleared) and the native event hooks removed: no module threads. |
| Vials / Workspaces | Planned: windows closed, hidden desktop items restored, watchers stopped. |

## Process lifecycle (shell)

- **Launch arguments:** `--autostart` (set by the start-with-system entry), `--quit` (ask a running
  instance to exit; exits at once if none is running).
- **Single instance:** a second launch never runs; its arguments go to the first one.
- **Window:** created hidden. It is shown unless the launch came from sign-in or *Start minimized* is on
  (and a tray icon exists). *Close to tray* hides instead of quitting.
- **Exit:** `RunEvent::Exit` makes every module restore what it changed before the process ends.
- **Autostart:** release builds use the product name as the Run value; debug builds use
  `<exe>-debug` so development never touches an installed app's start-up entry.

## Updates

Each product has its own signed feed (`updates/<product>/latest.json` on GitHub Pages) and its own
signing key. The shell downloads an update, stops all running modules (restoring windows), installs it
and restarts. Portable builds only link to the download page. See [releasing.md](releasing.md).

## Platforms

Windows only for now. Platform code lives under `platform/<os>/` with an `unsupported` stub, so other
backends can be added later; nothing in the docs or UI claims support for other systems.
