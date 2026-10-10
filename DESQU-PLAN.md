# DESQU-PLAN.md — Desqu monorepo transformation plan and progress tracker

This file is the **single source of truth for the Desqu transformation** (turning the ScreenBound repo
into the Desqu collection of desktop utilities). Feature status of Screenbound itself stays in
[PLAN.md](PLAN.md) (moves to `apps/screenbound/PLAN.md` in Phase 1).

If you are an AI agent or a developer picking this up in a new session: read **§0 How to use this
file** first, then [AGENTS.md](AGENTS.md), then continue at the first step that is not done.

---

## 0. How to use this file

### Status legend

| Mark | Meaning |
| --- | --- |
| ⬜ | Not started |
| 🟡 | In progress (someone started; see Progress log for where they stopped) |
| 🧪 | Implemented and unit-tested/compiles; **awaiting verification on real Windows** |
| ✅ | Done and verified (verification steps of the step were actually executed) |
| ⛔ | Blocked (write the reason next to it) |
| ❓ | Waiting for a decision from the owner (see §4 Open questions) |
| ⏭️ | Skipped / deferred on purpose (write why) |

### Rules for whoever works on this plan

1. Pick the **first** step (in order) whose status is ⬜ or 🟡 and whose **Depends on** steps are all ✅
   (or 🧪 if the step says 🧪 is enough). Do not jump ahead.
2. If the step depends on an open question (❓ in §4) that is unanswered, stop and ask the owner.
   Do not guess identifiers, URLs, keys or names.
3. Set the step to 🟡 **before** you start editing code and add a line to the **Progress log** (§9).
4. Do exactly what the step says. Do not refactor unrelated code. Follow [AGENTS.md](AGENTS.md).
5. Run every command in the step's **Verify** list. If a check needs a human on real Windows
   (manual test), set the step to 🧪 and say in the log what still needs a human.
6. When done, set the status (✅ or 🧪), tick the **Done when** boxes, and add a Progress log line.
7. If you discover the plan is wrong, fix the plan in this file (and note it in the log) rather than
   silently doing something else.
8. Never delete user data folders, registry entries or git branches without the owner confirming.
9. Commit after each completed step (one step = one commit is ideal). Commit message:
   `desqu(P<phase>.<step>): <short summary>`.

### Phase overview

| Phase | Name | Status | Depends on |
| --- | --- | --- | --- |
| 0 | Baseline measurements and decisions | ⬜ | — |
| 1 | Monorepo move (behaviour identical) | ⬜ | 0 |
| 2 | Screenbound module crate + UI package | ⬜ | 1 |
| 3 | Shared shell crate + shared UI kit | ⬜ | 1 (can run in parallel with 2) |
| 4 | Screenbound rebrand + settings migration | ⬜ | 2, 3, Q1, Q2 |
| 5 | Updater, CI and GitHub releases | ⬜ | 3, Q5, Q6 |
| 6 | Desqu collection app | ⬜ | 2, 3, 5 |
| 7 | Screenbound borderless gaming | ⬜ | 2 |
| 8 | Desktop layer spike + Vials | ⬜ | 3 |
| 9 | Virtual desktop spike + Workspaces | ⬜ | 8 |

---

## 1. Goal and products

Desqu is a repository and a collection app for desktop utilities. Each utility must also be buildable,
runnable, installable and publishable on its own.

| Product | What it is | Directory (target) |
| --- | --- | --- |
| **Desqu** | Collection app + central settings. Hosts all utilities in one process. | `apps/desqu/` |
| **Screenbound** | Screen/monitor utilities: existing fullscreen-to-zone feature + new borderless gaming. | `apps/screenbound/` |
| **Vials** | Desktop containers for shortcuts, apps and folders with optional auto-sort rules. | `apps/vials/` |
| **Workspaces** | Saves/restores desktop shortcuts, apps and folders per OS virtual desktop. | `apps/workspaces/` |

---

## 2. Decisions already made (do not re-litigate without the owner)

| ID | Decision | Consequence |
| --- | --- | --- |
| D1 | **In-process modules.** Each utility = Rust library crate exposing a **Tauri plugin** + a Svelte UI package. Desqu = one process hosting all modules. A standalone app = thin shell + one module. | One tray, one updater, one WebView in Desqu. "Off" = module stopped inside the process. |
| D2 | **Desqu takes over** from a standalone copy. Module settings live in the standalone app's settings folder and are shared. A per-module named mutex guarantees only one engine runs. | Desqu asks a running standalone copy to quit (`--quit`), removes its autostart entry, then starts the module. |
| D3 | **Windows only for now**, keep platform traits + `unsupported` stubs so other OS backends can be added later. | Never claim macOS/Linux support for a feature in docs or UI. |
| D4 | **Full rebrand of Screenbound with a new identifier.** | Settings must be migrated from `%APPDATA%\com.screenbound.app`. Old install, old autostart entry and old crash journal must be handled (Phase 4). |
| D5 | **Vials: references + hide on desktop** (Fences-like). Items stay in the Desktop folder; the vial shows them and they are hidden/covered on the desktop. | Needs a shared desktop-icon layer with a crash journal (Phase 8). |
| D6 | **Workspaces may use undocumented Windows virtual desktop APIs**, isolated in the platform layer and gated by Windows build number with graceful fallback. | Spike first (Phase 9). |
| D7 | **Four products**: Desqu, Screenbound, Vials, Workspaces. | — |

---

## 3. Verified facts about the repo at the start (baseline, 2026-10-10)

These were checked in the code. If a later step changes one of them, the step says so.

- Single Tauri 2 + SvelteKit (Svelte 5) app at the repo root.
  - [package.json](package.json): name `screenbound`, version `0.1.0`, deps `@tauri-apps/api`,
    `@tauri-apps/plugin-autostart`; lock file `package-lock.json` at root.
  - [src-tauri/Cargo.toml](src-tauri/Cargo.toml): package `screenbound` 0.1.0, lib `screenbound_lib`,
    deps `tauri` (feature `tray-icon`), `tauri-plugin-single-instance`, `tauri-plugin-autostart`,
    `windows` 0.62, `serde`, `serde_json`, `thiserror`, `tracing`, `tracing-subscriber`.
    Has a `[profile.release]` (lto, strip, `panic = "unwind"`, codegen-units 1, opt-level 3).
    `Cargo.lock` lives in `src-tauri/`.
  - [src-tauri/tauri.conf.json](src-tauri/tauri.conf.json): `productName` `ScreenBound`, `version`
    `0.1.0`, `identifier` `com.screenbound.app`, window label `main` (no `visible:false`), bundle
    targets `nsis` + `msi`, NSIS `installMode: currentUser`, frontend dist `../build`, dev URL port 1420.
  - Version `0.1.0` is duplicated in package.json, Cargo.toml and tauri.conf.json.
- Git remote `https://github.com/Wolfskii/Desqu.git`, working branch `develop`.
- **No `.github/` directory**: no CI, no release workflow.
- Releases are built locally: [Taskfile.yml](Taskfile.yml) `release` = `scripts/stop-screenbound.ps1`
  → `npm run tauri build` → `scripts/publish-artifacts.ps1` →
  `dist/ScreenBound_<ver>_x64-setup.exe`, `dist/ScreenBound_<ver>_x64.msi`,
  `dist/ScreenBound_<ver>_x64-portable.exe`. `deploy` installs the newest `*-setup.exe` with `/S`.
- [scripts/publish-artifacts.ps1](scripts/publish-artifacts.ps1) hardcodes
  `src-tauri/target/release/screenbound.exe` and `src-tauri/target/release/bundle/...`, reads
  version/productName from `src-tauri/tauri.conf.json`.
- [scripts/stop-screenbound.ps1](scripts/stop-screenbound.ps1) kills processes named
  `screenbound`/`ScreenBound`.
- **No update mechanism exists**: no `tauri-plugin-updater`, no `updater` config, no pubkey, no GitHub
  API calls, no version/update UI. Users on 0.1.0 can only upgrade by reinstalling manually.
- [src-tauri/src/lib.rs](src-tauri/src/lib.rs):
  - Plugins: autostart (no launch args), single-instance (second launch shows the main window).
  - Tray: "Show ScreenBound", separator, "Exit". Left click shows the window.
  - Close button **always hides** the window (hardcoded). If tray creation fails, close quits.
  - `RunEvent::Exit` → `EngineHandle::stop(3s)` restores managed windows, then drops the subscription.
  - Logging: `tracing_subscriber` to stderr, default filter `screenbound_lib=info,warn`. No log file.
  - UI events: `sb://activity`, `sb://managed-changed`, `sb://monitors-changed`, `sb://config-changed`.
- [src-tauri/src/commands.rs](src-tauri/src/commands.rs): commands `get_monitors`, `get_config`,
  `set_config` (also calls `autostart::sync`), `set_enabled`, `get_status`, `list_windows`,
  `get_activity`, `release_window`, `release_all`.
- [src-tauri/src/autostart.rs](src-tauri/src/autostart.rs): syncs `AppConfig.start_at_boot` via
  `tauri_plugin_autostart::ManagerExt::autolaunch()`.
- [src-tauri/src/core/config.rs](src-tauri/src/core/config.rs): `CONFIG_VERSION = 1`, no migrations.
  `AppConfig { version, enabled, transition, start_at_boot, zones, rules, groups, known_apps }`
  (serde camelCase). `write_json_atomic` lives here.
- Paths: config = `app_config_dir()/config.json`, journal = `app_data_dir()/managed-windows.json`
  → both in `%APPDATA%\com.screenbound.app\`.
- `AppConfig.enabled = false` only **pauses**: WinEvent hooks and the hidden display window keep running.
  Dropping `EventSubscription` ([src-tauri/src/platform/mod.rs](src-tauri/src/platform/mod.rs))
  unhooks and stops the event thread.
- Platform: `PlatformWindowManager` trait; Windows impl in `platform/windows/`; `UnsupportedPlatform`
  stub on other OSes. Already has `set_borderless`, `set_bounds`, `restore_state`, `set_topmost`,
  `set_hides_taskbar`.
- Frontend: [src/routes/+page.svelte](src/routes/+page.svelte) (header + 6 tabs),
  [src/lib/api.ts](src/lib/api.ts), [src/lib/state.svelte.ts](src/lib/state.svelte.ts),
  [src/lib/types.ts](src/lib/types.ts) (mirrors Rust), custom toggle
  [src/lib/components/Switch.svelte](src/lib/components/Switch.svelte), tokens in
  [src/app.css](src/app.css). [src/lib/apps.ts](src/lib/apps.ts) migrates legacy localStorage key
  `screenbound.seenApps`.
- Root [.gitignore](.gitignore) uses root-anchored patterns `/build`, `/dist`, `/.svelte-kit`.
  [src-tauri/.gitignore](src-tauri/.gitignore) ignores `/target/` and `/gen/schemas`.

### Not confirmed (must be measured in Phase 0)

- Whether autostart at sign-in really keeps the window hidden. The comment in `config.rs` says so, but
  nothing in code hides the window → it probably **shows**. (P0.2)
- The exact `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` value name written by
  tauri-plugin-autostart, and whether it also writes `...\Explorer\StartupApproved\Run`. (P0.3)
- The exact NSIS install folder and uninstall registry key of the current ScreenBound install, and
  whether a productName that differs only in case collides with it. (P0.4)
- Whether any GitHub releases / public 0.1.0 users exist. (Q3)

---

## 4. Open questions (owner must answer; record the answer here)

| ID | Question | Blocks | Answer |
| --- | --- | --- | --- |
| Q1 | New bundle identifiers and product names. Proposal: `io.github.wolfskii.desqu`, `io.github.wolfskii.screenbound`, `io.github.wolfskii.vials`, `io.github.wolfskii.workspaces`; product names `Desqu`, `Screenbound`, `Vials`, `Workspaces`. | P4.1, P6.1, P8, P9 | ❓ |
| Q2 | After migrating, keep `%APPDATA%\com.screenbound.app` as a backup (proposal: keep + marker file) or delete it? | P4.3 | ❓ |
| Q3 | Are there published releases / public 0.1.0 users? Decides how much migration UI is needed. | P4.4 | ❓ |
| Q4 | OK that a standalone app builds from its own folder but still needs the repo's shared `crates/` and `packages/` (monorepo)? Alternative = publish/vendor shared code. Proposal: yes, monorepo. | P1, P5 | ✅ Yes (2026-10-10): monorepo; standalone apps may depend on `crates/` and `packages/`. |
| Q5 | Update feed hosting: `gh-pages` branch served by GitHub Pages (proposal), plain `updates` branch via raw URL, or rolling release tag per product? | P5.2 | ❓ |
| Q6 | Updater signing: one key per product (proposal) or one shared key? Is an Authenticode code-signing certificate available? | P5.1, P5.5 | ❓ |
| Q7 | Updates: notify only (proposal) or auto-install? Daily update timer acceptable vs. "no polling" rule (proposal: yes, it is not window polling)? On close-to-tray: hide window (current) or destroy WebView to save RAM? | P3.6, P5.3 | ❓ |
| Q8 | Desqu branding: new logo/icons or reuse mascot assets in `assets/`? | P6.1 | ❓ |
| Q9 | Minimum Windows version: Windows 10 22H2 + 11, or 11 only? (Workspaces may end up 11-only.) | P8, P9 | ❓ |
| Q10 | When Vials and Workspaces are both on: proposal = Workspaces decides which items are visible per virtual desktop, Vials arranges within that. OK? | P8.3, P9.3 | ❓ |
| Q11 | Borderless gaming trigger: per-rule automatic only (proposal), or also a global hotkey (needs `RegisterHotKey`)? | P7 | ❓ |

---

## 5. Target architecture

### 5.1 Directory structure (end state)

```
/                               repo root
├─ Cargo.toml                   Cargo workspace + [profile.release] (MUST live here)
├─ Cargo.lock
├─ package.json                 npm workspaces (private, name "desqu-repo")
├─ package-lock.json
├─ Taskfile.yml                 includes every app's Taskfile; repo-wide ci
├─ AGENTS.md                    repo-wide engineering rules
├─ PLAN.md                      short index → links to each app's PLAN.md
├─ DESQU-PLAN.md                this file
├─ README.md                    Desqu collection README
├─ assets/                      shared images (buymeacoffee button, Desqu logo)
├─ docs/                        architecture.md, releasing.md, migration-screenbound.md
├─ scripts/                     shared, parameterized PowerShell scripts
│   ├─ publish-artifacts.ps1    -App <name>
│   ├─ stop-app.ps1             -Names <process names>
│   ├─ bump-version.ps1         -App <name> -Version <semver>
│   ├─ check-versions.ps1       -App <name> [-Tag <tag>]
│   ├─ check-standalone-deps.ps1
│   └─ write-update-feed.ps1    -App <name>
├─ .github/workflows/           ci.yml, release.yml
├─ crates/
│   ├─ desqu-shell/             shared Tauri shell (tray, settings, startup, autostart, updater, mutex…)
│   └─ desqu-desktop/           shared desktop-icon layer (Vials + Workspaces), Phase 8
├─ packages/
│   └─ ui/                      @desqu/ui: tokens, Switch, TriStateSwitch, SettingsRow, UpdateCard…
└─ apps/
    ├─ desqu/                   collection app (shell + all modules), no module/ of its own
    │   ├─ src/  static/  src-tauri/
    │   ├─ package.json  svelte.config.js  vite.config.js  tsconfig.json
    │   └─ Taskfile.yml  README.md  CHANGELOG.md
    ├─ screenbound/
    │   ├─ module/              Rust lib crate `screenbound-module` (core, engine, platform, plugin)
    │   ├─ ui/                  Svelte package `@desqu/screenbound-ui`
    │   ├─ src/  static/  src-tauri/   standalone shell app (bin `screenbound`)
    │   ├─ docs/                fullscreen-research.md, manual-testing.md
    │   ├─ scripts/             probe-resize.ps1
    │   ├─ package.json  svelte.config.js  vite.config.js  tsconfig.json
    │   └─ Taskfile.yml  PLAN.md  AGENTS.md  README.md  CHANGELOG.md
    ├─ vials/                   same shape as screenbound (Phase 8)
    └─ workspaces/              same shape as screenbound (Phase 9)
```

### 5.2 Crate and package names

| Path | Cargo package | Lib/bin name | Notes |
| --- | --- | --- | --- |
| `apps/screenbound/module` | `screenbound-module` | lib `screenbound_module` | Tauri plugin name `screenbound` |
| `apps/screenbound/src-tauri` | `screenbound` | bin `screenbound`, lib `screenbound_lib` | Keeps `screenbound.exe` |
| `apps/desqu/src-tauri` | `desqu` | bin `desqu`, lib `desqu_lib` | |
| `apps/vials/module` | `vials-module` | lib `vials_module` | plugin name `vials` |
| `apps/vials/src-tauri` | `vials` | bin `vials`, lib `vials_lib` | |
| `apps/workspaces/module` | `workspaces-module` | lib `workspaces_module` | plugin name `workspaces` |
| `apps/workspaces/src-tauri` | `workspaces` | bin `workspaces`, lib `workspaces_lib` | |
| `crates/desqu-shell` | `desqu-shell` | lib `desqu_shell` | plugin name `desqu-shell` |
| `crates/desqu-desktop` | `desqu-desktop` | lib `desqu_desktop` | |

| Path | npm package |
| --- | --- |
| `apps/screenbound` | `screenbound-app` (private) |
| `apps/screenbound/ui` | `@desqu/screenbound-ui` |
| `apps/desqu` | `desqu-app` (private) |
| `packages/ui` | `@desqu/ui` |

### 5.3 Dependency rules (enforced by `scripts/check-standalone-deps.ps1` in CI)

- `apps/<x>/src-tauri` may depend on: `apps/<x>/module`, `crates/*`.
- `apps/<x>` (npm) may depend on: `apps/<x>/ui`, `packages/*`.
- `apps/desqu` may depend on every module crate and every module UI package.
- **Nothing** may depend on `apps/desqu`.
- Module crates never depend on other module crates (Vials and Workspaces share `crates/desqu-desktop`).
- `core/` of any module never calls native APIs; native code only under `platform/<os>/`
  (same rule as today's AGENTS.md).

### 5.4 Module contract

- Every module crate exports:
  - `pub fn init<R: Runtime>(options: ModuleOptions) -> TauriPlugin<R>` built with
    `tauri::plugin::Builder::new("<plugin-name>")`.
  - A type implementing `desqu_shell::Module` (`id()`, `start()`, `stop(timeout)`, `status()`).
- `ModuleOptions { config_dir: PathBuf, data_dir: PathBuf, start_immediately: bool }`.
  The shell passes the folders explicitly, so standalone and Desqu read **the same** files.
- Frontend calls plugin commands as `invoke('plugin:<plugin-name>|<command>', args)`.
- The app's capability file must list `"<plugin-name>:default"`.
- Module status values: `off` (nothing running), `paused` (running but not acting; Screenbound's
  existing `enabled=false`), `on`, `unsupported` (OS not supported), `ownedElsewhere` (another process
  holds the module mutex).

### 5.5 Settings ownership

| Data | File | Owner |
| --- | --- | --- |
| Screenbound zones/rules/groups/apps | `%APPDATA%\<screenbound-id>\config.json` | Screenbound module (shared by standalone + Desqu) |
| Screenbound crash journal | `%APPDATA%\<screenbound-id>\managed-windows.json` | Screenbound module |
| Standalone shell settings | `%APPDATA%\<app-id>\shell.json` | `desqu-shell` in that standalone app |
| Desqu settings (shell + per-module enabled/startWithSystem) | `%APPDATA%\<desqu-id>\settings.json` | Desqu |
| Vials / Workspaces data | `%APPDATA%\<vials-id>\…`, `%APPDATA%\<workspaces-id>\…` | their modules |

### 5.6 What "Off" means per utility

| Utility | Off | Paused | On (background cost) |
| --- | --- | --- | --- |
| Screenbound | Engine stopped via `EngineHandle::stop` (managed windows restored, journal cleared) **and** `EventSubscription` dropped (hooks removed, hidden display window destroyed, event thread ended). Zero threads/hooks. | Existing `enabled=false` header toggle. Hooks still run, nothing is applied. | Engine thread + WinEvent hook thread. |
| Borderless gaming | Part of Screenbound module; off with it. | Same as Screenbound. | Same threads; triggers on matching windows. |
| Vials | Vial windows closed; every hidden/covered desktop item restored from the journal; desktop watcher stopped. | — | One window per vial (WebView cost) + desktop folder watcher. |
| Workspaces | Virtual-desktop watcher stopped; desktop restored to "all items visible"; saved layouts kept on disk. | — | One registry-notification thread; snapshots on desktop switch. |

---

## 6. Phases and steps

Each step has: **Status**, **Depends on**, **Files**, **Do**, **Verify**, **Done when**.

---

### Phase 0 — Baseline measurements and decisions

#### P0.1 Answer open questions — Status: 🟡
- **Depends on:** —
- **Do:** Ask the owner Q1–Q11 (§4). Write answers in the §4 table. Q1–Q4 are needed before Phase 4;
  Q5–Q7 before Phase 5. Phase 1–3 can start without them.
- **Done when:**
  - [x] Q4 answered (needed for Phase 1).
  - [ ] Remaining answers recorded or explicitly deferred with the step they block.

#### P0.2 Measure current Screenbound startup/tray behaviour — Status: ⬜
- **Depends on:** —
- **Files:** write results into [PLAN.md](PLAN.md) (section "Next steps", item about tray) and §3 of this file.
- **Do (human on real Windows):**
  1. `task deploy` to install the current build.
  2. Enable **Start at boot** in Settings. Sign out and in. Note: does the main window appear?
  3. Close the window with X: does it hide to tray? Tray left-click shows it? Tray **Exit** quits?
  4. With a browser constrained in a zone, tray **Exit**: is the browser restored to full monitor?
  5. Task Manager: idle CPU and memory (main process + WebView2 processes) with window hidden.
- **Done when:**
  - [ ] Results recorded (window shown/hidden at boot, tray behaviours, CPU %, RAM MB).

#### P0.3 Record the autostart registry entry — Status: ⬜
- **Depends on:** P0.2 (Start at boot enabled)
- **Do:** In PowerShell:
  `Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'` and
  `Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run'`.
  Record the value name(s) and data (exe path, arguments) in §3.
- **Done when:**
  - [ ] Exact value name and path recorded.

#### P0.4 Record the installer footprint — Status: ⬜
- **Depends on:** P0.2
- **Do:** Record:
  - Install folder (likely `%LOCALAPPDATA%\ScreenBound` — confirm).
  - Uninstall key: `Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall' | Where-Object { (Get-ItemProperty $_.PSPath).DisplayName -like '*ScreenBound*' }` → record key name, `UninstallString`, `QuietUninstallString`, `DisplayVersion`.
  - Any `HKCU:\Software\<publisher>\ScreenBound` key.
  - Start-menu shortcut path.
  - Whether `uninstall.exe` exists next to `screenbound.exe` in the install folder (used later to
    detect portable vs installed).
- **Done when:**
  - [ ] All values recorded in §3.

#### P0.5 Record artifact baseline — Status: ✅
- **Depends on:** —
- **Do:** Run `task release`. Record the exact file names and sizes in `dist/` in the Progress log.
  Run `task ci` and record pass/fail and test count.
- **Done when:**
  - [x] Artifact names/sizes and `task ci` result recorded.

---

### Phase 1 — Monorepo move (behaviour must stay identical)

Goal: Screenbound lives in `apps/screenbound/`, the root is a Cargo + npm workspace, everything still
builds and produces the same artifacts. **No identifier, name, or behaviour changes in this phase.**

#### P1.1 Move Screenbound files with git — Status: 🟡
- **Depends on:** P0.5, Q4
- **Do:** From the repo root, with a clean working tree (`git status` shows nothing to commit). Stop
  any running `task dev` first.
  ```powershell
  New-Item -ItemType Directory apps/screenbound/docs, apps/screenbound/scripts, apps/screenbound/assets -Force
  git mv src apps/screenbound/src
  git mv static apps/screenbound/static
  git mv src-tauri apps/screenbound/src-tauri
  git mv svelte.config.js apps/screenbound/svelte.config.js
  git mv vite.config.js apps/screenbound/vite.config.js
  git mv tsconfig.json apps/screenbound/tsconfig.json
  git mv package.json apps/screenbound/package.json
  git mv docs/fullscreen-research.md apps/screenbound/docs/fullscreen-research.md
  git mv docs/manual-testing.md apps/screenbound/docs/manual-testing.md
  git mv scripts/probe-resize.ps1 apps/screenbound/scripts/probe-resize.ps1
  git mv PLAN.md apps/screenbound/PLAN.md
  git mv assets/logo.png apps/screenbound/assets/logo.png
  git mv assets/logo.webp apps/screenbound/assets/logo.webp
  git mv assets/logo-character.webp apps/screenbound/assets/logo-character.webp
  git mv assets/logo-mascot.webp apps/screenbound/assets/logo-mascot.webp
  git mv apps/screenbound/src-tauri/Cargo.lock Cargo.lock
  ```
  - Keep at root: `package-lock.json` (becomes the workspace lock), `scripts/publish-artifacts.ps1`,
    `scripts/stop-screenbound.ps1`, `assets/buymeacoffee-button.png`, `AGENTS.md`, `README.md`,
    `Taskfile.yml`, `.gitignore`, `.vscode/`.
  - The old build cache `apps/screenbound/src-tauri/target/` (ignored) moved with the folder; it is now
    unused. Ask the owner before deleting it (it is only a cache).
- **Verify:** `git status` shows renames (R), not delete+add, for the moved files.
- **Done when:**
  - [x] All moves done; history preserved (`git log --follow apps/screenbound/src-tauri/src/lib.rs` shows old commits).

#### P1.2 Create the Cargo workspace — Status: ⬜
- **Depends on:** P1.1
- **Files:** new `Cargo.toml` (root), `apps/screenbound/src-tauri/Cargo.toml`
- **Do:**
  1. Create root `Cargo.toml`:
     ```toml
     [workspace]
     resolver = "2"
     members = ["apps/screenbound/src-tauri"]

     # Profiles are only honoured at the workspace root.
     [profile.release]
     codegen-units = 1
     lto = true
     opt-level = 3
     panic = "unwind"
     strip = true
     ```
     Copy the exact `[profile.release]` values from the app Cargo.toml (do not invent new ones).
  2. Delete the `[profile.release]` section from `apps/screenbound/src-tauri/Cargo.toml`.
  3. Run `cargo metadata --format-version 1 --no-deps` from the root to confirm the workspace parses.
- **Verify:** `cargo build -p screenbound` from root succeeds; output exe is `target/debug/screenbound.exe`
  at the **root** `target/`.
- **Done when:**
  - [ ] Workspace builds; only one `Cargo.lock` (root); release profile only in root `Cargo.toml`.

#### P1.3 Create the npm workspace — Status: ⬜
- **Depends on:** P1.1
- **Files:** new root `package.json`, `apps/screenbound/package.json`, `package-lock.json`
- **Do:**
  1. Create root `package.json`:
     ```json
     {
       "name": "desqu-repo",
       "private": true,
       "workspaces": ["apps/*", "apps/*/ui", "packages/*"]
     }
     ```
  2. In `apps/screenbound/package.json` change `"name"` to `"screenbound-app"` (npm workspace names must
     be unique and not collide with future packages). Keep `version` `0.1.0` and all scripts.
  3. Run `npm install` at the root. This rewrites `package-lock.json` for the workspace.
- **Verify:** `npm run check -w screenbound-app` passes.
- **Done when:**
  - [ ] `npm install` at root works; `node_modules/` only at root (plus any nested ones npm decides).

#### P1.4 Fix ignore files — Status: ⬜
- **Depends on:** P1.1
- **Files:** [.gitignore](.gitignore)
- **Do:** Root-anchored patterns no longer match the moved app. Replace the content with:
  ```gitignore
  .DS_Store
  node_modules
  /target
  /dist
  **/build
  **/.svelte-kit
  /package
  .env
  .env.*
  !.env.example
  vite.config.js.timestamp-*
  vite.config.ts.timestamp-*
  ```
  Keep `apps/screenbound/src-tauri/.gitignore` as is (`/gen/schemas` is still needed).
- **Verify:** after a build, `git status` shows no `build/`, `.svelte-kit/`, `target/` or `dist/` files.
- **Done when:**
  - [ ] No build output shows up in `git status`.

#### P1.5 Split Taskfiles — Status: ⬜
- **Depends on:** P1.2, P1.3
- **Files:** root [Taskfile.yml](Taskfile.yml), new `apps/screenbound/Taskfile.yml`
- **Do:**
  1. Create `apps/screenbound/Taskfile.yml` containing the current tasks `dev`, `dev:logs`, `check`,
     `test`, `probe`, `probe:resize`, `lint`, `ci`, `release`, `deploy` adapted as follows:
     - `install`: `npm install` with `dir: ../..` (root).
     - `dev`: `npm run tauri dev` (runs in the app folder, where its package.json is).
     - `check`: `npm run check`.
     - `test`: `cargo test -p screenbound`.
     - `probe`: `cargo test -p screenbound -- --ignored --nocapture`.
     - `probe:resize`: `powershell ... -File scripts/probe-resize.ps1 -ProcessId {{.PID}}...`.
     - `lint`: `cargo clippy -p screenbound --all-targets`.
     - `release`: `powershell ... -File ../../scripts/stop-app.ps1 -Names screenbound,ScreenBound`,
       `npm run tauri build`, `powershell ... -File ../../scripts/publish-artifacts.ps1 -App screenbound`.
     - `deploy`: same as today but searches `../../dist/screenbound` for `*-setup.exe`.
  2. Root `Taskfile.yml`:
     ```yaml
     version: "3"
     includes:
       screenbound:
         taskfile: ./apps/screenbound/Taskfile.yml
         dir: ./apps/screenbound
     tasks:
       default: { cmds: [task --list], silent: true }
       install: { desc: Install npm dependencies, cmds: [npm install] }
       ci:
         desc: CI for every app
         cmds:
           - task: screenbound:ci
     ```
  3. Keep task descriptions so `task --list` shows them.
- **Verify:** `task --list` shows `screenbound:*` tasks; `task screenbound:ci` passes; `cd apps/screenbound; task ci` passes.
- **Done when:**
  - [ ] Both root and app-folder invocations work.

#### P1.6 Parameterize release scripts — Status: ⬜
- **Depends on:** P1.2
- **Files:** `scripts/publish-artifacts.ps1`, rename `scripts/stop-screenbound.ps1` → `scripts/stop-app.ps1` (`git mv`)
- **Do:**
  1. `publish-artifacts.ps1`: add `param([Parameter(Mandatory)][string]$App)`. Compute
     `$root = Resolve-Path "$PSScriptRoot/.."`, `$appDir = "$root/apps/$App"`,
     read `tauri.conf.json` from `$appDir/src-tauri/`, read the bin name from
     `$appDir/src-tauri/Cargo.toml` (`[package] name`), take binaries from `$root/target/release/`
     and bundles from `$root/target/release/bundle/{nsis,msi}`. Output to `$root/dist/$App/`.
     Keep the artifact name pattern `{productName}_{version}_{arch}-setup.exe`, `.msi`, `-portable.exe`.
     Clear only `dist/$App/` (not other apps' folders).
  2. `stop-app.ps1`: add `param([Parameter(Mandatory)][string[]]$Names)` and stop those processes
     (same logic as today).
- **Verify:** `task screenbound:release` produces the same three file names as recorded in P0.5, now in `dist/screenbound/`.
- **Done when:**
  - [ ] Artifacts identical in name; sizes within a few % of P0.5.

#### P1.7 Split docs and agent rules — Status: ⬜
- **Depends on:** P1.1
- **Files:** [AGENTS.md](AGENTS.md), new `apps/screenbound/AGENTS.md`, [README.md](README.md),
  new `apps/screenbound/README.md`, new root `PLAN.md`
- **Do:**
  1. `apps/screenbound/README.md` = current README content with image paths fixed
     (`assets/logo.webp` stays valid because logos moved to `apps/screenbound/assets/`; buymeacoffee →
     `../../assets/buymeacoffee-button.png`). Build instructions → `task screenbound:dev` etc.
  2. Root `README.md`: short Desqu intro, list of products with links to their READMEs, build
     instructions (`task install`, `task --list`), link to this file.
  3. `apps/screenbound/AGENTS.md`: the Screenbound-specific parts of today's AGENTS.md ("What
     ScreenBound is", "Native rules", "Done means tested on real Windows", commands table with
     `screenbound:` prefixes, config location).
  4. Root `AGENTS.md`: repo-wide rules: §5.3 dependency rules, architecture layering
     (UI → commands/plugin → engine → core/platform), conventions (clippy clean, serde camelCase,
     TS types mirror Rust), "read DESQU-PLAN.md first", and links to each app's AGENTS.md.
  5. Root `PLAN.md`: a short index: "Transformation: see DESQU-PLAN.md. Per-app status:
     apps/screenbound/PLAN.md, …".
  6. Fix relative links inside moved docs (`apps/screenbound/PLAN.md` links to `docs/...` still work
     because docs moved together; check `../src-tauri/...` style links in
     `apps/screenbound/docs/fullscreen-research.md`).
- **Verify:** open each markdown file and click/inspect links; no broken relative links.
- **Done when:**
  - [ ] All docs in place, links valid.

#### P1.8 Phase 1 regression check — Status: ⬜
- **Depends on:** P1.2–P1.7
- **Do:**
  1. `task ci` (root) passes.
  2. `task screenbound:dev`: app starts, all tabs load, config from `%APPDATA%\com.screenbound.app`
     is shown (settings preserved).
  3. Run the **Core flow** and **Lifecycle** sections of `apps/screenbound/docs/manual-testing.md`.
  4. `task screenbound:release` + install; confirm exe is stripped (size similar to P0.5).
- **Done when:**
  - [ ] All four checks pass. Phase 1 → ✅ in the phase table.

---

### Phase 2 — Screenbound module crate + UI package

Goal: Screenbound's logic becomes a reusable Tauri plugin crate and a Svelte UI package. The
standalone app becomes a thin shell. Behaviour stays identical except: the module can now be
started/stopped at runtime.

#### P2.1 Create `screenbound-module` crate and move code — Status: ⬜
- **Depends on:** P1.8
- **Files:** new `apps/screenbound/module/Cargo.toml`, `apps/screenbound/module/src/lib.rs`;
  move `apps/screenbound/src-tauri/src/{core,engine,platform}` → `apps/screenbound/module/src/`
  (`git mv`); root `Cargo.toml` members.
- **Do:**
  1. `git mv apps/screenbound/src-tauri/src/core apps/screenbound/module/src/core` (same for `engine`, `platform`).
  2. `apps/screenbound/module/Cargo.toml`: package `screenbound-module`, version = app version,
     edition 2021, rust-version 1.80, `[lib] name = "screenbound_module"`. Move these deps from the
     app Cargo.toml: `serde`, `serde_json`, `thiserror`, `tracing`, `tauri` (no extra features),
     and the whole `[target.'cfg(windows)'.dependencies] windows = {...}` block with the same feature list.
  3. `module/src/lib.rs`: `pub mod core; pub mod engine; pub mod platform;` (engine/platform can stay
     `pub(crate)` later; start `pub` to keep the move mechanical).
  4. Add `"apps/screenbound/module"` to root workspace members.
  5. In the app crate add `screenbound-module = { path = "../module" }` and replace `crate::core`,
     `crate::engine`, `crate::platform` imports with `screenbound_module::...`.
  6. Fix any `pub(crate)` visibility errors by making items `pub` (only what the shell uses).
- **Verify:** `cargo test -p screenbound-module` runs the same number of unit tests as P0.5;
  `cargo build -p screenbound` succeeds; `cargo clippy --workspace --all-targets` clean.
- **Done when:**
  - [ ] Tests moved and passing; app still runs exactly as before.

#### P2.2 Turn the commands into a Tauri plugin — Status: ⬜
- **Depends on:** P2.1
- **Files:** `apps/screenbound/module/src/plugin.rs`, `apps/screenbound/module/build.rs`,
  `apps/screenbound/module/permissions/default.toml`, move `apps/screenbound/src-tauri/src/commands.rs`
  → `apps/screenbound/module/src/commands.rs`, `apps/screenbound/src-tauri/src/lib.rs`,
  `apps/screenbound/src-tauri/capabilities/default.json`
- **Do:**
  1. Module `build.rs`:
     ```rust
     const COMMANDS: &[&str] = &["get_monitors", "get_config", "set_config", "set_enabled", "get_status",
         "list_windows", "get_activity", "release_window", "release_all", "module_start", "module_stop", "module_status"];
     fn main() { tauri_plugin::Builder::new(COMMANDS).build(); }
     ```
     Add `[build-dependencies] tauri-plugin = { version = "2", features = ["build"] }` and set
     `links = "screenbound-module"` in `[package]` (required by tauri-plugin build).
  2. `permissions/default.toml`:
     ```toml
     [default]
     description = "Everything the Screenbound UI needs"
     permissions = ["allow-get-monitors", "allow-get-config", "allow-set-config", "allow-set-enabled",
       "allow-get-status", "allow-list-windows", "allow-get-activity", "allow-release-window",
       "allow-release-all", "allow-module-start", "allow-module-stop", "allow-module-status"]
     ```
  3. `plugin.rs`: a `ModuleState` managed in the app:
     `struct ModuleState { paths: EnginePaths, handle: Mutex<Option<EngineHandle>>, subscription: Mutex<Option<EventSubscription>> }`.
     Functions `start(app)` (= today's setup code from `lib.rs`: create platform, spawn engine with the
     notifier that emits the 4 `sb://` events, subscribe, `set_events_active(true)`) and
     `stop(timeout)` (= today's `RunEvent::Exit` code: `handle.stop(timeout)`, then drop subscription).
  4. `pub fn init<R: Runtime>(options: ModuleOptions) -> TauriPlugin<R>`:
     `Builder::new("screenbound").invoke_handler(generate_handler![...]).setup(|app, _| { manage ModuleState; if options.start_immediately { start } }).on_event(|app, ev| if let RunEvent::Exit = ev { stop(3s) }).build()`.
  5. Commands take `State<'_, ModuleState>` and use a helper `with_engine(|h| h.call(...))` that returns
     `Err("Screenbound is off")` when the engine is not running. Add `module_start`, `module_stop`,
     `module_status` (returns `"on" | "off" | "paused"`; `paused` when `config.enabled == false`).
  6. Autostart: keep current behaviour for now by keeping `start_at_boot` handling in the **app shell**:
     the app listens to `sb://config-changed` on the Rust side
     (`app.listen("sb://config-changed", ...)`) and calls its local `autostart::sync`. Remove the
     `autostart` call from the module's `set_config`. (Phase 3 replaces this.)
  7. App `lib.rs` becomes: logging, `platform::init_process()` (now `screenbound_module::platform::init_process()`),
     autostart + single-instance plugins, `.plugin(screenbound_module::init(ModuleOptions { config_dir, data_dir, start_immediately: true }))`,
     tray + close handling (unchanged). Paths are resolved in the app (`app.path().app_config_dir()`),
     so build the plugin inside `setup` using `app.handle().plugin(...)`.
  8. Capability `apps/screenbound/src-tauri/capabilities/default.json`: permissions
     `["core:default", "screenbound:default"]`.
- **Verify:** `cargo build -p screenbound`; app starts; DevTools console shows no permission errors
  once P2.3 switches the frontend.
- **Done when:**
  - [ ] All 9 old commands available as `plugin:screenbound|…`; 3 new lifecycle commands work.

#### P2.3 Create `@desqu/screenbound-ui` and switch invoke names — Status: ⬜
- **Depends on:** P2.2
- **Files:** new `apps/screenbound/ui/package.json`, `apps/screenbound/ui/src/index.ts`; move
  `apps/screenbound/src/lib/*` → `apps/screenbound/ui/src/` (`git mv`); new
  `apps/screenbound/ui/src/ScreenboundView.svelte`; `apps/screenbound/src/routes/+page.svelte`;
  `apps/screenbound/package.json`
- **Do:**
  1. `ui/package.json`:
     ```json
     {
       "name": "@desqu/screenbound-ui",
       "version": "0.1.0",
       "private": true,
       "type": "module",
       "svelte": "./src/index.ts",
       "exports": { ".": { "types": "./src/index.ts", "svelte": "./src/index.ts", "default": "./src/index.ts" } },
       "peerDependencies": { "svelte": "^5", "@tauri-apps/api": "^2" }
     }
     ```
  2. Move today's `+page.svelte` markup/script into `ScreenboundView.svelte` (header, tabs, panels).
     `+page.svelte` becomes `<script>import { ScreenboundView } from '@desqu/screenbound-ui';</script><ScreenboundView />`.
  3. `index.ts` exports `ScreenboundView` and the types.
  4. In `api.ts`, change every `invoke('x', ...)` to `invoke('plugin:screenbound|x', ...)`. Event names
     stay `sb://…`. Add `moduleStart`, `moduleStop`, `moduleStatus`.
  5. `ScreenboundView` shows an "Screenbound is off — Turn on" card when `moduleStatus()` is `off`.
  6. App `package.json`: add dependency `"@desqu/screenbound-ui": "*"`; run `npm install` at root.
  7. Keep `app.css` import in the app's `+layout.svelte` (moves to `@desqu/ui` in Phase 3).
- **Verify:** `task screenbound:check`; `task screenbound:dev` — all six tabs work, saving works,
  Diagnostics list loads, activity log streams.
- **Done when:**
  - [ ] UI identical to before; all calls go through the plugin.

#### P2.4 Runtime start/stop works — Status: ⬜
- **Depends on:** P2.3
- **Files:** `apps/screenbound/module/src/plugin.rs`, a temporary dev-only button is **not** needed:
  call from DevTools console `window.__TAURI_INTERNALS__.invoke('plugin:screenbound|module_stop')`.
- **Do (real Windows):**
  1. Constrain a browser into a zone. Call `module_stop`. Expect: browser returns to full-monitor
     fullscreen, activity log stops, Process Explorer → screenbound.exe threads: the
     `screenbound-engine` thread and the event thread are gone.
  2. Call `module_start`. Enter fullscreen again → constrained again.
  3. Call `module_stop` twice → second call is a no-op, no error.
- **Done when:**
  - [ ] All three behaviours confirmed (status 🧪 until a human confirms 1–2).

---

### Phase 3 — Shared shell crate + shared UI kit

Goal: tray, startup, close-to-tray, autostart, single instance, settings store and module mutex are
implemented once in `crates/desqu-shell` and reused by every app. Standalone Screenbound adopts it.

#### P3.1 Create `desqu-shell` crate skeleton — Status: ⬜
- **Depends on:** P1.8
- **Files:** `crates/desqu-shell/Cargo.toml`, `crates/desqu-shell/src/{lib.rs, settings.rs, args.rs, json.rs, logging.rs, module.rs, mutex.rs, tray.rs, window.rs, autostart.rs, instance.rs, platform/mod.rs, platform/windows.rs, platform/unsupported.rs}`
- **Do:**
  1. Deps: `tauri` (feature `tray-icon`), `tauri-plugin-autostart`, `tauri-plugin-single-instance`,
     `serde`, `serde_json`, `thiserror`, `tracing`, `tracing-subscriber`; windows-only `windows` with
     features `Win32_Foundation`, `Win32_System_Threading`, `Win32_System_Registry`,
     `Win32_UI_WindowsAndMessaging`.
  2. Add `"crates/desqu-shell"` to workspace members.
  3. `json.rs`: move/copy `write_json_atomic` logic (temp file + rename) as a generic helper.
     Screenbound may keep its own copy until P3.8; do not create a dependency from module → shell
     just for this.
- **Verify:** `cargo build -p desqu-shell`.
- **Done when:**
  - [ ] Crate compiles with empty-but-declared modules.

#### P3.2 Shell settings model + store — Status: ⬜
- **Depends on:** P3.1
- **Files:** `crates/desqu-shell/src/settings.rs`
- **Do:**
  ```rust
  #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
  #[serde(rename_all = "camelCase")]
  pub struct ShellSettings {
      #[serde(default = "v1")] pub version: u32,
      #[serde(default)] pub start_with_system: bool,
      #[serde(default)] pub start_minimized: bool,
      #[serde(default = "yes")] pub close_to_tray: bool,   // true keeps today's behaviour
      #[serde(default)] pub updates: UpdateSettings,         // { check_automatically: bool = true, install_automatically: bool = false }
  }
  ```
  `load(path) -> ShellSettings` (default when missing, error when corrupt — same policy as Screenbound
  `config::load`), `save(path, &settings)` (atomic). Unit tests: defaults, round trip, unknown fields ignored.
- **Verify:** `cargo test -p desqu-shell`.
- **Done when:**
  - [ ] Tests pass.

#### P3.3 Launch arguments — Status: ⬜
- **Depends on:** P3.1
- **Files:** `crates/desqu-shell/src/args.rs`
- **Do:** `pub struct LaunchArgs { pub autostart: bool, pub quit: bool }`,
  `pub fn parse<I: IntoIterator<Item = String>>(args: I) -> LaunchArgs` recognising `--autostart` and
  `--quit` (ignore everything else). Unit tests.
- **Done when:**
  - [ ] Tests pass.

#### P3.4 Single instance + `--quit` forwarding — Status: ⬜
- **Depends on:** P3.3
- **Files:** `crates/desqu-shell/src/instance.rs`
- **Do:** `pub fn plugin<R: Runtime>() -> TauriPlugin<R>` wrapping `tauri_plugin_single_instance::init`.
  Callback: parse args; if `quit` → `app.exit(0)` (this runs `RunEvent::Exit`, so modules restore
  windows); else → `window::show_main(app)`. Also, in the **first** instance's setup: if `--quit` was
  passed and no other instance existed → exit immediately without starting modules.
- **Verify (manual):** with app running, run `screenbound.exe --quit` → running app exits and restores
  managed windows.
- **Done when:**
  - [ ] Behaviour confirmed (🧪 until manual check).

#### P3.5 Autostart with `--autostart` arg — Status: ⬜
- **Depends on:** P3.3, P0.3
- **Files:** `crates/desqu-shell/src/autostart.rs`
- **Do:**
  1. `pub fn plugin<R>() -> TauriPlugin<R>` = `tauri_plugin_autostart::Builder::new().args(["--autostart"]).build()`.
     If the installed plugin version's API differs, use `tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--autostart"]))`.
  2. `pub fn sync(app, enabled) -> Result<(), String>` (move from Screenbound's `autostart.rs`).
     Because the args changed, when `enabled` is true always call `disable()` then `enable()` once at
     startup so an old entry without `--autostart` is rewritten.
  3. Windows helper `remove_run_value(name: &str)` in `platform/windows.rs` (deletes an HKCU Run value
     and the matching `StartupApproved\Run` value). Used by Phase 4 and Phase 6.
- **Verify:** enable → `Get-ItemProperty HKCU:\...\Run` shows exe path + `--autostart`.
- **Done when:**
  - [ ] Registry entry contains `--autostart`.

#### P3.6 Window lifecycle: start hidden, start minimized, close to tray — Status: ⬜
- **Depends on:** P3.2, P3.3, Q7 (hide vs destroy; default to **hide** if unanswered)
- **Files:** `crates/desqu-shell/src/window.rs`
- **Do:**
  1. Apps set `"visible": false` on the `main` window in `tauri.conf.json` (done per app in P3.8).
  2. `pub fn on_startup(app, args: &LaunchArgs, settings: &ShellSettings, tray_ok: bool)`:
     show the main window unless (`args.autostart` || `settings.start_minimized`) **and** `tray_ok`.
     If the tray failed, always show (otherwise the app would be invisible).
  3. `pub fn install_close_handler(app, settings_getter)`: on `CloseRequested`, if
     `close_to_tray && tray_ok` → `prevent_close()` + `hide()`; else let it close → the app exits →
     `RunEvent::Exit` → modules stop. Read the setting **at event time** (it can change while running).
  4. `pub fn show_main(app)` (move from Screenbound `lib.rs`).
- **Done when:**
  - [ ] Functions implemented and used by Screenbound in P3.8.

#### P3.7 Tray builder — Status: ⬜
- **Depends on:** P3.6
- **Files:** `crates/desqu-shell/src/tray.rs`
- **Do:** `TrayConfig { tooltip, show_label, extra_items: Vec<TrayItem>, on_item: Fn(&AppHandle, &str) }`
  where `TrayItem` is `Normal{id,label}` | `Check{id,label,checked}` | `Separator`.
  Build menu: `show_label`, separator, extra items, separator, "Check for updates" (id `updates`,
  hidden until Phase 5), "Exit" (id `exit` → `app.exit(0)`). Left click (button up) → `show_main`.
  Return the `TrayIcon` so callers can update check items. Use the app's default window icon
  (copy whatever icon handling Screenbound's `install_tray` uses today).
- **Done when:**
  - [ ] Screenbound tray looks and behaves as before (P3.8).

#### P3.8 Module mutex + `Module` trait — Status: ⬜
- **Depends on:** P3.1
- **Files:** `crates/desqu-shell/src/mutex.rs`, `crates/desqu-shell/src/module.rs`,
  `crates/desqu-shell/src/platform/*`
- **Do:**
  1. `pub struct ModuleLock` with `pub fn try_acquire(module_id: &str) -> Result<Option<ModuleLock>, String>`.
     Windows: `CreateMutexW(None, true, "Local\\Desqu.Module.<id>")`; if `GetLastError() ==
     ERROR_ALREADY_EXISTS` → close handle, return `Ok(None)`. `Drop` → `ReleaseMutex` + `CloseHandle`.
     Non-Windows stub: always `Ok(Some(lock))`.
  2. ```rust
     pub enum ModuleStatus { Off, Paused, On, Unsupported, OwnedElsewhere }
     pub trait Module: Send + Sync {
         fn id(&self) -> &'static str;
         fn start(&self, app: &AppHandle) -> Result<(), String>;
         fn stop(&self, timeout: Duration);
         fn status(&self) -> ModuleStatus;
     }
     ```
  3. In `screenbound-module`: `ModuleState::start` acquires the lock first; if `None` → status
     `OwnedElsewhere`, do not start the engine. `stop` releases it. Implement `Module` for a small
     `ScreenboundModule` handle type. (The module crate now depends on `desqu-shell` — allowed by §5.3
     because it is in `crates/`.)
- **Verify:** start two dev builds (`target/debug/screenbound.exe` twice is blocked by single instance,
  so test with a unit/integration test that acquires the lock twice in one process → second is `None`).
- **Done when:**
  - [ ] Lock test passes; Screenbound reports `OwnedElsewhere` correctly.

#### P3.9 Shell plugin commands — Status: ⬜
- **Depends on:** P3.2–P3.7
- **Files:** `crates/desqu-shell/src/lib.rs`, `crates/desqu-shell/build.rs`, `crates/desqu-shell/permissions/default.toml`
- **Do:** Tauri plugin `desqu-shell` with commands: `get_shell_settings`, `set_shell_settings`
  (saves, then calls `autostart::sync`), `app_info` (→ `{ name, version, identifier, portable: bool }`;
  `portable` = no `uninstall.exe` next to the current exe — confirm the file name from P0.4).
  `pub fn init(config_dir)` loads settings into managed state. Same build.rs/permissions pattern as P2.2.
- **Done when:**
  - [ ] Commands callable as `plugin:desqu-shell|…`.

#### P3.10 Shared UI kit `@desqu/ui` — Status: ⬜
- **Depends on:** P1.8
- **Files:** `packages/ui/package.json`, `packages/ui/src/{index.ts, tokens.css, Switch.svelte, TriStateSwitch.svelte, SettingsRow.svelte, ShellSettingsSection.svelte, shell-api.ts}`
- **Do:**
  1. `git mv apps/screenbound/ui/src/components/Switch.svelte packages/ui/src/Switch.svelte`; update
     imports in the Screenbound UI to `import { Switch } from '@desqu/ui'`.
  2. `tokens.css` = the `:root` variables + base element styles from `apps/screenbound/src/app.css`.
     The app's `app.css` imports `@desqu/ui/tokens.css` and keeps only app-specific rules.
  3. `TriStateSwitch.svelte`: same visuals as `Switch`, props `state: 'on' | 'off' | 'mixed'`,
     `onchange(next: boolean)`; mixed shows the knob centred and an accent outline. Keyboard: Space/Enter
     toggles; `role="switch"`, `aria-checked="mixed"` when mixed. Respect `prefers-reduced-motion`.
  4. `SettingsRow.svelte`: label, description, control slot.
  5. `shell-api.ts`: typed wrappers for `plugin:desqu-shell|…` + TS `ShellSettings` type mirroring Rust.
  6. `ShellSettingsSection.svelte`: rows for Start with system, Start minimized to tray, Close to tray.
- **Verify:** `task screenbound:check`.
- **Done when:**
  - [ ] Screenbound UI compiles using `@desqu/ui`.

#### P3.11 Standalone Screenbound adopts the shell — Status: ⬜
- **Depends on:** P2.4, P3.4–P3.10
- **Files:** `apps/screenbound/src-tauri/src/lib.rs`, delete `apps/screenbound/src-tauri/src/autostart.rs`,
  `apps/screenbound/src-tauri/tauri.conf.json`, capabilities, `apps/screenbound/ui/src/components/SettingsPanel.svelte`,
  `apps/screenbound/module/src/core/config.rs`, `apps/screenbound/ui/src/types.ts`
- **Do:**
  1. `tauri.conf.json`: `"visible": false` on the main window.
  2. `lib.rs`: logging (`desqu_shell::logging::init("screenbound_lib=info,screenbound_module=info,desqu_shell=info,warn")`),
     `desqu_shell::instance::plugin()`, `desqu_shell::autostart::plugin()`, `desqu_shell::init(config_dir)`,
     `screenbound_module::init(...)`, tray via `desqu_shell::tray` with label "Show ScreenBound",
     `window::install_close_handler`, `window::on_startup`. Remove the Rust-side `sb://config-changed`
     autostart listener from P2.2.
  3. **Config migration v1 → v2** in `module/src/core/config.rs`:
     - `CONFIG_VERSION = 2`.
     - Replace field `start_at_boot` with
       `#[serde(rename = "startAtBoot", default, skip_serializing)] pub legacy_start_at_boot: Option<bool>`.
     - Pure fn `pub fn take_legacy_start_at_boot(&mut self) -> Option<bool>` + unit tests
       (v1 JSON with `startAtBoot: true` → returns `Some(true)`, re-serialised JSON has no `startAtBoot`).
     - Shell startup: if `shell.json` does not exist and `legacy_start_at_boot == Some(v)` → write
       `ShellSettings { start_with_system: v, ..default }`, then save the config (now v2).
  4. Remove `startAtBoot` from `types.ts`; Settings panel renders `ShellSettingsSection` from
     `@desqu/ui` plus the existing animation setting.
  5. Capabilities: add `"desqu-shell:default"`.
- **Verify (real Windows):**
  - Existing config with `startAtBoot: true` → after first run `shell.json` has `startWithSystem: true`,
    `config.json` has `version: 2`, zones/rules/groups/knownApps unchanged (diff the JSON).
  - Sign-in with start-with-system on → window stays hidden, tray present, engine running.
  - Manual launch → window shows. Start minimized on → manual launch stays in tray.
  - Close to tray off → X quits and restores managed windows.
- **Done when:**
  - [ ] All checks pass; add results to `apps/screenbound/PLAN.md`.

---

### Phase 4 — Screenbound rebrand + settings migration

Goal: new identifier and product name (Q1) without losing any user settings or leaving the old app
fighting over windows.

#### P4.1 Change identity — Status: ⬜
- **Depends on:** P3.11, Q1
- **Files:** `apps/screenbound/src-tauri/tauri.conf.json` (`identifier`, `productName`, window
  `title`), tray tooltip/labels in `apps/screenbound/src-tauri/src/lib.rs`, `apps/screenbound/src/app.html`
  `<title>`, user-visible strings in `apps/screenbound/ui/**`, README/AGENTS/PLAN of the app,
  `apps/screenbound/Taskfile.yml` (`stop-app.ps1 -Names screenbound,ScreenBound,Screenbound`).
- **Do:** Use the exact values from Q1. Do **not** rename the Cargo package/bin (`screenbound.exe` stays).
  Keep `sb://` event names.
- **Done when:**
  - [ ] Build shows new name; `app.path().app_config_dir()` resolves to `%APPDATA%\<new id>`.

#### P4.2 Legacy config + journal migration — Status: ⬜
- **Depends on:** P4.1
- **Files:** `apps/screenbound/module/src/migrate.rs` (+ tests), called from the module plugin setup
  **before** the engine starts (so it also runs when Desqu hosts the module first).
- **Do:**
  1. `legacy_dir = app.path().config_dir()?.join("com.screenbound.app")` (Roaming base + old id).
  2. If `<new>/config.json` is missing and `<legacy>/config.json` exists → **copy** it to the new
     folder (never move). If the legacy config has `startAtBoot`, the P3.11 logic will pick it up.
  3. If `<legacy>/managed-windows.json` exists → run the engine's journal recovery with that path
     first (add `EnginePaths::legacy_journal: Option<PathBuf>`; `recover_journal` processes both),
     then delete only the legacy **journal** file.
  4. Write `<legacy>/MIGRATED.txt` with the date, new folder path, and "safe to delete" note.
  5. Never migrate twice: skip if `MIGRATED.txt` exists or the new config exists.
  6. Unit-test the pure decision function `fn plan_migration(new_cfg_exists, legacy_cfg_exists, marker_exists) -> MigrationPlan`.
- **Verify (real Windows):** copy a real 0.1.0 `%APPDATA%\com.screenbound.app` folder, start the new
  build → new folder has the same zones/rules/groups/knownApps; marker written; legacy config untouched.
- **Done when:**
  - [ ] Verified with a real 0.1.0 settings folder.

#### P4.3 Remove old autostart entry + detect old app — Status: ⬜
- **Depends on:** P4.2, P0.3, P0.4, Q2
- **Files:** `apps/screenbound/src-tauri/src/legacy.rs` (Windows-only code goes through
  `desqu-shell` platform helpers), `apps/screenbound/ui/src/components/SettingsPanel.svelte`
- **Do:**
  1. If the old Run value (name from P0.3) exists **and** its exe path differs from the current exe →
     `desqu_shell::autostart::remove_run_value(old_name)`.
  2. If a process named `screenbound.exe` with a **different** exe path is running → show a blocking
     notice in the UI: "An older ScreenBound is running. Right-click its tray icon and choose Exit."
     0.1.0 does not understand `--quit`, and a plain `taskkill` (WM_CLOSE) only hides it to the tray;
     never force-kill it (that skips its window restore). Re-check when the user clicks "Done".
     Until the old process is gone, keep the Screenbound engine **off** (do not fight).
  3. If the old uninstall key (P0.4) exists → Settings shows "Old ScreenBound is still installed —
     Uninstall" which runs its `UninstallString` (user sees the old uninstaller).
  4. Q2: if "delete", offer "Delete old settings folder" after successful migration; otherwise only
     mention the backup location.
- **Done when:**
  - [ ] Side-by-side install test passes (old running + new started → no window fighting).

#### P4.4 Migration documentation — Status: ⬜
- **Depends on:** P4.3
- **Files:** `docs/migration-screenbound.md`, `apps/screenbound/README.md`
- **Do:** Describe for users: install the new version, what is migrated, where the backup is,
  how to remove the old version. Keep it short. (Skip the user-facing part if Q3 says no users.)
- **Done when:**
  - [ ] Doc written and linked from README.

---

### Phase 5 — Updater, CI and GitHub releases

#### P5.1 Updater signing keys — Status: ⬜
- **Depends on:** Q6
- **Do (owner, not the AI):**
  1. For each product: `npx tauri signer generate -w $HOME/.tauri/<product>.key` (set a password).
  2. Add GitHub repo secrets: `<PRODUCT>_UPDATER_KEY` (file content), `<PRODUCT>_UPDATER_KEY_PASSWORD`
     (e.g. `SCREENBOUND_UPDATER_KEY`). If Q6 = shared key, use `DESQU_UPDATER_KEY` for all.
  3. Give the AI the **public** key text (`<product>.key.pub`) only. Never paste private keys in chat.
- **Done when:**
  - [ ] Secrets exist; public keys available.

#### P5.2 Update feed hosting — Status: ⬜
- **Depends on:** Q5
- **Do (proposal = gh-pages):** create an orphan `gh-pages` branch with `updates/.keep`; enable GitHub
  Pages from `gh-pages` / root. Feed URL per product:
  `https://wolfskii.github.io/Desqu/updates/<product>/latest.json`.
  Reason: GitHub's `releases/latest` is repo-wide, so one "latest" release cannot serve four products.
- **Done when:**
  - [ ] `https://wolfskii.github.io/Desqu/updates/.keep` returns 200.

#### P5.3 Updater in `desqu-shell` — Status: ⬜
- **Depends on:** P3.9, P5.1, P5.2, Q7
- **Files:** `crates/desqu-shell/Cargo.toml` (`tauri-plugin-updater = "2"`, `tauri-plugin-process = "2"`),
  `crates/desqu-shell/src/updates.rs`, permissions, each app's `tauri.conf.json` and capabilities,
  `packages/ui/src/UpdateCard.svelte`
- **Do:**
  1. App `tauri.conf.json`:
     ```json
     "bundle": { "createUpdaterArtifacts": true, ... },
     "plugins": { "updater": {
       "pubkey": "<public key text>",
       "endpoints": ["https://wolfskii.github.io/Desqu/updates/<product>/latest.json"],
       "windows": { "installMode": "passive" } } }
     ```
  2. Shell commands `check_update` (→ `{ available, version, notes, date } | null`) and
     `install_update`. `install_update` must **stop all modules first** (call `Module::stop(3s)` for
     each registered module) because the Windows updater ends the process without guaranteeing
     `RunEvent::Exit`; then `download_and_install`, then `app.restart()`.
  3. Portable (`app_info.portable == true`): `install_update` is not offered; UI shows "Download"
     linking to the release page.
  4. Automatic check: on startup (after 30 s) and every 24 h if `updates.checkAutomatically`; emit
     `desqu://update-available`; tray item "Check for updates" becomes "Update to vX".
  5. `UpdateCard.svelte`: current version, last check time, Check now, Install/Download, toggles for
     the two update settings.
  6. Capabilities: `"updater:default"`, `"process:allow-restart"` (or route everything through shell
     commands and skip these — preferred, fewer permissions).
- **Verify:** local test with a hand-written `latest.json` served from a local static server pointing
  at a higher-version setup exe signed with the key (`TAURI_SIGNING_PRIVATE_KEY` env var at build).
- **Done when:**
  - [ ] vN detects vN+1, installs passively, relaunches, settings intact; tampered `.sig` rejected.

#### P5.4 Version scripts — Status: ⬜
- **Depends on:** P1.6
- **Files:** `scripts/bump-version.ps1`, `scripts/check-versions.ps1`
- **Do:**
  - Source of truth = `apps/<app>/src-tauri/Cargo.toml` `[package] version`.
  - `bump-version.ps1 -App x -Version 1.2.3` sets the version in: app `src-tauri/Cargo.toml`,
    `src-tauri/tauri.conf.json`, app `package.json`, `apps/<x>/module/Cargo.toml` (if it exists),
    `apps/<x>/ui/package.json` (if it exists). Then `cargo update -w` to refresh `Cargo.lock`.
  - `check-versions.ps1 -App x [-Tag x-v1.2.3]` fails if any of those differ or the tag mismatches.
- **Done when:**
  - [ ] Both scripts work for screenbound.

#### P5.5 CI workflow — Status: ⬜
- **Depends on:** P1.8
- **Files:** `.github/workflows/ci.yml`, `scripts/check-standalone-deps.ps1`
- **Do:** On `pull_request` and `push` to `develop`/`main`, `runs-on: windows-latest`:
  1. `actions/checkout@v4`, `actions/setup-node@v4` (node 20, cache npm), `dtolnay/rust-toolchain@stable`
     (components clippy), `Swatinem/rust-cache@v2`, install Task (`arduino/setup-task@v2`).
  2. `npm ci`, `task ci`.
  3. `pwsh scripts/check-standalone-deps.ps1`: for every standalone app package (`screenbound`,
     `vials`, `workspaces`), `cargo tree -p <pkg> -e normal --prefix none` must not contain a line
     starting with `desqu ` (the collection app package); and their `package.json` files must not
     depend on `desqu-app`.
  4. `cargo build -p <pkg>` for each app separately.
  5. If no Authenticode cert (Q6), skip signing; note in `docs/releasing.md` that SmartScreen will warn.
- **Done when:**
  - [ ] CI green on a PR.

#### P5.6 Release workflow — Status: ⬜
- **Depends on:** P5.3, P5.4, P5.5
- **Files:** `.github/workflows/release.yml`, `scripts/write-update-feed.ps1`, `docs/releasing.md`
- **Do:** Trigger: `push` tags matching `desqu-v*`, `screenbound-v*`, `vials-v*`, `workspaces-v*`, plus
  `workflow_dispatch` with inputs `app`, `version`.
  1. Parse `<app>` and `<version>` from the tag (`screenbound-v0.2.0` → `screenbound`, `0.2.0`).
  2. `pwsh scripts/check-versions.ps1 -App <app> -Tag <tag>`.
  3. Env `TAURI_SIGNING_PRIVATE_KEY` / `_PASSWORD` from the product's secrets.
  4. `task <app>:release` → `dist/<app>/` contains setup.exe, setup.exe.sig, .msi, .msi.sig,
     portable.exe (update `publish-artifacts.ps1` to also copy the `.sig` files).
  5. Create `SHA256SUMS.txt`.
  6. `pwsh scripts/write-update-feed.ps1 -App <app>` → `dist/<app>/latest.json`:
     ```json
     { "version": "0.2.0", "notes": "<from CHANGELOG section>", "pub_date": "<UTC ISO8601>",
       "platforms": { "windows-x86_64": {
         "signature": "<content of setup.exe.sig>",
         "url": "https://github.com/Wolfskii/Desqu/releases/download/screenbound-v0.2.0/<setup file name>" } } }
     ```
  7. `gh release create <tag> dist/<app>/* --title "<ProductName> <version>" --notes-file <changelog excerpt> --latest=<true only for desqu>`
     (env `GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}`, job `permissions: contents: write`).
  8. Publish feed: `peaceiris/actions-gh-pages@v4` with `publish_branch: gh-pages`,
     `publish_dir: dist/<app>/feed` (only `latest.json`), `destination_dir: updates/<app>`,
     `keep_files: true` (keeps the other products' feeds).
  9. `docs/releasing.md`: how to bump (`bump-version.ps1`), update CHANGELOG, tag (`git tag screenbound-v0.2.0; git push origin screenbound-v0.2.0`), what the workflow does, how to build locally without CI.
- **Done when:**
  - [ ] A test pre-release (e.g. `screenbound-v0.2.0-rc.1`, marked prerelease) publishes assets and feed.

#### P5.7 First update-enabled Screenbound release — Status: ⬜
- **Depends on:** P4.4, P5.6
- **Do:** Bump to the agreed version (e.g. 0.2.0), update CHANGELOG, tag, let CI release. Install it
  manually over/next to 0.1.0 (migration path), then publish a 0.2.1 and confirm in-app update.
- **Done when:**
  - [ ] Real update from 0.2.0 → 0.2.1 works on a real machine.

---

### Phase 6 — Desqu collection app

#### P6.1 Scaffold `apps/desqu` — Status: ⬜
- **Depends on:** P3.11, P5.3, Q1, Q8
- **Files:** `apps/desqu/**` (copy the structure of `apps/screenbound` shell: `package.json`
  name `desqu-app`, `svelte.config.js`, `vite.config.js` with **port 1430 / HMR 1431**, `tsconfig.json`,
  `src/app.html`, `src/app.css`, `src/routes/+layout.ts` with `export const ssr = false;`,
  `src-tauri/{Cargo.toml, build.rs, tauri.conf.json, capabilities/default.json, icons/, src/main.rs, src/lib.rs}`),
  `apps/desqu/Taskfile.yml`, root `Taskfile.yml` include, root `Cargo.toml` member.
- **Do:** `tauri.conf.json`: productName `Desqu`, identifier from Q1, version `0.1.0`, main window
  `visible:false`, 1100×760, min 820×560, same CSP, NSIS `currentUser`, `createUpdaterArtifacts`,
  updater endpoint `…/updates/desqu/latest.json`. Icons: `npx tauri icon <source.png>` from Q8 asset.
  Cargo deps: `desqu-shell`, `screenbound-module`.
- **Verify:** `task desqu:dev` opens an empty window.
- **Done when:**
  - [ ] App builds and runs.

#### P6.2 Desqu settings and module registry — Status: ⬜
- **Depends on:** P6.1
- **Files:** `apps/desqu/src-tauri/src/{settings.rs, registry.rs, commands.rs}`
- **Do:**
  1. `DesquSettings { version, shell: ShellSettings (flattened), modules: BTreeMap<String, ModuleSettings> }`,
     `ModuleSettings { enabled: bool (default false), start_with_system: bool (default false) }`.
     File `%APPDATA%\<desqu-id>\settings.json`.
  2. Registry: list of `ModuleDescriptor { id, name, description, version, supported: bool }` +
     the `Arc<dyn Module>` from each module crate. Screenbound's `config_dir`/`data_dir` =
     `app.path().config_dir()?.join(<screenbound-id>)` (the **standalone** folder — D2).
  3. Commands: `list_modules` (descriptor + settings + live status), `set_module_enabled(id, bool)`
     (starts/stops the module immediately), `set_module_start_with_system(id, bool)`,
     `set_all_start_with_system(bool)`.
  4. Desqu's own Run entry is registered iff any module has `enabled && start_with_system`
     (call `desqu_shell::autostart::sync`).
  5. Startup: `--autostart` → start modules with `enabled && start_with_system`; manual launch → start
     all `enabled` modules. Window visibility via `window::on_startup`.
- **Done when:**
  - [ ] Unit tests for "should Desqu autostart" and "which modules start" (pure functions).

#### P6.3 Take-over from standalone — Status: ⬜
- **Depends on:** P6.2, P3.4, P3.8
- **Files:** `apps/desqu/src-tauri/src/takeover.rs`, `desqu-shell` platform helpers
- **Do:** When starting a module whose status becomes `OwnedElsewhere`:
  1. Emit `desqu://takeover-needed { id }`; UI asks "Screenbound is running on its own. Let Desqu
     manage it?".
  2. On yes: find the owning process' exe (process name from descriptor + `QueryFullProcessImageNameW`),
     run `<exe> --quit`, wait up to 5 s for the mutex to become free (retry `try_acquire` every 250 ms
     — this is a bounded wait, not background polling), then start the module.
  3. Remove the standalone app's Run value (`remove_run_value(<standalone run name>)`) and set its
     `shell.json` `startWithSystem=false` so it does not come back at sign-in.
  4. Standalone side (Screenbound shell): when its module reports `OwnedElsewhere` at startup, show the
     window with a single card "Screenbound is managed by Desqu" + buttons "Open Desqu" (launch Desqu
     exe found via its uninstall key, or hide if not found) and "Exit".
- **Verify (real Windows):** standalone running with a constrained window → enable in Desqu → standalone
  exits (window restored), Desqu re-constrains on next fullscreen; standalone Run value gone.
- **Done when:**
  - [ ] Both directions verified.

#### P6.4 Desqu UI — Status: ⬜
- **Depends on:** P6.2, P3.10
- **Files:** `apps/desqu/src/routes/{+layout.svelte, +page.svelte, settings/+page.svelte, [module]/+page.svelte}`,
  `apps/desqu/src/lib/{api.ts, state.svelte.ts, types.ts, components/ModuleCard.svelte, components/Sidebar.svelte}`
- **Do:**
  1. Layout: left sidebar (Home, one entry per module with status dot, Settings), content area.
     Use `@desqu/ui` tokens; custom design, no default checkboxes anywhere.
  2. Home: one `ModuleCard` per module: icon, name, one-line description, status text
     (On / Paused / Off / Running standalone / Not available on this OS), big `Switch` for enabled,
     smaller `Switch` "Start with system", "Open" button. Above the cards: `TriStateSwitch`
     "Start all apps with system".
  3. Module page: renders `ScreenboundView` from `@desqu/screenbound-ui` (its `api.ts` already calls
     `plugin:screenbound|…`). If off → the module's own "off" card with a Turn on button.
  4. Settings: `ShellSettingsSection` (start minimized, close to tray — note: Desqu's own
     start-with-system is derived from module settings, show it read-only with explanation),
     `UpdateCard`, About (version of Desqu + each module).
  5. Per module "More": "Reset settings" (confirm dialog warns that the standalone app shares them),
     "Remove standalone copy" (only if its uninstall key exists; runs its `UninstallString`).
- **Verify:** `task desqu:check`; manual click-through.
- **Done when:**
  - [ ] All screens work with keyboard and mouse.

#### P6.5 Desqu tray — Status: ⬜
- **Depends on:** P6.2, P3.7
- **Do:** Tray items: Show Desqu · ✓ Screenbound (check item toggles enabled) · Screenbound ▸ Pause /
  Release all windows (only when on) · Check for updates · Exit. Keep check states in sync when the
  UI changes settings (update the `CheckMenuItem` on every settings change).
- **Done when:**
  - [ ] Tray reflects UI changes and vice versa.

#### P6.6 Desqu verification + first release — Status: ⬜
- **Depends on:** P6.3–P6.5, P5.6
- **Do:** Run §7 acceptance checks A3–A7 for Desqu; tag `desqu-v0.1.0`.
- **Done when:**
  - [ ] Release published with `latest` flag; feed live.

---

### Phase 7 — Screenbound borderless gaming

#### P7.1 Design note + model — Status: ⬜
- **Depends on:** P2.4, Q11
- **Files:** `apps/screenbound/PLAN.md` (design decision entry), `apps/screenbound/module/src/core/rules.rs`,
  `apps/screenbound/ui/src/types.ts`
- **Do:**
  1. Add `Action::Borderless { target: BorderlessTarget, topmost: bool }` with
     `enum BorderlessTarget { Monitor, Zone { zone_id: String } }` (serde `tag = "type"`, camelCase;
     mirror in `types.ts`).
  2. Validation in `config.rs`: `zone_id` must exist (same as `FullscreenZone`). Unit tests.
- **Done when:**
  - [ ] Model + validation tests pass.

#### P7.2 Engine support — Status: ⬜
- **Depends on:** P7.1
- **Files:** `apps/screenbound/module/src/engine/mod.rs`, `apps/screenbound/module/src/core/*` (pure helpers)
- **Do:**
  1. Read `Engine::evaluate` (engine/mod.rs) first. Today it only acts when a window is
     fullscreen-like. Add a second path: when a visible, normal (not minimized), **framed** top-level
     window matches an enabled rule with a `Borderless` action and is not already managed →
     capture state (`platform.state`), journal it, `set_borderless`, compute target rect (monitor
     bounds of the monitor containing the window centre, or the resolved zone), `set_bounds`,
     re-read and verify (same verify-and-restore pattern as zone apply), `set_topmost` if requested.
  2. Release when: window destroyed, rule disabled/removed, module stopped, user "Release".
     Restore with `restore_state` (same as zone release).
  3. Do not apply to windows that are already fullscreen-like (that is the zone feature's job).
  4. Pure decision helpers in `core/` with unit tests (e.g. "should_apply_borderless(state, rule)").
- **Done when:**
  - [ ] Unit tests pass; journal recovery covers borderless entries.

#### P7.3 UI + manual tests — Status: ⬜
- **Depends on:** P7.2
- **Files:** `apps/screenbound/ui/src/components/RulesPanel.svelte`, `apps/screenbound/docs/manual-testing.md`
- **Do:** Rule action type selector (Fullscreen zone / Borderless window), target selector, topmost
  switch. Add a "Borderless gaming" section to manual-testing.md: windowed game, game that recreates
  its swapchain on resize, elevated game (expect "access denied" in activity log), exclusive fullscreen
  game (expect no change), exit game (window gone), quit Screenbound while applied (restore).
- **Done when:**
  - [ ] Manual results recorded in `apps/screenbound/PLAN.md` (🧪 → ✅ per game tested).

---

### Phase 8 — Desktop layer spike + Vials

#### P8.1 Spike: desktop icon layer (research only) — Status: ⬜
- **Depends on:** P3.11, Q9
- **Files:** `crates/desqu-desktop/` (throwaway `examples/spike_*.rs` allowed),
  results in `apps/vials/docs/desktop-research.md`
- **Answer each question with an observed result (Windows build number noted):**
  1. Enumerate desktop items from `FOLDERID_Desktop` (incl. OneDrive-redirected) and `FOLDERID_PublicDesktop`.
  2. Get the desktop `IFolderView` (shell windows → desktop → `IShellBrowser` → `IShellView` →
     `IFolderView`) and read/write item positions with `GetItemPosition` / `SelectAndPositionItems`.
     Does it work with "Auto arrange" on/off and "Align to grid"?
  3. Hiding options: (a) move icons under the vial area and draw the vial on top; (b)
     `FILE_ATTRIBUTE_HIDDEN`; (c) other. Which survive Explorer restart, Win+D, sign-out?
     Which modify the user's files (b does — avoid unless nothing else works)?
  4. Vial window placement: child of the `WorkerW`/`Progman` desktop window (undocumented
     `0x052C` message) vs. bottom-most normal window. Which survives Win+D and does not steal focus?
  5. Icon extraction (`IShellItemImageFactory`) cost for 200 items.
  6. Change notifications: `SHChangeNotifyRegister` vs `ReadDirectoryChangesW` (event-driven only).
  7. Per-monitor DPI effects on icon positions.
  8. Explorer restart detection (`RegisterWindowMessage("TaskbarCreated")`).
- **Done when:**
  - [ ] Each question answered; chosen approach written as a decision in the research doc.

#### P8.2 `desqu-desktop` crate — Status: ⬜
- **Depends on:** P8.1
- **Do:** Implement only what the spike chose, behind a `DesktopLayer` trait (Windows impl +
  unsupported stub): `items()`, `icon(item)`, `positions()`, `hide(items)`, `show(items)`,
  `subscribe(sink)`. Journal every hide/position change to `<data_dir>/desktop-journal.json` and
  restore on start/stop/crash recovery. Cross-process lock `Local\Desqu.DesktopLayer` (Q10: single owner).
- **Done when:**
  - [ ] Kill -9 while items hidden → next start restores them.

#### P8.3 Vials module + standalone app + Desqu integration — Status: ⬜
- **Depends on:** P8.2, Q10, Q1
- **Do:** Same shape as Screenbound (P2.1–P2.3, P3.11): `apps/vials/module` (core: `Vial { id, name,
  monitor_id, rect: NormalizedRect, items: Vec<ItemRef>, collapsed }`, `SortRule { id, name, matcher:
  ItemMatcher (extension | kind: shortcut/folder/app/file | name glob), target_vial }` — pure, unit
  tested), plugin `vials`, `apps/vials/ui`, standalone shell with its own identifier, add to Desqu
  registry and UI. Off behaviour per §5.6. Update feed `updates/vials/`.
- **Done when:**
  - [ ] Manual test doc `apps/vials/docs/manual-testing.md` written and passed.

---

### Phase 9 — Virtual desktop spike + Workspaces

#### P9.1 Spike: Windows virtual desktops — Status: ⬜
- **Depends on:** P8.2, Q9
- **Files:** results in `apps/workspaces/docs/virtual-desktop-research.md`
- **Answer on Windows 10 22H2, Windows 11 23H2 and 24H2 (record build numbers):**
  1. List desktop IDs and names: registry `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops`
     (`VirtualDesktopIDs`, `Desktops\{GUID}\Name`) vs. undocumented `IVirtualDesktopManagerInternal`.
  2. Detect current desktop + switches event-driven: `RegNotifyChangeKeyValue` on `CurrentVirtualDesktop`
     (where does it live on each build?) vs. undocumented `IVirtualDesktopNotification`.
  3. Public `IVirtualDesktopManager::GetWindowDesktopId` for other processes' windows — works?
  4. Moving another process' window to a desktop: public `MoveWindowToDesktop` (expected: fails for
     foreign windows) vs. undocumented internal API. Record GUIDs per build.
  5. Getting a running app's launch info (exe path from `QueryFullProcessImageNameW`; command line
     needs WMI/NtQueryInformationProcess — feasible unelevated?).
  6. Open Explorer folder windows via `IShellWindows` → folder paths.
- **Done when:**
  - [ ] Matrix filled; supported builds list decided; fallback behaviour defined for unknown builds.

#### P9.2 Workspaces module + app — Status: ⬜
- **Depends on:** P9.1, Q10
- **Do:** Same shape as Vials. Core model: `Workspace { desktop_id, name, visible_items: Vec<ItemRef>,
  apps: Vec<AppLaunch { exe, args: Option<String>, cwd: Option<PathBuf> }>, folders: Vec<PathBuf> }`.
  On desktop switch: snapshot leaving desktop (if "auto-save" on), apply arriving desktop
  (`desqu-desktop` show/hide). Restore action: launch missing apps/folders and move their windows to the
  desktop (only on supported builds; otherwise launch only and tell the user). Off per §5.6.
- **Done when:**
  - [ ] Manual test doc written and passed on at least two Windows builds.

---

## 7. Acceptance criteria (repo-wide)

| ID | Check | How |
| --- | --- | --- |
| A1 | Every app builds/tests from its own folder and from the root | `cd apps/<x>; task ci` and `task ci` |
| A2 | Standalone apps never depend on Desqu | `scripts/check-standalone-deps.ps1` in CI |
| A3 | Screenbound settings preserved through move, rebrand and Desqu take-over | Diff JSON of zones/rules/groups/knownApps before/after |
| A4 | "Off" really frees resources | Process Explorer: no module threads; Task Manager idle CPU ~0% |
| A5 | Startup matrix | Per-app start with system, All apps tri-state, autostart hidden, start minimized, close to tray on/off, tray Exit restores windows |
| A6 | Take-over both directions | P6.3 verify |
| A7 | Updates per product isolated | Release screenbound → Desqu does not see an update; Desqu release → standalone Screenbound does not |
| A8 | Updater security | Tampered `.sig` / wrong key → update refused |
| A9 | Portable builds | No install button; download link shown; still works |
| A10 | Desktop items never lost | Kill -9 / Explorer restart / reboot while Vials or Workspaces active → all icons back |
| A11 | Existing Screenbound manual tests still pass | `apps/screenbound/docs/manual-testing.md` |

---

## 8. Risks and mitigations

| Risk | Mitigation | Step |
| --- | --- | --- |
| `[profile.release]` silently ignored after the workspace move (loses LTO/strip/`panic = "unwind"`) | Move it to root `Cargo.toml`; check exe size | P1.2, P1.8 |
| Root-anchored `.gitignore` stops ignoring `build/`, `.svelte-kit/` | New patterns | P1.4 |
| Build output moves to root `target/` → scripts break | Parameterized scripts | P1.6 |
| Command renames (`plugin:screenbound|…`) break UI | Change `api.ts` + capabilities in the same step | P2.2, P2.3 |
| New identifier loses settings | Copy-not-move migration + marker + tests | P4.2 |
| Old ScreenBound and new Screenbound fight over windows | Detect old process, keep engine off until gone | P4.3 |
| Standalone uninstaller "Delete app data" deletes settings Desqu uses | Warn in Desqu UI and docs | P6.4 |
| Windows updater exits without `RunEvent::Exit` → windows left constrained | Stop modules before `download_and_install`; journal recovers anyway | P5.3 |
| `releases/latest` is repo-wide | Per-product feeds on gh-pages | P5.2 |
| No Authenticode → SmartScreen warnings | Document; decide Q6 | P5.5 |
| Vials and Workspaces both touch desktop icons | Single owner lock in `desqu-desktop`, Q10 | P8.2 |
| Undocumented virtual desktop APIs change per build | Build-gated, fallback to "launch only" | P9.1 |
| Many vials = many WebViews | Spike one window per monitor | P8.1 |
| Anti-cheat / elevated / exclusive-fullscreen games | Document as limitations; log clearly | P7.3 |

### Cross-platform status (do not overclaim)

| Feature | Windows | macOS | Linux |
| --- | --- | --- | --- |
| Desqu UI, settings, toggles | Target | Builds (Tauri), untested | Builds (Tauri), untested |
| Tray | Target | Tauri supports | Needs appindicator; limited click events |
| Start with system, single instance | Target | Plugins support | Plugins support |
| Updater | NSIS/MSI | `.app.tar.gz` | AppImage only |
| Screenbound zones + borderless | Target (only implemented backend) | Not planned (native fullscreen = separate Space; no API to restyle other apps' windows) | X11 maybe; Wayland not possible generally |
| Vials | Target (spike) | Not possible per item (Finder hides all or none) | Desktop-environment specific |
| Workspaces | Partial (spike; undocumented APIs) | No public Spaces API | X11 EWMH possible; Wayland compositor-specific |

---

## 9. Progress log

Append one line per work session / step change. Newest at the bottom.
Format: `YYYY-MM-DD — <step id> — <status> — <what was done / where you stopped / what is next>`

- 2026-10-10 — plan — created — Plan written from repo inspection. Next: P0.1 (answer Q1–Q11) and P0.2–P0.5 baseline on real Windows.
- 2026-10-10 — P0.1 — 🟡 — Q4 answered: yes, monorepo. Phase 1 now only waits on P0.5 (artifact baseline). Remaining: Q1–Q3, Q5–Q11.
- 2026-10-10 — P0.5 — ✅ — `task ci` passes: svelte-check clean, 37 Rust unit tests passed + 2 ignored (probes), clippy clean. `task release` → `dist/ScreenBound_0.1.0_x64-setup.exe` 2,506,038 B; `dist/ScreenBound_0.1.0_x64.msi` 3,813,376 B; `dist/ScreenBound_0.1.0_x64-portable.exe` 10,521,088 B. Next: P1.1.
- 2026-10-10 — P1.1 — ✅ — Moved Screenbound with git mv (81 renames). Root Cargo.lock now; apps/screenbound/src-tauri/target (ignored cache) and gen/ moved along, not deleted (owner to decide). Needed to stop rust-analyzer/VS Code handles on src/. Next: P1.2.
