# Manual testing: Desqu Fullscreen

Automated tests cover the rules and the protocol. Fullscreen behavior has to be checked by hand in
each browser. Record results in `apps/screenbound/docs/fullscreen-research.md` (browser, version,
date, what was seen).

## Setup

1. `task browser:dev` (Chrome) or `task browser:dev:firefox` (Firefox). Or load `.output/chrome-mv3`
   unpacked by hand.
2. `task browser:pages`, then open `http://127.0.0.1:5180/` in a normal window.
3. In the extension options, set the default mode, or set this host (`127.0.0.1`) with the popup.

## Checklist (test page)

For each mode: `native`, `fullscreen`, `window`. (`zone` is tested after Phase C.)

| # | Case | native | fullscreen | window |
| --- | --- | --- | --- | --- |
| 1 | Video, native fullscreen button | browser fullscreen | browser fullscreen | fills the tab, no OS fullscreen |
| 2 | Custom player in a transformed parent | browser fullscreen | browser fullscreen | fills the tab |
| 3 | Iframe with allowfullscreen | fullscreen | fullscreen | fills the tab (also the iframe's content) |
| 4 | Iframe without allowfullscreen | refused (log: rejected) | refused | refused (log: rejected) |
| 5 | Shadow DOM button | browser fullscreen | browser fullscreen | fills the tab |
| 6 | Esc | leaves fullscreen | leaves fullscreen | leaves the fake fullscreen, the page sees `fullscreenchange` |
| 7 | `document.fullscreenElement` in the log | set while fullscreen | set while fullscreen | set while faked, `null` after Esc |
| 8 | Rule edit while the tab is open | — | — | the new mode applies without a reload |

## Real sites (window mode unless stated)

- YouTube watch page (button, Esc, theater mode, and the iframe embed on a third-party page).
- Twitch, Vimeo, and a site with its own player.
- Netflix: DRM video. Expect that some DRM players refuse fake fullscreen. Record what happens.
- An incognito/private window: the extension must be allowed in private windows, and the page works the same.
- `chrome://extensions` and `about:addons`: the extension must stay out of the way.

## Browsers

Chrome, Edge, Brave, Opera, Vivaldi, Firefox, LibreWolf. Note the version of each. Safari is not in scope.

## Desqu integration (run after the window checklist)

1. Start Desqu (`task desqu:dev`). The popup and the options page say "Connected to Desqu <version>".
2. Registration: `reg query "HKCU\Software\Google\Chrome\NativeMessagingHosts\io.github.wolfskii.desqu.browser"`
   shows the manifest path under `%LOCALAPPDATA%\Desqu\browser-host`.
3. Rule sync: change a rule in the extension options; it appears in the Browsers tab within a second. Change a
   rule in the Browsers tab; the extension's options show it.
4. Real fullscreen: `fullscreen` mode on YouTube. Screenbound's Activity log shows "Left alone".
5. Zone mode (needs the C1 spike results): YouTube in `zone` mode. The video fills the zone with no
   monitor-sized flash. Esc, switching tabs, closing the tab, and moving the browser window each give the
   window back exactly. Killing Desqu while zoned puts the window back on the next start.
6. Desqu closed: the popup says "Desqu is not running"; `window` mode still works; `zone` mode shows "!"
   on the toolbar icon with the reason.
