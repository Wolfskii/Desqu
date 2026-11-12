# Desqu Fullscreen (browser add-on)

An optional add-on for Screenbound, the fullscreen window limiter (Desqu hosts Screenbound). Browsers
make a video fullscreen by resizing the whole window, which causes a flash before Screenbound limits it.
This add-on keeps the video in the window or moves it into a zone instead. It decides, per website, how
fullscreen video behaves: real fullscreen, in the browser window, or inside a zone.

Without Screenbound running, the add-on still works for "In window" mode. It is versioned with the rest
of the family (see docs/releasing.md).

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
