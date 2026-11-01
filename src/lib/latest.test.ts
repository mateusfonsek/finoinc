import { describe, expect, it } from "vitest";
import { latestOnly } from "./latest";

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

describe("latestOnly", () => {
  // Typing "ab" fires two queries; if "a" answers last, its rows must not
  // replace the ones for "ab".
  it("drops_a_reply_that_arrives_after_a_newer_call", async () => {
    const latest = latestOnly();
    const a = deferred<string>();
    const ab = deferred<string>();
    const first = latest(() => a.promise);
    const second = latest(() => ab.promise);
    ab.resolve("ab");
    a.resolve("a");
    expect(await second).toEqual({ current: true, value: "ab" });
    expect(await first).toEqual({ current: false });
  });

  it("passes_the_only_call_through", async () => {
    const latest = latestOnly();
    expect(await latest(async () => 42)).toEqual({ current: true, value: 42 });
  });
});
