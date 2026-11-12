// Pure pieces of the bridge client: reconnect backoff, request/reply matching, and reading the
// reason the native host went away. No browser APIs here, so these are unit-tested.
import type { ServerMessage } from "./protocol";

/** Reconnect delays: 5 s, then 30 s, then 5 min at most. */
const BACKOFF_MS = [5_000, 30_000, 300_000];

/** The delay before the next attempt, given the last one (null after a successful connection). */
export function nextBackoff(previous: number | null): number {
  if (previous === null) return BACKOFF_MS[0]!;
  const index = BACKOFF_MS.indexOf(previous);
  return BACKOFF_MS[Math.min(index + 1, BACKOFF_MS.length - 1)]!;
}

export type HostState = "notInstalled" | "notRunning" | "connected" | "unknown";

/** Chrome and Firefox both say "not found" when the native host is not registered. */
export function hostStateAfterDisconnect(reason: string | undefined): HostState {
  if (reason && /not found|not registered|no such native application/i.test(reason)) {
    return "notInstalled";
  }
  return "notRunning";
}

export const REQUEST_TIMEOUT_MS = 5_000;

type Reply = ServerMessage & { replyTo: string };

/** Replies are matched by `replyTo`. A request with no reply in time fails instead of hanging. */
export class PendingRequests {
  private waiting = new Map<string, { resolve: (m: Reply) => void; reject: (e: Error) => void; timer: ReturnType<typeof setTimeout> }>();

  add(id: string, timeoutMs = REQUEST_TIMEOUT_MS): Promise<Reply> {
    return new Promise<Reply>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.waiting.delete(id);
        reject(new Error(`no reply to ${id} within ${timeoutMs} ms`));
      }, timeoutMs);
      this.waiting.set(id, { resolve, reject, timer });
    });
  }

  /** Delivers a reply. Returns false when nobody is waiting for it. */
  deliver(message: ServerMessage): boolean {
    if (!("replyTo" in message)) return false;
    const entry = this.waiting.get(message.replyTo);
    if (!entry) return false;
    clearTimeout(entry.timer);
    this.waiting.delete(message.replyTo);
    entry.resolve(message as Reply);
    return true;
  }

  /** Everything in flight fails, e.g. because the port closed. */
  failAll(reason: string): void {
    for (const [id, entry] of this.waiting) {
      clearTimeout(entry.timer);
      entry.reject(new Error(`${reason} (request ${id})`));
    }
    this.waiting.clear();
  }

  get size(): number {
    return this.waiting.size;
  }
}

export type BrowserName = "Chrome" | "Edge" | "Firefox" | "Opera" | "Other";

/** The browser name and version to report in `hello`. Brave identifies as Chrome here; that is accepted. */
export function browserFromUserAgent(ua: string): { name: BrowserName; version: string } {
  const pick = (re: RegExp): string => re.exec(ua)?.[1] ?? "";
  if (/Firefox\//.test(ua)) return { name: "Firefox", version: pick(/Firefox\/([\d.]+)/) };
  if (/Edg\//.test(ua)) return { name: "Edge", version: pick(/Edg\/([\d.]+)/) };
  if (/OPR\//.test(ua)) return { name: "Opera", version: pick(/OPR\/([\d.]+)/) };
  if (/Chrome\//.test(ua)) return { name: "Chrome", version: pick(/Chrome\/([\d.]+)/) };
  return { name: "Other", version: "" };
}

/** One line for the popup and options. */
export function describeStatus(status: { state: HostState; product?: string; appVersion?: string }): string {
  switch (status.state) {
    case "connected": {
      const product = status.product === "screenbound" ? "Screenbound" : "Desqu";
      return `Connected to ${product} ${status.appVersion ?? ""}`.trim();
    }
    case "notRunning":
      return "Desqu is not running. Start Desqu to use the Desqu zone mode.";
    case "notInstalled":
      return "Desqu is not installed (or its browser entries are missing). Install Desqu to use the Desqu zone mode.";
    default:
      return "Not connected to Desqu.";
  }
}
