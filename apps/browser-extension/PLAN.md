# Plan: Desqu Fullscreen browser extension + Screenbound browser bridge

## Context

Screenbound (the fullscreen limiter in `apps/screenbound`, also hosted by Desqu) only sees a browser's
fullscreen **after** the browser has resized its window to the whole monitor. Then it shrinks the
window into the zone. The user sees a monitor‑sized frame for a moment on enter and again on exit,
and the taskbar flickers. [apps/screenbound/docs/fullscreen-research.md](apps/screenbound/docs/fullscreen-research.md)
already concludes that this can't be fixed from outside the browser process.

The fix is an **optional browser extension** that intercepts the page's `requestFullscreen()` *inside
the page*, so the OS window never goes fullscreen:

| Mode (per site) | What happens | Needs Desqu? |
| --- | --- | --- |
| `native` | Pass through; Screenbound limits it as today (glitch remains) | no |
| `fullscreen` | Real fullscreen, and Desqu is told to **leave it alone** (exempt) | for the exemption |
| `window` | The element fills the tab's viewport; the browser window is unchanged | no |
| `zone` | `window` + Desqu moves/clips the browser window so the page viewport covers the zone exactly (tabs/toolbar clipped away) | yes; falls back to `window` |

Rules: a global default mode plus per‑site rules (exact host / domain + subdomains / URL prefix).
They are editable **in both** the extension options page and Desqu's Screenbound UI. While connected,
Desqu is the source of truth; otherwise last‑write‑wins on reconnect.

Transport: **Native Messaging**. The browser spawns a tiny relay exe (`desqu-browser-host.exe`) that
connects to a named pipe served by whichever process runs the Screenbound engine (Desqu or
standalone Screenbound).

Browsers: one MV3 codebase (WXT + Svelte 5 + TS), with builds for **Chrome, Brave, Vivaldi, Opera,
Chromium, Arc** (Chromium build), **Edge** (same build, Edge store), and **Firefox, LibreWolf,
Waterfox, Floorp** (Firefox build). **Safari is out of scope**: Desqu is Windows‑only (decision D3),
and Safari extensions need a macOS app built with Xcode. The UI must say so, never claim support.

---

## 0. Rules for the implementer (read first)

- Read `AGENTS.md`, `DESQU-PLAN.md`, `apps/screenbound/AGENTS.md` before coding. Their rules apply
  here: core = pure + unit tests, platform code only under `platform/windows/`, engine is
  single‑threaded, serde `camelCase`, TS types mirror Rust, no `if chrome` branches, never panic on
  vanished windows, journal before modifying a window.
- Do steps **in order**. Each step lists files, what to write, and a **Done when** check. Commit after
  each step with a conventional message (`feat:`, `fix:`, `docs:`, `build:`). Don't push.
- Commit messages: **no** `Co-Authored-By` or "Generated with" lines (owner preference).
- This file lives at `apps/browser-extension/PLAN.md`. `DESQU-PLAN.md` has a "Phase 10" section
  with one status line per step (legend ⬜ 🟡 🧪 ✅). Update those statuses as you go.
- When a step says **verify on real browser**, record the result (date, browser + version,
  observed behavior) in `apps/screenbound/docs/fullscreen-research.md` under a new "Browser
  extension" heading.

---

## 1. Shared contracts (write these exactly; both sides depend on them)

### 1.1 Names and ids

| Thing | Value |
| --- | --- |
| Extension display name | `Desqu Fullscreen` |
| Firefox add‑on id (`browser_specific_settings.gecko.id`) | `desqu-fullscreen@wolfskii.github.io` |
| Native host name | `io.github.wolfskii.desqu.browser` |
| Host exe | `desqu-browser-host.exe` (next to the app exe) |
| Pipe name | `\\.\pipe\desqu-browser-bridge` |
| Protocol version | `1` |
| Ids file (single source) | `apps/browser-extension/extension-ids.json` |

`extension-ids.json` (store ids filled in after first publish; dev id from the pinned `key`):
```json
{
  "chromium": { "dev": "<id from key>", "chromeWebStore": null, "edgeAddons": null, "opera": null },
  "firefox":  { "gecko": "desqu-fullscreen@wolfskii.github.io" },
  "storeLinks": { "chrome": null, "edge": null, "firefox": null, "opera": null }
}
```
Rust reads it with `include_str!` to build host manifests (`allowed_origins` = every non‑null
Chromium id as `chrome-extension://<id>/`). The Svelte UIs import it for store links (`null` →
show "coming soon" and the manual‑install instructions).

### 1.2 Site rules (TS in `apps/browser-extension/src/shared/rules.ts`, Rust in `apps/screenbound/module/src/core/browser.rs`, TS mirror in `apps/screenbound/ui/src/types.ts`)

```ts
export type FullscreenMode = "native" | "fullscreen" | "window" | "zone";
export type SitePattern =
  | { type: "host"; value: string }       // exact host, e.g. "www.youtube.com"
  | { type: "domain"; value: string }     // domain and all subdomains, e.g. "youtube.com"
  | { type: "urlPrefix"; value: string }; // e.g. "https://www.youtube.com/watch"
export interface SiteRule {
  id: string; enabled: boolean; pattern: SitePattern;
  mode: FullscreenMode; zoneId: string | null; // zone mode only; null = rule default zone
}
export interface SiteRules {
  version: 1; updatedAt: number;          // ms since epoch; set by whoever edits
  defaultMode: FullscreenMode; defaultZoneId: string | null; rules: SiteRule[];
}
```
Rust: `#[serde(tag = "type", content... )]` must produce exactly this JSON (use `#[serde(tag = "type", rename_all = "camelCase")]` with a `value` field per variant). Default: `defaultMode: "native"`, no rules, `updatedAt: 0`.

**Matching (TS only; Desqu never matches URLs).** `resolveMode(rules, url) -> { mode, zoneId, ruleId | null }`:
1. Parse with `new URL(url)`; non‑`http(s)` → `{ mode: "native" }`.
2. Normalize host: lowercase, strip trailing `.`; normalize pattern values the same way (and strip
   `*.` prefix from domain values, a scheme/path from host/domain values).
3. Candidate enabled rules and their specificity score:
   - `urlPrefix`: `url.href.startsWith(value)` → score `3000 + value.length`
   - `host`: `host === value` → score `2000 + value.length`
   - `domain`: `host === value || host.endsWith("." + value)` → score `1000 + value.length`
4. Highest score wins; ties → earlier in the list wins. None → defaults.
5. `zoneId` = rule.zoneId ?? defaultZoneId (only meaningful for `zone`).

Unit tests (vitest) must cover: subdomain vs domain, `www.` exact host beating domain, URL prefix
beating host, disabled rules skipped, ties, IDN/uppercase host, `about:`/`chrome:` URLs, trailing dot.

**Sync (LWW over the whole document).** `mergeRules(local, remote)` returns the one with larger
`updatedAt` (tie → remote/Desqu). Same function in Rust (`core/browser.rs::newer`) with tests.

**Rust validation** in `AppConfig::validated()`: unknown `zoneId`s become `null` (don't reject: the
extension may have stale zones). Trim pattern values, drop rules with empty values, dedupe ids.

### 1.3 Screenbound config addition (`core/config.rs`)

```rust
#[serde(default)] pub browser: BrowserSettings,
// BrowserSettings { #[serde(default = "default_true")] integration_enabled: bool, #[serde(default)] site_rules: SiteRules }
```
Additive with defaults, so `CONFIG_VERSION` stays 2. Add a round‑trip test that an old config without
`browser` loads with defaults.

### 1.4 Wire protocol (JSON, `apps/browser-extension/src/shared/protocol.ts` ⇄ `apps/screenbound/module/src/bridge/protocol.rs`)

Framing everywhere (stdio and pipe): 4‑byte **little‑endian** u32 length + UTF‑8 JSON. Max frame
64 KiB (reject larger; close the connection). Every message has `type`. Requests carry `id`
(string); replies carry `replyTo`.

Host → Desqu, first frame on every pipe connection:
- `hostHello { browserPid, browserProcessName, browserExePath, hostVersion }`

Extension → Desqu:
- `hello { id, protocol: 1, extensionVersion, browser: { name, version }, rules: SiteRules }` → reply `helloAck { replyTo, protocol: 1, product: "desqu"|"screenbound", appVersion, zones: [{id,name}], rules: SiteRules }` (rules = merged winner; if the extension's were newer Desqu saves them first).
- `setRules { id, rules }` → reply `ack`. Desqu saves and broadcasts `rulesChanged` to **all** connections.
- `exemptNext { id, tabTitle }` → reply `ack` (sent *before* calling the real `requestFullscreen`).
- `zoneEnter { id, tabId, tabTitle, zoneId|null, viewport?: Rect }` → reply `zoneApplied { replyTo, ok, zoneName?, error? }`.
  `viewport` = page viewport in **physical screen px**, only sent when the page can measure it
  (Firefox: `mozInnerScreenX/Y × devicePixelRatio`, size `innerWidth/innerHeight × devicePixelRatio`).
- `viewportReport { id, tabId, viewport: Rect }` → reply `ack` (re‑measure after a move, see 4.4).
- `zoneExit { id, tabId }` → reply `ack`.
- `fullscreenNative { tabId, host }` (notification, activity log only).

Desqu → extension (unsolicited):
- `rulesChanged { rules }`, `zonesChanged { zones }`
- `zoneReleased { tabId, reason: "userMoved"|"windowGone"|"released"|"disabled" }` → the extension stays
  in `window` mode (video keeps filling the tab).
- `requestViewport { tabId }` → the extension answers with `viewportReport`.

Host → extension (host‑generated):
- `bridgeStatus { connected: boolean }`, and `error { replyTo, code: "desquUnavailable" }` for
  requests that arrive while the pipe is down.

Errors: `error { replyTo, code, message }`; codes `unsupportedProtocol`, `badRequest`,
`windowNotFound`, `zoneUnavailable`, `disabled`, `desquUnavailable`.

Rust: `#[serde(tag = "type", rename_all = "camelCase")]` enums `ClientMessage` / `ServerMessage`,
with round‑trip tests that compare against literal JSON strings, the same strings the TS tests use. Put
them in `apps/browser-extension/test-fixtures/protocol/*.json` and read them from both test suites.

---

## Phase A — Extension that works without Desqu (`native`, `window`, rules UI)

### A1. Scaffold `apps/browser-extension`
- WXT (`wxt` + `@wxt-dev/module-svelte`), Svelte 5, TypeScript, vitest. npm workspace already covers `apps/*`.
- `package.json` name `@desqu/browser-extension`, scripts: `dev`, `dev:firefox`, `build`, `build:all`
  (`wxt build -b chrome && wxt build -b firefox --mv3 && wxt build -b edge && wxt build -b opera`),
  `zip:all` (same with `wxt zip`; Firefox zip also produces the sources zip AMO needs), `check`
  (`svelte-check`), `test` (`vitest run`).
- `wxt.config.ts`: manifest name/description/icons; permissions `storage`, `tabs`,
  `nativeMessaging` (optional at first, see C3); `host_permissions: ["<all_urls>"]`;
  `browser_specific_settings.gecko = { id, strict_min_version: "128.0" }` (needed for `world: "MAIN"`).
  Chrome min version 111.
- Pin the Chromium dev id: generate an RSA key once (`openssl genrsa 2048 | openssl rsa -pubout -outform DER | base64`), put the public key in the manifest `key` for **dev builds only**, record the resulting id in `extension-ids.json`. Keep the private key out of git.
- `Taskfile.yml` (pattern: `apps/screenbound/Taskfile.yml`): `dev`, `dev:firefox`, `build`, `zip`,
  `test`, `check`, `ci` (check + test + build:all). Include it in the root `Taskfile.yml` as `browser`,
  and add `task: browser:ci` to the root `ci`.
- `README.md` + `AGENTS.md` (short: layering below, how to load unpacked in each browser).
- **Done when:** `task browser:ci` passes and `task browser:dev` opens Chrome with the empty extension loaded.

Layout:
```
apps/browser-extension/
  extension-ids.json  test-fixtures/protocol/*.json  test-pages/
  src/shared/   rules.ts rules.test.ts protocol.ts protocol.test.ts storage.ts messages.ts
  src/entrypoints/
    background.ts                 rule store, sync, bridge client, zone orchestration
    fullscreen-main.content.ts    MAIN world: patches the Fullscreen API
    fullscreen-bridge.content.ts  ISOLATED world: relays MAIN ⇄ background
    popup/ (App.svelte)           this-site mode switch + status
    options/ (App.svelte)         default mode, rules editor, Desqu connection, limitations
```

### A2. Rules model + storage
- Implement `rules.ts` exactly as in 1.2, with tests.
- `storage.ts`: `chrome.storage.local` key `siteRules` (not `sync`; Desqu is the cross‑device
  owner, and `sync` quota is tiny). `getRules()`, `saveRules(r)` (sets `updatedAt = Date.now()`),
  `onRulesChanged(cb)`.
- **Done when:** vitest passes all cases listed in 1.2.

### A3. MAIN‑world Fullscreen API patch (`fullscreen-main.content.ts`)
Content script config: `world: "MAIN"`, `runAt: "document_start"`, `allFrames: true`,
`matchAboutBlank: true`, matches `<all_urls>`.

1. Save the originals: `Element.prototype.requestFullscreen`, `webkitRequestFullscreen`,
   `webkitRequestFullScreen`, `Document.prototype.exitFullscreen`, `webkitExitFullscreen`,
   `webkitCancelFullScreen`, and the property descriptors of `Document.prototype.fullscreenElement`,
   `webkitFullscreenElement`, `webkitCurrentFullScreenElement`, `fullscreen`, `webkitIsFullScreen`,
   `ShadowRoot.prototype.fullscreenElement`.
2. Hold `mode` (default `"native"`). It's updated by a `CustomEvent("desqu-fs-mode", { detail })` on
   `document` from the isolated script (see A4).
3. Patched `requestFullscreen(options)`:
   - If the original `document.fullscreenEnabled` is false (iframe without `allowfullscreen`) →
     call the original (gives the correct rejection).
   - `native` → dispatch `desqu-fs-request {mode:"native"}`, call the original.
   - `fullscreen` → dispatch `desqu-fs-request {mode:"fullscreen"}`, wait for `desqu-fs-ready` or
     150 ms, then call the original (keep the user activation: call it synchronously if the wait
     would lose activation. Check `navigator.userActivation.isActive`. If not active anymore, call
     the original immediately and accept that Desqu might not exempt in time.)
     **Simpler rule for v1:** call the original **immediately** and send the exemption in parallel;
     Desqu waits 300 ms for exemptions before it acts (the engine already settles monitor‑sized
     changes for 300 ms, `FULLSCREEN_SETTLE`). Use this simpler rule.
   - `window` / `zone` → `fakeEnter(this)`; return `Promise.resolve()`. Dispatch
     `desqu-fs-request {mode}` so the background can do the zone work.
4. `fakeEnter(el)`:
   - If another element is fake‑fullscreen, `fakeExit()` first.
   - Inject the stylesheet once (into `document.documentElement`, an `adoptedStyleSheets` entry if
     available, else a `<style>`):
     ```css
     [data-desqu-fs]{position:fixed!important;inset:0!important;width:100vw!important;height:100vh!important;
       max-width:none!important;max-height:none!important;min-width:0!important;min-height:0!important;
       margin:0!important;padding:0!important;border:0!important;box-sizing:border-box!important;
       z-index:2147483647!important;transform:none!important;translate:none!important;scale:none!important;
       background:#000!important}
     video[data-desqu-fs]{object-fit:contain!important}
     [data-desqu-fs-ancestor]{transform:none!important;translate:none!important;scale:none!important;
       filter:none!important;perspective:none!important;contain:none!important;will-change:auto!important;
       backdrop-filter:none!important;container-type:normal!important;z-index:2147483647!important;
       overflow:visible!important;clip-path:none!important}
     html[data-desqu-fs-root],html[data-desqu-fs-root] body{overflow:hidden!important}
     ```
     (These ancestor properties create containing blocks or stacking contexts that would trap a
     `position:fixed` child.)
   - Set `data-desqu-fs` on `el`, `data-desqu-fs-ancestor` on every ancestor up to `<html>`
     (crossing shadow roots via `getRootNode().host`), `data-desqu-fs-root` on `<html>`.
   - Record state `{ el, ancestors }`. Fire `fullscreenchange` + `webkitfullscreenchange`
     (`bubbles: true, composed: true`) on `el`, and `fullscreenchange` on `document`. Fire them
     asynchronously (`queueMicrotask`) after the promise resolves, as browsers do.
   - If `window !== top`: `parent.postMessage({ __desquFs: "enter" }, "*")` (see 5).
5. **Iframes:** a MAIN‑world `message` listener. For `{__desquFs:"enter"}`, find the `<iframe>` whose
   `contentWindow === event.source` and call `fakeEnter(iframe)` (which recurses upward). For
   `{__desquFs:"exit"}` from a parent, call `fakeExit({ fromParent: true })`.
6. `fakeExit()`: remove the attributes, clear state, fire the change events, tell the child frame
   (if the element is an iframe) `{__desquFs:"exit"}`. If not from the parent and not top, tell the
   parent `exit`. Dispatch `desqu-fs-exit` so the background can undo the zone.
7. Getters: `document.fullscreenElement` / `webkitFullscreenElement` /
   `webkitCurrentFullScreenElement` return the fake element (retargeted: if it's inside a shadow
   root, return the outermost shadow host in this document), else the original getter.
   `document.fullscreen` / `webkitIsFullScreen` return `true` while fake.
   `ShadowRoot.prototype.fullscreenElement` likewise.
8. `exitFullscreen` patched: if fake → `fakeExit()` and resolve; else the original.
9. Esc: capture‑phase `keydown` on `window`; if fake and `key === "Escape"` →
   `stopImmediatePropagation()`, `fakeExit()`.
10. Patch functions with `Object.defineProperty` keeping `name`/`length`. Make `toString()` return the
    original's string (some players check for native code).
11. **Done when:** on `test-pages/` (A6) with the mode forced to `window`, every scenario works in
    Chrome and Firefox: video button, custom div, iframe with `allowfullscreen`, nested iframe,
    shadow‑DOM element, Esc exits, the page's `fullscreenchange` handler fires, and
    `document.fullscreenElement` is correct.

### A4. Isolated bridge script + background mode resolution
- `fullscreen-bridge.content.ts` (ISOLATED, `document_start`, `allFrames: true`): on load, send
  `{kind:"getMode", url: location.href}` to the background and forward the answer as
  `desqu-fs-mode`. Listen to the MAIN events (`desqu-fs-request`, `desqu-fs-exit`) and forward them
  to the background as `{kind:"fsRequest", mode, frameIsTop}` / `{kind:"fsExit"}`. Accept
  `{kind:"setMode"}` / `{kind:"forceExit"}` from the background and forward them to MAIN
  (`forceExit` → MAIN calls `fakeExit()`).
- `background.ts`: use the **top frame URL** (`sender.tab.url`), not the frame URL, for `getMode`, so a
  YouTube embed follows the embedding site's rule. Push a new mode to all frames of a tab on
  `tabs.onUpdated` (url change: SPA navigation) and on rules change.
- **Done when:** changing a rule in options changes behavior on an already open tab without a reload.

### A5. Popup + options UI (Svelte)
- **Popup:** current host, resolved mode + which rule matched, a 4‑way switch "This site:
  Native / Real fullscreen / In window / Desqu zone" (+ "Use default"). It creates or updates a
  `host` rule for the current host. There's a Desqu connection dot (A: always "not connected"), and a
  link to options.
- **Options:** default mode + default zone (zone list disabled until connected), rules table (add /
  edit / delete / enable, pattern type select, value, mode, zone), import/export JSON, a
  **Limitations** section (text in 7.2), and a **Desqu** section (status, "Get Desqu" link to the
  GitHub releases page).
- Explain the modes in plain words under each choice (same wording as the Context table).
- **Done when:** rules round‑trip through storage and the popup's per‑site switch works.

### A6. Test pages + manual test doc
- `apps/browser-extension/test-pages/index.html`: a `<video>` with a sample (generate a canvas
  stream so no media file is needed), a "custom player" div with its own button, an iframe
  (`allowfullscreen`) and one without it, a nested iframe, a shadow‑DOM player, and a live log of
  `fullscreenchange`, `document.fullscreenElement`, `innerWidth×innerHeight`. Serve with
  `npx http-server test-pages -p 5180`, plus a second origin port 5181 for cross‑origin iframes.
  Task `browser:pages`.
- `apps/browser-extension/docs/manual-testing.md`: a checklist per browser (Chrome, Edge, Brave,
  Opera, Vivaldi, Firefox, LibreWolf) × mode, including YouTube, Twitch, Vimeo, Netflix (DRM),
  embedded YouTube on a third‑party page, and an incognito/private window.
- **Done when:** the checklist has passed in Chrome and Firefox for `native` and `window`. Commit,
  and mark Phase A done in DESQU‑PLAN.

---

## Phase B — Native host + bridge in Screenbound (connection, sync, exemption)

### B1. Core types (`apps/screenbound/module/src/core/browser.rs`, add to `core/mod.rs`)
- `FullscreenMode`, `SitePattern`, `SiteRule`, `SiteRules`, `BrowserSettings` per 1.2/1.3.
  `SiteRules::newer(a, b)`, `SiteRules::sanitize(&mut self, zones: &[Zone])`.
- Pure geometry for the zone mode (used in Phase C, write now with tests):
  `zone_window_plan(zone: Rect, window: Rect, viewport: Rect) -> (window_rect: Rect, region: Rect)`
  - `inset = viewport − window` per edge (left, top, right, bottom).
  - `window_rect = Rect::new(zone.left − inset.left, zone.top − inset.top, zone.right + inset.right, zone.bottom + inset.bottom)`
  - `region` (window‑relative) `= Rect::new(inset.left, inset.top, inset.left + zone.width(), inset.top + zone.height())`
  - Tests: zero insets, a typical Chrome top inset (~80 px at 100 %, ~120 at 150 %), negative
    monitor origins, a zone at the very top of a monitor (window extends above: allowed).
- Wire `browser` into `AppConfig` + `validated()`; TS mirror in `apps/screenbound/ui/src/types.ts`.
- **Done when:** `task screenbound:test` passes.

### B2. Protocol (`apps/screenbound/module/src/bridge/protocol.rs`)
- Serde enums for 1.4, `read_frame(&mut impl Read) -> io::Result<Option<Vec<u8>>>` and
  `write_frame(&mut impl Write, &[u8])` with the 64 KiB cap. Tests: fixtures from
  `apps/browser-extension/test-fixtures/protocol/`, framing round‑trip over a `Cursor`, oversize
  rejected, EOF → `None`.
- Framing lives in a tiny **new crate** `crates/desqu-ipc` (no Tauri, no windows deps), so the
  host exe can reuse it without depending on the module: `frame.rs` (read/write) + `pipe.rs` (see B3).
  The module and the host depend on it. Add it to the workspace `members`.

### B3. Named pipe server in the module
- Add `interprocess = "2"` to `crates/desqu-ipc`. Expose `desqu_ipc::pipe::{listen(name), connect(name)}`
  returning `Read + Write` streams (Windows named pipe via `interprocess::os::windows::named_pipe`,
  or `local_socket`). **Security:** create the pipe with a DACL that allows only the current user
  and SYSTEM. Build the SDDL `D:P(A;;GA;;;SY)(A;;GA;;;<current user SID>)`; get the SID with
  `GetTokenInformation(TokenUser)` + `ConvertSidToStringSidW`. That Win32 code lives in
  `crates/desqu-ipc/src/platform/windows.rs` with an `unsupported` stub.
- `apps/screenbound/module/src/bridge/server.rs`:
  - `BridgeServer::start(engine_tx: Sender<EngineMessage>) -> BridgeServer` spawns an accept
    thread named `screenbound-bridge`. For each client: assign `ConnId(u64)`, create an
    `mpsc::Sender<ServerMessage>` + writer thread, and a reader thread that parses frames and
    posts `EngineMessage::Call(Box::new(move |e| e.on_browser_message(conn, msg)))`. On EOF/error it
    posts `on_browser_disconnected(conn)`. The first frame must be `hostHello`, else close.
  - Dropping `BridgeServer` stops accepting (connect to itself once to unblock `accept`) and closes
    all clients.
  - Add `EngineHandle::post(f)` (fire‑and‑forget `Call`) in `engine/handle.rs`.
- `plugin.rs::start`: after the engine spawns, if `config.browser.integration_enabled`, start the
  server and store it in a new `ModuleState.bridge: Mutex<Option<BridgeServer>>`. `stop` drops it
  **before** stopping the engine. Only the process holding the module lock serves the pipe, so
  Desqu and standalone Screenbound never both serve.
- **Done when:** a test in `crates/desqu-ipc` connects a client to a listener and exchanges frames.

### B4. Engine side (`apps/screenbound/module/src/engine/browser.rs`, an `impl Engine` block; keep `mod.rs` lean)
State added to `Engine`: `browser_conns: HashMap<ConnId, BrowserConn { tx, host: HostHello, hello: Option<Hello>, since_ms }>`,
`exempt: HashMap<WindowId, Instant>`, `browser_zone: BTreeMap<WindowId, BrowserZoneWindow>` (Phase C).
- `on_browser_message`:
  - `hostHello` → store.
  - `hello` → protocol ≠ 1 → `error unsupportedProtocol`. Else take
    `SiteRules::newer(ext.rules, config.browser.site_rules)`; if the extension's won, save the config
    (through the normal `set_config` path) and broadcast `rulesChanged` to the other connections.
    Reply `helloAck` with zones + rules. Emit `Notification::BrowserChanged`.
  - `setRules` → sanitize, save, broadcast `rulesChanged` to **all** connections, `ack`.
  - `exemptNext` → `find_browser_window(conn, tab_title)` (below). If found → `exempt.insert(id, now + 5 s)`, `ack`. Else `error windowNotFound`.
  - `fullscreenNative` → activity log line "Browser fullscreen on <host> (native)".
- `find_browser_window(conn, tab_title) -> Option<WindowId>`: `platform.foreground_window()`
  (new trait method, Windows `GetForegroundWindow`). Accept it only if
  `platform.process_id(w) == host.browser_pid` **or** its process name equals
  `host.browser_process_name` (some browsers own windows from a child process). If `tab_title` is
  non‑empty, also require `identity.title` to contain it case‑insensitively. Fullscreen needs a
  user click, so the foreground window is the right one.
- **Exemption in `evaluate()`** (mod.rs, right before `self.manage(...)`): if
  `exempt.get(&id)` is present and not expired → log "Left alone (browser asked for real
  fullscreen)" once and skip. When the window is seen **not** monitor‑fullscreen and the exemption
  is older than 1 s, remove it. Also prune expired ones in `process_due`.
  Because an exemption can arrive just after the first WinEvent, extend the settle rule: windows of
  a process with a connected browser (`browser_pid` match) always wait the full
  `FULLSCREEN_SETTLE` (300 ms) before `manage`. They already do for monitor‑sized changes, so
  just make sure no fast path skips it.
- `set_config` → if `browser.site_rules` changed, broadcast `rulesChanged`. If zones changed,
  broadcast `zonesChanged`. If `integration_enabled` flipped, the plugin starts/stops the server
  (the command layer calls `plugin::sync_bridge(app)` after `set_config`).
- `on_browser_disconnected` → release any `browser_zone` windows of that conn, remove it, notify.
- New `Notification::BrowserChanged` → plugin emits `sb://browser-changed`.
- Status: `Engine::browser_status() -> BrowserStatus { integration_enabled, host_registered: bool, host_path, connections: [{ connId, browserName, browserVersion, processName, extensionVersion, sinceMs }] }`.
  Command `get_browser_status`; register it in `plugin.rs` `generate_handler!` and in the plugin
  permissions (follow how existing commands are permitted under `apps/screenbound/module/permissions`).
- Unit tests: use the existing fake platform pattern in the engine tests (search `impl PlatformWindowManager for` in tests). Cover: newer‑wins on hello, broadcast on setRules, exempt prevents `manage`, an expired exemption doesn't.

### B5. Native host exe (`apps/screenbound/browser-host`, bin `desqu-browser-host`)
- Deps: `desqu-ipc`, `serde_json`, `windows` (`Win32_System_Diagnostics_ToolHelp`, `Win32_System_Threading`). No Tauri. `#![windows_subsystem = "windows"]` (no console flash).
- Startup: walk the parent chain (`CreateToolhelp32Snapshot`) from its own pid, skipping
  `cmd.exe` and `conhost.exe` (Chrome on Windows launches hosts through `cmd.exe`). The first other
  process is the browser: record its pid, exe name and path.
- Threads: (1) stdin reader → frames into a channel. EOF → exit the process (the browser closed
  the port). (2) The main loop keeps a pipe connection. On connect, send `hostHello`, write
  `bridgeStatus{connected:true}` to stdout, relay channel → pipe, and a pipe reader thread relays
  pipe → stdout. On pipe failure write `bridgeStatus{connected:false}` and retry every 2 s. While
  disconnected, answer requests (frames with an `id`) with `error{replyTo:id, code:"desquUnavailable"}`
  and drop notifications.
- Stdout carries **only** frames. Log to stderr (the browser captures it), never to stdout.
- Tests: the relay logic as a function over `Read`/`Write` pairs.
- **Done when:** `cargo test -p desqu-browser-host` passes and running it by hand with a framed
  `hello` on stdin (small PowerShell test script `apps/screenbound/scripts/host-smoke.ps1`) reaches a
  running dev Screenbound and prints the `helloAck`.

### B6. Host registration (`apps/screenbound/module/src/platform/windows/native_host.rs` + `unsupported` stub; facade in `platform/mod.rs` as free functions `register_native_host(exe, manifest_dir)` / `unregister_native_host()` / `native_host_registered()`)
- Write two manifests to `%LOCALAPPDATA%\Desqu\browser-host\`:
  - `chromium.json`: `{ name, description, path: <host exe>, type: "stdio", allowed_origins: [...] }`
  - `firefox.json`: same but `allowed_extensions: ["desqu-fullscreen@wolfskii.github.io"]`
- Registry (HKCU, default value = manifest path), key `<base>\io.github.wolfskii.desqu.browser`:
  - chromium.json → `Software\Google\Chrome\NativeMessagingHosts`, `Software\Chromium\NativeMessagingHosts`,
    `Software\Microsoft\Edge\NativeMessagingHosts`, `Software\BraveSoftware\Brave-Browser\NativeMessagingHosts`,
    `Software\Vivaldi\NativeMessagingHosts`, `Software\Opera Software\NativeMessagingHosts`
  - firefox.json → `Software\Mozilla\NativeMessagingHosts`, `Software\LibreWolf\NativeMessagingHosts`,
    `Software\Waterfox\NativeMessagingHosts`
  Extra keys are harmless. Check which key each browser actually reads during manual testing (C5),
  and remove keys that turn out to be useless.
- Write only when the content differs (idempotent). Called from `plugin::start` when
  `integration_enabled`, and from a command `set_browser_integration(enabled)`. Disabling
  unregisters and stops the server.
- Host exe path: `std::env::current_exe()?.with_file_name("desqu-browser-host.exe")`. If it's
  missing (dev), fall back to `target/<profile>/desqu-browser-host.exe` in debug builds only. Else
  `host_registered = false` with a reason in the status.
- **Done when:** after dev start, `reg query "HKCU\Software\Google\Chrome\NativeMessagingHosts\io.github.wolfskii.desqu.browser"` shows the manifest path, and the manifest has the right exe path.

### B7. Bundle the host with both apps
- `apps/screenbound/src-tauri/tauri.conf.json` and `apps/desqu/src-tauri/tauri.conf.json`:
  `bundle.externalBin: ["binaries/desqu-browser-host"]`. Tauri expects
  `src-tauri/binaries/desqu-browser-host-x86_64-pc-windows-msvc.exe`. Add `binaries/` to each
  `src-tauri/.gitignore`.
- `scripts/tauri-build.ps1`: before `tauri build`, run `cargo build -p desqu-browser-host --release`
  and copy the exe to that name in `apps/$App/src-tauri/binaries/`. Do it only for
  `screenbound` and `desqu`. Do the same in the dev tasks (`task screenbound:dev`, `task desqu:dev`,
  with a debug build).
- Portable zip (`scripts/publish-artifacts.ps1`): include `desqu-browser-host.exe` next to the main exe.
- Uninstall: add NSIS hooks (`bundle.windows.nsis.installerHooks: "windows/hooks.nsh"`) with
  `NSIS_HOOK_POSTUNINSTALL` that deletes the registry keys from B6 and
  `$LOCALAPPDATA\Desqu\browser-host`. If a hooks file already exists, extend it.
- `scripts/check-standalone-deps.ps1`: allow `apps/screenbound/browser-host → crates/desqu-ipc`;
  the host must not depend on any module or on Tauri.
- **Done when:** `pwsh scripts/tauri-build.ps1 -App desqu -NoSign` produces an installer whose
  install folder has `desqu-browser-host.exe`.

### B8. Extension bridge client (`background.ts` + `nativeMessaging` permission)
- Make `nativeMessaging` a **required** permission (simplest; the store review notes it as
  "communicates with the optional Desqu desktop app").
- `connectNative("io.github.wolfskii.desqu.browser")`. On disconnect (host missing → `lastError`
  "Specified native messaging host not found"), back off 5 s → 30 s → 5 min max, and show
  "Desqu not installed" in the UI. On `bridgeStatus{connected:false}` show "Desqu not running"
  and keep the port open (the host retries).
- On `bridgeStatus{connected:true}` → send `hello`; apply `helloAck` (replace local rules with the
  returned ones, store zones in `storage.local` `desquZones`).
- A request helper with `id` + 5 s timeout.
- Local edits: if connected → `setRules`, else save locally with a new `updatedAt` (merged on the next hello).
- `rulesChanged` → save locally **without** touching `updatedAt`, push modes to tabs.
- `fsRequest{mode:"fullscreen"}` → `exemptNext{tabTitle}`. `fsRequest{mode:"native"}` →
  `fullscreenNative`.
- Chrome MV3: an open native port keeps the service worker alive. If the worker restarts anyway,
  reconnect on startup (`runtime.onStartup` + top‑level code).
- Popup/options: real connection status (product name + version), zone pickers filled from `desquZones`.
- **Done when:** with dev Screenbound running, the extension shows "Connected to Screenbound
  0.x". Editing a rule in the extension appears in Desqu's config file, and vice versa (after B9).

### B9. Desqu / Screenbound UI: "Browsers" tab
- `apps/screenbound/ui/src/components/BrowsersPanel.svelte`, added as a tab "Browsers" in
  `ScreenboundView.svelte` (between Rules and Settings). Desqu renders the module UI, so this covers
  both apps. Sections:
  1. **Browser extension**: what it does (one paragraph) and an integration toggle (`set_browser_integration`).
     Status: host registered / path / error.
  2. **Install**: one row per browser (Chrome, Edge, Brave, Opera, Vivaldi, Firefox, LibreWolf) with
     a store link from `extension-ids.json` (`storeLinks`). Brave/Vivaldi use the Chrome link,
     LibreWolf the Firefox link. Each row shows "Connected" when a connection reports that browser.
     Safari row: "Not available: Desqu runs on Windows only."
  3. **Connected browsers**: from `get_browser_status`, live via `sb://browser-changed`.
  4. **Site rules**: the same editor as the extension options (default mode + zone, rules table).
     Saves through the existing `app.scheduleSave` → `set_config` path, setting
     `siteRules.updatedAt = Date.now()` on every edit.
  5. **Without the extension** (limitations, text in 7.1).
- Add `api.getBrowserStatus()`, `api.setBrowserIntegration()` to `apps/screenbound/ui/src/api.ts`,
  types to `types.ts`, and listen for the event in `state.svelte.ts` (follow how `sb://managed-changed` is handled).
- In `SettingsPanel.svelte` add a one‑line card: "Browser fullscreen without the flash: install the
  Desqu Fullscreen extension → Browsers tab".
- **Done when:** `task screenbound:ci` and `task desqu:ci` pass. Verify on a real browser: the
  connected browser shows up, a rule edited in Desqu appears in the extension popup within 1 s, and
  `fullscreen` mode on a site makes Screenbound leave YouTube truly fullscreen (activity log shows
  "Left alone").

---

## Phase C — Zone mode (Desqu places the page viewport in the zone)

### C1. Spike (research only, record results before building C2+)
Verify on real Chrome, Edge and Firefox at 100 % and 150 % scaling, with a temporary probe example
(`apps/screenbound/module/examples/probe_region.rs`, pattern: existing `task screenbound:probe`):
1. `SetWindowRgn` on another process's top‑level browser window is accepted, and the browser keeps
   it until the window is resized again. (Chromium resets regions in `ResetWindowRegion` on size
   changes. We set the region **after** our last `set_bounds`, so it should persist. Confirm.)
2. Viewport rect source:
   - Chromium: the largest visible child window (`Chrome_RenderWidgetHostHWND`) equals the page
     viewport in screen px. Implement it generically as "largest visible child window rect" (no
     class‑name branch).
   - Firefox has no child HWNDs, so it uses the extension‑reported `mozInnerScreenX/Y × dpr`.
     Confirm the error is ≤ 2 px.
3. With the window positioned so the viewport equals the zone and the region clipped, check that
   the video plays smoothly, input works, and Esc reaches the page.
4. A maximized browser window: `set_bounds` restores it first (existing behavior). Confirm that
   `restore_state` re‑maximizes on exit.
**Outcome:** If 1 fails for a browser family, zone mode for that family degrades to "move the
window so its viewport covers the zone, and leave browser UI visible above the zone (clipped only by
the monitor edge)". Record which in fullscreen-research.md. Don't build a workaround beyond that.

### C2. Platform additions (`platform/mod.rs` trait + `platform/windows/window.rs` + `unsupported.rs`)
- `fn foreground_window(&self) -> Option<WindowId>` (if not done in B4)
- `fn set_region(&self, window: WindowId, region: Option<Rect>) -> PlatformResult<()>`
  (`CreateRectRgn` + `SetWindowRgn(hwnd, rgn, TRUE)`; `None` → `SetWindowRgn(hwnd, null, TRUE)`;
  the system owns the region after success, so don't delete it).
- `fn largest_child_rect(&self, window: WindowId) -> Option<Rect>` (`EnumChildWindows`, visible
  only, largest area, `GetWindowRect`).

### C3. Engine zone flow (`engine/browser.rs`)
`BrowserZoneWindow { id, identity, conn, tab_id, zone_id, detected: WindowState, applied: WindowState, target: Rect, region: Rect, since_ms }`
plus a new `JournalKind::BrowserZone`. Recovery: `restore_state(detected)` + `set_region(None)`.
1. `zoneEnter` → reject with `disabled` if `!config.enabled`. `w = find_browser_window(...)`, else
   `windowNotFound`. If `w` is managed by the fullscreen limiter, `release` it first.
2. Resolve the zone: `zoneId` → `config.zone(id)`, else the first `FullscreenZone` rule that
   matches the browser identity (`config.match_fullscreen_rule`), else the first zone. None →
   `zoneUnavailable`. The monitor is the one the window is mostly on (`monitor_for_rect`), with
   `zone.target_monitor(...)` + `zone.resolve_on(monitor)`. Reuse that code from `plan_zone`, and
   refactor a small shared helper if needed. Don't duplicate it.
3. `detected = platform.state(w)`. **Journal first** (`save_journal` includes browser_zone entries).
4. Viewport = message `viewport` if present, else `platform.largest_child_rect(w)`. Neither →
   `error windowNotFound`.
5. `(window_rect, region) = zone_window_plan(zone, detected.bounds, viewport)`. `set_bounds(w, window_rect)`,
   then `set_region(w, Some(region))`, then `cover_taskbar(w)` (existing; raises topmost and marks fullscreen).
6. Verify: re‑measure the viewport (child rect, or send `requestViewport` and continue when
   `viewportReport` arrives, with a 500 ms timeout). If it's off by > 2 px, apply one correction
   using the new measurement (recompute the plan with the current window rect), then stop.
7. Reply `zoneApplied{ok:true, zoneName}`, log activity, `Notification::ManagedChanged`.
   `ManagedSummary` gets `kind: ManagedKind::BrowserZone` so Diagnostics and "Release all" include it
   (add the variant in Rust + TS).
8. `evaluate()`: if `browser_zone.contains_key(&id)` → handle like managed. A user move or resize
   (`MoveSizeStart`) → release with reason `userMoved`. The window is gone → forget + send
   `zoneReleased{windowGone}`. The window became monitor‑fullscreen on its own (F11) → release
   ours, and let the normal limiter flow take it.
9. `zoneExit` / disconnect / `release_window` / `release_all` / shutdown / `config.enabled = false`
   → `set_region(None)`, `restore_state(detected)`, `restore_taskbar`, remove the journal entry,
   send `zoneReleased` (except on `zoneExit`, which gets an `ack`).
10. Fullscreen limiter interplay: a zone‑mode window is never monitor‑fullscreen unless the zone is
    the whole monitor. Step 8's early return keeps the limiter and borderless rules away from it.
- Tests with the fake platform: plan math, journal written before `set_bounds`, release restores
  region + state, a user move releases, a disconnect releases.

### C4. Extension zone orchestration (`background.ts`)
- `fsRequest{mode:"zone"}` → the page is already in fake fullscreen (window mode). If not
  connected → stay in window mode and set the badge to `!` with the tooltip "Desqu not running:
  showing in window".
- If connected: get the viewport from the top frame (`tabs.sendMessage(tabId, {kind:"measure"}, {frameId:0})`).
  The isolated script returns `{ viewport }` only if `typeof mozInnerScreenX === "number"`
  (read via MAIN relay), else `{}`. Then send `zoneEnter{tabId, tabTitle: tab.title, zoneId, viewport?}`.
  Remember `zoneTabs.set(tabId, windowId)`.
- `requestViewport` from Desqu → measure as above → `viewportReport`.
- Exit triggers → `zoneExit` + tell the tab `forceExit` where needed: page `fsExit`; `tabs.onActivated`
  in that window (a different tab now shows through the zone); `tabs.onRemoved`; `tabs.onUpdated`
  with a url change; `windows.onRemoved`.
- `zoneReleased` from Desqu → keep fake fullscreen (window mode) and clear the badge.
- **Done when:** manual: YouTube in `zone` mode in Chrome and Firefox shows **no** monitor‑sized
  frame on enter or exit, the video exactly fills the zone, Esc returns the window exactly to its
  previous bounds (also when it was maximized), and switching tabs exits the zone.

### C5. Manual verification + docs
- Run the full `docs/manual-testing.md` checklist for all listed browsers × all four modes. Record
  which registry keys each browser used (B6) and the C1 outcomes in fullscreen-research.md.
- Update `apps/screenbound/README.md`, `apps/desqu/README.md`, `CHANGELOG.md`s, the
  `docs/architecture.md` (one short "Browser bridge" section: extension → host → pipe → engine), and
  the `apps/screenbound/AGENTS.md` architecture block (add `bridge/`).

---

## Phase D — Store publishing (owner does the account steps; implementer prepares everything)

- `task browser:zip` → `.output/*.zip` for chrome, edge, opera, firefox (+ firefox sources zip).
- `apps/browser-extension/docs/publishing.md`: per store, the account URL, the zip to upload,
  permission justifications (`<all_urls>`: "intercepts the page's fullscreen request on any site";
  `nativeMessaging`: "optional connection to the Desqu desktop app"; `tabs`, `storage`), a privacy
  statement (no data leaves the machine; native messaging is local only), and screenshots to take.
  - Chrome Web Store (also serves Brave, Vivaldi, Arc), Microsoft Edge Add‑ons, Firefox AMO (also
    serves LibreWolf/Waterfox/Floorp), Opera Add‑ons.
  - Safari: not published (state why).
- After each store assigns an id: put it in `extension-ids.json` (`chromeWebStore`, `edgeAddons`,
  `opera`) and the store URL in `storeLinks`. The next Desqu/Screenbound build writes those ids into
  `allowed_origins`. **An extension id missing from `allowed_origins` can't connect**, so a Desqu
  release must follow each new store id.
- Version: the extension has its own semver in `apps/browser-extension/package.json` (not managed by
  `scripts/bump-version.ps1`). The protocol version guards compatibility.

---

## 7. User‑facing text (use as is, adjust only for length)

### 7.1 In Desqu (Browsers tab, "Without the extension")
- When a website goes fullscreen, the browser first fills the whole monitor, then Desqu shrinks it
  into your zone. You see a short flash on entering and leaving fullscreen, and the taskbar may flicker.
- Desqu can't tell which website is fullscreen, so every site gets the same rule.
- You can't let one site use real fullscreen while others stay in the zone.

### 7.2 In the extension ("Limitations")
- Fullscreen from the browser itself (F11 or the browser menu) isn't a website request, so the
  extension can't change it. Desqu still limits it as usual.
- In "In window" and "Desqu zone" modes, the page isn't *really* fullscreen. A few sites style
  their player with fullscreen‑only CSS and may look slightly different.
- Doesn't run on browser pages (settings, extension stores, the built‑in PDF viewer). Private
  windows need "Allow in private/incognito" turned on.
- "Desqu zone" needs the Desqu (or Screenbound) app running on Windows. Without it, the extension
  shows the video in the window instead.
- Safari isn't supported: Desqu runs on Windows only.

---

## Critical files

New: `apps/browser-extension/**`, `crates/desqu-ipc/**`, `apps/screenbound/browser-host/**`,
`apps/screenbound/module/src/core/browser.rs`, `apps/screenbound/module/src/bridge/{mod,protocol,server}.rs`,
`apps/screenbound/module/src/engine/browser.rs`, `apps/screenbound/module/src/platform/windows/native_host.rs`,
`apps/screenbound/ui/src/components/BrowsersPanel.svelte`.

Modified: `Cargo.toml` (members), `Taskfile.yml` (include + ci),
`apps/screenbound/module/src/{core/config.rs, core/mod.rs, engine/mod.rs, engine/handle.rs, engine/journal.rs, plugin.rs, commands.rs, platform/mod.rs, platform/windows/window.rs, platform/unsupported.rs}`,
`apps/screenbound/ui/src/{ScreenboundView.svelte, api.ts, types.ts, state.svelte.ts, components/SettingsPanel.svelte}`,
`apps/{screenbound,desqu}/src-tauri/tauri.conf.json`, `scripts/tauri-build.ps1`,
`scripts/publish-artifacts.ps1`, `scripts/check-standalone-deps.ps1`, `DESQU-PLAN.md`, docs listed in C5.

Reuse, don't rewrite: `Zone::target_monitor` / `resolve_on` (`core/zone.rs`),
`AppConfig::match_fullscreen_rule`, `monitor_for_rect`, `Engine::cover_taskbar` /
`restore_taskbar` / `save_journal` / `log`, `PlatformWindowManager::{set_bounds, restore_state, state, identity, process_id}`,
`EngineHandle` channel, `desqu_shell::mutex` (lock already guarantees one pipe server), `@desqu/ui` components (`SettingsRow`, `Switch`).

## Verification (end to end)

1. `task ci` (root) is green: Rust tests incl. new core/protocol/engine tests, clippy, svelte‑check, and `task browser:ci` (vitest + builds for all targets).
2. `task browser:pages` + `task browser:dev` (Chrome) and `task browser:dev:firefox`: walk the
   `window` mode checklist without Desqu running. Every scenario passes and the badge shows "Desqu not running".
3. Start `task desqu:dev`. The extension shows "Connected to Desqu". The Browsers tab lists the browser.
   Edit a rule on each side and see it on the other within 1 s. Restart the browser offline, edit a
   rule, reconnect: the newer side wins.
4. YouTube with `native` → today's behavior; `fullscreen` → real fullscreen, activity log "Left alone";
   `window` → video fills the tab; `zone` → video fills the zone with no monitor‑sized flash, Esc
   restores the exact previous bounds and maximized state.
5. Kill Desqu (Task Manager) while a browser window is in zone mode, then start it again: journal
   recovery restores the window and clears the region.
6. Uninstall Desqu via NSIS: the registry keys and `%LOCALAPPDATA%\Desqu\browser-host` are gone.
