import { afterEach, describe, expect, it, vi } from "vitest";
import {
  browserFromUserAgent,
  describeStatus,
  hostStateAfterDisconnect,
  nextBackoff,
  PendingRequests,
} from "./bridge-state";

describe("nextBackoff", () => {
  it("starts at 5 s, then 30 s, then stays at 5 min", () => {
    const first = nextBackoff(null);
    const second = nextBackoff(first);
    const third = nextBackoff(second);
    const fourth = nextBackoff(third);
    expect([first, second, third, fourth]).toEqual([5_000, 30_000, 300_000, 300_000]);
  });
});

describe("hostStateAfterDisconnect", () => {
  it("reports a missing host registration as not installed", () => {
    expect(hostStateAfterDisconnect("Specified native messaging host not found.")).toBe("notInstalled");
  });

  it("reports anything else as not running (Desqu may just be closed)", () => {
    expect(hostStateAfterDisconnect("Native host has exited.")).toBe("notRunning");
    expect(hostStateAfterDisconnect(undefined)).toBe("notRunning");
  });
});

describe("PendingRequests", () => {
  afterEach(() => vi.useRealTimers());

  it("matches a reply to its request by replyTo", async () => {
    const pending = new PendingRequests();
    const reply = pending.add("7");
    expect(pending.deliver({ type: "ack", replyTo: "7" })).toBe(true);
    await expect(reply).resolves.toEqual({ type: "ack", replyTo: "7" });
    expect(pending.size).toBe(0);
  });

  it("ignores a reply nobody is waiting for", () => {
    const pending = new PendingRequests();
    expect(pending.deliver({ type: "ack", replyTo: "unknown" })).toBe(false);
    expect(pending.deliver({ type: "bridgeStatus", connected: true })).toBe(false);
  });

  it("fails a request that gets no reply in time", async () => {
    vi.useFakeTimers();
    const pending = new PendingRequests();
    const reply = pending.add("3", 1000);
    const outcome = reply.then(() => "replied", (e: Error) => e.message);
    vi.advanceTimersByTime(1000);
    await expect(outcome).resolves.toContain("no reply to 3");
    expect(pending.size).toBe(0);
  });

  it("fails everything in flight when the port closes", async () => {
    const pending = new PendingRequests();
    const a = pending.add("a");
    const b = pending.add("b");
    pending.failAll("port closed");
    await expect(a).rejects.toThrow("port closed");
    await expect(b).rejects.toThrow("port closed");
    expect(pending.size).toBe(0);
  });
});

describe("browserFromUserAgent", () => {
  it("names the browser and its version", () => {
    expect(browserFromUserAgent("Mozilla/5.0 ... Chrome/130.0.6723.70 Safari/537.36")).toEqual({
      name: "Chrome",
      version: "130.0.6723.70",
    });
    expect(browserFromUserAgent("... Chrome/130.0 Safari/537.36 Edg/130.0.2849.56")).toEqual({
      name: "Edge",
      version: "130.0.2849.56",
    });
    expect(browserFromUserAgent("Mozilla/5.0 ... Firefox/132.0")).toEqual({ name: "Firefox", version: "132.0" });
    expect(browserFromUserAgent("curl/8")).toEqual({ name: "Other", version: "" });
  });
});

describe("describeStatus", () => {
  it("says which product is connected and its version", () => {
    expect(describeStatus({ state: "connected", product: "desqu", appVersion: "0.2.1" })).toBe("Connected to Desqu 0.2.1");
    expect(describeStatus({ state: "connected", product: "screenbound", appVersion: "0.1.0" })).toBe(
      "Connected to Screenbound 0.1.0",
    );
  });

  it("explains a missing or stopped Desqu in plain words", () => {
    expect(describeStatus({ state: "notRunning" })).toContain("not running");
    expect(describeStatus({ state: "notInstalled" })).toContain("not installed");
  });
});
