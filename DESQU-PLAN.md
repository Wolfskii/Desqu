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
9. Commit after each completed step with a short conventional message (`feat:`, `fix:`, `docs:`,
   `refactor:`, `build:`, `chore:`; e.g. `feat: implement x and y`). **Never push**, force-push or
   rewrite pushed history without the owner's explicit say-so (the owner pushes). Stage explicit
   paths only. Never add Claude/Anthropic attribution to commits or PRs.

### Phase overview

| Phase | Name | Status | Depends on |
| --- | --- | --- | --- |
| 0 | Baseline measurements and decisions | 🟡 | — |
| 1 | Monorepo move (behaviour identical) | ✅ | 0 |
| 2 | Screenbound module crate + UI package | ✅ | 1 |
| 3 | Shared shell crate + shared UI kit | ✅ | 1 (can run in parallel with 2) |
| 4 | Screenbound rebrand + settings migration | 🧪 | 2, 3, Q1, Q2 |
| 5 | Updater, CI and GitHub releases | 🧪 | 3, Q5, Q6 |
| 6 | Desqu collection app | 🟡 | 2, 3, 5 |
| 7 | Screenbound borderless gaming | 🧪 | 2 |
| 8 | Desktop layer spike + Vials | 🧪 | 3 |
| 9 | Virtual desktop spike + Workspaces | 🧪 | 8 |

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

### Measured on the owner's machine (2026-10-10, read-only registry/file queries)

Installed ScreenBound 0.1.0 (NSIS, current user):

- **Run entry** (P0.3): `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value name
  `ScreenBound` (the Tauri productName), data `C:\Users\dawid\AppData\Local\ScreenBound\screenbound.exe`
  with **no arguments**. `...\Explorer\StartupApproved\Run` also has a `ScreenBound` value
  (Windows writes it; the plugin only writes `Run`). Registry value names are case-insensitive, so a
  rebrand to `Screenbound` **overwrites** the same Run value rather than adding a second one.
- **Install folder** (P0.4): `%LOCALAPPDATA%\ScreenBound` containing `screenbound.exe` and
  `uninstall.exe` (so "installed" = `uninstall.exe` next to the exe; portable has none).
- **Uninstall key**: `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\ScreenBound`
  — `DisplayName` ScreenBound, `DisplayVersion` 0.1.0, `Publisher` `screenbound`,
  `InstallLocation` `"C:\Users\dawid\AppData\Local\ScreenBound"`,
  `UninstallString` `"C:\Users\dawid\AppData\Local\ScreenBound\uninstall.exe"`,
  `QuietUninstallString` empty (no silent flag recorded; NSIS accepts `/S`).
- **Other**: `HKCU\Software\screenbound` key exists (empty); Start-menu shortcut
  `%APPDATA%\Microsoft\Windows\Start Menu\Programs\ScreenBound.lnk`; settings folder
  `%APPDATA%\com.screenbound.app` held only `config.json` (3,426 bytes), no journal.

---

## 4. Open questions (owner must answer; record the answer here)

| ID | Question | Blocks | Answer |
| --- | --- | --- | --- |
| Q1 | New bundle identifiers and product names. Proposal: `io.github.wolfskii.desqu`, `io.github.wolfskii.screenbound`, `io.github.wolfskii.vials`, `io.github.wolfskii.workspaces`; product names `Desqu`, `Screenbound`, `Vials`, `Workspaces`. | P4.1, P6.1, P8, P9 | **Assumed 2026-10-10 (owner said proceed without confirmation; change any time):** proposal as written — identifiers `io.github.wolfskii.{desqu,screenbound,vials,workspaces}`, product names Desqu, Screenbound, Vials, Workspaces. |
| Q2 | After migrating, keep `%APPDATA%\com.screenbound.app` as a backup (proposal: keep + marker file) or delete it? | P4.3 | **Assumed 2026-10-10 (owner said proceed without confirmation; change any time):** keep `%APPDATA%\com.screenbound.app` as a backup plus a `MIGRATED.txt` marker; Desqu/Screenbound only *offer* to delete it. |
| Q3 | Are there published releases / public 0.1.0 users? Decides how much migration UI is needed. | P4.4 | **Assumed 2026-10-10 (owner said proceed without confirmation; change any time):** treat 0.1.0 as possibly in use: full migration (copy config, recover journal, remove old Run entry, detect old app). |
| Q4 | OK that a standalone app builds from its own folder but still needs the repo's shared `crates/` and `packages/` (monorepo)? Alternative = publish/vendor shared code. Proposal: yes, monorepo. | P1, P5 | ✅ Yes (2026-10-10): monorepo; standalone apps may depend on `crates/` and `packages/`. |
| Q5 | Update feed hosting: `gh-pages` branch served by GitHub Pages (proposal), plain `updates` branch via raw URL, or rolling release tag per product? | P5.2 | **Assumed 2026-10-10 (owner said proceed without confirmation; change any time):** `gh-pages` branch served by GitHub Pages, `updates/<product>/latest.json`. |
| Q6 | Updater signing: one key per product (proposal) or one shared key? Is an Authenticode code-signing certificate available? | P5.1, P5.5 | **Assumed 2026-10-10 (owner said proceed without confirmation; change any time):** one updater key per product (keys generated locally, never committed); no Authenticode certificate (SmartScreen will warn; documented). |
| Q7 | Updates: notify only (proposal) or auto-install? Daily update timer acceptable vs. "no polling" rule (proposal: yes, it is not window polling)? On close-to-tray: hide window (current) or destroy WebView to save RAM? | P3.6, P5.3 | **Assumed 2026-10-10 (owner said proceed without confirmation; change any time):** notify only (install on click); daily check timer is acceptable; closing the window hides it (WebView kept). |
| Q8 | Desqu branding: new logo/icons or reuse mascot assets in `assets/`? | P6.1 | **Assumed 2026-10-10 (owner said proceed without confirmation; change any time):** reuse the existing mascot logo from `apps/screenbound/assets/` for a placeholder Desqu icon until real branding exists. |
| Q9 | Minimum Windows version: Windows 10 22H2 + 11, or 11 only? (Workspaces may end up 11-only.) | P8, P9 | **Assumed 2026-10-10 (owner said proceed without confirmation; change any time):** Windows 10 22H2 and Windows 11; Workspaces' virtual-desktop features gated by build number with launch-only fallback. |
| Q10 | When Vials and Workspaces are both on: proposal = Workspaces decides which items are visible per virtual desktop, Vials arranges within that. OK? | P8.3, P9.3 | **Assumed 2026-10-10 (owner said proceed without confirmation; change any time):** Workspaces decides which items are visible per virtual desktop, Vials arranges them within that. |
| Q11 | Borderless gaming trigger: per-rule automatic only (proposal), or also a global hotkey (needs `RegisterHotKey`)? | P7 | **Assumed 2026-10-10 (owner said proceed without confirmation; change any time):** per-rule automatic only; no global hotkey. |

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
| `apps/screenbound/module` | `screenbound-module` | lib `screenbound_module` | Tauri plugin name `screenbound-module` |
| `apps/screenbound/src-tauri` | `screenbound` | bin `screenbound`, lib `screenbound_lib` | Keeps `screenbound.exe` |
| `apps/desqu/src-tauri` | `desqu` | bin `desqu`, lib `desqu_lib` | |
| `apps/vials/module` | `vials-module` | lib `vials_module` | plugin name `vials-module` |
| `apps/vials/src-tauri` | `vials` | bin `vials`, lib `vials_lib` | |
| `apps/workspaces/module` | `workspaces-module` | lib `workspaces_module` | plugin name `workspaces-module` |
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
- The plugin name **must equal the Cargo package name** (`screenbound-module`, `vials-module`, …):
  `tauri-plugin` derives the permission namespace from the package name and `links` must match it,
  so a shorter runtime name would fail the permission check (found in P2.2).
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

#### P0.3 Record the autostart registry entry — Status: ✅
- **Depends on:** P0.2 (Start at boot enabled)
- **Do:** In PowerShell:
  `Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'` and
  `Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run'`.
  Record the value name(s) and data (exe path, arguments) in §3.
- **Done when:**
  - [x] Exact value name and path recorded.

#### P0.4 Record the installer footprint — Status: ✅
- **Depends on:** P0.2
- **Do:** Record:
  - Install folder (likely `%LOCALAPPDATA%\ScreenBound` — confirm).
  - Uninstall key: `Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall' | Where-Object { (Get-ItemProperty $_.PSPath).DisplayName -like '*ScreenBound*' }` → record key name, `UninstallString`, `QuietUninstallString`, `DisplayVersion`.
  - Any `HKCU:\Software\<publisher>\ScreenBound` key.
  - Start-menu shortcut path.
  - Whether `uninstall.exe` exists next to `screenbound.exe` in the install folder (used later to
    detect portable vs installed).
- **Done when:**
  - [x] All values recorded in §3.

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

#### P1.1 Move Screenbound files with git — Status: ✅
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

#### P1.2 Create the Cargo workspace — Status: ✅
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
  - [x] Workspace builds; only one `Cargo.lock` (root); release profile only in root `Cargo.toml`.

#### P1.3 Create the npm workspace — Status: ✅
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
  - [x] `npm install` at root works; `node_modules/` only at root (plus any nested ones npm decides).

#### P1.4 Fix ignore files — Status: ✅
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

#### P1.5 Split Taskfiles — Status: ✅
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
  - [x] Both root and app-folder invocations work.

#### P1.6 Parameterize release scripts — Status: ✅
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
  - [x] Artifacts identical in name; sizes within a few % of P0.5.

#### P1.7 Split docs and agent rules — Status: ✅
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
  - [x] All docs in place, links valid.

#### P1.8 Phase 1 regression check — Status: ✅
- **Depends on:** P1.2–P1.7
- **Do:**
  1. `task ci` (root) passes.
  2. `task screenbound:dev`: app starts, all tabs load, config from `%APPDATA%\com.screenbound.app`
     is shown (settings preserved).
  3. Run the **Core flow** and **Lifecycle** sections of `apps/screenbound/docs/manual-testing.md`.
  4. `task screenbound:release` + install; confirm exe is stripped (size similar to P0.5).
- **Done when:**
  - [x] All four checks pass. Phase 1 → ✅ in the phase table.

---

### Phase 2 — Screenbound module crate + UI package

Goal: Screenbound's logic becomes a reusable Tauri plugin crate and a Svelte UI package. The
standalone app becomes a thin shell. Behaviour stays identical except: the module can now be
started/stopped at runtime.

#### P2.1 Create `screenbound-module` crate and move code — Status: ✅
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
  - [x] Tests moved and passing; app still runs exactly as before.

#### P2.2 Turn the commands into a Tauri plugin — Status: ✅
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
     `Builder::new("screenbound-module").invoke_handler(generate_handler![...]).setup(|app, _| { manage ModuleState; if options.start_immediately { start } }).on_event(|app, ev| if let RunEvent::Exit = ev { stop(3s) }).build()`.
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
     `["core:default", "screenbound-module:default"]`.
- **Verify:** `cargo build -p screenbound`; app starts; DevTools console shows no permission errors
  once P2.3 switches the frontend.
- **Done when:**
  - [x] All 9 old commands available as `plugin:screenbound-module|…`; 3 new lifecycle commands work.

#### P2.3 Create `@desqu/screenbound-ui` and switch invoke names — Status: ✅
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
  4. In `api.ts`, change every `invoke('x', ...)` to `invoke('plugin:screenbound-module|x', ...)`. Event names
     stay `sb://…`. Add `moduleStart`, `moduleStop`, `moduleStatus`.
  5. `ScreenboundView` shows an "Screenbound is off — Turn on" card when `moduleStatus()` is `off`.
  6. App `package.json`: add dependency `"@desqu/screenbound-ui": "*"`; run `npm install` at root.
  7. Keep `app.css` import in the app's `+layout.svelte` (moves to `@desqu/ui` in Phase 3).
- **Verify:** `task screenbound:check`; `task screenbound:dev` — all six tabs work, saving works,
  Diagnostics list loads, activity log streams.
- **Done when:**
  - [x] UI identical to before; all calls go through the plugin.

#### P2.4 Runtime start/stop works — Status: ✅
- **Depends on:** P2.3
- **Files:** `apps/screenbound/module/src/plugin.rs`, a temporary dev-only button is **not** needed:
  call from DevTools console `window.__TAURI_INTERNALS__.invoke('plugin:screenbound-module|module_stop')`.
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

#### P3.1 Create `desqu-shell` crate skeleton — Status: ✅
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
  - [x] Crate compiles with empty-but-declared modules.

#### P3.2 Shell settings model + store — Status: ✅
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

#### P3.3 Launch arguments — Status: ✅
- **Depends on:** P3.1
- **Files:** `crates/desqu-shell/src/args.rs`
- **Do:** `pub struct LaunchArgs { pub autostart: bool, pub quit: bool }`,
  `pub fn parse<I: IntoIterator<Item = String>>(args: I) -> LaunchArgs` recognising `--autostart` and
  `--quit` (ignore everything else). Unit tests.
- **Done when:**
  - [ ] Tests pass.

#### P3.4 Single instance + `--quit` forwarding — Status: ✅
- **Depends on:** P3.3
- **Files:** `crates/desqu-shell/src/instance.rs`
- **Do:** `pub fn plugin<R: Runtime>() -> TauriPlugin<R>` wrapping `tauri_plugin_single_instance::init`.
  Callback: parse args; if `quit` → `app.exit(0)` (this runs `RunEvent::Exit`, so modules restore
  windows); else → `window::show_main(app)`. Also, in the **first** instance's setup: if `--quit` was
  passed and no other instance existed → exit immediately without starting modules.
- **Verify (manual):** with app running, run `screenbound.exe --quit` → running app exits and restores
  managed windows.
- **Done when:**
  - [x] Behaviour confirmed (🧪 until manual check).

#### P3.5 Autostart with `--autostart` arg — Status: ✅
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
  - [x] Registry entry contains `--autostart`.

#### P3.6 Window lifecycle: start hidden, start minimized, close to tray — Status: ✅
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
  - [x] Functions implemented and used by Screenbound in P3.8.

#### P3.7 Tray builder — Status: ✅
- **Depends on:** P3.6
- **Files:** `crates/desqu-shell/src/tray.rs`
- **Do:** `TrayConfig { tooltip, show_label, extra_items: Vec<TrayItem>, on_item: Fn(&AppHandle, &str) }`
  where `TrayItem` is `Normal{id,label}` | `Check{id,label,checked}` | `Separator`.
  Build menu: `show_label`, separator, extra items, separator, "Check for updates" (id `updates`,
  hidden until Phase 5), "Exit" (id `exit` → `app.exit(0)`). Left click (button up) → `show_main`.
  Return the `TrayIcon` so callers can update check items. Use the app's default window icon
  (copy whatever icon handling Screenbound's `install_tray` uses today).
- **Done when:**
  - [x] Screenbound tray looks and behaves as before (P3.8).

#### P3.8 Module mutex + `Module` trait — Status: ✅
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
  - [x] Lock test passes; Screenbound reports `OwnedElsewhere` correctly (two-process check happens in P6.3).

#### P3.9 Shell plugin commands — Status: ✅
- **Depends on:** P3.2–P3.7
- **Files:** `crates/desqu-shell/src/lib.rs`, `crates/desqu-shell/build.rs`, `crates/desqu-shell/permissions/default.toml`
- **Do:** Tauri plugin `desqu-shell` with commands: `get_shell_settings`, `set_shell_settings`
  (saves, then calls `autostart::sync`), `app_info` (→ `{ name, version, identifier, portable: bool }`;
  `portable` = no `uninstall.exe` next to the current exe — confirm the file name from P0.4).
  `pub fn init(config_dir)` loads settings into managed state. Same build.rs/permissions pattern as P2.2.
- **Done when:**
  - [ ] Commands callable as `plugin:desqu-shell|…`.

#### P3.10 Shared UI kit `@desqu/ui` — Status: ✅
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
  - [x] Screenbound UI compiles using `@desqu/ui`.

#### P3.11 Standalone Screenbound adopts the shell — Status: ✅
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

#### P4.1 Change identity — Status: ✅
- **Depends on:** P3.11, Q1
- **Files:** `apps/screenbound/src-tauri/tauri.conf.json` (`identifier`, `productName`, window
  `title`), tray tooltip/labels in `apps/screenbound/src-tauri/src/lib.rs`, `apps/screenbound/src/app.html`
  `<title>`, user-visible strings in `apps/screenbound/ui/**`, README/AGENTS/PLAN of the app,
  `apps/screenbound/Taskfile.yml` (`stop-app.ps1 -Names screenbound,ScreenBound,Screenbound`).
- **Do:** Use the exact values from Q1. Do **not** rename the Cargo package/bin (`screenbound.exe` stays).
  Keep `sb://` event names.
- **Done when:**
  - [ ] Build shows new name; `app.path().app_config_dir()` resolves to `%APPDATA%\<new id>`.

#### P4.2 Legacy config + journal migration — Status: ✅
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

#### P4.3 Remove old autostart entry + detect old app — Status: 🧪
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

#### P4.4 Migration documentation — Status: ✅
- **Depends on:** P4.3
- **Files:** `docs/migration-screenbound.md`, `apps/screenbound/README.md`
- **Do:** Describe for users: install the new version, what is migrated, where the backup is,
  how to remove the old version. Keep it short. (Skip the user-facing part if Q3 says no users.)
- **Done when:**
  - [x] Doc written and linked from README.

---

### Phase 5 — Updater, CI and GitHub releases

#### P5.1 Updater signing keys — Status: 🧪
- **Depends on:** Q6
- **Do (owner, not the AI):**
  1. For each product: `npx tauri signer generate -w $HOME/.tauri/<product>.key` (set a password).
  2. Add GitHub repo secrets: `<PRODUCT>_UPDATER_KEY` (file content), `<PRODUCT>_UPDATER_KEY_PASSWORD`
     (e.g. `SCREENBOUND_UPDATER_KEY`). If Q6 = shared key, use `DESQU_UPDATER_KEY` for all.
  3. Give the AI the **public** key text (`<product>.key.pub`) only. Never paste private keys in chat.
- **Done when:**
  - [ ] Secrets exist; public keys available.

#### P5.2 Update feed hosting — Status: ✅
- **Depends on:** Q5
- **Do (proposal = gh-pages):** create an orphan `gh-pages` branch with `updates/.keep`; enable GitHub
  Pages from `gh-pages` / root. Feed URL per product:
  `https://wolfskii.github.io/Desqu/updates/<product>/latest.json`.
  Reason: GitHub's `releases/latest` is repo-wide, so one "latest" release cannot serve four products.
- **Done when:**
  - [x] `https://wolfskii.github.io/Desqu/updates/.keep` returns 200.

#### P5.3 Updater in `desqu-shell` — Status: 🧪
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

#### P5.4 Version scripts — Status: ✅
- **Depends on:** P1.6
- **Files:** `scripts/bump-version.ps1`, `scripts/check-versions.ps1`
- **Do:**
  - Source of truth = `apps/<app>/src-tauri/Cargo.toml` `[package] version`.
  - `bump-version.ps1 -App x -Version 1.2.3` sets the version in: app `src-tauri/Cargo.toml`,
    `src-tauri/tauri.conf.json`, app `package.json`, `apps/<x>/module/Cargo.toml` (if it exists),
    `apps/<x>/ui/package.json` (if it exists). Then `cargo update -w` to refresh `Cargo.lock`.
  - `check-versions.ps1 -App x [-Tag x-v1.2.3]` fails if any of those differ or the tag mismatches.
- **Done when:**
  - [x] Both scripts work for screenbound.

#### P5.5 CI workflow — Status: 🧪
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

#### P5.6 Release workflow — Status: ✅
- **Depends on:** P5.3, P5.4, P5.5
- **Files:** `.github/workflows/release.yml`, `scripts/write-update-feed.ps1`, `docs/releasing.md`
- **Do:** Trigger: `push` tags matching `desqu-v*`, `screenbound-v*`, `vials-v*`, `workspaces-v*`, plus
  `workflow_dispatch` with inputs `app`, `version`.
  1. Parse `<app>` and `<version>` from the tag (`screenbound-v0.2.0` → `screenbound`, `0.2.0`).
  2. `pwsh scripts/check-versions.ps1 -App <app> -Tag <tag>`.
  3. Env `TAURI_SIGNING_PRIVATE_KEY` / `_PASSWORD` from the product's secrets.
  4. `task <app>:release:signed` (not `release`, which builds unsigned since 2026-10-10) →
     `dist/<app>/` contains setup.exe, setup.exe.sig, .msi, .msi.sig,
     portable.exe (`publish-artifacts.ps1` also copies the `.sig` files).
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
  - [x] A test pre-release (e.g. `screenbound-v0.2.0-rc.1`, marked prerelease) publishes assets and feed.
    Verified for real on 2026-10-17 by `desqu-v0.2.0` (run 39043500529): 6 assets on the release,
    feed at `updates/desqu/latest.json` (200). The `-rc.1` pre-release path is the same code minus
    the `--latest` flag; not separately exercised.

#### P5.7 First update-enabled Screenbound release — Status: 🧪
- **Depends on:** P4.4, P5.6
- **Do:** Bump to the agreed version (e.g. 0.2.0), update CHANGELOG, tag, let CI release. Install it
  manually over/next to 0.1.0 (migration path), then publish a 0.2.1 and confirm in-app update.
- **Done when:**
  - [ ] Real update from 0.2.0 → 0.2.1 works on a real machine.

---

### Phase 6 — Desqu collection app

#### P6.1 Scaffold `apps/desqu` — Status: ✅
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

#### P6.2 Desqu settings and module registry — Status: ✅
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

#### P6.3 Take-over from standalone — Status: 🧪
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

#### P6.4 Desqu UI — Status: 🧪
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
     `plugin:screenbound-module|…`). If off → the module's own "off" card with a Turn on button.
  4. Settings: `ShellSettingsSection` (start minimized, close to tray — note: Desqu's own
     start-with-system is derived from module settings, show it read-only with explanation),
     `UpdateCard`, About (version of Desqu + each module).
  5. Per module "More": "Reset settings" (confirm dialog warns that the standalone app shares them),
     "Remove standalone copy" (only if its uninstall key exists; runs its `UninstallString`).
- **Verify:** `task desqu:check`; manual click-through.
- **Done when:**
  - [ ] All screens work with keyboard and mouse.

#### P6.5 Desqu tray — Status: 🧪
- **Depends on:** P6.2, P3.7
- **Do:** Tray items: Show Desqu · ✓ Screenbound (check item toggles enabled) · Screenbound ▸ Pause /
  Release all windows (only when on) · Check for updates · Exit. Keep check states in sync when the
  UI changes settings (update the `CheckMenuItem` on every settings change).
- **Done when:**
  - [ ] Tray reflects UI changes and vice versa.

#### P6.6 Desqu verification + first release — Status: 🧪
- **Depends on:** P6.3–P6.5, P5.6
- **Do:** Run §7 acceptance checks A3–A7 for Desqu; tag `desqu-v0.1.0`.
- **Done when:**
  - [ ] Release published with `latest` flag; feed live.

---

### Phase 7 — Screenbound borderless gaming

#### P7.1 Design note + model — Status: ✅
- **Depends on:** P2.4, Q11
- **Files:** `apps/screenbound/PLAN.md` (design decision entry), `apps/screenbound/module/src/core/rules.rs`,
  `apps/screenbound/ui/src/types.ts`
- **Do:**
  1. Add `Action::Borderless { target: BorderlessTarget, topmost: bool }` with
     `enum BorderlessTarget { Monitor, Zone { zone_id: String } }` (serde `tag = "type"`, camelCase;
     mirror in `types.ts`).
  2. Validation in `config.rs`: `zone_id` must exist (same as `FullscreenZone`). Unit tests.
- **Done when:**
  - [x] Model + validation tests pass.

#### P7.2 Engine support — Status: ✅
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
  - [x] Unit tests pass; journal recovery covers borderless entries.

#### P7.3 UI + manual tests — Status: 🧪
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

#### P8.1 Spike: desktop icon layer (research only) — Status: ✅
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
  - [x] Each question answered; chosen approach written as a decision in the research doc.

#### P8.2 `desqu-desktop` crate — Status: ✅
- **Depends on:** P8.1
- **Do:** Implement only what the spike chose, behind a `DesktopLayer` trait (Windows impl +
  unsupported stub): `items()`, `icon(item)`, `positions()`, `hide(items)`, `show(items)`,
  `subscribe(sink)`. Journal every hide/position change to `<data_dir>/desktop-journal.json` and
  restore on start/stop/crash recovery. Cross-process lock `Local\Desqu.DesktopLayer` (Q10: single owner).
- **Done when:**
  - [x] Kill -9 while items hidden → next start restores them.

#### P8.3 Vials module + standalone app + Desqu integration — Status: 🧪
- **Depends on:** P8.2, Q10, Q1
- **Do:** Same shape as Screenbound (P2.1–P2.3, P3.11): `apps/vials/module` (core: `Vial { id, name,
  monitor_id, rect: NormalizedRect, items: Vec<ItemRef>, collapsed }`, `SortRule { id, name, matcher:
  ItemMatcher (extension | kind: shortcut/folder/app/file | name glob), target_vial }` — pure, unit
  tested), plugin `vials`, `apps/vials/ui`, standalone shell with its own identifier, add to Desqu
  registry and UI. Off behaviour per §5.6. Update feed `updates/vials/`.
- **Done when:**
  - [x] Manual test doc `apps/vials/docs/manual-testing.md` written.
  - [ ] Manual cases passed by a human (visual, DPI, Explorer restart, Win+D).

---

### Phase 9 — Virtual desktop spike + Workspaces

#### P9.1 Spike: Windows virtual desktops — Status: ✅
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
  - [x] Matrix filled for the one build available (26300); supported builds list decided (empty for window moving); fallback defined (launch only). Other builds need a human.

#### P9.2 Workspaces module + app — Status: 🧪
- **Depends on:** P9.1, Q10
- **Do:** Same shape as Vials. Core model: `Workspace { desktop_id, name, visible_items: Vec<ItemRef>,
  apps: Vec<AppLaunch { exe, args: Option<String>, cwd: Option<PathBuf> }>, folders: Vec<PathBuf> }`.
  On desktop switch: snapshot leaving desktop (if "auto-save" on), apply arriving desktop
  (`desqu-desktop` show/hide). Restore action: launch missing apps/folders and move their windows to the
  desktop (only on supported builds; otherwise launch only and tell the user). Off per §5.6.
- **Done when:**
  - [x] Manual test doc `apps/workspaces/docs/manual-testing.md` written.
  - [ ] Passed by a human on at least two Windows builds (only build 26300 is available to the agent).

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
| Command renames (`plugin:screenbound-module|…`) break UI | Change `api.ts` + capabilities in the same step | P2.2, P2.3 |
| New identifier loses settings | Copy-not-move migration + marker + tests | P4.2 |
| Old ScreenBound and new Screenbound fight over windows | Detect old process, keep engine off until gone | P4.3 |
| Standalone uninstaller "Delete app data" deletes settings Desqu uses | Warn in Desqu UI and docs | P6.4 |
| Plugin registered from inside another plugin's `setup` deadlocks Tauri's plugin store (app never starts) | Register on the Builder; always do a real launch + `--quit` after touching startup | P5.3 (found by smoke test) |
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
- 2026-10-10 — P1.2 — ✅ — Root Cargo.toml workspace (member apps/screenbound/src-tauri) + [profile.release] moved to root. cargo build -p screenbound OK, output target/debug/screenbound.exe at root. Next: P1.3.
- 2026-10-10 — P1.3 — ✅ — Root package.json (desqu-repo, workspaces). App renamed to screenbound-app. npm install OK; npm run check -w screenbound-app: 189 files, 0 errors. node_modules only at root.
- 2026-10-10 — P1.4 — ✅ — .gitignore replaced with P1.4 content (adds /target; **/build and **/.svelte-kit now match in apps). Found while doing P1.2: git add -A had staged 3278 files from root target/ into a commit; undone (soft reset, unpushed) before commit f64f9d0.
- 2026-10-10 — P1.5 — ✅ — apps/screenbound/Taskfile.yml holds all screenbound tasks (paths adapted). Root Taskfile includes it (screenbound:*) + install/ci. Verified: task --list; task screenbound:ci and (cd apps/screenbound) task ci both pass. Note: root no longer has dev/release/deploy; use screenbound:dev etc. release task references scripts/stop-app.ps1 and publish-artifacts.ps1 -App, which P1.6 creates (release not yet verified).
- 2026-10-10 — P1.6 — ✅ — stop-screenbound.ps1 → scripts/stop-app.ps1 (-Names). publish-artifacts.ps1 -App <name> (reads apps/<name>, root target/, writes dist/<name>/ and clears only that folder). task screenbound:release OK: dist/screenbound/ names identical to P0.5, sizes within 0.1% (setup 2,506,870 B; msi 3,813,376 B; portable 10,522,624 B). Old root dist/ files from baseline left in place (ignored build output; delete when owner agrees).
- 2026-10-10 — P1.7 — ✅ — Root README = Desqu intro + products + build; apps/screenbound/README.md = old README (image paths fixed, task screenbound:* commands). Root AGENTS.md = repo-wide rules (dependency rules, layering, conventions); apps/screenbound/AGENTS.md = Screenbound rules + screenbound:* commands. Root PLAN.md = index. All relative links checked. Next: P1.8 (regression, partly needs a human on Windows).
- 2026-10-10 — P1.8 — 🧪 — Automated: root task ci exit 0 (189 files 0 errors; 37 Rust tests pass, clippy clean). Release via new task verified (P1.6). NOT yet done (needs human on Windows): (2) task screenbound:dev — app starts, tabs load, settings from %APPDATA%\com.screenbound.app shown; (3) manual-testing.md Core flow + Lifecycle; (4) confirm installed exe size ~ P0.5 (P1.6 build: 10.5 MB portable, same as baseline).
- 2026-10-10 — P1.8 — ✅ — Owner ran the app after the move and reported it works (not a full manual-testing.md run). Phase 1 done. Starting P2.1.
- 2026-10-10 — P2.1 — ✅ — apps/screenbound/module (screenbound-module, lib screenbound_module) holds core/engine/platform (19 files git mv). App uses screenbound_module::{engine,platform,core}; moved serde/serde_json/thiserror/windows to module. No visibility changes were needed. 37 tests pass in module; workspace clippy clean; check 0 errors. Taskfile test/probe/lint now target screenbound-module too. Note: module declares tauri dep (unused until P2.2). Next: P2.2.
- 2026-10-10 — P2.2 — 🧪 — module/src/plugin.rs (ModuleState, start/stop, init, current_config), commands.rs moved into module, build.rs + permissions/default.toml, links="screenbound-module". PLAN FIX: plugin name must equal the Cargo package name (tauri-plugin derives ACL namespace from it) so plugin is `screenbound-module`, invoke `plugin:screenbound-module|x`, capability `screenbound-module:default`; plan text updated. App lib.rs now thin: registers plugin in setup, autostart synced at startup + on sb://config-changed (errors only logged now, no longer returned to UI), tray/close unchanged. Log filter now includes screenbound_module (engine logs would otherwise vanish); dev:logs updated. Verified: build, clippy clean, 37 module tests, ACL manifest lists all 12 commands. NOT verifiable until UI switches (P2.3): runtime invocation. UI is broken at this commit by design.
- 2026-10-10 — P2.3 — 🧪 — src/lib/* moved to apps/screenbound/ui/src ($lib imports made relative), ScreenboundView.svelte (old +page markup + 'Screenbound is off / Turn on' card), index.ts exports ScreenboundView + types, api.ts uses plugin:screenbound-module|x and adds moduleStart/Stop/Status, state gets moduleStatus/turnOn. +page.svelte is now just <ScreenboundView />; app depends on @desqu/screenbound-ui. Verified: npm install links package; task screenbound:check 191 files 0 errors; vite build OK. NEEDS HUMAN: task screenbound:dev — all six tabs load, saving works, Diagnostics lists windows, activity streams, no permission errors in DevTools console (this also confirms P2.2 ACL at runtime).
- 2026-10-10 — P2.2/P2.3 — ✅ — Owner ran task screenbound:dev and reported it works. P2.4 (module_stop/start on real windows) still needs a human run; left 🧪. Moving to Phase 3 (depends only on P1.8).
- 2026-10-10 — P3.1 — ✅ — crates/desqu-shell skeleton (deps per plan; all modules declared; json.rs write_json_atomic and logging.rs init implemented, others are doc stubs). Added to workspace; cargo build/clippy clean.
- 2026-10-10 — P3.2 — ✅ — ShellSettings/UpdateSettings (camelCase, defaults keep current behaviour), load (missing=default, corrupt=error)/save (atomic). 6 unit tests.
- 2026-10-10 — P3.3 — ✅ — args::parse recognises --autostart and --quit, ignores others. 3 unit tests (9 total in desqu-shell pass; clippy clean).
- 2026-10-10 — P0.3/P0.4 — ✅ — Read the registry/files of the owner's installed 0.1.0 (see §3 'Measured'): Run value 'ScreenBound' with no args, install dir %LOCALAPPDATA%\ScreenBound with uninstall.exe, uninstall key 'ScreenBound'. Key finding: case-only rebrand overwrites the same Run value. P0.2 (hidden-at-boot, tray, CPU/RAM) still needs a human.
- 2026-10-10 — P3.4 — 🧪 — instance::plugin (decide(): --quit exits the running app via app.exit, else show_main), exit_if_quit_requested for first launch; window::show_main added early. 3 unit tests. Manual check (screenbound.exe --quit) possible only after P3.11 adopts the shell.
- 2026-10-10 — workflow — rule 9 changed — Owner now makes all commits; after each step Claude suggests a short commit message and waits. History-rewrite to drop Claude Co-Authored-By trailers was blocked by the permission system; backup branch backup/pre-attribution-cleanup exists; owner to run the filter-branch themselves or allow it.
- 2026-10-10 — P3.5 — 🧪 — autostart::plugin() (args --autostart), sync(), sync_at_startup() (disable+enable so old entries are rewritten). platform::remove_run_value(name) deletes the HKCU Run value + StartupApproved value (unsupported stub returns Ok(false)); tested against the real registry with a throwaway value. 13 tests pass, clippy clean. Still to verify by a human after P3.11: enabling writes exe path + --autostart in the Run key.
- 2026-10-10 — P3.6 — 🧪 — window.rs: should_show_on_startup / close_action (pure, 5 tests), on_startup, install_close_handler (reads close_to_tray at event time, tray_ok captured), show_main. Q7 unanswered: used the plan default (hide, not destroy the WebView). 18 desqu-shell tests pass, clippy clean. Used by Screenbound in P3.11; behaviour verified then.
- 2026-10-10 — P3.7 — 🧪 — tray.rs: layout() (pure, 4 tests: show/sep/extras/sep/[updates]/exit) + build() (TrayConfig with tooltip, show_label, extra_items, updates_item flag off by default, on_item handler; show/exit handled by shell, left click shows window; same icon handling as old install_tray). 22 desqu-shell tests pass, clippy clean. Look-and-behaviour check happens when Screenbound adopts it in P3.11.
- 2026-10-10 — P3.8 — ✅ — desqu-shell: mutex::try_acquire (Local\Desqu.Module.<id>), module::{Module, ModuleStatus}; platform LockHandle. DEVIATIONS: (1) lock = mutex existence (bInitialOwner=false, no ReleaseMutex) because ReleaseMutex only works on the owning thread and stop() runs on another; needs Win32_Security feature. (2) Module trait is generic over the runtime and every method takes the AppHandle (start/stop/status), so ScreenboundModule is a stateless unit struct. screenbound-module now depends on desqu-shell; start() takes the lock before spawning the engine (None -> status ownedElsewhere, Err), stop() releases it; plugin setup logs a failed start instead of failing app startup. module_status returns ModuleStatus; UI shows 'running elsewhere' / 'not supported' cards (types.ts ModuleStatus extended). 28 desqu-shell tests (lock twice in-process -> None, release from another thread), screenbound ci green.
- 2026-10-10 — P3.9 — 🧪 — desqu-shell plugin (name = package name desqu-shell): get_shell_settings, set_shell_settings (saves shell.json, then autostart::sync; sync failure returns 'Saved, but ...'), app_info {name, version, identifier, portable}; init(config_dir), current_settings(app) helper for startup/close handler. Corrupt shell.json -> defaults + shell.json.corrupt copy (app still starts). build.rs + permissions/default.toml, links=desqu-shell. 31 desqu-shell tests, clippy clean. KNOWN LIMIT: portable = no uninstall.exe next to exe, so an MSI install (which has none) is reported portable; revisit in P5.3 if MSI updates matter. Callable as plugin:desqu-shell|... only once an app registers it (P3.11).
- 2026-10-10 — P3.10 — ✅ — packages/ui (@desqu/ui): Switch (git mv), tokens.css (whole old app.css; app.css now just @imports it), TriStateSwitch (on/off/mixed, mixed = centred knob + accent outline, aria-checked mixed), SettingsRow, shell-api.ts (ShellSettings/AppInfo types + plugin:desqu-shell wrappers), ShellSettingsSection (start with system / start minimized / close to tray). Screenbound imports Switch from @desqu/ui; ui and app depend on it. task screenbound:check 196 files 0 errors/warnings; vite build OK and CSS still contains the tokens. Not yet used at runtime: ShellSettingsSection arrives in P3.11; TriStateSwitch is for Desqu (P6.2).
- 2026-10-10 — P3.11 — 🧪 — Screenbound standalone now uses desqu-shell: lib.rs = logging::init, args::parse, instance::plugin, autostart::plugin, desqu_shell::init + screenbound_module::init in setup, autostart::sync_at_startup, tray::build ('Show ScreenBound'), window::install_close_handler/on_startup; window visible:false; capability desqu-shell:default; src/autostart.rs deleted. Config v1->v2: AppConfig.legacy_start_at_boot (read-only, skip_serializing), take_legacy_start_at_boot, AppConfig::migrate_legacy_start_at_boot(path) rewrites a v1 file as v2; settings::adopt_legacy_start_with_system seeds shell.json only if absent; app runs both before the plugins load. types.ts drops startAtBoot; SettingsPanel renders ShellSettingsSection + animation. DEVIATIONS: removed capabilities/desktop.json (granted the UI direct autostart access, unused now, and its plugin is no longer a direct dep) and the unused @tauri-apps/plugin-autostart npm dependency. Verified here: 41 module + 34 shell tests (incl. migration keeps zones/groups, never overwrites shell.json), type check 0 errors, clippy clean, built binary run with --quit as first instance exits 0 leaving no process. KNOWN FLAKE: cargo clippy right after cargo test occasionally fails in the screenbound build script; a rerun passes. NEEDS HUMAN on real Windows (also confirms P3.4-P3.7, P3.9): (1) copy of a real v1 config with startAtBoot:true -> after first run shell.json has startWithSystem:true, config.json version 2 with zones/rules/groups/knownApps unchanged (diff the JSON); (2) sign-in with Start with system on -> window hidden, tray present, engine running, Run key = exe + --autostart; (3) manual launch shows window; Start minimized on -> manual launch stays in tray; (4) Close to tray off -> X quits and restores managed windows; (5) second launch of the exe shows the window; 'screenbound.exe --quit' while running exits it and restores windows; (6) tray looks/behaves as before. Add results to apps/screenbound/PLAN.md.
- 2026-10-10 — P2.4, P3.4-P3.7, P3.9, P3.11 — ✅ — Owner ran the real-Windows checks and reported all of them work (details not recorded). Phases 2 and 3 done. Still open from Phase 0: P0.2 (hidden-at-boot/tray/CPU/RAM numbers not provided) and open questions Q1-Q3, Q5-Q11.
- 2026-10-10 — plan hygiene — fixed — Several steps (P1.1-P1.3, P1.5-P1.8, P2.1-P2.3, P3.1, P3.4-P3.7) still showed 🟡/🧪 in the plan although done and logged (status edits had silently not applied). Corrected all to ✅, ticked their Done-when boxes, and fixed the Phase 0/1 table rows. Phase 0 stays 🟡: P0.1 (open questions) and P0.2 (measurements) remain.
- 2026-10-10 — P0.1 — assumed — Owner asked to finish everything without confirmations. Q1-Q3 and Q5-Q11 answered with my proposals (marked *Assumed* in §4). Work continues through Phases 4-9; steps that need real-machine checks or owner-only actions (signing keys in GitHub secrets, Pages, publishing) are left 🧪/handed off with exact instructions.
- 2026-10-10 — P4.1 — ✅ — Identity changed: productName Screenbound, identifier io.github.wolfskii.screenbound, window title, tray tooltip/label, app.html title, UI brand, README/AGENTS text and config path; Taskfile stops screenbound,ScreenBound,Screenbound. Cargo package/bin (screenbound.exe), sb:// events and the Win32 window class name are unchanged. FINDING: the NSIS install folder (%LOCALAPPDATA%\Screenbound) and uninstall key (...\Uninstall\Screenbound) differ from 0.1.0's only by letter case, and Windows treats both case-insensitively, so installing the new build over 0.1.0 is an in-place upgrade (same folder, same uninstall key, same Run value), not a side-by-side install. P4.3 must not treat that as "an old install".
- 2026-10-10 — P4.2 — 🧪 — module/src/migrate.rs: plan_migration (truth table), migrate_config (copy never move, MIGRATED.txt marker with ISO UTC date, skip if marker or new config exists), iso_utc without a date crate; EnginePaths.legacy_journal is recovered before the normal journal and then deleted (deliberately independent of the marker: a leftover legacy journal always means windows to restore); ModuleOptions.legacy_dir; run in plugin setup and, for the startAtBoot hand-over order, also at the top of the app's setup. 7 new unit tests; module 47 tests pass, clippy clean. NEEDS HUMAN: copy a real 0.1.0 %APPDATA%\com.screenbound.app folder and start the new build: same zones/rules/groups/knownApps in the new folder, MIGRATED.txt written, legacy config untouched, startAtBoot carried into shell.json.
- 2026-10-10 — P4.3 — 🧪 — Adapted to the P4.1 finding (0.1.0 and the new build share install folder, Run value and uninstall key case-insensitively): (1) the old Run entry is rewritten or removed by the shell's autostart::sync_at_startup, so no extra removal code is needed; (2) the new risk is an older screenbound.exe still RUNNING (different single-instance id, no module lock). desqu-shell platform gained find_processes, read_run_value, read_uninstall_entry, launch_uninstaller (+ ProcessInfo/UninstallEntry types; 4 new tests incl. real process/registry lookups). screenbound-module/src/legacy.rs (in the module, not the app, so Desqu gets it too): older_instance_running, old_install (only if its InstallLocation differs from our exe folder), settings_backup (only with the MIGRATED.txt marker), uninstall_old, delete_settings_backup (refuses any folder not named com.screenbound.app or without the marker). plugin::start() acquires the module lock first, then refuses to start while another screenbound.exe runs (message: right-click its tray icon and Exit); never kills it. New plugin commands legacy_status / legacy_uninstall_old / legacy_delete_settings_backup (permissions added). UI: 'An older Screenbound is running' card with 'Done, turn on', Settings 'Older Screenbound' section (uninstall other install; delete old settings with an explicit confirm step, per Q2 assumption = keep by default). 52 module + 38 shell tests, ci green. NEEDS HUMAN: start the old 0.1.0 exe, then the new build -> card shown, engine off, no window fighting; exit old, click 'Done, turn on' -> engine starts.
- 2026-10-10 — P4.4 — ✅ — docs/migration-screenbound.md (steps, what migrates, backup + MIGRATED.txt, optional cleanup, rollback) linked from apps/screenbound/README.md. Phase 4 is 🧪 overall: P4.2 and P4.3 need a human run against a real 0.1.0 settings folder / running old copy.
- 2026-10-10 — P5.1 — 🧪 — Generated one updater key pair per product with the repo's tauri CLI (empty password, --ci) into C:\Users\dawid\.tauri\{desqu,screenbound,vials,workspaces}.key(.pub); nothing is in the repo and no private key was printed. Public keys are embedded in each app's tauri.conf.json (Screenbound now; the others when scaffolded). OWNER MUST: (a) add the private keys as GitHub secrets, e.g. `gh secret set SCREENBOUND_UPDATER_KEY < $HOME/.tauri/screenbound.key` (same for DESQU_/VIALS_/WORKSPACES_) and leave the *_UPDATER_KEY_PASSWORD secrets empty/unset; (b) back the .key files up somewhere safe (losing one means installed apps can never be updated); (c) optionally regenerate with a password later (then the public key in tauri.conf.json changes and already-shipped builds stop accepting updates).
- 2026-10-10 — P5.2 — 🧪 — Feed hosting prepared, not enabled: feed URL scheme https://wolfskii.github.io/Desqu/updates/<product>/latest.json is in Screenbound's updater config. The release workflow (P5.6) creates/updates the gh-pages branch itself. OWNER MUST, once after the first release has created gh-pages: enable Pages (repo Settings -> Pages -> Deploy from branch gh-pages / root, or `gh api -X POST repos/Wolfskii/Desqu/pages -f "source[branch]=gh-pages" -f "source[path]=/"`). Until then the in-app check just reports a failed check.
- 2026-10-10 — P5.3 — 🧪 — desqu-shell updater: updates.rs (check/install/status, UpdateInfo/UpdateStatus, pure auto_action: found+install_automatically+not portable => install, else notify; background check 30 s after start then every 24 h if checkAutomatically; install = download -> stop every running module -> install -> restart, modules started again if install fails; portable builds never install), module::ModuleRegistry (register/stop_running/start_ids), updater plugin registered by the shell only when plugins.updater exists in tauri.conf.json (otherwise updates are off), commands update_status/check_update/install_update/open_releases_page (fixed URL via ShellExecuteW), tray::Tray handle with relabel 'Update to vX' / 'Check for updates'. Screenbound: createUpdaterArtifacts + updater config (real public key, passive Windows install), module registered, tray updates item + handler, Settings shows UpdateCard (@desqu/ui: version, last check, Check now, Install/Download, the two toggles). Used the shell commands for everything so no updater:default / process permissions are needed. 44 shell tests, ci green (198 files 0 errors, 52 module tests). NEEDS HUMAN (also P5.7): local update test with a hand-written latest.json + higher-version signed setup exe; tampered .sig must be rejected.
- 2026-10-10 — P5.4 — ✅ — scripts/versions-lib.ps1 + bump-version.ps1 (-App -Version, semver check, edits only the first version in each file, then `cargo update --workspace`, then runs the check) + check-versions.ps1 (all files vs src-tauri/Cargo.toml, optional -Tag <app>-v<version>, exit 1 on mismatch). Tested: pass, wrong tag fails with exit 1, bump 0.1.1 and back to 0.1.0 leaves the tree identical, bad version rejected. Covers module/ and ui/ files when present.
- 2026-10-10 — P5.5 — 🧪 — .github/workflows/ci.yml (windows-latest; checkout, node 20 + npm cache, rust stable + clippy, rust-cache, setup-task; npm ci; task ci; scripts/check-standalone-deps.ps1; builds every apps/*/src-tauri package separately via discovery so new apps are included automatically). scripts/check-standalone-deps.ps1 checks: standalone app never depends on `desqu`, module crates never depend on other module crates, no standalone package.json depends on desqu-app (passes today). YAML validated locally with PyYAML. No signing step (no Authenticode cert, Q6 assumption); noted for docs/releasing.md. NEEDS: a push/PR to see it green on GitHub.
- 2026-10-10 — P5.6 — 🧪 — Release pipeline built and exercised locally: scripts/tauri-build.ps1 (uses CI's TAURI_SIGNING_PRIVATE_KEY or ~/.tauri/<app>.key; screenbound release task now calls it), publish-artifacts.ps1 (also copies *.sig, only picks installers of the current version, writes SHA256SUMS.txt via .NET because Windows PowerShell 5.1 started from pwsh can lose Get-FileHash), write-update-feed.ps1 (latest.json in Tauri updater format + RELEASE_NOTES.md from the CHANGELOG section), .github/workflows/release.yml (tags <app>-v<semver> or manual dispatch; check-versions; build+sign with the product's *_UPDATER_KEY secret, fails if missing; gh release create with all assets, --latest only for desqu, prerelease for versions with a suffix; feed to gh-pages updates/<app>/ via peaceiris/actions-gh-pages, prerelease feeds go to updates/<app>/prerelease/ so no installed app reads them), docs/releasing.md (one-time owner setup, how to release, prerelease flow, local build, SmartScreen note, undo). VERIFIED HERE: a real `task screenbound:release` with the real key produced signed installers (setup.exe.sig and msi.sig, 440 chars each, empty-password key works), dist/screenbound has 3 artifacts + 2 sigs + SHA256SUMS.txt, and write-update-feed produced a correct latest.json. Portable exe grew 10.0 -> 13.5 MiB (updater/TLS stack). Workflow YAML validated locally; not run on GitHub. NEEDS OWNER: secrets + Pages (docs/releasing.md), then push a tag; a pre-release tag (screenbound-v0.2.0-rc.1) is the intended first test.
- 2026-10-10 — P5.7 — 🧪 — Hand-off: not something I can do (needs tags, GitHub Actions, two published versions and an install on a real machine). Steps are in docs/releasing.md. Screenbound's version stays 0.1.0 in the repo for now; bump with `pwsh scripts/bump-version.ps1 -App screenbound -Version 0.2.0` and rename the CHANGELOG 'Unreleased' section when releasing.
- 2026-10-10 — BUG FOUND + FIXED (P5.3) — A real launch of the Screenbound debug AND release exe (Start-Process, then a second `--quit` launch) showed the app never started its event loop: responding=False, only the single-instance helper window, second launch hung. Cause: P5.3 registered the updater plugin from inside desqu-shell's own plugin setup; Tauri holds its plugin-store lock while a plugin initialises, so the nested app.plugin(..) deadlocks. Fix: desqu_shell::updates::plugin() is registered by each app on the Builder; the shell plugin only checks that plugins.updater is configured. After the fix both apps start (responding, titled window) and `--quit` forwarding works end to end (second launch exits 0, first exits 0). This also verifies the second-instance half of P3.4. Lesson recorded in §8: unit tests and CI cannot see this; every phase that touches startup needs a real launch.
- 2026-10-10 — P6.1 — ✅ — apps/desqu scaffolded (package desqu-app; vite port 1430/1431; Tauri package desqu, lib desqu_lib; productName Desqu, identifier io.github.wolfskii.desqu, window visible:false 1100x760, same CSP, NSIS currentUser, updater with Desqu's real public key and updates/desqu/latest.json; icons generated from the existing mascot logo (Q8 assumption; Android/iOS icon folders removed); capability desqu-shell:default + screenbound-module:default; Taskfile + root include; workspace member). Verified: builds, task desqu:ci green, real launch (see above).
- 2026-10-10 — P6.2 — ✅ — settings.rs (ModuleSettings, ModulesFile, pure should_autostart / modules_to_start / set_all_start_with_system, atomic load/save, 7 tests), registry.rs (descriptors() kept free of Wry so tests can run, entries() joins the running module + plugin builder, 3 tests), commands.rs (list_modules, set_module_enabled, set_module_start_with_system, set_all_start_with_system, takeover_module, reset_module_settings, remove_standalone; Desqu's Run entry registered iff any module is enabled+startWithSystem; a module turned on from its own page is picked up as enabled), lib.rs startup (from sign-in only enabled+startWithSystem modules start, manual launch starts all enabled). DEVIATIONS: (1) module settings live in modules.json, not settings.json, because the shell plugin rewrites shell.json wholesale and would drop the module map; (2) desqu-shell gained ShellOptions{manage_autostart} so Desqu's own Run entry follows its modules instead of shell.json's startWithSystem. Note: a test binary that links the Wry runtime cannot start on Windows (STATUS_ENTRYPOINT_NOT_FOUND, no app manifest), so tests only touch Wry-free code.
- 2026-10-10 — P6.3 — 🧪 — takeover.rs: quit_targets (pure), disable_standalone_autostart (removes the standalone Run value + clears startWithSystem in its shell.json), take_over (runs `<exe> --quit` on the standalone process, retries module.start every 250 ms for up to 5 s, then disables the standalone autostart); UI banner 'Screenbound is running on its own. Let Desqu manage it?'. Standalone side: ScreenboundView(onOpenDesqu,onExit) shows 'Screenbound is managed by Desqu' with Open Desqu (finds desqu.exe via the Desqu uninstall entry, else hides the window) and Exit. 3 tests + real --quit forwarding verified. NEEDS HUMAN: both directions with a constrained window (standalone running -> enable in Desqu -> standalone exits and restores, Desqu re-constrains; standalone Run value gone; then start standalone while Desqu manages -> 'managed by Desqu' card).
- 2026-10-10 — P6.4 — 🧪 — Desqu UI: Sidebar (Home, one entry per supported module with status dot, Settings), Home (cards: icon, name, description, status, enabled Switch, Start with system Switch, Open; TriStateSwitch 'Start all apps with system'; take-over banner), module page hosting ScreenboundView with a 'More' menu (Reset settings with a warning that the standalone app shares them; Remove standalone copy when its uninstall entry exists), Settings (ShellSettingsSection with the start-with-system row hidden + explanation, UpdateCard, About with versions). Uses @desqu/ui tokens and custom switches only. task desqu:check 207 files 0 errors; vite build OK. NEEDS HUMAN: click-through with keyboard and mouse.
- 2026-10-10 — P6.5 — 🧪 — Desqu tray (desqu-shell Tray now remembers check items; set_checked/set_enabled; relabel_on_updates and updates::on_tray_click shared with Screenbound): Show Desqu, a check item per module, Pause Screenbound (check) and Release all windows (enabled only while running), Check for updates / Update to vX, Exit; tray::refresh runs after every settings change; module toggles run off the tray's event thread. Flat items instead of a submenu (TrayItem has no submenu). NEEDS HUMAN: tray reflects UI changes and vice versa.
- 2026-10-10 — INCIDENT + FIX (autostart, found while smoke-testing) — Launching my debug builds ran the real startup path, including autostart::sync_at_startup. Because the Run value name is the product name and Windows compares value names case-insensitively, a dev build overwrote the owner's real start-up entry (it pointed at target\debug\screenbound.exe --autostart instead of the installed 0.1.0). Restored to the recorded original data (C:\Users\dawid\AppData\Local\ScreenBound\screenbound.exe; value name now has capital S only, which Windows treats as the same value). Fix: debug builds use their own Run value (<exe>-debug, e.g. screenbound-debug) via autostart::debug_run_value_name (2 tests), so dev/test runs can never touch an installed release's entry. The same smoke tests also ran the real settings migration on the owner's real %APPDATA% (see P4.2) and added MIGRATED.txt to com.screenbound.app; both are the designed, non-destructive outputs.
- 2026-10-10 — P4.2 — ✅ — Verified on the owner's real 0.1.0 settings (migration ran during the smoke tests): legacy config (version 1, startAtBoot true, 1 zone, 1 rule, 0 groups, 15 known apps) vs new config (version 2, no startAtBoot, identical zones/rules/groups/knownApps — node deep compare: identical apart from startAtBoot/version); shell.json has startWithSystem true (carried over); legacy config.json untouched (same size and timestamp), MIGRATED.txt written next to it with the date and the 'safe to delete' note. This is also the A3 check (settings preserved through the move, rebrand and migration) for the Screenbound standalone path.
- 2026-10-10 — P6.3 (partly verified) — Real two-process test of the core design with the debug builds: Desqu with screenbound enabled in modules.json started the engine in-process (log 'starting Screenbound', module lock Local\Desqu.Module.screenbound HELD); a standalone Screenbound started alongside logged 'Screenbound did not start: Screenbound is already running in another process' and stayed up; `--quit` on the standalone exited it while Desqu kept the lock; `--quit` on Desqu exited with code 0 and released the lock (module stopped on exit). Test data (modules.json) and the debug Run values were removed afterwards. Still needs a human: the take-over UI flow and the visible window restore/re-constrain.
- 2026-10-10 — P6.6 — 🧪 — Prepared: apps/desqu/README.md, apps/desqu/CHANGELOG.md (0.1.0 section), docs/architecture.md, root README lists Desqu. NEEDS HUMAN (acceptance checks for Desqu): A3 settings preserved when switching standalone <-> Desqu (diff config.json/zones/rules/groups/knownApps); A4 turning Screenbound off in Desqu frees resources (Process Explorer: no screenbound engine/event threads, idle CPU ~0%); A5 startup matrix (per-app start with system, master tri-state, --autostart hidden, start minimized, close to tray on/off, tray Exit restores windows); A6 take-over both directions; A7 updates isolated (releasing screenbound does not offer an update in Desqu and vice versa). Then tag desqu-v0.1.0 (docs/releasing.md) so the release is published with the Latest flag and the feed goes live.
- 2026-10-10 — P7.1 — ✅ — core/rules.rs: Action::Borderless{target: BorderlessTarget(Monitor|Zone{zoneId}), topmost} (serde tag type, camelCase), BorderlessAction + WindowRule::borderless_action(), first_match_where so rule kinds are matched separately; config.rs: validation (unknown zone rejected) and match_fullscreen_rule / match_borderless_rule so a borderless rule listed first can never shadow a fullscreen rule (test); types.ts mirrors Action. Design note added to apps/screenbound/PLAN.md (Q11 = per-rule automatic only, framed normal windows only, limits). 58 module tests pass, clippy clean.
- 2026-10-10 — P7.2 — ✅ — core/borderless.rs (pure: should_apply = visible, not minimized/cloaked/click-through, framed, not owned, real size; observe -> Unchanged/Dormant/Reassert/AppFullscreen; 9 tests). Engine: separate `borderless` map (the fullscreen state machine is untouched), try_borderless/apply_borderless (set_borderless -> set_bounds -> verify bounds stuck, else restore and remember the window as rejected; Monitor target also marks the window fullscreen for the taskbar, optional topmost), evaluate_borderless with the same loop guard as fullscreen (>4 re-asserts in 3 s -> leave alone), reapply on display change, reconcile after config edits (changed/removed rule -> release then re-apply; zone edit -> re-fit), release (restore_state when still in place, otherwise only the frame is given back so a window is never left borderless), user moves respected, journal entries carry a kind (old journals = fullscreen), crash recovery restores borderless windows without the fullscreen taskbar mark and restores the frame even if the window moved, forget on destroy, release_all/shutdown/Release include borderless, status reports kind. Fullscreen and borderless rules are matched separately. VERIFIED ON REAL WINDOWS (task screenbound:probe): two #[ignore] tests drive the engine with a temp config+journal against a winver.exe window this test spawns: (1) borderless Monitor -> frame gone, visible bounds == monitor, status kind borderless, journal written; release_all -> frame and bounds restored, journal removed; (2) borderless Zone fits the zone, then a NEW engine on the same journal (simulated crash) restores the window. A startup scan correctly re-applies a still-present rule after recovery (so test 2 starts the second engine without the rule). Module 71 tests + 2 real-window tests pass, clippy clean. NOT yet exercised: real games (swapchain recreation, elevated, exclusive fullscreen) — see P7.3.
- 2026-10-10 — P7.3 — 🧪 — Rules tab: per-rule Action selector (Fullscreen -> zone / Borderless window), 'Fit to' (Whole monitor or a zone), 'Keep on top' switch, '+ Borderless rule' button, explanatory hint with the limits; zoneInUse also counts borderless zone targets; Diagnostics shows a 'borderless' tag and neutral column names (Original/Target bounds); types.ts mirrors Action + ManagedSummary.kind. Manual test section 'Borderless gaming' (12 cases: windowed game, swapchain recreation, zone target + live zone edit, keep on top, elevated, exclusive fullscreen, release, pause, quit, kill + recovery, both rule kinds, display change) added to apps/screenbound/docs/manual-testing.md; CHANGELOG updated. task ci green (check 0 errors, module 69 tests + 2 real-window tests under `task screenbound:probe`, clippy clean). NEEDS HUMAN: run the manual cases with real games and record results in apps/screenbound/PLAN.md (🧪 -> ✅ per game).
- 2026-10-10 — P8.1 — ✅ — Spike done on ONE machine (Windows 11 26H2 build 26300.9550, 4 monitors at 100%, 394 desktop items, align-to-grid on, auto-arrange off); only a temp file the spike creates itself was ever moved/hidden. Examples: crates/desqu-desktop/examples/spike_desktop.rs and spike_desktop_2.rs; results and decision in apps/vials/docs/desktop-research.md. KEY FINDINGS: IFolderView position writes snap to the nearest free grid cell and are clamped onto the grid (cannot park off-screen); FILE_ATTRIBUTE_HIDDEN works but edits user files (rejected); LVM_SETITEMPOSITION to the desktop SysListView32 DOES park items at negative coordinates, reads back exactly and restores (CHOSEN, journaled write-ahead); a bottom-most WS_EX_TOOLWINDOW|NOACTIVATE popup is visible above the icons without stealing focus (a child of Progman is painted over by Wallpaper Engine: rejected); ReadDirectoryChangesW is instant while SHChangeNotify delivered nothing in 8 s (CHOSEN: directory watcher); the desktop view itself lags the file system by ~4 s so lookups need bounded retries; icons ~5.8 ms each (200 in 1.15 s). NOT OBSERVED and stated as such: Win+D, Explorer restart/sign-out persistence of parked positions, non-100% DPI, other Windows builds, auto-arrange on. Windows 10 22H2 / 11 23H2 / 24H2 need a human with those machines.
- 2026-10-10 — P8.2 — ✅ — crates/desqu-desktop implements exactly what the spike chose. model.rs (pure, 8 tests): Point/PARK_POINT/is_parked, ItemKind + classify, DesktopItem, Journal of ParkedItem{id, original, owners[]} with park/unpark/hidden_by/release_owner (an item returns only when the LAST owner lets go, which is the Q10 sharing rule) and plan_recovery; atomic load/save (empty journal removes the file; corrupt = empty). layer.rs: DesktopLayer trait (items, icon_png, can_hide, hide, show, show_all, hidden, subscribe), DesktopError (OwnedElsewhere, AutoArrange, ViewUnavailable, ...), HideReport{hidden, missing} (missing = not in the lagging view yet, caller retries after the next event). Windows backend under platform/windows/: one STA worker thread owns all COM objects and the journal (handles talk to it over a channel); desktop view found fresh each call (Explorer restart safe); hide = write-ahead journal THEN LVM_SETITEMPOSITION park; show = restore THEN commit journal (a crash leaves the journal claiming more, never fewer); startup recovery and last-handle drop both restore everything; recover_from(path) restores a foreign journal; cross-process named lock Local\Desqu.DesktopLayer (existence = lock, freed when the process dies); in-process sharing via a Weak singleton so Vials and Workspaces inside Desqu share one layer; ReadDirectoryChangesW watchers (overlapped, stop event, 150 ms coalescing) on Desktop + Public Desktop; icon PNGs via IShellItemImageFactory + GetDIBits + png crate. Root Taskfile gained `crates` (tests + clippy of desqu-shell and desqu-desktop, part of ci) and `probe`. VERIFIED ON REAL WINDOWS (tests only ever touch temp files they create): tests/real_desktop.rs (#[ignore]) — (1) events fire for a new file, item appears in the view, PNG icon valid, hide verified against Explorer's own view (position parked), journal holds the true original position, two owners share an item (stays hidden until both show), recover_from restores and deletes a foreign journal, last owner restores and the journal file disappears, dropping the last handle restores; (2) KILL -9: a child process hides an item and is killed with no cleanup, the lock is free again, the next open restores the item to its original (16,2) and clears the journal. 10 unit tests + both real tests pass, clippy clean. NOT verified: Explorer restart, reboot/sign-out, auto-arrange on, other Windows builds and DPIs (see apps/vials/docs/desktop-research.md).
- 2026-10-10 — P8.3 — 🧪 — Vials complete: module crate (core model/rules/layout/glob, engine worker with debounce and delayed re-check for the ~4 s view lag, per-vial windows `vial-<id>` that are bottom-most non-focusable tool windows, 11 commands, VialsModule), @desqu/vials-ui (VialsView tabs Vials/Items/Rules/Settings, VialView, lazy icons), standalone app apps/vials (identifier io.github.wolfskii.vials, own updater key and feed), Desqu integration (registry entry + init, module page renders VialsView without the shell settings section, bare `/vial/[id]` route without the sidebar, `vial-windows` capability, tray check item). VERIFIED ON REAL WINDOWS with temp files only: apps/vials/scripts/smoke-test.ps1 (item parked, vial window visible/tool-window/non-activating, --quit exits 0 and restores the item, kill -9 keeps the item parked and the next start restores it) and apps/desqu/scripts/smoke-vials.ps1 (Desqu with only Vials on parks the item, shows the vial window, --quit restores it). Docs: README, CHANGELOG, AGENTS, docs/manual-testing.md. NEEDS HUMAN: manual-testing.md (look and feel, DPI, Explorer restart, Win+D, rules, take-over).
- 2026-10-10 — P9.1 — ✅ — Spike crates/desqu-desktop/examples/spike_vdesktop.rs, results in apps/workspaces/docs/virtual-desktop-research.md. Observed on ONE build (Windows 11 build 26300, 2 desktops); Windows 10 22H2 / 11 23H2 / 24H2 NOT tested. Findings: desktop ids and names readable from the registry (VirtualDesktopIDs + Desktops\{GUID}\Name; the Desktops subkey keeps stale entries so the live list must come from VirtualDesktopIDs); CurrentVirtualDesktop lives in the same key on this build and RegNotifyChangeKeyValue registers on it (an event firing on a real switch was NOT observed: switching the owner's desktop was not automated); public IVirtualDesktopManager reads other processes' window desktops (tool windows are unknown to it) and MoveWindowToDesktop works only for the caller's own windows, foreign windows give E_ACCESSDENIED (verified on a winver.exe the spike started); exe path and command line of a same-user process readable unelevated (QueryFullProcessImageNameW, NtQueryInformationProcess class 60); IShellWindows lists open folder windows (verified with a folder window the spike opened and closed). Decision: public surface only, event-driven switch detection, no foreign window moves (launch-only restore on the current desktop), supported-builds list for window moving is empty. Nothing of the owner's desktops or windows was changed.
- 2026-10-10 — P9.2 — 🧪 — Workspaces built in the Vials shape. DEVIATION from the plan's model: a workspace stores `hiddenItems` (icons to hide on that desktop) instead of `visibleItems`, so files created later stay visible; `manageIcons` switches it on; apps are `{exe, args, name}`, folders are paths. Core (pure, 17 tests): model (GUID normalization incl. registry byte layout, validation, atomic load/save), visibility plan (hide/show diff against what this owner already hides), apps (command-line split, candidate filter that skips shell parts and our own apps, dedupe, missing-apps by exe, file URL -> path). Platform: registry desktop list/current, RegNotifyChangeKeyValue watcher thread (re-armed per signal, stop event, retried until the key exists), IVirtualDesktopManager window filter, exe path + command line, IShellWindows folders, ShellExecuteW launch/open (absolute existing paths only). Engine: worker thread reacting to the registry watcher and desktop-folder events, auto-save of the leaving desktop when enabled, hides through desqu-desktop owner `workspaces`, restores on stop/exit/crash. 11 commands; restore only for the current desktop (foreign windows cannot be moved). UI @desqu/workspaces-ui (card per desktop, icon picker, saved apps/folders, settings), standalone app apps/workspaces (identifier io.github.wolfskii.workspaces, own updater key/feed, ports 1450/1451), Desqu integration (registry entry, module page, capability, tray check item). VERIFIED ON REAL WINDOWS (temp files/processes only): apps/workspaces/scripts/smoke-test.ps1 (hidden on the current desktop; stays visible when the workspace belongs to another desktop; --quit restores to the original place; kill -9 then recovery on next start), `cargo test -p workspaces-module -- --ignored real_running` (helper window found via virtual desktop APIs, closed, relaunched from the saved entry), apps/desqu/scripts/smoke-workspaces.ps1 (Desqu hosting Workspaces; Desqu with Vials AND Workspaces sharing one item, both restored). NOT verified: an actual desktop switch (needs a human; the registry event firing on a switch has not been observed), other Windows builds, tabbed Explorer folders, elevated apps. Root task ci/probe include workspaces.
- 2026-10-10 — P9.2 REWORK (owner clarification) — 🧪 — The owner clarified that Workspaces is about DESKTOP ITEMS (shortcuts, files, folders) tied to a virtual desktop, not about apps and windows. The first draft (per-desktop hide list, saved apps/folders, launch) was replaced: model is now Workspace{desktopId, name, items[]} where an item is owned by at most one desktop; an owned item is shown on its desktop and hidden on every other live desktop, unowned items show everywhere, items of a removed desktop are shown again (hiding must never lose an item), and with an unknown current desktop nothing is hidden. `autoAssignNew` (default on): a file that appears while Workspaces runs is tied to the desktop you are on; files already there stay on every desktop. Removed: apps/window capture, launching, IShellWindows, process queries, the real-window test and all related UI; the platform layer now only reads the virtual desktop registry key and watches it (RegNotifyChangeKeyValue). New command assign_item; UI tabs Desktops (cards with the items tied to each), Items (pick a desktop per item), Settings. 14 module tests (ownership, plan incl. switching/removed desktop/unknown desktop, config validation, old draft configs still load), clippy and svelte-check clean. The real-desktop smoke scripts (apps/workspaces/scripts/smoke-test.ps1, apps/desqu/scripts/smoke-workspaces.ps1) were updated to the new model but NOT re-run: the owner's installed Desqu (first-draft build) is running with Workspaces enabled and holds the module lock, and closing a running app of the owner's was not authorized. The earlier smoke runs described in the previous P9.2 entry applied to the first draft. NEEDS: quit Desqu from the tray, run both smoke scripts (or `task desqu:deploy` to install the new build), then the manual cases.
- 2026-10-10 — P8.3 REWORK (owner clarification) — 🧪 — Vials must behave like Stardock Fences / PecoFence, not like a dialog: drag a selection box on the EMPTY desktop and a small context-menu-like popup offers to create a vial there; items are dragged from the desktop into the vial. Added: core/gesture.rs (pure, 4 tests: marquee rect from any corner with a 90x70 minimum, next free `Vial N` / `vial-N`, popup position kept on the monitor); platform MouseWatcher (a WH_MOUSE_LL hook on its own thread; it only records left-button down on the desktop icon view — WindowFromPoint is a SysListView32 under SHELLDLL_DefView — and the matching up, and hands the two points over through a channel, no work inside the hook); desqu-desktop `selection_count()` (read-only IFolderView2 ItemCount(SVGIO_SELECTION)); engine Job::Marquee: waits 220 ms for Explorer to settle, offers the popup only when the selection is empty (so a box that caught icons or a drag that started on an icon is left alone), pending rect kept in state; popup = Tauri window `vials-menu` (frameless, topmost, focused, closes on focus loss or Esc) with command create_vial_from_menu (vial created at the dragged rect on the monitor under it) and dismiss_menu; drop_items command (paths dropped on a vial window are matched against real desktop items only); VialView listens to the webview drag-drop event (highlight while hovering, drop assigns); vial windows are now transparent with a translucent panel; config `createOnDrag` (default on) with a Settings switch; routes/capabilities for `vials-menu` in the Vials app and in Desqu (Desqu layout treats `/vials-menu` as bare content). Unit tests 28 pass, clippy and svelte-check clean. NOT VERIFIED on the real desktop: the owner's installed Desqu is running and holds the desktop-layer lock, closing it was not authorized, and driving the real mouse for a synthetic box drag also needs the owner's OK. So the hook, the popup timing/placement, the drop from the desktop, DPI and the translucent look are untested; manual cases 0a-0k were added to apps/vials/docs/manual-testing.md. Known gap: renaming a vial is done in the Vials window, not on the vial's title bar (vial windows are non-focusable by design).
- 2026-10-10 — P9.2 FOLLOW-UP — 🧪 — Owner feedback: (1) do not make me set items one by one: new files already join the desktop you are on (autoAssignNew); added a first-run question ("Where do your desktop items belong?") answered once with one click per virtual desktop, or "Leave them on every desktop" (config `initialized`, command assign_all_items; unowned items only). (2) Owner still saw an item tied to one desktop on both desktops. Read-only look at the owner's live files showed the config and journal were right for desktop 753C (Books & Reading visible, CasaOS and Dev parked by 'workspaces'), so the ownership logic works and the suspect is switch detection: the registry value CurrentVirtualDesktop is not guaranteed to follow the desktop on screen, and its change notification was never observed to fire (SessionInfo\1\VirtualDesktops has no CurrentVirtualDesktop on build 26300). Fix: the current desktop is now taken from the foreground window (public IVirtualDesktopManager::GetWindowDesktopId; taskbar/pinned windows are ignored), re-checked event-driven on every EVENT_SYSTEM_FOREGROUND via a WinEvent hook thread that only signals when the desktop changed, with the registry value and its notification kept as fallback/extra signal. Added crates/desqu-desktop/examples/vdwatch.rs (read-only diagnostic printing registry-current vs foreground-desktop changes). Unit tests 14 pass, clippy/svelte-check clean. NOT verified on a real switch (needs the owner to switch desktops; the owner's installed Desqu holds the module lock so the smoke scripts cannot run). A diagnostic vdwatch run was started while the owner was on one desktop; it has not seen a switch yet.
- 2026-10-10 — P8.3 FOLLOW-UP — 🧪 — Owner feedback on Vials (screenshot of an empty 'Vial 1' on startup): (1) No starter vial: VialsConfig::default has no vials, and the untouched starter that early builds saved (id vial-1, name 'Vial 1', default rect, empty, no rules) is dropped on load (a used or edited one is kept; 2 tests). (2) Right-click creation: the mouse hook also records the last right-button press on the desktop icon view; with `contextMenu` on (default) Vials writes HKCU\Software\Classes\DesktopBackground\Shell\DesquVials ('Create vial here', command `<exe> --action=create-vial`) while it runs and removes it on stop/exit or when switched off; the shell gained `--action=<name>` launch args (forwarded to the running instance as the `shell://action` event, never shows the window, delivered after startup when the app had to start) and Vials turns `create-vial` into a 380x260 (logical) vial with its corner at the remembered click (fallback: centre of the primary monitor). Native Windows 11 menu shows static verbs only under 'Show more options'; a modern-menu entry would need a packaged shell extension and was not attempted. (3) Drag OUT like Explorer: no x buttons; pulling an icon past 5 px starts a real OLE drag (IDropSource with Esc to cancel and button release to drop, the item's own shell data object via BHID_DataObject so targets see the real file, the shell drag-image helper for the icon ghost) run on the main thread; dropped on the desktop icon view the item leaves the vial and lands at the drop point (new DesktopLayer::show_at, which converts screen to list-view coordinates and restores there only if this was the last owner), dropped on another vial the drop handler moves it, dropped elsewhere Windows does what it does for the real file. Inspiration checked: PecoFence docs/FEATURES.md (drag-out semantics: back to empty desktop returns the item, Esc cancels, modifier keys pick copy/move/link, drag image) — its source files were not readable through the fetch tool. (4) Removed the item counter and the 'Empty. Drag icons...' text; panel is more opaque (0.9). Unit tests, clippy and svelte-check clean. NOT VERIFIED on the real desktop (installed Desqu holds the layer lock; an OLE drag needs a real mouse): the drag-out, drag image, drop placement, the right-click entry and its position, and all earlier Vials gesture items; cases 0a-0k and 0g2-0g7 are in apps/vials/docs/manual-testing.md.
- 2026-10-10 — P9.2 CORRECTION — 🧪 — A read-only diagnostic (crates/desqu-desktop/examples/vdwatch.rs) ran about 18 minutes while the owner switched between two desktops many times on build 26300: the registry value CurrentVirtualDesktop changed together with the desktop on screen, so my earlier suspicion that it is stale was wrong. At the instant of a switch the foreground window is the taskbar/Progman, which IVirtualDesktopManager reports as TYPE_E_ELEMENTNOTFOUND (0x8002802B), and pinned windows report the all-zero GUID. The code now takes the registry value first and uses the foreground window's desktop only as a fallback; both still trigger a re-check. So the cause of the owner's 'visible on both desktops' report is NOT established; whether RegNotifyChangeKeyValue fires on a switch is still unobserved (vdwatch now also prints NOTIFY lines to settle it). Needs the owner to retry with the new build.
- 2026-10-10 — P8.3 FOLLOW-UP 2 — 🧪 — Owner: after dragging a box on the empty desktop the popup glitched and vanished (right-click creation worked); wants to click a vial's name on the desktop to rename it. Likely causes fixed: the popup used to be shown immediately, before its page had loaded (blank flash), and was closed by any Focused(false) event, which a web view produces by itself when it takes focus. Now the popup stays hidden until its page calls menu_ready (1.5 s fallback), and is dismissed by the mouse hook: every button press is reported (MouseEvent::ButtonDown) and closes the popup when it lands outside the popup's rect (Esc still works from the page). Rename: the vial title is a button (the drag area is the rest of the header); click -> begin_rename makes that vial window focusable and focuses it, an input appears, Enter/blur saves, Esc cancels, end_rename makes it non-focusable again and puts it back at the bottom of the z-order (mark_as_tool_window). Clippy and svelte-check clean. NOT verified on the real desktop (same blocker as before); manual cases 0g4b and 0g4c added.
- 2026-10-10 — P8.3 FOLLOW-UP 3 — 🧪 — Owner: dragging an item out of a vial onto the desktop made Explorer show 'the destination folder is the same as the source folder' (Swedish: 'Målmappen är samma som källmappen'). Cause: our drag carries the real shell item, which lives in the Desktop folder, so Explorer treats a drop on the desktop as a file move into the same folder and refuses. Fix: DropSource no longer lets such a drop reach Explorer. When the button is released over the empty desktop icon view (WindowFromPoint is the SysListView32 under SHELLDLL_DefView) the drag is cancelled from QueryContinueDrag and the release point is recorded; drag_item returns DragEnd::OnDesktop(point) and the engine places the icon there (DesktopLayer::show_at) and removes it from the vial. Drops on other targets (another vial, an Explorer folder, other apps) are still real Windows drops. Clippy clean; NOT verified on the real desktop (same blocker); manual case 0g2 covers it.
- 2026-10-10 — LOCAL BUILDS + GLASS — 🧪 — (1) `task <app>:release` and `<app>:deploy` are now local unsigned builds: scripts/tauri-build.ps1 -NoSign writes a temp config with bundle.createUpdaterArtifacts=false and calls the tauri CLI directly (npm swallows `--config`), so no key and no password prompt; the prompt could not be avoided any other way because Tauri asks on the console, not on stdin, when the key password is empty. `task <app>:release:signed` keeps the signed build for testing the update pipeline (press Enter at the prompt); CI releases are unchanged. Verified: workspaces -NoSign build produced installer, MSI and portable exe with no prompt and no .sig. publish-artifacts.ps1 now says 'Signatures: none (local unsigned build)' instead of warning. (2) Vial windows and the create-vial popup got a liquid-glass look: Windows acrylic blur behind the window (tauri window effects, dark tint rgba(18,22,30,96)), DWM rounded corners (DwmSetWindowAttribute DWMWCP_ROUND, also clips the blur), a window shadow, and CSS light on top (specular top edge, soft gradients, light borders, pill hover states, text shadows for legibility). Both effects are best effort and silently skipped where unsupported (Windows 10 gets square corners). Clippy and svelte-check clean. NOT verified visually: the real desktop was not available to me, so contrast over bright wallpapers, acrylic cost while dragging a vial, corner clipping and the popup need the owner's eyes.
- 2026-10-10 — VIAL THEMES — 🧪 — New Vials setting `theme` (`glass` default, `classic`): Classic is the previous plain dark panel (no blur, no shadow, 8 px corners), Glass is the frosted look. The theme is chosen when a window is built (acrylic, DWM corners and shadow are window properties), so changing it closes and rebuilds the vial windows in place; the create popup follows it too. UI: a 'Look of vials' select in Settings; VialView and VialMenu use Classic as the base CSS with the glass rules scoped under `.glass`. 1 new test (default, round trip, old configs). Clippy and svelte-check clean. NOT verified on the real desktop; manual case 0l added.
- 2026-10-10 — FIX deploy task — The inline PowerShell of `task <app>:deploy` lost every `$variable` (Task's shell expands `$setup` before PowerShell sees it), so the install step failed with a parse error after a successful build. The step is now scripts/install-local.ps1 -App <app> (newest NSIS setup under dist/<app>, silent install for the current user); all four Taskfiles use it. Checked: the script runs and reports a missing installer correctly; the actual install was not run here because it would replace the owner's running Desqu.
- 2026-10-10 — GLASS TUNING — 🧪 — Owner screenshot of the first Glass build: the vial looked grey, noisy and quite opaque, not like Apple's glass. Cause: acrylic always adds a grain and a grey luminosity wash, and the dark tint (alpha 96) plus my white CSS gradients stacked on top of it. Changes: new settings `glassBlur` (frosted = acrylic, clear = plain blur behind the window, no grain) and `glassTint` (0-100, default 20, clamped; tint colour rgb(24,28,38) with that alpha), both in Settings under Glass and applied by rebuilding the windows (on slider release); the CSS wash is weaker so the blur shows (edge highlights kept). 1 new test. I tried to compare the options on screen with temporary borderless WinForms test windows over the owner's wallpaper (screenshots), but WinForms windows do not behave like Tauri/WebView2 transparent windows (only the white variant showed), so the defaults are a guess and the settings are the way to tune them: manual case 0m. A true Apple look (strong blur plus saturation and a refractive edge) is not available from Windows' window effects; the closest is Clear blur with a low tint and the bright edges.
- 2026-10-10 — Vials panel trimmed — Owner: the Vials tab should not show monitor, left/top/width % fields. Each vial card now only has its name and Delete (the item counter and the Collapsed switch went too: collapse stays on the vial's own chevron). Position and size are still stored and edited on the desktop. svelte-check clean.
- 2026-10-11 — HIDING AT SCALE + SHOW DESKTOP ICONS — 🧪 — Owner screenshot: after tying all ~395 desktop items to the other desktop they moved slightly but stayed visible on the current one. Read-only look at the live files: config correct (Arbete owns 395, current desktop Normal), the workspaces journal gone, so the hide did not stick. Cause is not proven (nothing reproduces on a 2-item test), but two defects in desqu-desktop explain it and are fixed: (1) hide() wrote the whole batch to the journal first and then aborted at the FIRST failed LVM_SETITEMPOSITION (`?`), leaving the rest on screen while the journal claimed them hidden, and nothing ever retried because the plan only hides items not yet claimed; now each failure is logged and skipped, positions are READ BACK afterwards, items that are still on screen come back in the new HideReport.failed, and an item that is claimed but found on screen is parked again by the next hide (both engines now retry on `failed` and Workspaces passes claimed-but-visible items too); (2) every hidden item was parked on the SAME point, which Explorer can nudge apart; each now gets its own off-screen cell (park_slot, 100 px apart, test for 2000 slots). Vials: new `DesktopIconsWatcher`-style hook (WinEvent EVENT_OBJECT_SHOW/HIDE filtered to the desktop's SysListView32) plus desqu_desktop::icons_visible(): when 'Show desktop icons' is switched off the vial windows hide, on again they come back (new windows stay hidden while the icons are off). Event-driven, no polling. Unit tests and clippy clean; NOT verified on the real desktop (the toggle would change the owner's desktop and the owner's Desqu holds the layer lock). Manual cases added: Vials 0n, Workspaces 13.
- 2026-10-11 — FIX theme change hid the vials — Changing the look closed all vial windows and rebuilt them under the same labels in the same pass; the old window was still being destroyed, so get_webview_window still found it, the 'existing window' branch ran and no new window was built: the vials vanished. Now a look change restyles the open windows in place (style_window: acrylic/blur and tint via set_effects, shadow via set_shadow, DWM corner preference round or do-not-round; Classic clears the effects). Settings texts and docs say 'restyles in place'. Clippy clean; not verified on the real desktop (case 0l).
- 2026-10-11 — WORKSPACES: ITEMS STILL VISIBLE ON OTHER DESKTOPS (investigation) — 🧪 — Owner: items tied to Arbete are still visible on every desktop. Checked read-only against the owner's real files and desktop: (1) the config is right (Arbete owns 395 items, Normal 0); (2) visibility::plan run on the real config with the real virtual desktops and the real Desktop folder gives hide 393 / show 0 for Normal and 0/0 for Arbete, so the decision logic is correct; (3) switch detection works: vdwatch (about 50 minutes, many switches) shows the registry notification firing exactly once per switch about 0.3 s after the foreground becomes Progman, with CurrentVirtualDesktop already updated, and the installed build (02:36) contains the earlier park fix; (4) a position write costs 14.9 ms on the real desktop (new example layer_timing: 100 writes of a temp file onto its own position, no move) versus 1.3 ms in a plain list view, so hiding 393 items takes about 6 s per pass, well under the 30 s reply timeout; (5) a plain Win32 list view parks 395 items off-screen fine, stacked or at distinct cells (private test window). The failing step is therefore inside the real Explorer desktop and could not be reproduced without taking the layer lock from the owner's running Desqu (not authorised) or polluting the owner's config with temp files (autoAssignNew would tie them to a desktop). Changes to find and survive it: hide() now parks, reads the positions back and retries up to 3 passes (150 ms apart) for items Explorer left on screen, logging each outcome (`parked N items in X ms (P passes, M missing, F failed)`); every app now writes %APPDATA%\<identifier>\logs\app.log (2 MB, one .old kept; info level for desqu_desktop and the modules); Workspaces logs each desktop pass (current desktop, item counts, planned hides/shows), catches a panic inside a pass instead of silently ending the engine thread, and the Settings tab shows 'Last check HH:MM:SS: on <desktop>, X of Y desktop items hidden, Z could not be hidden, Problem: ...'. NEXT: the owner installs this build, switches to the desktop that should hide items, and the log plus the Last check line show where it fails; then fix that cause.
- 2026-10-11 — NO ITEMS TABS, CTRL+X / CTRL+V, START WITH SYSTEM RULE — 🧪 — Owner: remove the Items tab from Vials and Workspaces (items are dragged in, not listed), keep a count per workspace, move items between workspaces with Ctrl+X / Ctrl+V, and do not allow 'start with system' on an app that is off. (1) Items tabs and their components removed in both UIs; Workspaces cards now show only the item count plus 'Show on every desktop' (releases the workspace's items); the first-run question stays. (2) Ctrl+X/Ctrl+V: a low-level keyboard hook (event-driven, looks only at the V key with Ctrl held, no Shift/Alt, and only when the foreground window is the desktop: class Progman or WorkerW) reads the clipboard INSIDE the hook, before Explorer handles the key (a finished cut clears it): it needs CF_HDROP and the 'Preferred DropEffect' format with the MOVE bit (so copy never counts). The paths are matched against the real desktop items (pure core::paste::desktop_items_in, case and slash insensitive, 2 tests) so nothing else can be tied to a desktop, then assigned to the CURRENT virtual desktop and saved; reconcile hides them on the others. Explorer's own handling of a cut-and-paste into the same folder is expected to be a silent no-op (not yet seen on the real desktop). Not tested for real: the clipboard parsing (a test would overwrite the owner's clipboard) and the hook. (3) Desqu: the per-app 'Start with system' switch is disabled while the app is off and shows off; the master switch only counts apps that are on and is disabled when none is; backend refuses 'start with system' on an app that is off ('Turn X on first'), set_enabled(false) clears it, set_all only applies to apps that are on, and old modules.json files are normalized on load (3 new tests). Clippy and svelte-check clean.
- 2026-10-17 — P5.1/P5.2/P5.6 — ✅ — First release published and the updater feed is live. Owner setup done for Desqu: `DESQU_UPDATER_KEY` repo secret set from `~/.tauri/desqu.key`; `gh-pages` orphan branch created with `updates/.keep`; Pages enabled (gh-pages / root, `https://wolfskii.github.io/Desqu/`) — the POST returned 409 "already enabled" because pushing the `gh-pages` branch auto-enabled it. Two real bugs found on the way: (1) the Release workflow called `task <app>:release`, which became the *unsigned* local build (-NoSign, 2026-10-10), so no `.sig` files existed and `write-update-feed.ps1` aborted with "Missing signature" — the workflow now calls `<app>:release:signed` (docs/releasing.md corrected to match); (2) nothing had ever been released, so `latest.json` 404'd and Settings showed the raw plugin error "Could not fetch a valid release JSON from the remote" — expected per P5.2 until a release existed. Result: `desqu-v0.2.0` (run 39043500529, workflow_dispatch) released 6 assets (setup.exe + .sig, msi + .sig, portable.exe, SHA256SUMS.txt) marked Latest, and `updates/desqu/latest.json` returns 200 with the 0.2.0 signature. Remaining: secrets for screenbound/vials/workspaces (P5.1) and the first update-enabled Screenbound release (P5.7); the installed Desqu 0.1.0 on the owner's machine should now offer 0.2.0.
