import { describe, it, expect } from "vitest";
import { parseLrc } from "./lrc";

describe("parseLrc", () => {
  it("parses standard single-timestamp LRC lines", () => {
    const lrc = [
      "[00:05.50] First line",
      "[00:10.00] Second line",
      "[01:00.25] Third line",
    ].join("\n");

    const result = parseLrc(lrc);
    expect(result.fileOffsetMs).toBe(0);
    expect(result.lines).toHaveLength(3);
    expect(result.lines[0]).toEqual({ timeMs: 5500, text: "First line" });
    expect(result.lines[1]).toEqual({ timeMs: 10000, text: "Second line" });
    expect(result.lines[2]).toEqual({ timeMs: 60250, text: "Third line" });
  });

  it("parses multiple timestamps per line and sorts by time", () => {
    const lrc = "[00:10.00][00:25.00] Repeated chorus line";
    const result = parseLrc(lrc);
    expect(result.lines).toHaveLength(2);
    expect(result.lines[0]).toEqual({ timeMs: 10000, text: "Repeated chorus line" });
    expect(result.lines[1]).toEqual({ timeMs: 25000, text: "Repeated chorus line" });
  });

  it("parses Enhanced LRC word-by-word timing tags and strips literal tags from line text", () => {
    const lrc = "[00:12.00]<00:12.00>Hello <00:12.50>world <00:13.00>how <00:13.50>are <00:14.00>you";
    const result = parseLrc(lrc);
    expect(result.lines).toHaveLength(1);
    const line = result.lines[0];
    expect(line.timeMs).toBe(12000);
    expect(line.text).toBe("Hello world how are you");
    expect(line.words).toBeDefined();
    expect(line.words).toEqual([
      { timeMs: 12000, text: "Hello " },
      { timeMs: 12500, text: "world " },
      { timeMs: 13000, text: "how " },
      { timeMs: 13500, text: "are " },
      { timeMs: 14000, text: "you" },
    ]);
  });

  it("handles word tags when the first word does not have an explicit leading tag", () => {
    const lrc = "[00:10.00]Start <00:10.80>finish";
    const result = parseLrc(lrc);
    expect(result.lines).toHaveLength(1);
    const line = result.lines[0];
    expect(line.text).toBe("Start finish");
    expect(line.words).toEqual([
      { timeMs: 10000, text: "Start " },
      { timeMs: 10800, text: "finish" },
    ]);
  });

  it("applies file [offset:±ms] header (positive makes lyrics appear earlier)", () => {
    const lrc = [
      "[offset:500]",
      "[00:10.00] Lyrics line",
    ].join("\n");

    const result = parseLrc(lrc);
    expect(result.fileOffsetMs).toBe(500);
    // 10000ms - 500ms = 9500ms
    expect(result.lines[0].timeMs).toBe(9500);
  });

  it("applies negative file [offset:±ms] header (negative makes lyrics appear later)", () => {
    const lrc = [
      "[offset:-300]",
      "[00:10.00] Lyrics line",
    ].join("\n");

    const result = parseLrc(lrc);
    expect(result.fileOffsetMs).toBe(-300);
    // 10000ms - (-300ms) = 10300ms
    expect(result.lines[0].timeMs).toBe(10300);
  });

  it("applies user timing offset on top of file offset", () => {
    const lrc = [
      "[offset:200]",
      "[00:10.00]<00:10.00>Word1 <00:11.00>Word2",
    ].join("\n");

    // File offset = +200ms, user offset = +500ms => total offset = +700ms
    const result = parseLrc(lrc, 500);
    expect(result.fileOffsetMs).toBe(200);
    expect(result.lines[0].timeMs).toBe(9300); // 10000 - 700
    expect(result.lines[0].words).toEqual([
      { timeMs: 9300, text: "Word1 " },
      { timeMs: 10300, text: "Word2" },
    ]);
  });

  it("segments unbroken Chinese, Japanese, and Thai word cues using Intl.Segmenter", () => {
    const lrc = "[00:05.00]<00:05.00>こんにちは世界<00:09.00>";
    const result = parseLrc(lrc);
    expect(result.lines).toHaveLength(1);
    const line = result.lines[0];
    expect(line.text).toBe("こんにちは世界");
    expect(line.words).toBeDefined();
    expect(line.words!.length).toBeGreaterThan(1);
    expect(line.words!.map((w) => w.text)).toEqual(["こんにちは", "世界"]);
    expect(line.words![0].timeMs).toBe(5000);
    expect(line.words![1].timeMs).toBeGreaterThan(5000);
  });

  it("handles empty strings and plain text gracefully", () => {
    expect(parseLrc("")).toEqual({ lines: [], fileOffsetMs: 0 });
    expect(parseLrc("[synced:false]\nJust plain text")).toEqual({ lines: [], fileOffsetMs: 0 });
    expect(parseLrc("No timestamps here at all")).toEqual({ lines: [], fileOffsetMs: 0 });
  });
});
