/**
 * Updates: whether a newer Finoinc exists and installing it in place.
 *
 * A store, not component state, because two surfaces read it — the Updates
 * section in Settings and the activity center item an automatic check leaves
 * behind — and the 24 h timer must outlive any screen.
 */
import {
  checkForUpdate,
  installKind,
  prepareForUpdate,
  readAutoCheck,
  relaunchApp,
  writeAutoCheck,
  type DownloadEvent,
  type Update,
} from "$lib/api/update";
import { watch } from "$lib/stores/watch.svelte";
import { downloadProgress, viewFromCheck, type UpdateView } from "$lib/update/state";

const DAY_MS = 24 * 60 * 60 * 1000;

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export function createUpdateStore() {
  let view = $state<UpdateView>({ kind: "idle" });
  let autoCheck = $state(false);
  /** A version an AUTOMATIC check found and the user has not dismissed — the
   *  activity center shows it. A manual check needs no reminder: the user is
   *  looking at the answer. */
  let found = $state<string | null>(null);
  let pending: Update | null = null;
  let inFlight: Promise<void> | null = null;
  let timer: ReturnType<typeof setInterval> | undefined;

  function run(automatic: boolean): Promise<void> {
    // A second click while a request is out joins it instead of sending
    // another; and nothing re-checks while an update is downloading.
    if (inFlight) return inFlight;
    if (view.kind === "downloading") return Promise.resolve();
    inFlight = (async () => {
      // A background check never takes a known update off the screen: it
      // runs out of sight, and failing overnight offline must not replace
      // "version N is available" with "couldn't check".
      const known = automatic && view.kind === "available" ? view : null;
      if (!known) view = { kind: "checking" };
      try {
        const [update, kind] = await Promise.all([checkForUpdate(), installKind()]);
        pending = update;
        view = viewFromCheck(update, kind);
        if (!update) found = null;
        else if (automatic) found = update.version;
      } catch (e) {
        view = known ?? { kind: "error", message: message(e) };
      } finally {
        inFlight = null;
      }
    })();
    return inFlight;
  }

  function arm() {
    clearInterval(timer);
    timer = setInterval(() => void run(true), DAY_MS);
  }

  function disarm() {
    clearInterval(timer);
    timer = undefined;
  }

  async function install() {
    if (view.kind !== "available" || !view.installable || !pending) return;
    const version = view.version;
    const update = pending;
    view = { kind: "downloading", version, progress: null };
    let total: number | null = null;
    let done = 0;
    try {
      // Download (and signature check) first, while everything keeps running:
      // a failed download must leave the app exactly as it was.
      await update.download((event: DownloadEvent) => {
        if (event.event === "Started") total = event.data.contentLength ?? null;
        else if (event.event === "Progress") done += event.data.chunkLength;
        view = { kind: "downloading", version, progress: downloadProgress(done, total) };
      });
    } catch (e) {
      view = { kind: "available", version, installable: true, error: message(e) };
      return;
    }
    try {
      // The installer replaces the app: stop what writes in the background.
      await prepareForUpdate();
      watch.pause();
      await update.install();
      await relaunchApp();
    } catch (e) {
      // The MCP server stays off until the next launch; folder scans resume.
      watch.resume();
      view = { kind: "available", version, installable: true, error: message(e) };
    }
  }

  return {
    get view() {
      return view;
    },
    get autoCheck() {
      return autoCheck;
    },
    get found() {
      return found;
    },
    check: () => run(false),
    install,
    /** Called once the app is past the language gate. Off by default, so on
     *  most installs this reads one setting and does nothing else. */
    async start() {
      autoCheck = await readAutoCheck().catch(() => false);
      if (!autoCheck) return;
      arm();
      void run(true);
    },
    async setAutoCheck(on: boolean) {
      await writeAutoCheck(on);
      autoCheck = on;
      if (on) {
        arm();
        // The user is in Settings looking at the answer: no reminder needed.
        void run(false);
      } else {
        disarm();
      }
    },
    dismissFound() {
      found = null;
    },
  };
}

export const update = createUpdateStore();
