// Site rules: which fullscreen mode applies to which website. Pure logic, no browser APIs.
// The shape mirrors the Rust serde types in apps/screenbound/module/src/core/browser.rs.

export type FullscreenMode = "native" | "fullscreen" | "window" | "zone";

export type SitePattern =
  | { type: "host"; value: string }
  | { type: "domain"; value: string }
  | { type: "urlPrefix"; value: string };

export interface SiteRule {
  id: string;
  enabled: boolean;
  pattern: SitePattern;
  mode: FullscreenMode;
  /** Zone mode only. null = use the document's default zone. */
  zoneId: string | null;
}

export interface SiteRules {
  version: 1;
  /** ms since epoch; set by whoever last edited the document. */
  updatedAt: number;
  defaultMode: FullscreenMode;
  defaultZoneId: string | null;
  rules: SiteRule[];
}

export interface ResolvedMode {
  mode: FullscreenMode;
  zoneId: string | null;
  ruleId: string | null;
}

export const DEFAULT_RULES: SiteRules = {
  version: 1,
  updatedAt: 0,
  defaultMode: "native",
  defaultZoneId: null,
  rules: [],
};

export const MODES: FullscreenMode[] = ["native", "fullscreen", "window", "zone"];

/** Lowercase, trim, drop a scheme, wildcard, path and trailing dot. Used for hosts and host-like patterns. */
export function normalizeHost(value: string): string {
  let v = value.trim().toLowerCase();
  v = v.replace(/^[a-z][a-z0-9+.-]*:\/\//, "");
  v = v.replace(/^\*\./, "");
  v = v.split("/")[0] ?? "";
  return v.replace(/\.$/, "");
}

function normalizePattern(p: SitePattern): SitePattern {
  if (p.type === "urlPrefix") return { type: "urlPrefix", value: p.value.trim() };
  return { type: p.type, value: normalizeHost(p.value) };
}

/**
 * Picks the mode for a page URL. Most specific rule wins: URL prefix, then exact host, then domain
 * (with subdomains). Ties go to the earlier rule. Disabled rules and non-web pages are skipped.
 */
export function resolveMode(rules: SiteRules, url: string): ResolvedMode {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return { mode: "native", zoneId: null, ruleId: null };
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    return { mode: "native", zoneId: null, ruleId: null };
  }
  const host = normalizeHost(parsed.hostname);
  const href = parsed.href;

  let best: { score: number; rule: SiteRule } | null = null;
  for (const rule of rules.rules) {
    if (!rule.enabled) continue;
    const pattern = normalizePattern(rule.pattern);
    if (!pattern.value) continue;
    let score = -1;
    if (pattern.type === "urlPrefix") {
      if (href.startsWith(pattern.value)) score = 3000 + pattern.value.length;
    } else if (pattern.type === "host") {
      if (host === pattern.value) score = 2000 + pattern.value.length;
    } else if (host === pattern.value || host.endsWith("." + pattern.value)) {
      score = 1000 + pattern.value.length;
    }
    if (score >= 0 && (best === null || score > best.score)) {
      best = { score, rule };
    }
  }
  if (!best) {
    return { mode: rules.defaultMode, zoneId: rules.defaultZoneId, ruleId: null };
  }
  return {
    mode: best.rule.mode,
    zoneId: best.rule.zoneId ?? rules.defaultZoneId,
    ruleId: best.rule.id,
  };
}

/** Whole-document last-write-wins. Ties keep `remote` (Desqu is the source of truth). */
export function mergeRules(local: SiteRules, remote: SiteRules): SiteRules {
  return local.updatedAt > remote.updatedAt ? local : remote;
}

/** Trims values, drops empty patterns, and dedupes rule ids (first one wins). */
export function sanitizeRules(input: SiteRules): SiteRules {
  const seen = new Set<string>();
  const rules: SiteRule[] = [];
  for (const rule of input.rules) {
    const pattern = normalizePattern(rule.pattern);
    if (!pattern.value || seen.has(rule.id)) continue;
    seen.add(rule.id);
    rules.push({ ...rule, pattern, zoneId: rule.zoneId || null });
  }
  return {
    version: 1,
    updatedAt: input.updatedAt,
    defaultMode: MODES.includes(input.defaultMode) ? input.defaultMode : "native",
    defaultZoneId: input.defaultZoneId || null,
    rules,
  };
}

export function isSiteRules(value: unknown): value is SiteRules {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    v.version === 1 &&
    typeof v.updatedAt === "number" &&
    typeof v.defaultMode === "string" &&
    MODES.includes(v.defaultMode as FullscreenMode) &&
    Array.isArray(v.rules)
  );
}
