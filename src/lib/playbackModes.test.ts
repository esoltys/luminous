import { describe, it, expect } from "vitest";
import {
  SHUFFLE_MODES,
  REPEAT_MODES,
  nextShuffleMode,
  nextRepeatMode,
  shuffleModeLabel,
  shuffleModeDescription,
  shuffleTypeIcon,
  repeatModeLabel,
  repeatModeDescription,
  repeatTypeIcon,
} from "./playbackModes";
import type { RepeatMode, ShuffleMode } from "./types";

describe("playbackModes", () => {
  it("cycles through all shuffle modes in order and wraps around", () => {
    const expected: ShuffleMode[] = ["off", "all", "inside_album", "albums", "artists"];
    expect([...SHUFFLE_MODES]).toEqual(expected);

    let mode: ShuffleMode = "off";
    const visited: ShuffleMode[] = [];
    for (let i = 0; i < expected.length; i++) {
      visited.push(mode);
      mode = nextShuffleMode(mode);
    }
    expect(visited).toEqual(expected);
    expect(mode).toBe("off");
  });

  it("cycles through all UI repeat modes in order and wraps around", () => {
    const expected: RepeatMode[] = ["off", "track", "album", "playlist"];
    expect([...REPEAT_MODES]).toEqual(expected);

    let mode: RepeatMode = "off";
    const visited: RepeatMode[] = [];
    for (let i = 0; i < expected.length; i++) {
      visited.push(mode);
      mode = nextRepeatMode(mode);
    }
    expect(visited).toEqual(expected);
    expect(mode).toBe("off");
  });

  it("handles unlisted RepeatMode ('intro') gracefully", () => {
    expect(nextRepeatMode("intro")).toBe("off");
    expect(repeatModeLabel("intro")).toBe("intro");
    expect(repeatModeDescription("intro")).toBe("");
    expect(repeatTypeIcon("intro")).toBeNull();
  });

  it("provides a translated label, description, and expected icon for every ShuffleMode", () => {
    const allModes: ShuffleMode[] = ["off", "all", "inside_album", "albums", "artists"];

    for (const mode of allModes) {
      const label = shuffleModeLabel(mode);
      const desc = shuffleModeDescription(mode);
      expect(label).toBeTruthy();
      expect(label).not.toMatch(/^playerBar\./);
      expect(desc).toBeTruthy();
      expect(desc).not.toMatch(/^playerBar\./);

      const icon = shuffleTypeIcon(mode);
      if (mode === "off" || mode === "all") {
        expect(icon).toBeNull();
      } else {
        expect(icon).not.toBeNull();
      }
    }
  });

  it("provides a translated label, description, and expected icon for every cycleable RepeatMode", () => {
    const allModes: RepeatMode[] = ["off", "track", "album", "playlist"];

    for (const mode of allModes) {
      const label = repeatModeLabel(mode);
      const desc = repeatModeDescription(mode);
      expect(label).toBeTruthy();
      expect(label).not.toMatch(/^playerBar\./);
      expect(desc).toBeTruthy();
      expect(desc).not.toMatch(/^playerBar\./);

      const icon = repeatTypeIcon(mode);
      if (mode === "off") {
        expect(icon).toBeNull();
      } else {
        expect(icon).not.toBeNull();
      }
    }
  });
});
