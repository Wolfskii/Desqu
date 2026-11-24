# Desqu

<p align="center">
  <a href="https://www.buymeacoffee.com/wolfskii">
    <img src="assets/buymeacoffee-button.png" alt="Buy me a coffee" height="50" />
  </a>
</p>

Desqu is a collection of small desktop utilities for Windows. Each utility also builds, runs and
installs on its own.

| Product | What it does | Status |
| --- | --- | --- |
| [Desqu](apps/desqu/README.md) | One app that hosts all the utilities below, with one tray icon and central settings | Available |
| [Screenbound](apps/screenbound/README.md) | Keeps fullscreen apps inside a zone you choose on your monitor | Available |
| [Vials](apps/vials/README.md) | Desktop containers for shortcuts, apps and folders | Available |
| [Workspaces](apps/workspaces/README.md) | Desktop icons that belong to a virtual desktop | Available |

## Build

Prerequisites: Rust (stable, MSVC) and [Task](https://taskfile.dev). Node 20+ only for the browser extension.

```powershell
task dev                   # run Desqu with its apps (builds them first; the first build compiles GPUI and takes a few minutes)
task gpui:vials            # run one app on its own: gpui:vials, gpui:workspaces, gpui:screenbound
task test                  # unit tests for every app and shared crate
task ci                    # unit tests and clippy for everything, plus the browser extension
task --list                # every task
task gpui:package APP=desqu   # build an installer (vials, workspaces, screenbound or desqu)
```

## Repository layout

- `apps/` — one folder per product
- `crates/`, `packages/` — code shared between products (added as the transformation progresses)
- `scripts/` — shared, parameterized PowerShell scripts

## Docs

- [docs/architecture.md](docs/architecture.md) — how modules, shell and apps fit together
- [docs/releasing.md](docs/releasing.md) — how releases and updates work
- [AGENTS.md](AGENTS.md) — repository-wide engineering rules
- [DESQU-PLAN.md](DESQU-PLAN.md) — the transformation plan and progress log
- [PLAN.md](PLAN.md) — index of per-product status
