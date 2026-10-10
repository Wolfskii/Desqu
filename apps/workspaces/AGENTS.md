# AGENTS.md — engineering rules for Workspaces

Read [docs/virtual-desktop-research.md](docs/virtual-desktop-research.md) first. Repo-wide rules:
[../../AGENTS.md](../../AGENTS.md).

- Only the virtual desktop registry layout is used. Never use the undocumented virtual desktop COM
  interfaces and never touch other apps' windows.
- `module/src/core/` is pure (model, ownership, icon-visibility plan) and unit tested; native code
  lives in `module/src/platform/`.
- Icons are hidden only through `desqu-desktop` (owner id `workspaces`) and must always be undone.
- Desktop switches are detected by `RegNotifyChangeKeyValue`, never by polling.
- Items owned by a removed desktop are shown again; hiding must never make an item unreachable.
- Verify with `pwsh apps/workspaces/scripts/smoke-test.ps1`; tests may only touch temp files they
  create. It needs the module lock, so quit a running Desqu/Workspaces first.
