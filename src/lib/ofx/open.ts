import { push } from "svelte-spa-router";
import { loadOfxFromPath } from "./load";
import type { ParsedOfx } from "./types";

/** What Import reads on mount. `watchHash` only exists when the file came from
 *  a watched folder — it is what allows marking it imported at the end. */
export type PendingImport = { file: File; parsed: ParsedOfx; watchHash?: string };

type StashWindow = Window & { __finanPending?: PendingImport };

export function stashPending(pending: PendingImport): void {
  (window as unknown as StashWindow).__finanPending = pending;
}

export function takeStashed(): PendingImport | undefined {
  const w = window as unknown as StashWindow;
  const stash = w.__finanPending;
  w.__finanPending = undefined;
  return stash;
}

/** Loads an .ofx by path, stashes it and navigates to Import. Used by both
 *  "Open with Finoinc" and the watched folder. */
export async function openOfxPath(path: string, watchHash?: string): Promise<void> {
  const loaded = await loadOfxFromPath(path);
  stashPending({ ...loaded, watchHash });
  push("/import");
}
