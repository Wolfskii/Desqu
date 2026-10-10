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
| [Screenbound](apps/screenbound/README.md) | Keeps fullscreen apps inside a zone you choose on your monitor | Available |
| Vials | Desktop containers for shortcuts, apps and folders | Planned |
| Workspaces | Saves and restores desktop layouts per virtual desktop | Planned |

Desqu (the collection app that hosts all of them in one process) is planned.

## Build

Prerequisites: Rust (stable, MSVC), Node 20+, [Task](https://taskfile.dev), WebView2.

```powershell
task install      # npm dependencies for all workspaces
task --list       # every task, grouped by app (e.g. screenbound:dev)
task ci           # type check, unit tests and clippy for every app
```

## Repository layout

- `apps/` — one folder per product
- `crates/`, `packages/` — code shared between products (added as the transformation progresses)
- `scripts/` — shared, parameterized PowerShell scripts

## Docs

- [AGENTS.md](AGENTS.md) — repository-wide engineering rules
- [DESQU-PLAN.md](DESQU-PLAN.md) — the transformation plan and progress log
- [PLAN.md](PLAN.md) — index of per-product status
