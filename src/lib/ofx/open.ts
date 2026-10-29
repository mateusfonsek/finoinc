import { push } from "svelte-spa-router";
import { loadOfxFromPath } from "./load";
import type { ParsedOfx } from "./types";

/** What Import reads on mount. `watchHash` only exists when the file came from
 *  a watched folder — it is what allows marking it imported at the end. */
export type PendingImport = { file: File; parsed: ParsedOfx; watchHash?: string };

/** A statement opened from Finder, or why it could not be read. */
export type Stashed = PendingImport | { error: string };

/** Fired when something is queued, for an Import screen already on display:
 *  navigating to the route it is on remounts nothing. */
export const STASHED_EVENT = "finoinc:stashed";

// Finder can hand over several files at once; each waits its turn.
const queue: Stashed[] = [];

export function takeStashed(): Stashed | undefined {
  return queue.shift();
}

/** Loads an .ofx opened with Finoinc from Finder, queues it and navigates to
 *  Import. */
export async function openOfxPath(path: string): Promise<void> {
  try {
    queue.push(await loadOfxFromPath(path));
  } catch (e) {
    queue.push({ error: e instanceof Error ? e.message : String(e) });
  }
  window.dispatchEvent(new Event(STASHED_EVENT));
  push("/import");
}
