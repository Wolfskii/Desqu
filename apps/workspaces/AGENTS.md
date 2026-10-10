# AGENTS.md — engineering rules for Workspaces

Read [docs/virtual-desktop-research.md](docs/virtual-desktop-research.md) first. Repo-wide rules:
[../../AGENTS.md](../../AGENTS.md).

- Use only documented surfaces (registry layout, `IVirtualDesktopManager`, `IShellWindows`, process
  queries, `ShellExecuteW`). Never use the undocumented virtual desktop COM interfaces and never move
  foreign windows.
- `module/src/core/` is pure (model, icon-visibility plan, app/folder logic) and unit tested; native
  code lives in `module/src/platform/`.
- Icons are hidden only through `desqu-desktop` (owner id `workspaces`) and must always be undone.
- Desktop switches are detected by `RegNotifyChangeKeyValue`, never by polling.
- Verify with `pwsh apps/workspaces/scripts/smoke-test.ps1`; tests may only touch temp files and
  processes they create.
