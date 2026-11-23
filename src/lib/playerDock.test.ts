import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  PLAYER_DOCK_HEIGHT_PX,
  PLAYER_DOCK_INSET_PX,
  PLAYER_DOCK_MAX_WIDTH_PX,
  doesPlayerDockCoverLeft,
} from "./constants";

// Tailwind spacing steps are 4px, so `h-20` is 80px. Reading the real sources makes a dock
// resize that forgets constants.ts fail here instead of hiding content under the dock.
const read = (path: string) => readFileSync(resolve(__dirname, path), "utf-8");

describe("PlayerBar dock geometry", () => {
  it("keeps the constants equal to the dock's h-*, max-w-*, and bottom-* classes", () => {
    const match = read("components/PlayerBar.svelte").match(/'h-(\d+) max-w-\[(\d+)px\]/);
    const inset = read("../routes/+layout.svelte").match(
      /class="absolute inset-x-4 bottom-(\d+) z-40"/,
    );
    expect(Number(match?.[1]) * 4).toBe(PLAYER_DOCK_HEIGHT_PX);
    expect(Number(match?.[2])).toBe(PLAYER_DOCK_MAX_WIDTH_PX);
    expect(Number(inset?.[1]) * 4).toBe(PLAYER_DOCK_INSET_PX);
  });

  it("has pb-dock and bottom-dock read the one --dock-content-inset variable", () => {
    const css = read("../app.css");
    expect(css).toMatch(/@utility pb-dock\s*\{\s*padding-bottom:\s*var\(--dock-content-inset\)/);
    expect(css).toMatch(/@utility bottom-dock\s*\{\s*bottom:\s*var\(--dock-content-inset\)/);
    expect(css).toMatch(/:root\s*\{\s*--dock-content-inset:/);
  });

  it("detects when the centered dock overlaps a left-anchored panel", () => {
    // Narrower viewport: centered 1200px dock starts at (1400 - 1200) / 2 = 100px < 256px
    expect(doesPlayerDockCoverLeft(1400, 256)).toBe(true);
    // Clamped to inset-x-4 (16px) when viewport is narrower than 1200 + 32
    expect(doesPlayerDockCoverLeft(1000, 64)).toBe(true);
    // Wider viewport: dock starts at (1920 - 1200) / 2 = 360px >= 256px
    expect(doesPlayerDockCoverLeft(1920, 256)).toBe(false);
    // Exact boundary: playbarLeft === leftWidth (256px at 1712px viewport) does not overlap
    expect(doesPlayerDockCoverLeft(1712, 256)).toBe(false);
  });
});
