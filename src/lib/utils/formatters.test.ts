import { describe, it, expect, beforeEach } from "vitest";
import {
  toTitleCase,
  formatDuration,
  formatFileSize,
  formatSampleRate,
  formatBitDepth,
  formatChannels,
  formatWindowTitle,
} from "./formatters";
import { i18n } from "../stores/i18n.svelte";

describe("toTitleCase", () => {
  it("capitalizes single-word lowercase tags", () => {
    expect(toTitleCase("canadian")).toBe("Canadian");
  });

  it("capitalizes multi-word tags separated by spaces", () => {
    expect(toTitleCase("folk metal")).toBe("Folk Metal");
    expect(toTitleCase("female vocalists")).toBe("Female Vocalists");
  });

  it("capitalizes hyphenated tags", () => {
    expect(toTitleCase("prog-rock")).toBe("Prog-Rock");
    expect(toTitleCase("singer-songwriter")).toBe("Singer-Songwriter");
  });

  it("handles empty or falsy strings", () => {
    expect(toTitleCase("")).toBe("");
  });
});

describe("formatters", () => {
  it("formats channels", () => {
    expect(formatChannels(1)).toBe("Mono");
    expect(formatChannels(2)).toBe("Stereo");
    expect(formatChannels(6)).toBe("6 ch");
  });

  it("formats duration", () => {
    expect(formatDuration(65_000_000_000)).toBe("1:05");
  });
});

describe("formatWindowTitle", () => {
  beforeEach(() => {
    i18n.currentLocale = "en";
  });

  it("returns 'Luminous' when stopped or paused, even with a song", () => {
    const song = { title: "Anti-Hero", artist: "Taylor Swift" };
    expect(formatWindowTitle(song, "stopped")).toBe("Luminous");
    expect(formatWindowTitle(song, "paused")).toBe("Luminous");
  });

  it("returns 'Luminous' when song is missing, even if state is playing", () => {
    expect(formatWindowTitle(null, "playing")).toBe("Luminous");
    expect(formatWindowTitle(undefined, "playing")).toBe("Luminous");
  });

  it("formats title and artist when both are present and playing", () => {
    const song = { title: "Starboy", artist: "The Weeknd" };
    expect(formatWindowTitle(song, "playing")).toBe("Starboy - The Weeknd - Luminous");
  });

  it("formats title only when artist is absent or whitespace", () => {
    expect(formatWindowTitle({ title: "Prelude", artist: null }, "playing")).toBe("Prelude - Luminous");
    expect(formatWindowTitle({ title: "Prelude", artist: "" }, "playing")).toBe("Prelude - Luminous");
    expect(formatWindowTitle({ title: "Prelude", artist: "   " }, "playing")).toBe("Prelude - Luminous");
  });

  it("falls back to localized Unknown Song when title is absent or whitespace", () => {
    expect(formatWindowTitle({ title: "", artist: "Queen" }, "playing")).toBe("Unknown Song - Queen - Luminous");
    expect(formatWindowTitle({ title: "   ", artist: null }, "playing")).toBe("Unknown Song - Luminous");
  });

  it("respects French locale for fallback title", () => {
    i18n.currentLocale = "fr";
    expect(formatWindowTitle({ title: "", artist: "Daft Punk" }, "playing")).toBe("Chanson inconnue - Daft Punk - Luminous");
    expect(formatWindowTitle({ title: "", artist: "" }, "playing")).toBe("Chanson inconnue - Luminous");
  });
});

