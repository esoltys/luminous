import { describe, it, expect } from "vitest";
import { toTitleCase, formatDuration, formatFileSize, formatSampleRate, formatBitDepth, formatChannels } from "./formatters";

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
