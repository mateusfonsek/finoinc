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

describe("dialog height", () => {
  // A dialog centred in a `fixed inset-0 grid place-items-center` layer sits
  // in an auto-height grid row. A percentage max-height (`max-h-full`,
  // `min(78vh,100%)`) resolves against that row, which Chromium — WebView2 on
  // Windows — grows to the dialog's whole content: the panel is capped but
  // centred in a row taller than the window, so it lands below the screen.
  // Heights must come from the viewport instead.
  it("centred_dialogs_cap_height_by_the_viewport", () => {
    const offenders = svelteFiles("src").filter((f) => {
      const src = readFileSync(f, "utf8");
      if (!/fixed inset-0[^"]*grid[^"]*place-items-center/.test(src)) return false;
      return /max-h-full\b|max-h-\[[^\]]*100%[^\]]*\]/.test(src);
    });
    expect(offenders).toEqual([]);
  });
});
