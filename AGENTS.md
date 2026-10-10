# AGENTS.md — repo-wide engineering rules

Read [DESQU-PLAN.md](DESQU-PLAN.md) first: it is the source of truth for the transformation and its
progress log. Then read the AGENTS.md of the app you are changing:

- [apps/screenbound/AGENTS.md](apps/screenbound/AGENTS.md) — Screenbound

## Dependency rules

- `apps/<x>/src-tauri` may depend on `apps/<x>/module` and `crates/*`.
- `apps/<x>` (npm) may depend on `apps/<x>/ui` and `packages/*`.
- `apps/desqu` may depend on every module crate and every module UI package.
- Nothing may depend on `apps/desqu`.
- Module crates never depend on other module crates; shared desktop code lives in `crates/desqu-desktop`.
- Enforced by `scripts/check-standalone-deps.ps1` once it exists (DESQU-PLAN.md P5).

## Layering

UI (Svelte) → Tauri commands / plugin → engine (single-threaded state machine) → core (pure logic)
and platform (native code).

- Platform-specific code lives only under `platform/<os>/`. No `cfg(windows)` in core or engine.
- Core never calls native APIs; anything decidable from data belongs in core, with unit tests.
- The engine owns its state; the UI talks to it only through the command layer.

## Conventions

- Rust: `cargo clippy` clean, serde `camelCase` for anything crossing to the UI.
- TypeScript types mirror the Rust serde types; update both together.
- Keep comments short; explain *why*, not *what*.
- Windows only for now (decision D3): keep platform traits and `unsupported` stubs, and never claim
  macOS or Linux support in docs or UI.
- Commit per completed plan step, using conventional messages (`feat:`, `fix:`, `docs:`,
  `refactor:`, `build:`, `chore:`). The owner makes the commits; agents only suggest the message.
