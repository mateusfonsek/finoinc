import { push } from "svelte-spa-router";
import { listCsvFormats } from "$lib/api/csvFormats";
import { loadStatementFromPath, type LoadResult } from "$lib/statement/load";

/** What Import reads on mount. `watchHash` only exists when the file came from
 *  a watched folder — it is what allows marking it imported at the end. */
export type PendingImport = LoadResult & { watchHash?: string };

/** A statement opened from the file manager, or why it could not be read. */
export type Stashed = PendingImport | { error: string };

/** Fired when something is queued, for an Import screen already on display:
 *  navigating to the route it is on remounts nothing. */
export const STASHED_EVENT = "finoinc:stashed";

// The file manager can hand over several files at once; each waits its turn.
const queue: Stashed[] = [];

export function takeStashed(): Stashed | undefined {
  return queue.shift();
}

/** Queues a statement for Import and navigates there. */
export function stash(item: Stashed): void {
  queue.push(item);
  window.dispatchEvent(new Event(STASHED_EVENT));
  push("/import");
}

/** Loads a statement opened with Finoinc from the file manager, queues it and
 *  navigates to Import. */
export async function openStatementPath(path: string): Promise<void> {
  try {
    stash(await loadStatementFromPath(path, await listCsvFormats()));
  } catch (e) {
    stash({ error: e instanceof Error ? e.message : String(e) });
  }
}
