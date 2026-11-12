import { describe, expect, it } from "vitest";
import {
  DEFAULT_RULES,
  isSiteRules,
  mergeRules,
  resolveMode,
  sanitizeRules,
  type SiteRule,
  type SiteRules,
} from "./rules";

const rule = (
  id: string,
  pattern: SiteRule["pattern"],
  mode: SiteRule["mode"],
  extra: Partial<SiteRule> = {},
): SiteRule => ({ id, enabled: true, pattern, mode, zoneId: null, ...extra });

const doc = (rules: SiteRule[], extra: Partial<SiteRules> = {}): SiteRules => ({
  ...DEFAULT_RULES,
  rules,
  ...extra,
});

describe("resolveMode", () => {
  it("uses the default mode when no rule matches", () => {
    const r = resolveMode(doc([], { defaultMode: "window", defaultZoneId: "z1" }), "https://example.com/");
    expect(r).toEqual({ mode: "window", zoneId: "z1", ruleId: null });
  });

  it("matches a domain and its subdomains but not look-alikes", () => {
    const rules = doc([rule("d", { type: "domain", value: "youtube.com" }, "window")]);
    expect(resolveMode(rules, "https://youtube.com/").ruleId).toBe("d");
    expect(resolveMode(rules, "https://m.youtube.com/watch").ruleId).toBe("d");
    expect(resolveMode(rules, "https://notyoutube.com/").ruleId).toBeNull();
    expect(resolveMode(rules, "https://youtube.com.evil.net/").ruleId).toBeNull();
  });

  it("prefers an exact host over a domain rule", () => {
    const rules = doc([
      rule("domain", { type: "domain", value: "youtube.com" }, "window"),
      rule("host", { type: "host", value: "www.youtube.com" }, "zone"),
    ]);
    expect(resolveMode(rules, "https://www.youtube.com/").mode).toBe("zone");
    expect(resolveMode(rules, "https://music.youtube.com/").mode).toBe("window");
  });

  it("prefers a URL prefix over a host rule", () => {
    const rules = doc([
      rule("host", { type: "host", value: "www.youtube.com" }, "window"),
      rule("prefix", { type: "urlPrefix", value: "https://www.youtube.com/embed" }, "fullscreen"),
    ]);
    expect(resolveMode(rules, "https://www.youtube.com/embed/abc").mode).toBe("fullscreen");
    expect(resolveMode(rules, "https://www.youtube.com/watch?v=1").mode).toBe("window");
  });

  it("skips disabled rules", () => {
    const rules = doc([
      rule("off", { type: "host", value: "a.example" }, "zone", { enabled: false }),
      rule("on", { type: "domain", value: "example" }, "window"),
    ]);
    expect(resolveMode(rules, "https://a.example/").ruleId).toBe("on");
  });

  it("breaks ties by list order", () => {
    const rules = doc([
      rule("first", { type: "domain", value: "example.com" }, "window"),
      rule("second", { type: "domain", value: "example.com" }, "zone"),
    ]);
    expect(resolveMode(rules, "https://example.com/").ruleId).toBe("first");
  });

  it("normalizes uppercase hosts and trailing dots", () => {
    const rules = doc([rule("h", { type: "host", value: "WWW.Example.COM." }, "window")]);
    expect(resolveMode(rules, "https://www.example.com./page").ruleId).toBe("h");
    expect(resolveMode(rules, "HTTPS://WWW.EXAMPLE.COM/").ruleId).toBe("h");
  });

  it("accepts a pasted URL or wildcard as a pattern value", () => {
    const rules = doc([rule("w", { type: "domain", value: "*.vimeo.com/" }, "window")]);
    expect(resolveMode(rules, "https://player.vimeo.com/video/1").ruleId).toBe("w");
  });

  it("treats non-web pages as native", () => {
    const rules = doc([], { defaultMode: "window" });
    expect(resolveMode(rules, "about:blank").mode).toBe("native");
    expect(resolveMode(rules, "chrome://settings").mode).toBe("native");
    expect(resolveMode(rules, "edge://extensions").mode).toBe("native");
    expect(resolveMode(rules, "moz-extension://abc/popup.html").mode).toBe("native");
    expect(resolveMode(rules, "not a url").mode).toBe("native");
  });

  it("falls back to the rule's zone, then the default zone", () => {
    const rules = doc([rule("z", { type: "host", value: "a.example" }, "zone", { zoneId: "left" })], {
      defaultZoneId: "fallback",
    });
    expect(resolveMode(rules, "https://a.example/").zoneId).toBe("left");
    const noZone = doc([rule("z", { type: "host", value: "b.example" }, "zone")], {
      defaultZoneId: "fallback",
    });
    expect(resolveMode(noZone, "https://b.example/").zoneId).toBe("fallback");
  });
});

describe("mergeRules", () => {
  const older = doc([], { updatedAt: 100, defaultMode: "window" });
  const newer = doc([], { updatedAt: 200, defaultMode: "zone" });

  it("keeps the document with the larger updatedAt", () => {
    expect(mergeRules(older, newer)).toBe(newer);
    expect(mergeRules(newer, older)).toBe(newer);
  });

  it("keeps the remote copy on a tie", () => {
    const remote = doc([], { updatedAt: 5, defaultMode: "fullscreen" });
    const local = doc([], { updatedAt: 5, defaultMode: "window" });
    expect(mergeRules(local, remote)).toBe(remote);
  });
});

describe("sanitizeRules", () => {
  it("drops empty patterns and duplicate ids, trims values", () => {
    const clean = sanitizeRules(
      doc([
        rule("a", { type: "host", value: "  Example.com  " }, "window"),
        rule("a", { type: "host", value: "dup.example" }, "zone"),
        rule("b", { type: "domain", value: "   " }, "zone"),
        rule("c", { type: "urlPrefix", value: "" }, "zone"),
      ]),
    );
    expect(clean.rules.map((r) => r.id)).toEqual(["a"]);
    expect(clean.rules[0]!.pattern.value).toBe("example.com");
  });

  it("replaces an unknown default mode with native", () => {
    const clean = sanitizeRules({ ...DEFAULT_RULES, defaultMode: "bogus" as never });
    expect(clean.defaultMode).toBe("native");
  });
});

describe("isSiteRules", () => {
  it("accepts a valid document and rejects others", () => {
    expect(isSiteRules(DEFAULT_RULES)).toBe(true);
    expect(isSiteRules({ ...DEFAULT_RULES, version: 2 })).toBe(false);
    expect(isSiteRules({ ...DEFAULT_RULES, defaultMode: "popup" })).toBe(false);
    expect(isSiteRules(null)).toBe(false);
  });
});
