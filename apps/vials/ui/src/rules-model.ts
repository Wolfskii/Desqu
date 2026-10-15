// Automatic rules as the rules window edits them: a list of conditions that all (or any) have to match.
// Pure, so the window can show what a rule would catch before it is saved.

import type { ItemInfo, ItemKind, ItemMatcher, SortRule } from "./types";
import { KIND_LABELS } from "./types";

export type CondKind = "contains" | "startsWith" | "endsWith" | "pattern" | "extension" | "kind" | "advanced";

export interface Condition {
    kind: CondKind;
    /** The text for the text kinds, the extensions (comma-separated) or the pattern. */
    text: string;
    /** For kind "kind". */
    itemKind: ItemKind;
    /** A condition this window cannot show (a combination nested inside one condition): kept as it is. */
    raw?: ItemMatcher;
}

export interface RuleDraft {
    /** null for a rule that is not saved yet. */
    id: string | null;
    name: string;
    enabled: boolean;
    targetVial: string;
    mode: "all" | "any";
    conditions: Condition[];
}

export const CONDITION_LABELS: Record<Exclude<CondKind, "advanced">, string> = {
    contains: "name contains",
    startsWith: "name starts with",
    endsWith: "name ends with",
    pattern: "name matches pattern",
    extension: "has the extension",
    kind: "is a",
};

/** The kinds of condition the window offers, in menu order. */
export const CONDITION_KINDS: Exclude<CondKind, "advanced">[] = ["contains", "startsWith", "endsWith", "pattern", "extension", "kind"];

export const CONDITION_HINTS:Record<Exclude<CondKind, "advanced" | "kind">, string> = {
    contains: "Android",
    startsWith: "Report",
    endsWith: "Studio",
    pattern: "*.txt or Report*",
    extension: "pdf, docx",
};

export const KINDS = Object.keys(KIND_LABELS) as ItemKind[];

export function emptyCondition(kind: CondKind = "contains"): Condition {
    return { kind, text: "", itemKind: "shortcut" };
}

function extensionsOf(text: string): string[] {
    const list = text.split(/[\s,;]+/).map((x) => x.replace(/^\./, "").toLowerCase()).filter(Boolean);
    return [...new Set(list)];
}

/** A condition as one matcher, or null while it is not filled in. */
export function conditionMatcher(c: Condition): ItemMatcher | null {
    if (c.kind === "advanced") return c.raw ?? null;
    if (c.kind === "kind") return { type: "kind", value: c.itemKind };
    const text = c.text.trim();
    if (!text) return null;
    switch (c.kind) {
        case "contains":
            return { type: "nameContains", value: text };
        case "startsWith":
            return { type: "nameStartsWith", value: text };
        case "endsWith":
            return { type: "nameEndsWith", value: text };
        case "pattern":
            return { type: "nameGlob", value: text };
        case "extension": {
            const leaves: ItemMatcher[] = extensionsOf(text).map((value) => ({ type: "extension", value }));
            if (leaves.length === 0) return null;
            return leaves.length === 1 ? leaves[0] : { type: "any", value: leaves };
        }
    }
}

/** The draft as the rule's matcher, or null while a condition is empty. */
export function draftMatcher(d: RuleDraft): ItemMatcher | null {
    if (d.conditions.length === 0) return null;
    const parts = d.conditions.map(conditionMatcher);
    if (parts.some((p) => p === null)) return null;
    const list = parts as ItemMatcher[];
    return list.length === 1 ? list[0] : { type: d.mode, value: list };
}

function conditionFrom(m: ItemMatcher): Condition {
    const keep: Condition = { kind: "advanced", text: "", itemKind: "shortcut", raw: m };
    switch (m.type) {
        case "nameContains":
            return { kind: "contains", text: m.value, itemKind: "shortcut" };
        case "nameStartsWith":
            return { kind: "startsWith", text: m.value, itemKind: "shortcut" };
        case "nameEndsWith":
            return { kind: "endsWith", text: m.value, itemKind: "shortcut" };
        case "nameGlob":
            return { kind: "pattern", text: m.value, itemKind: "shortcut" };
        case "extension":
            return { kind: "extension", text: m.value, itemKind: "shortcut" };
        case "kind":
            return { kind: "kind", text: "", itemKind: m.value };
        case "any":
            if (m.value.every((x) => x.type === "extension")) {
                const exts = m.value.map((x) => (x.type === "extension" ? x.value : ""));
                return { kind: "extension", text: exts.join(", "), itemKind: "shortcut" };
            }
            return keep;
        default:
            return keep;
    }
}

/** The window's view of a saved rule. */
export function draftFrom(rule: SortRule): RuleDraft {
    const m = rule.matcher;
    const parts = m.type === "all" || m.type === "any" ? m.value : [m];
    return {
        id: rule.id,
        name: rule.name,
        enabled: rule.enabled,
        targetVial: rule.targetVial,
        mode: m.type === "any" ? "any" : "all",
        conditions: parts.map(conditionFrom),
    };
}

/** A name for a rule that was not given one, from its conditions. */
export function describe(d: RuleDraft): string {
    const parts = d.conditions.map((c) => {
        if (c.kind === "advanced") return "advanced condition";
        if (c.kind === "kind") return `is a ${KIND_LABELS[c.itemKind].toLowerCase()}`;
        return `${CONDITION_LABELS[c.kind]} "${c.text.trim()}"`;
    });
    return parts.join(d.mode === "any" ? " or " : " and ") || "New rule";
}

function extensionOf(id: string): string {
    const dot = id.lastIndexOf(".");
    const slash = Math.max(id.lastIndexOf("\\"), id.lastIndexOf("/"));
    return dot > slash ? id.slice(dot + 1).toLowerCase() : "";
}

/** `*` and `?` wildcards, case-insensitive, as the matcher in the module reads them. */
function globToRegExp(pattern: string): RegExp {
    const body = pattern
        .split("")
        .map((c) => (c === "*" ? ".*" : c === "?" ? "." : c.replace(/[.+^${}()|[\]\\]/g, "\\$&")))
        .join("");
    return new RegExp(`^${body}$`, "i");
}

/** Whether a rule matcher catches a desktop item. Mirrors the module's matching. */
export function matches(m: ItemMatcher, item: ItemInfo): boolean {
    const name = item.name.toLowerCase();
    switch (m.type) {
        case "extension":
            return extensionOf(item.id) === m.value.toLowerCase().replace(/^\./, "");
        case "kind":
            return item.kind === m.value;
        case "nameGlob":
            return m.value !== "" && globToRegExp(m.value).test(item.name);
        case "nameContains":
            return m.value.trim() !== "" && name.includes(m.value.trim().toLowerCase());
        case "nameStartsWith":
            return m.value.trim() !== "" && name.startsWith(m.value.trim().toLowerCase());
        case "nameEndsWith":
            return m.value.trim() !== "" && name.endsWith(m.value.trim().toLowerCase());
        case "any":
            return m.value.some((x) => matches(x, item));
        case "all":
            return m.value.length > 0 && m.value.every((x) => matches(x, item));
        default:
            return false;
    }
}
