// What the popup and options show about Desqu. Written by the background, read by the UI.
import { browser } from "wxt/browser";
import type { HostState } from "./bridge-state";
import type { ZoneRef } from "./protocol";

export const STATUS_KEY = "desquStatus";
export const ZONES_KEY = "desquZones";

export interface DesquStatus {
  state: HostState;
  product?: "desqu" | "screenbound";
  appVersion?: string;
}

export async function getStatus(): Promise<DesquStatus> {
  const stored = (await browser.storage.local.get(STATUS_KEY))[STATUS_KEY] as DesquStatus | undefined;
  return stored ?? { state: "unknown" };
}

export async function setStatus(status: DesquStatus): Promise<void> {
  await browser.storage.local.set({ [STATUS_KEY]: status });
}

export async function getZones(): Promise<ZoneRef[]> {
  const stored = (await browser.storage.local.get(ZONES_KEY))[ZONES_KEY] as ZoneRef[] | undefined;
  return stored ?? [];
}

export async function setZones(zones: ZoneRef[]): Promise<void> {
  await browser.storage.local.set({ [ZONES_KEY]: zones });
}

export function onStatusChanged(cb: (status: DesquStatus) => void): () => void {
  const listener = (changes: Record<string, { newValue?: unknown }>, area: string) => {
    if (area === "local" && STATUS_KEY in changes) cb(changes[STATUS_KEY]?.newValue as DesquStatus);
  };
  browser.storage.onChanged.addListener(listener);
  return () => browser.storage.onChanged.removeListener(listener);
}
