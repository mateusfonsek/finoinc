import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

function svelteFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return svelteFiles(path);
    return path.endsWith(".svelte") ? [path] : [];
  });
}

describe("per-OS copy", () => {
  // The app runs on Windows and Linux too: a macOS folder written into a
  // screen is wrong on two of the three systems. Paths come from the backend.
  it("no_screen_hardcodes_a_macos_path", () => {
    const offenders = svelteFiles("src").filter((f) =>
      readFileSync(f, "utf8").includes("Library/Application Support"),
    );
    expect(offenders).toEqual([]);
  });
});
