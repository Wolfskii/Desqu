# AGENTS.md — engineering rules for the Desqu Fullscreen extension

Read [PLAN.md](PLAN.md) before changing code; it is the step-by-step plan and the source of truth
for the protocol and site-rule contracts. Repo-wide rules are in [../../AGENTS.md](../../AGENTS.md).

## Layering

- `src/shared/` is pure TypeScript (rules, protocol, storage helpers). It has unit tests and no
  browser API calls except `storage.ts`.
- `src/entrypoints/` holds the WXT entrypoints (background, content scripts, popup, options).
- The MAIN-world script never touches `chrome.*`. It talks to the isolated script with DOM events only.
- Matching of URLs to modes happens only here. Desqu never matches URLs.

## Rules

- Keep comments short; explain why, not what.
- Public contract changes must update `test-fixtures/protocol/*.json`, the Rust side, and PLAN.md together.
- Never commit the dev private key. It lives outside the repo.
- Do not claim Safari support anywhere.
