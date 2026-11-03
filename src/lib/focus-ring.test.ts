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

describe("focus ring", () => {
  // A text field always matches `:focus-visible`, even on a mouse click, so
  // the global ring lands on the bare <input> too. A control that draws its
  // own ring around the field (`focus-within:`) must switch the inner one off,
  // or two rings show — the rectangle plus the capsule in the search box.
  it("controls_with_their_own_ring_turn_off_the_inner_one", () => {
    const offenders = svelteFiles("src").filter((f) => {
      const src = readFileSync(f, "utf8");
      return src.includes("focus-within:shadow-[var(--focus-ring)]") && !src.includes('data-focus-ring="none"');
    });
    expect(offenders).toEqual([]);
  });
});
