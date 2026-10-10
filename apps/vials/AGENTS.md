# AGENTS.md — engineering rules for Vials

Read [docs/desktop-research.md](docs/desktop-research.md) first: it records what was observed on a
real desktop and why items are parked off-screen. Repo-wide rules: [../../AGENTS.md](../../AGENTS.md).

- `module/src/core/` is pure (model, rules, layout math, glob matcher); keep it free of native
  calls and unit tested.
- The engine owns all state and talks to the desktop only through `desqu-desktop`
  (`DesktopLayer`, owner id `vials`). Never move or edit the user's files.
- Hiding must always be undone: write-ahead journal, restore on stop/exit, recover on next start.
- Vial windows are labelled `vial-<id>`; capabilities must cover `vial-*` in every host app
  (standalone and Desqu).
- Verify with `pwsh apps/vials/scripts/smoke-test.ps1`; tests may only touch temp files they create.
