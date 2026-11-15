import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { PLAYER_DOCK_HEIGHT_PX, PLAYER_DOCK_INSET_PX } from "./constants";

// Tailwind spacing steps are 4px, so `h-20` is 80px. Reading the real sources makes a dock
// resize that forgets constants.ts fail here instead of hiding content under the dock.
const read = (path: string) => readFileSync(resolve(__dirname, path), "utf-8");

describe("PlayerBar dock geometry", () => {
  it("keeps the constants equal to the dock's h-* and bottom-* classes", () => {
    const height = read("components/PlayerBar.svelte").match(/'h-(\d+) max-w-\[1200px\]/);
    const inset = read("../routes/+layout.svelte").match(
      /class="absolute inset-x-4 bottom-(\d+) z-40"/,
    );
    expect(Number(height?.[1]) * 4).toBe(PLAYER_DOCK_HEIGHT_PX);
    expect(Number(inset?.[1]) * 4).toBe(PLAYER_DOCK_INSET_PX);
  });
});
