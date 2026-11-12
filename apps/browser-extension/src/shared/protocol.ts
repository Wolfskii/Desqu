// Wire protocol to Desqu / Screenbound (through the native host). Mirrors
// apps/screenbound/module/src/bridge/protocol.rs; the fixtures in test-fixtures/protocol/ are read by
// both test suites. Change all three together.
import type { FullscreenMode, SiteRules } from "./rules";

export const PROTOCOL_VERSION = 1;

export interface Rect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export interface ZoneRef {
  id: string;
  name: string;
}

export type ErrorCode =
  | "unsupportedProtocol"
  | "badRequest"
  | "windowNotFound"
  | "zoneUnavailable"
  | "disabled"
  | "desquUnavailable";

export type ReleaseReason = "userMoved" | "windowGone" | "released" | "disabled";

/** Extension -> Desqu (via the host). Requests carry an `id`; the host itself sends `bridgeStatus`. */
export type ClientMessage =
  | { type: "hello"; id: string; protocol: number; extensionVersion: string; browser: { name: string; version: string }; rules: SiteRules }
  | { type: "setRules"; id: string; rules: SiteRules }
  | { type: "exemptNext"; id: string; tabTitle: string }
  | { type: "zoneEnter"; id: string; tabId: number; tabTitle: string; zoneId: string | null; viewport: Rect | null }
  | { type: "viewportReport"; id: string; tabId: number; viewport: Rect }
  | { type: "zoneExit"; id: string; tabId: number }
  | { type: "fullscreenNative"; tabId: number; host: string };

/** Desqu -> extension (replies carry `replyTo`), plus host-generated `bridgeStatus`. */
export type ServerMessage =
  | { type: "bridgeStatus"; connected: boolean }
  | { type: "helloAck"; replyTo: string; protocol: number; product: "desqu" | "screenbound"; appVersion: string; zones: ZoneRef[]; rules: SiteRules }
  | { type: "ack"; replyTo: string }
  | { type: "zoneApplied"; replyTo: string; ok: boolean; zoneName: string | null; error: string | null }
  | { type: "error"; replyTo: string; code: ErrorCode; message: string }
  | { type: "rulesChanged"; rules: SiteRules }
  | { type: "zonesChanged"; zones: ZoneRef[] }
  | { type: "zoneReleased"; tabId: number; reason: ReleaseReason }
  | { type: "requestViewport"; tabId: number };

export const SERVER_TYPES: ServerMessage["type"][] = [
  "bridgeStatus",
  "helloAck",
  "ack",
  "zoneApplied",
  "error",
  "rulesChanged",
  "zonesChanged",
  "zoneReleased",
  "requestViewport",
];

export function isServerMessage(value: unknown): value is ServerMessage {
  if (typeof value !== "object" || value === null) return false;
  const type = (value as { type?: unknown }).type;
  return typeof type === "string" && (SERVER_TYPES as string[]).includes(type);
}

export type { FullscreenMode };
