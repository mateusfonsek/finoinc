import { describe, it, expect, vi, beforeEach } from "vitest";

const push = vi.hoisted(() => vi.fn());
vi.mock("svelte-spa-router", () => ({ push }));
const loadOfxFromPath = vi.hoisted(() => vi.fn());
vi.mock("./load", () => ({ loadOfxFromPath }));

import { openOfxPath, takeStashed, STASHED_EVENT } from "./open";

const loaded = (name: string) => ({ file: new File([], name), parsed: { name } });

describe("openOfxPath", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    while (takeStashed()) {}
  });

  it("queues every file opened at once, in order", async () => {
    loadOfxFromPath.mockImplementation(async (path: string) => loaded(path));
    await openOfxPath("/a.ofx");
    await openOfxPath("/b.ofx");
    expect(takeStashed()).toMatchObject({ file: { name: "/a.ofx" } });
    expect(takeStashed()).toMatchObject({ file: { name: "/b.ofx" } });
    expect(takeStashed()).toBeUndefined();
  });

  // Import may already be on screen, where navigating to it remounts nothing.
  it("tells a mounted Import screen that something arrived", async () => {
    loadOfxFromPath.mockResolvedValue(loaded("/a.ofx"));
    const heard = vi.fn();
    window.addEventListener(STASHED_EVENT, heard);
    await openOfxPath("/a.ofx");
    window.removeEventListener(STASHED_EVENT, heard);
    expect(heard).toHaveBeenCalledOnce();
    expect(push).toHaveBeenCalledWith("/import");
  });

  it("hands Import the error of a file that could not be read", async () => {
    loadOfxFromPath.mockRejectedValue(new Error("not an OFX"));
    await openOfxPath("/broken.ofx");
    expect(takeStashed()).toEqual({ error: "not an OFX" });
  });
});
