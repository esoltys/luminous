import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { setVolumeFromSlider, volumeSliderBackground } from "./volumeSlider";
import { playerStore } from "./stores/player.svelte";

const safety = (enabled: boolean, ceiling = 0.7) => ({ enabled, ceiling, min: 0.2, max: 0.95 });

describe("volumeSliderBackground", () => {
  it("fills up to the volume with no notch while the limit is off", () => {
    const style = volumeSliderBackground(0.5, safety(false));
    expect(style).toContain("var(--color-accent) 50%");
    expect(style).not.toContain("calc(");
    expect(volumeSliderBackground(0.5, null)).toBe(style);
  });

  it("marks the ceiling with a notch and dims the range above it", () => {
    const style = volumeSliderBackground(0.4, safety(true, 0.7));
    expect(style).toContain("var(--color-accent) 40%");
    expect(style).toContain("var(--color-text-secondary) calc(70% - 1px)");
    expect(style).toMatch(/color-mix\(.*\) calc\(70% \+ 1px\), color-mix\(.*\) 100%\)$/);
  });
});

describe("setVolumeFromSlider", () => {
  beforeEach(() => vi.mocked(invoke).mockClear());

  it("snaps the thumb back to the level the backend applied", async () => {
    playerStore.volume = 0.7;
    vi.mocked(invoke).mockImplementationOnce(async () => 0.7);
    const input = document.createElement("input");
    input.type = "range";
    input.min = "0";
    input.max = "1";
    input.step = "0.01";
    input.value = "0.9";

    await setVolumeFromSlider(input);

    expect(invoke).toHaveBeenCalledWith("set_volume", { volume: 0.9 });
    expect(playerStore.volume).toBe(0.7);
    expect(input.value).toBe("0.7");
  });

  it("leaves the thumb alone when the level was applied as asked", async () => {
    const input = document.createElement("input");
    input.type = "range";
    input.min = "0";
    input.max = "1";
    input.step = "0.01";
    input.value = "0.4";
    // The pointer has moved on by the time the backend replies.
    vi.mocked(invoke).mockImplementationOnce(async () => {
      input.value = "0.45";
      return 0.4;
    });

    await setVolumeFromSlider(input);

    expect(input.value).toBe("0.45");
  });
});
