# Desqu Fullscreen (browser extension)

Optional extension for the Desqu collection app. It decides, per website, how fullscreen video
behaves: real fullscreen, in the browser window, or inside a Desqu zone.

Without Desqu running, the extension still works for "In window" mode.

## Develop

```
task browser:dev            # Chrome, unpacked, live reload
task browser:dev:firefox    # Firefox (MV3)
task browser:ci             # type check, tests, all builds
```

Load the unpacked build by hand from `apps/browser-extension/.output/chrome-mv3` (or `firefox-mv3`)
if you are not using `task browser:dev`.

Platforms: Chrome, Edge, Opera, Brave, Vivaldi, Firefox, LibreWolf. Safari is not supported.
Desqu runs on Windows only.

The full plan is in [PLAN.md](PLAN.md).
