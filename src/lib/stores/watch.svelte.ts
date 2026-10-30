import {
  getAppSetting,
  markFile,
  scanWatchedFolders,
  setAppSetting,
  WATCH_ENABLED_KEY,
  type FileStatus,
} from "$lib/api/watch";
import { OfxReadError } from "$lib/ofx/errors";
import { loadOfxFromPath } from "$lib/ofx/load";

/** A validated discovery: only what parsed as OFX gets here. */
export type Discovery = {
  hash: string;
  path: string;
  fileName: string;
  txCount: number;
  earliest: string | null;
  latest: string | null;
};

/** Scanning on window focus must not become an I/O burst when the user is
 *  switching between apps. */
const THROTTLE_MS = 10_000;

export function createWatchStore() {
  let enabled = $state(false);
  let discoveries = $state<Discovery[]>([]);
  /** The toast never interrupts an import already in progress. */
  let suppressToast = $state(false);
  let lastScan = 0;
  let scanning = false;
  /** Set while an update installs: the installer replaces the app, and a scan
   *  writing file marks at that moment would be cut mid-way. */
  let paused = false;
  /** Callers are not required to call `loadEnabled()` before the first
   *  `refresh()`. Loading on demand here keeps the `enabled` gate from running
   *  against a value that was never read — assuming enabled until the read
   *  lands would risk scanning disk before knowing the user wants it.
   */
  let settingsLoaded = false;
  /** In-flight promise for the flag read. Boot and the focus listener can call
   *  `refresh()` almost simultaneously; without sharing, each would fire its own
   *  `getAppSetting`. Harmless (the read is idempotent) but double the I/O for
   *  nothing.
   */
  let loadingSettings: Promise<void> | null = null;
  /** Hashes resolved (toast/import) while a scan is in flight. The scan works
   *  from a snapshot taken at the start; without this, its final assignment
   *  would revive a discovery the user resolved mid-scan. */
  const resolvedDuringScan = new Set<string>();

  async function refresh(opts: { force?: boolean } = {}) {
    if (!settingsLoaded) await loadEnabled();
    if (!enabled || paused) return;
    const now = Date.now();
    if (!opts.force && now - lastScan < THROTTLE_MS) return;
    if (scanning) return;

    scanning = true;
    lastScan = now;
    resolvedDuringScan.clear();
    try {
      const files = await scanWatchedFolders();
      const next: Discovery[] = [];
      for (const f of files) {
        try {
          const { parsed } = await loadOfxFromPath(f.path);
          next.push({
            hash: f.content_hash,
            path: f.path,
            fileName: f.file_name,
            txCount: parsed.transactions.length,
            earliest: parsed.summary.earliest ?? null,
            latest: parsed.summary.latest ?? null,
          });
        } catch (e) {
          await noteLoadFailure(f.content_hash, e);
        }
      }
      // Drop from the snapshot any hash the toast/import already resolved
      // enquanto o scan estava em voo — ver `resolvedDuringScan` acima.
      discoveries = next.filter((d) => !resolvedDuringScan.has(d.hash));
    } finally {
      scanning = false;
    }
  }

  async function loadEnabled() {
    if (loadingSettings) return loadingSettings;
    loadingSettings = (async () => {
      try {
        enabled = (await getAppSetting(WATCH_ENABLED_KEY)) === "1";
        settingsLoaded = true;
      } finally {
        loadingSettings = null;
      }
    })();
    return loadingSettings;
  }

  async function setEnabled(value: boolean) {
    await setAppSetting(WATCH_ENABLED_KEY, value ? "1" : "0");
    enabled = value;
    settingsLoaded = true;
    if (!value) discoveries = [];
    else await refresh({ force: true });
  }

  async function resolve(hash: string, status: FileStatus) {
    await markFile(hash, status);
    discoveries = discoveries.filter((d) => d.hash !== hash);
    // A scan in flight has a snapshot that predates this resolution — record it
    // so the scan does not revive the item when it finishes.
    if (scanning) resolvedDuringScan.add(hash);
  }

  /** Removes the discovery from the list **without** touching the DB: the file
   *  stays `pending` and the next scan retries. The opposite of `resolve`,
   *  which is permanent. */
  function skip(hash: string) {
    discoveries = discoveries.filter((d) => d.hash !== hash);
    if (scanning) resolvedDuringScan.add(hash);
  }

  /** Single policy for "could not open this discovery".
   *
   *  Read failed → transient (iCloud stub evicted after the scan, file moved,
   *  permissions): stays `pending` and returns on the next scan.
   *  Parse failed → the content is not OFX and never will be: `invalid`, which
   *  is permanent, and the user never sees an error that is ours. */
  async function noteLoadFailure(hash: string, e: unknown) {
    if (e instanceof OfxReadError) skip(hash);
    else await resolve(hash, "invalid");
  }

  /** Request to open this discovery in Import, coming from the toast.
   *
   *  A signal rather than direct navigation because `push("/import")` while
   *  already on `/import` fires no `hashchange` — Import never remounts and
   *  `onMount` never reruns. The Import screen watches this signal instead, so
   *  it works whether freshly mounted or already on screen. */
  let openRequest = $state<Discovery | null>(null);

  function requestOpen(discovery: Discovery) {
    openRequest = discovery;
  }

  /** Consumes the request; it can only be served once, otherwise a stale
   *  request would reopen a statement on the next visit. */
  function takeOpenRequest(): Discovery | null {
    const req = openRequest;
    openRequest = null;
    return req;
  }

  return {
    get enabled() {
      return enabled;
    },
    get discoveries() {
      return discoveries;
    },
    get pendingCount() {
      return discoveries.length;
    },
    get suppressToast() {
      return suppressToast;
    },
    set suppressToast(v: boolean) {
      suppressToast = v;
    },
    get openRequest() {
      return openRequest;
    },
    refresh,
    loadEnabled,
    setEnabled,
    resolve,
    skip,
    noteLoadFailure,
    requestOpen,
    takeOpenRequest,
    pause() {
      paused = true;
    },
    resume() {
      paused = false;
    },
  };
}

export const watch = createWatchStore();
