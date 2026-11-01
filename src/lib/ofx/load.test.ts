import { describe, it, expect, vi, beforeEach } from "vitest";

const readFileBytes = vi.hoisted(() => vi.fn());
vi.mock("$lib/api/files", () => ({ readFileBytes }));

import { StatementReadError } from "$lib/statement/errors";
import { loadOfxFromPath } from "./load";

describe("loadOfxFromPath", () => {
  beforeEach(() => vi.clearAllMocks());

  it("a disk failure is an StatementReadError", async () => {
    readFileBytes.mockRejectedValue(new Error("No such file or directory (os error 2)"));

    await expect(loadOfxFromPath("/tmp/sumiu.ofx")).rejects.toBeInstanceOf(StatementReadError);
  });

  it("a content failure is NOT an StatementReadError", async () => {
    // Whatever decides to mark a file `invalid` (permanent) relies on exactly
    // this distinction: if parse threw StatementReadError, junk would stay pending
    // forever; if a read threw a plain error, a good statement would be buried.
    readFileBytes.mockResolvedValue(new TextEncoder().encode("<OFX>"));

    await expect(loadOfxFromPath("/tmp/lixo.ofx")).rejects.toThrow();
    await expect(loadOfxFromPath("/tmp/lixo.ofx")).rejects.not.toBeInstanceOf(StatementReadError);
  });
});
