import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  checkForUpdate: vi.fn(),
  installKind: vi.fn(),
  prepareForUpdate: vi.fn(),
  relaunchApp: vi.fn(),
  readAutoCheck: vi.fn(),
  writeAutoCheck: vi.fn(),
}));
vi.mock("$lib/api/update", () => ({ ...api, AUTO_CHECK_KEY: "update_auto_check" }));

const watch = vi.hoisted(() => ({ pause: vi.fn(), resume: vi.fn() }));
vi.mock("$lib/stores/watch.svelte", () => ({ watch }));

import { createUpdateStore } from "./update.svelte";

const DAY = 24 * 3600 * 1000;

function fakeUpdate(version: string, over: Partial<{ download: unknown; install: unknown }> = {}) {
  return {
    version,
    download: vi.fn().mockResolvedValue(undefined),
    install: vi.fn().mockResolvedValue(undefined),
    ...over,
  };
}

/** Lets pending promise chains settle under fake timers. */
async function flush() {
  for (let i = 0; i < 5; i++) await Promise.resolve();
}

describe("update store", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    api.installKind.mockResolvedValue("installable");
    api.readAutoCheck.mockResolvedValue(false);
    api.writeAutoCheck.mockResolvedValue(undefined);
    api.prepareForUpdate.mockResolvedValue(undefined);
    api.relaunchApp.mockResolvedValue(undefined);
  });
  afterEach(() => vi.useRealTimers());

  it("check_reports_available_version", async () => {
    api.checkForUpdate.mockResolvedValue(fakeUpdate("0.13.0"));
    const store = createUpdateStore();
    await store.check();
    expect(store.view).toEqual({
      kind: "available",
      version: "0.13.0",
      installable: true,
      error: null,
    });
  });

  it("check_failure_shows_error_and_allows_retry", async () => {
    api.checkForUpdate.mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce(null);
    const store = createUpdateStore();
    await store.check();
    expect(store.view.kind).toBe("error");
    await store.check();
    expect(store.view.kind).toBe("up_to_date");
  });

  it("concurrent_checks_share_one_request", async () => {
    let resolve!: (v: null) => void;
    api.checkForUpdate.mockReturnValue(new Promise((r) => (resolve = r)));
    const store = createUpdateStore();
    const a = store.check();
    const b = store.check();
    resolve(null);
    await Promise.all([a, b]);
    expect(api.checkForUpdate).toHaveBeenCalledTimes(1);
  });

  it("auto_check_off_by_default_makes_no_request", async () => {
    const store = createUpdateStore();
    await store.start();
    vi.advanceTimersByTime(2 * DAY);
    await flush();
    expect(api.checkForUpdate).not.toHaveBeenCalled();
    expect(store.autoCheck).toBe(false);
  });

  it("auto_check_on_checks_at_start_and_every_day", async () => {
    api.readAutoCheck.mockResolvedValue(true);
    api.checkForUpdate.mockResolvedValue(null);
    const store = createUpdateStore();
    await store.start();
    await flush();
    expect(api.checkForUpdate).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(DAY);
    await flush();
    expect(api.checkForUpdate).toHaveBeenCalledTimes(2);
  });

  it("disabling_auto_check_clears_the_timer", async () => {
    api.readAutoCheck.mockResolvedValue(true);
    api.checkForUpdate.mockResolvedValue(null);
    const store = createUpdateStore();
    await store.start();
    await flush();
    await store.setAutoCheck(false);
    vi.advanceTimersByTime(2 * DAY);
    await flush();
    expect(api.checkForUpdate).toHaveBeenCalledTimes(1);
    expect(api.writeAutoCheck).toHaveBeenCalledWith(false);
    expect(store.autoCheck).toBe(false);
  });

  it("auto_found_update_is_quiet", async () => {
    api.readAutoCheck.mockResolvedValue(true);
    api.checkForUpdate.mockResolvedValue(fakeUpdate("0.13.0"));
    const store = createUpdateStore();
    await store.start();
    await flush();
    expect(store.found).toBe("0.13.0");
    store.dismissFound();
    expect(store.found).toBeNull();

    const manual = createUpdateStore();
    await manual.check();
    expect(manual.found).toBeNull();
  });

  it("install_failure_returns_to_available_with_error", async () => {
    const u = fakeUpdate("0.13.0", {
      download: vi.fn().mockRejectedValue(new Error("signature mismatch")),
    });
    api.checkForUpdate.mockResolvedValue(u);
    const store = createUpdateStore();
    await store.check();
    await store.install();
    expect(store.view).toMatchObject({ kind: "available", version: "0.13.0" });
    expect(store.view.kind === "available" && store.view.error).toContain("signature mismatch");
    expect(api.relaunchApp).not.toHaveBeenCalled();
    // The download failed before anything was stopped.
    expect(api.prepareForUpdate).not.toHaveBeenCalled();
  });

  it("install_stops_background_work_first", async () => {
    const order: string[] = [];
    const u = fakeUpdate("0.13.0", {
      install: vi.fn(async () => {
        order.push("install");
      }),
    });
    api.prepareForUpdate.mockImplementation(async () => {
      order.push("prepare");
    });
    watch.pause.mockImplementation(() => order.push("pause"));
    api.relaunchApp.mockImplementation(async () => {
      order.push("relaunch");
    });
    api.checkForUpdate.mockResolvedValue(u);
    const store = createUpdateStore();
    await store.check();
    await store.install();
    expect(order).toEqual(["prepare", "pause", "install", "relaunch"]);
  });

  it("download_only_install_never_downloads", async () => {
    api.installKind.mockResolvedValue("download_only");
    const u = fakeUpdate("0.13.0");
    api.checkForUpdate.mockResolvedValue(u);
    const store = createUpdateStore();
    await store.check();
    await store.install();
    expect(u.download).not.toHaveBeenCalled();
  });
});
