import { describe, it, expect, vi, beforeEach } from "vitest";

const api = vi.hoisted(() => ({
  scanWatchedFolders: vi.fn(),
  markFile: vi.fn(),
  getAppSetting: vi.fn(),
  setAppSetting: vi.fn(),
}));

vi.mock("$lib/api/watch", () => ({
  ...api,
  WATCH_ENABLED_KEY: "watch_enabled",
  WATCH_HINT_DISMISSED_KEY: "watch_hint_dismissed",
}));

const loadStatementFromPath = vi.hoisted(() => vi.fn());
vi.mock("$lib/statement/load", () => ({ loadStatementFromPath }));
const listCsvFormats = vi.hoisted(() => vi.fn(async () => []));
vi.mock("$lib/api/csvFormats", () => ({ listCsvFormats }));

import { StatementReadError } from "$lib/statement/errors";
import { createWatchStore } from "./watch.svelte";

function discovered(hash: string, name: string) {
  return {
    id: 1,
    content_hash: hash,
    path: `/tmp/${name}`,
    file_name: name,
    size: 100,
    status: "pending",
    seen_at: "2026-07-26",
  };
}

function parsed(txCount: number) {
  return {
    kind: "parsed",
    file: new File([""], "x.ofx"),
    parsed: {
      transactions: Array.from({ length: txCount }, () => ({})),
      summary: { earliest: "2026-07-01", latest: "2026-07-31" },
    },
  };
}

describe("watch store", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.getAppSetting.mockResolvedValue("1");
  });

  it("paused_store_does_not_scan", async () => {
    api.scanWatchedFolders.mockResolvedValue([]);
    const store = createWatchStore();
    store.pause();
    await store.refresh({ force: true });
    expect(api.scanWatchedFolders).not.toHaveBeenCalled();
    store.resume();
    await store.refresh({ force: true });
    expect(api.scanWatchedFolders).toHaveBeenCalledTimes(1);
  });

  it("exposes discoveries that parse, with their count and date range", async () => {
    api.scanWatchedFolders.mockResolvedValue([discovered("h1", "extrato.ofx")]);
    loadStatementFromPath.mockResolvedValue(parsed(87));

    const store = createWatchStore();
    await store.refresh();

    expect(store.pendingCount).toBe(1);
    expect(store.discoveries[0]).toMatchObject({
      hash: "h1",
      fileName: "extrato.ofx",
      txCount: 87,
      earliest: "2026-07-01",
      latest: "2026-07-31",
    });
  });

  it("a file that does not parse is marked invalid and never exposed", async () => {
    api.scanWatchedFolders.mockResolvedValue([discovered("h2", "lixo.ofx")]);
    loadStatementFromPath.mockRejectedValue(new Error("not ofx"));

    const store = createWatchStore();
    await store.refresh();

    expect(store.pendingCount).toBe(0);
    expect(api.markFile).toHaveBeenCalledWith("h2", "invalid");
  });

  // A Downloads folder is full of CSVs that are not statements. One whose
  // layout no format knows stays out of sight without being buried: once the
  // user saves a format for it, the next scan picks it up.
  it("unknown_csv_is_skipped_not_invalid", async () => {
    api.scanWatchedFolders.mockResolvedValue([discovered("h9", "planilha.csv")]);
    loadStatementFromPath.mockResolvedValue({ kind: "needs_mapping", file: new File([""], "planilha.csv"), table: {}, guess: {} });

    const store = createWatchStore();
    await store.refresh();

    expect(store.pendingCount).toBe(0);
    expect(api.markFile).not.toHaveBeenCalled();
  });

  // Re-reading every non-statement CSV in Downloads on each window focus is
  // wasted work; it only needs another look once the saved formats change.
  it("unknown_csv_not_reread_until_formats_change", async () => {
    api.scanWatchedFolders.mockResolvedValue([discovered("h11", "planilha.csv")]);
    loadStatementFromPath.mockResolvedValue({ kind: "needs_mapping", file: new File([""], "planilha.csv"), table: {}, guess: {} });

    const store = createWatchStore();
    await store.refresh({ force: true });
    await store.refresh({ force: true });
    expect(loadStatementFromPath).toHaveBeenCalledTimes(1);

    listCsvFormats.mockResolvedValueOnce([{ id: "novo", signature: "a|b" }] as never);
    await store.refresh({ force: true });
    expect(loadStatementFromPath).toHaveBeenCalledTimes(2);
  });

  it("known_csv_becomes_discovery", async () => {
    const mine = [{ id: "inter" }];
    listCsvFormats.mockResolvedValueOnce(mine as never);
    api.scanWatchedFolders.mockResolvedValue([discovered("h10", "nubank.csv")]);
    loadStatementFromPath.mockResolvedValue(parsed(4));

    const store = createWatchStore();
    await store.refresh();

    expect(loadStatementFromPath).toHaveBeenCalledWith("/tmp/nubank.csv", mine);
    expect(store.discoveries[0]).toMatchObject({ hash: "h10", txCount: 4 });
  });

  it("a read failure does not mark invalid — the file stays pending", async () => {
    // An iCloud stub evicted back to a placeholder, a moved file, permission
    // denied: `invalid` is permanent and would bury the statement forever, since
    // the key is the content hash.
    api.scanWatchedFolders.mockResolvedValue([discovered("h6", "nubank.ofx")]);
    loadStatementFromPath.mockRejectedValue(
      new StatementReadError("/tmp/nubank.ofx", new Error("No such file or directory")),
    );

    const store = createWatchStore();
    await store.refresh();

    expect(api.markFile).not.toHaveBeenCalled();
    expect(store.pendingCount).toBe(0);
  });

  it("a file that could not be read comes back on the next scan", async () => {
    api.scanWatchedFolders.mockResolvedValue([discovered("h7", "nubank.ofx")]);
    loadStatementFromPath.mockRejectedValueOnce(
      new StatementReadError("/tmp/nubank.ofx", new Error("No such file or directory")),
    );

    const store = createWatchStore();
    await store.refresh();
    expect(store.pendingCount).toBe(0);

    // iCloud finished downloading: the same discovery now reads and parses.
    loadStatementFromPath.mockResolvedValue(parsed(12));
    await store.refresh({ force: true });

    expect(store.pendingCount).toBe(1);
    expect(store.discoveries[0]).toMatchObject({ hash: "h7", txCount: 12 });
  });

  it("an open request is served exactly once", async () => {
    const store = createWatchStore();
    const discovery = {
      hash: "h8",
      path: "/tmp/a.ofx",
      fileName: "a.ofx",
      txCount: 3,
      earliest: null,
      latest: null,
    };

    store.requestOpen(discovery);

    expect(store.openRequest).toEqual(discovery);
    expect(store.takeOpenRequest()).toEqual(discovery);
    // Without this, a stale request would reopen a statement on returning to the screen.
    expect(store.takeOpenRequest()).toBeNull();
    expect(store.openRequest).toBeNull();
  });

  it("resolve removes the discovery from the list", async () => {
    api.scanWatchedFolders.mockResolvedValue([discovered("h3", "a.ofx")]);
    loadStatementFromPath.mockResolvedValue(parsed(10));

    const store = createWatchStore();
    await store.refresh();
    expect(store.pendingCount).toBe(1);

    await store.resolve("h3", "ignored");

    expect(api.markFile).toHaveBeenCalledWith("h3", "ignored");
    expect(store.pendingCount).toBe(0);
  });

  it("does not scan again inside the throttle window", async () => {
    api.scanWatchedFolders.mockResolvedValue([]);
    const store = createWatchStore();

    await store.refresh();
    await store.refresh();

    expect(api.scanWatchedFolders).toHaveBeenCalledTimes(1);
  });

  it("scans anyway when forced", async () => {
    api.scanWatchedFolders.mockResolvedValue([]);
    const store = createWatchStore();

    await store.refresh();
    await store.refresh({ force: true });

    expect(api.scanWatchedFolders).toHaveBeenCalledTimes(2);
  });

  it("does not scan while the feature is off", async () => {
    api.getAppSetting.mockResolvedValue(null);
    const store = createWatchStore();

    await store.loadEnabled();
    await store.refresh();

    expect(store.enabled).toBe(false);
    expect(api.scanWatchedFolders).not.toHaveBeenCalled();
  });

  it("enabling persists and flips the flag", async () => {
    const store = createWatchStore();

    await store.setEnabled(true);

    expect(api.setAppSetting).toHaveBeenCalledWith("watch_enabled", "1");
    expect(store.enabled).toBe(true);
  });

  it("enabling scans immediately", async () => {
    api.scanWatchedFolders.mockResolvedValue([]);
    const store = createWatchStore();

    await store.setEnabled(true);

    expect(api.scanWatchedFolders).toHaveBeenCalled();
  });

  it("disabling persists \"0\" and clears the flag", async () => {
    const store = createWatchStore();

    await store.setEnabled(false);

    expect(api.setAppSetting).toHaveBeenCalledWith("watch_enabled", "0");
    expect(store.enabled).toBe(false);
  });

  it("disabling clears the discovery list", async () => {
    api.scanWatchedFolders.mockResolvedValue([discovered("h4", "a.ofx")]);
    loadStatementFromPath.mockResolvedValue(parsed(5));
    const store = createWatchStore();

    await store.refresh();
    expect(store.pendingCount).toBe(1);

    await store.setEnabled(false);

    expect(store.pendingCount).toBe(0);
  });

  it("disabling does not scan", async () => {
    const store = createWatchStore();

    await store.setEnabled(false);

    expect(api.scanWatchedFolders).not.toHaveBeenCalled();
  });

  it("two concurrent refreshes share a single settings read", async () => {
    api.scanWatchedFolders.mockResolvedValue([]);
    const store = createWatchStore();

    // No await between the two calls — simulates App.svelte (boot) and the
    // focus listener firing at almost the same time.
    await Promise.all([store.refresh(), store.refresh()]);

    expect(api.getAppSetting).toHaveBeenCalledTimes(1);
  });

  it("resolving mid-scan does not revive the file the scan still had in its snapshot", async () => {
    let releaseScan!: (files: ReturnType<typeof discovered>[]) => void;
    const scanPromise = new Promise<ReturnType<typeof discovered>[]>((resolve) => {
      releaseScan = resolve;
    });
    api.scanWatchedFolders.mockReturnValue(scanPromise);
    loadStatementFromPath.mockResolvedValue(parsed(3));

    const store = createWatchStore();
    const refreshPromise = store.refresh();

    // Waits for the scan to actually start (scanWatchedFolders already called)
    // before resolving the file — only then does the tested scenario (resolve()
    // while a scan is in flight) really happen.
    while (api.scanWatchedFolders.mock.calls.length === 0) {
      await Promise.resolve();
    }

    await store.resolve("h5", "ignored");

    releaseScan([discovered("h5", "b.ofx")]);
    await refreshPromise;

    expect(store.discoveries.find((d) => d.hash === "h5")).toBeUndefined();
    expect(store.pendingCount).toBe(0);
  });
});
