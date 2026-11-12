# Publishing the Desqu Fullscreen extension

Written for the owner, who creates the store accounts and uploads. The add-on is an optional part of
Screenbound (and Desqu, which hosts it). The build side is done:
`task browser:zip` (or `npm run zip:all` in this folder) writes one zip per store into `.output/`.

## GitHub release (optional)

Pushing a tag `browser-v<version>` runs `.github/workflows/browser-extension.yml`: it checks the version
against `package.json`, runs `task browser:ci`, and attaches the zips to a GitHub release (not marked as
latest). The zips install in developer mode (Chromium) or only after Mozilla signs them (Firefox), so this
release is for testing until the store listings exist.

## Before the first upload

1. Run `task browser:ci` (type check, unit tests, all builds) and the manual checklist in
   [manual-testing.md](manual-testing.md) in Chrome and Firefox.
2. Take screenshots: the popup (per-site modes), the options page (rules), and a fullscreen video in
   "In window" mode. Store listings want 1280×800 or 640×400.
3. Decide the store copy from the plan's user-facing text (PLAN.md §7). Keep the limitations visible.

## Per store

| Store | Serves | Account | Upload | Notes |
| --- | --- | --- | --- | --- |
| Chrome Web Store | Chrome, Brave, Vivaldi, Arc | Chrome Web Store developer dashboard (one-time fee) | `.output/*-chrome.zip` | Review can take days. Permissions need a justification (below). |
| Microsoft Edge Add-ons | Edge | Partner Center (free) | `.output/*-edge.zip` | Same package as Chrome. |
| Firefox Add-ons (AMO) | Firefox, LibreWolf, Waterfox, Floorp | addons.mozilla.org developer hub (free) | `.output/*-firefox.zip` **and** the sources zip | Firefox needs the sources zip because the build is minified. Keep the add-on id `desqu-fullscreen@wolfskii.github.io`. |
| Opera add-ons | Opera | Opera add-ons developer portal (free) | `.output/*-opera.zip` | Same package as Chrome. |

Check the exact names and URLs on each store's own site when you sign up; they change.

Safari is not published. Desqu runs on Windows only, and a Safari extension needs a macOS app built in Xcode.

## Permission justifications (paste per store)

- `<all_urls>` (host permission): "The extension decides, for the website the user is on, whether a
  fullscreen request stays in the page or goes to the browser. It needs to see every site to do that."
- `nativeMessaging`: "Optional connection to the Desqu desktop app on the user's Windows PC. Nothing is
  sent to any server."
- `tabs`: "Finds the tab that asked for fullscreen, and notices when that tab is closed or left, so the
  window can be given back."
- `storage`: "Keeps the user's per-site fullscreen rules on this computer."

## Privacy statement (paste per store)

The extension does not collect, store on a server, or sell any data. Rules are kept in the browser's
local storage. The only other program it talks to is the Desqu app on the same computer, through
Chrome's or Firefox's native messaging. The extension makes no network requests of its own.

## After the store assigns an id

1. Put the id in `extension-ids.json`: `chromeWebStore`, `edgeAddons` and `opera` under `chromium`.
   For Firefox the id is already fixed in `wxt.config.ts`.
2. Put the store URL in `storeLinks` in the same file, and in `browserStores.ts` in the Screenbound UI
   (`url` field), so the Browsers tab offers the install link.
3. Rebuild and ship a Desqu or Screenbound release. The host manifest lists every Chromium id from
   `extension-ids.json` as an allowed origin. **An id that is missing there cannot connect to Desqu.**
4. Bump the extension version (`apps/browser-extension/package.json`) for each store upload. The
   version is not managed by `scripts/bump-version.ps1`.

## Updating later

Upload a new zip to each store. Reviews of updates are usually faster. Keep the protocol version
(`PROTOCOL_VERSION` in `src/shared/protocol.ts` and `bridge/protocol.rs`) unchanged unless the
Desqu side changes too.
