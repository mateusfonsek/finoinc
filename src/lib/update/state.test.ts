import { describe, expect, it } from "vitest";
import { downloadProgress, releaseUrl, viewFromCheck } from "./state";

describe("update state", () => {
  it("view_from_check", () => {
    expect(viewFromCheck(null, "installable")).toEqual({ kind: "up_to_date" });
    expect(viewFromCheck({ version: "0.13.0" }, "installable")).toEqual({
      kind: "available",
      version: "0.13.0",
      installable: true,
      error: null,
    });
    expect(viewFromCheck({ version: "0.13.0" }, "download_only")).toEqual({
      kind: "available",
      version: "0.13.0",
      installable: false,
      error: null,
    });
  });

  it("download_progress", () => {
    expect(downloadProgress(50, 200)).toBe(0.25);
    expect(downloadProgress(10, null)).toBeNull();
    expect(downloadProgress(10, 0)).toBeNull();
    expect(downloadProgress(300, 200)).toBe(1);
  });

  it("release_url", () => {
    expect(releaseUrl("0.13.0")).toBe(
      "https://github.com/MateusFonseK/finoinc/releases/tag/v0.13.0",
    );
  });
});
