// Local storage for site rules. Desqu is the cross-device owner, so this is `local`, not `sync`
// (sync quota is too small for a rules document).
import { browser } from "wxt/browser";
import { DEFAULT_RULES, isSiteRules, sanitizeRules, type SiteRules } from "./rules";

export const RULES_KEY = "siteRules";

export async function getRules(): Promise<SiteRules> {
  const stored = (await browser.storage.local.get(RULES_KEY))[RULES_KEY];
  return isSiteRules(stored) ? sanitizeRules(stored) : DEFAULT_RULES;
}

/** Saves and stamps `updatedAt` so the newer side wins on sync. */
export async function saveRules(rules: SiteRules): Promise<SiteRules> {
  const next = sanitizeRules({ ...rules, updatedAt: Date.now() });
  await browser.storage.local.set({ [RULES_KEY]: next });
  return next;
}

/** Saves a copy received from Desqu without changing its timestamp. */
export async function saveRulesAsReceived(rules: SiteRules): Promise<void> {
  await browser.storage.local.set({ [RULES_KEY]: sanitizeRules(rules) });
}

export function onRulesChanged(cb: (rules: SiteRules) => void): () => void {
  const listener = (changes: Record<string, { newValue?: unknown }>, area: string) => {
    if (area !== "local" || !(RULES_KEY in changes)) return;
    const next = changes[RULES_KEY]?.newValue;
    if (isSiteRules(next)) cb(sanitizeRules(next));
  };
  browser.storage.onChanged.addListener(listener);
  return () => browser.storage.onChanged.removeListener(listener);
}
