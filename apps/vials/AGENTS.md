# AGENTS.md — engineering rules for Vials

Read [docs/desktop-research.md](docs/desktop-research.md) first: it records what was observed on a
real desktop and why items are parked off-screen. Repo-wide rules: [../../AGENTS.md](../../AGENTS.md).

- `module/src/core/` is pure (model, rules, layout math, glob matcher); keep it free of native
  calls and unit tested.
- The engine owns all state and talks to the desktop only through `desqu-desktop`
  (`DesktopLayer`, owner id `vials`). Never move, rename, delete or edit the user's files. The one
  thing the layer changes about a file is the hidden attribute, and only to hide it.
- The desktop stays Explorer's by default: items in a vial get the hidden attribute (the layer tells
  the shell, so it reacts in under a second). Optionally (`drawDesktop`) the desktop's own icons are
  switched off and the built-in "Desktop" vial (`Vial.inbox`) draws everything in no other vial.
  Parking icons off-screen is gone: Windows 11 build 26300 clamps them back (see the research doc).
- Hiding must always be undone: write-ahead journal (hidden files, and "we switched the desktop's
  icons off"), restore on stop/exit, recover on next start. A file that was hidden before we touched
  it keeps its attribute.
- Vial windows are labelled `vial-<id>`; capabilities must cover `vial-*` in every host app
  (standalone and Desqu).
- Verify with `pwsh apps/vials/scripts/smoke-test.ps1`; tests may only touch temp files they create.
