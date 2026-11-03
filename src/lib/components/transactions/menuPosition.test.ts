import { describe, expect, it } from "vitest";
import { menuPosition } from "./menuPosition";

const rect = (top: number, left: number, h = 24, w = 100) => ({ top, left, bottom: top + h, right: left + w });
const VIEW = { width: 1180, height: 760 };

describe("menuPosition", () => {
  it("opens_below_when_it_fits", () => {
    expect(menuPosition(rect(100, 400), VIEW, { width: 240, height: 288 })).toEqual({ top: 130, left: 400 });
  });

  // The last rows of a short table: no room below, plenty above.
  it("opens_above_near_the_bottom", () => {
    // Anchored by its bottom edge, so a short menu still sits against the trigger.
    expect(menuPosition(rect(700, 400), VIEW, { width: 240, height: 288 })).toEqual({ bottom: 66, left: 400 });
  });

  it("stays_inside_the_window_horizontally", () => {
    expect(menuPosition(rect(100, 1100), VIEW, { width: 240, height: 288 }).left).toBe(1180 - 240 - 8);
  });

  // Neither side fits: it goes where there is more room.
  it("picks_the_roomier_side_when_neither_fits", () => {
    expect(menuPosition(rect(200, 400), { width: 1180, height: 300 }, { width: 240, height: 288 })).toEqual({ bottom: 106, left: 400 });
  });
});
