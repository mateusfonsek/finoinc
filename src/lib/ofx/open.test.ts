import { describe, it, expect, vi, beforeEach } from "vitest";

const push = vi.hoisted(() => vi.fn());
vi.mock("svelte-spa-router", () => ({ push }));
const loadStatementFromPath = vi.hoisted(() => vi.fn());
vi.mock("$lib/statement/load", () => ({ loadStatementFromPath }));
const listCsvFormats = vi.hoisted(() => vi.fn(async () => []));
vi.mock("$lib/api/csvFormats", () => ({ listCsvFormats }));

import { openStatementPath, stash, takeStashed, STASHED_EVENT } from "./open";

const loaded = (name: string) => ({ kind: "parsed", file: new File([], name), parsed: { name } });

describe("openStatementPath", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    while (takeStashed()) {}
  });

  it("queues every file opened at once, in order", async () => {
    loadStatementFromPath.mockImplementation(async (path: string) => loaded(path));
    await openStatementPath("/a.ofx");
    await openStatementPath("/b.ofx");
    expect(takeStashed()).toMatchObject({ file: { name: "/a.ofx" } });
    expect(takeStashed()).toMatchObject({ file: { name: "/b.ofx" } });
    expect(takeStashed()).toBeUndefined();
  });

  // Import may already be on screen, where navigating to it remounts nothing.
  it("tells a mounted Import screen that something arrived", async () => {
    loadStatementFromPath.mockResolvedValue(loaded("/a.ofx"));
    const heard = vi.fn();
    window.addEventListener(STASHED_EVENT, heard);
    await openStatementPath("/a.ofx");
    window.removeEventListener(STASHED_EVENT, heard);
    expect(heard).toHaveBeenCalledOnce();
    expect(push).toHaveBeenCalledWith("/import");
  });

  it("hands Import the error of a file that could not be read", async () => {
    loadStatementFromPath.mockRejectedValue(new Error("not an OFX"));
    await openStatementPath("/broken.ofx");
    expect(takeStashed()).toEqual({ error: "not an OFX" });
  });

  it("passes_saved_formats_to_the_loader", async () => {
    const mine = [{ id: "inter" }];
    listCsvFormats.mockResolvedValueOnce(mine as never);
    loadStatementFromPath.mockResolvedValue(loaded("/a.csv"));
    await openStatementPath("/a.csv");
    expect(loadStatementFromPath).toHaveBeenCalledWith("/a.csv", mine);
  });

  // Onboarding hands its dropped file over the same queue.
  it("stash_queues_and_navigates", () => {
    const item = loaded("/c.csv") as never;
    stash(item);
    expect(takeStashed()).toBe(item);
    expect(push).toHaveBeenCalledWith("/import");
  });
});
