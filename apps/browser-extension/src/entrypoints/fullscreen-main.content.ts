// MAIN world: replaces the page's Fullscreen API so "window" and "zone" modes never ask the browser
// for real fullscreen. The isolated script (fullscreen-bridge.content.ts) sets the mode and hears
// our requests through DOM events. Event details are strings only, so they cross worlds safely.
import { defineContentScript } from "wxt/utils/define-content-script";
import { EV_EXIT, EV_FORCE_EXIT, EV_MODE, EV_REQUEST } from "../shared/messages";
import type { FullscreenMode as Mode } from "../shared/rules";

const ATTR_EL = "data-desqu-fs";
const ATTR_ANCESTOR = "data-desqu-fs-ancestor";
const ATTR_ROOT = "data-desqu-fs-root";
const STYLE_ID = "desqu-fs-style";
const MSG = "__desquFs";

const CSS = `
[${ATTR_EL}]{position:fixed!important;inset:0!important;width:100vw!important;height:100vh!important;
max-width:none!important;max-height:none!important;min-width:0!important;min-height:0!important;
margin:0!important;padding:0!important;border:0!important;box-sizing:border-box!important;
z-index:2147483647!important;transform:none!important;translate:none!important;scale:none!important;
background:#000!important;}
video[${ATTR_EL}]{object-fit:contain!important;}
[${ATTR_ANCESTOR}]{transform:none!important;translate:none!important;scale:none!important;
filter:none!important;perspective:none!important;contain:none!important;will-change:auto!important;
backdrop-filter:none!important;container-type:normal!important;z-index:2147483647!important;
overflow:visible!important;clip-path:none!important;}
html[${ATTR_ROOT}],html[${ATTR_ROOT}] body{overflow:hidden!important;}
`;

interface FakeState {
  el: Element;
  ancestors: Element[];
}

let mode: Mode = "native";
let fake: FakeState | null = null;

/** The outermost shadow host that lives in this document (or the element itself). */
function retarget(el: Element): Element {
  let node: Element = el;
  for (;;) {
    const root = node.getRootNode();
    if (root instanceof ShadowRoot) node = root.host;
    else return node;
  }
}

/** Parent in the flat tree, crossing shadow roots. null at the top of the document. */
function flatParent(el: Element): Element | null {
  if (el.parentElement) return el.parentElement;
  const root = el.getRootNode();
  return root instanceof ShadowRoot ? root.host : null;
}

function injectStyle(): void {
  if (document.getElementById(STYLE_ID)) return;
  const style = document.createElement("style");
  style.id = STYLE_ID;
  style.textContent = CSS;
  (document.head ?? document.documentElement).appendChild(style);
}

function fireChange(el: Element): void {
  // Browsers fire these asynchronously, after the request promise settles.
  queueMicrotask(() => {
    const init = { bubbles: true, composed: true };
    el.dispatchEvent(new Event("fullscreenchange", init));
    el.dispatchEvent(new Event("webkitfullscreenchange", init));
    document.dispatchEvent(new Event("fullscreenchange"));
    document.dispatchEvent(new Event("webkitfullscreenchange"));
  });
}

/** Starts fake fullscreen on `el`, then asks the parent frame to do the same for its iframe. */
function fakeEnter(el: Element): void {
  if (fake) fakeExit(null);
  injectStyle();
  const ancestors: Element[] = [];
  for (let p = flatParent(el); p && p !== document.documentElement; p = flatParent(p)) {
    ancestors.push(p);
  }
  el.setAttribute(ATTR_EL, "");
  for (const a of ancestors) a.setAttribute(ATTR_ANCESTOR, "");
  document.documentElement.setAttribute(ATTR_ROOT, "");
  fake = { el, ancestors };
  fireChange(el);

  if (window.top !== window) {
    window.parent.postMessage({ [MSG]: "enter" }, "*");
  }
}

/**
 * Ends fake fullscreen. `from` is the window the exit came from (null when local). Exit is passed
 * on to the parent and to the child iframe, except back to where it came from.
 */
function fakeExit(from: Window | null): void {
  if (!fake) return;
  const { el, ancestors } = fake;
  fake = null;
  el.removeAttribute(ATTR_EL);
  for (const a of ancestors) a.removeAttribute(ATTR_ANCESTOR);
  document.documentElement.removeAttribute(ATTR_ROOT);
  fireChange(el);
  document.dispatchEvent(new CustomEvent(EV_EXIT));

  if (el instanceof HTMLIFrameElement && el.contentWindow && el.contentWindow !== from) {
    el.contentWindow.postMessage({ [MSG]: "exit" }, "*");
  }
  if (window.top !== window && window.parent !== from) {
    window.parent.postMessage({ [MSG]: "exit" }, "*");
  }
}

function onWindowMessage(event: MessageEvent): void {
  const data = event.data as Record<string, unknown> | null;
  if (!data || typeof data !== "object" || typeof data[MSG] !== "string") return;
  if (data[MSG] === "enter") {
    // A child frame went fullscreen: make the iframe that holds it fullscreen too.
    const frame = Array.from(document.querySelectorAll("iframe")).find(
      (f) => f.contentWindow === event.source,
    );
    if (frame) fakeEnter(frame);
  } else if (data[MSG] === "exit" && fake) {
    const fakeFrame = fake.el instanceof HTMLIFrameElement ? fake.el.contentWindow : null;
    // Only react to the parent, or to the child we are fullscreening into.
    if (event.source === window.parent || (fakeFrame !== null && event.source === fakeFrame)) {
      fakeExit(event.source as Window);
    }
  }
}

// ---- patching -------------------------------------------------------------------------------

// Patched functions report the original's source text, since some players check for native code.
const origFunctionToString = Function.prototype.toString;
const originals = new WeakMap<object, Function>();

function installToStringSpoof(): void {
  const proxy = new Proxy(origFunctionToString, {
    apply(target, thisArg, args) {
      const original = typeof thisArg === "function" ? originals.get(thisArg) : undefined;
      return original
        ? Reflect.apply(target, original, [])
        : Reflect.apply(target, thisArg, args);
    },
  });
  Object.defineProperty(Function.prototype, "toString", { value: proxy, writable: true, configurable: true });
}

function replaceMethod(target: object, name: string, replacement: Function): boolean {
  const desc = Object.getOwnPropertyDescriptor(target, name);
  if (!desc || typeof desc.value !== "function") return false;
  Object.defineProperty(replacement, "name", { value: name });
  Object.defineProperty(replacement, "length", { value: desc.value.length });
  originals.set(replacement, desc.value);
  Object.defineProperty(target, name, { ...desc, value: replacement });
  return true;
}

function replaceGetter(target: object, name: string, make: (originalGet: () => unknown) => () => unknown): void {
  const desc = Object.getOwnPropertyDescriptor(target, name);
  if (!desc?.get) return;
  const originalGet = desc.get as () => unknown;
  const get = make(originalGet);
  originals.set(get, originalGet);
  Object.defineProperty(target, name, { ...desc, get });
}

function userActive(): boolean {
  return navigator.userActivation ? navigator.userActivation.isActive : true;
}

function install(): void {
  installToStringSpoof();

  // Request methods. Window and zone modes fake the fullscreen; native and fullscreen go to the browser.
  for (const name of ["requestFullscreen", "webkitRequestFullscreen", "webkitRequestFullScreen"]) {
    const original = (Element.prototype as unknown as Record<string, Function>)[name];
    if (!original) continue;
    replaceMethod(Element.prototype, name, {
      [name](this: Element, ...args: unknown[]) {
        if (!document.fullscreenEnabled || !userActive()) {
          return Reflect.apply(original, this, args);
        }
        document.dispatchEvent(new CustomEvent(EV_REQUEST, { detail: mode }));
        if (mode === "window" || mode === "zone") {
          fakeEnter(this);
          return Promise.resolve();
        }
        return Reflect.apply(original, this, args);
      },
    }[name]!);
  }

  // Exit methods.
  for (const name of ["exitFullscreen", "webkitExitFullscreen", "webkitCancelFullScreen"]) {
    const original = (Document.prototype as unknown as Record<string, Function>)[name];
    if (!original) continue;
    replaceMethod(Document.prototype, name, {
      [name](this: Document, ...args: unknown[]) {
        if (fake) {
          fakeExit(null);
          return Promise.resolve();
        }
        return Reflect.apply(original, this, args);
      },
    }[name]!);
  }

  // Getters: while faking, the page sees the fake element as its fullscreen element.
  for (const name of ["fullscreenElement", "webkitFullscreenElement", "webkitCurrentFullScreenElement"]) {
    replaceGetter(Document.prototype, name, (originalGet) =>
      function (this: Document): unknown {
        return fake ? retarget(fake.el) : originalGet.call(this);
      },
    );
  }
  for (const name of ["fullscreen", "webkitIsFullScreen"]) {
    replaceGetter(Document.prototype, name, (originalGet) =>
      function (this: Document): unknown {
        return fake ? true : originalGet.call(this);
      },
    );
  }
  replaceGetter(ShadowRoot.prototype, "fullscreenElement", (originalGet) =>
    function (this: ShadowRoot): unknown {
      return fake && fake.el.getRootNode() === this ? fake.el : originalGet.call(this);
    },
  );

  // Escape leaves our fake fullscreen before the page sees the key.
  window.addEventListener(
    "keydown",
    (e) => {
      if (fake && e.key === "Escape") {
        e.stopImmediatePropagation();
        fakeExit(null);
      }
    },
    true,
  );

  window.addEventListener("message", onWindowMessage);

  document.addEventListener(EV_MODE, (e) => {
    const detail = (e as CustomEvent<unknown>).detail;
    if (detail === "native" || detail === "fullscreen" || detail === "window" || detail === "zone") {
      mode = detail;
    }
  });
  document.addEventListener(EV_FORCE_EXIT, () => fakeExit(null));
}

export default defineContentScript({
  matches: ["<all_urls>"],
  runAt: "document_start",
  allFrames: true,
  matchAboutBlank: true,
  world: "MAIN",
  main() {
    install();
  },
});
