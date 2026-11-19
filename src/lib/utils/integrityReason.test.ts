import { describe, it, expect } from "vitest";
import { describeIntegrityIssue } from "./integrityReason";

describe("describeIntegrityIssue", () => {
  it("maps FLAC MD5 mismatch to a playable non-technical label and description", () => {
    const res = describeIntegrityIssue("FLAC MD5 checksum mismatch", true);
    expect(res.label).toBe("Checksum mismatch");
    expect(res.description).toContain("plays fine");
  });

  it("distinguishes warning vs corrupt truncation", () => {
    const warn = describeIntegrityIssue("Truncated FLAC audio stream: premature end-of-file", true);
    expect(warn.label).toBe("Minor ending truncation");
    expect(warn.description).toContain("intact");

    const corrupt = describeIntegrityIssue(
      "Truncated FLAC audio stream: premature end-of-file",
      false,
    );
    expect(corrupt.label).toBe("Incomplete file");
    expect(corrupt.description).toContain("cuts off mid-song");
  });

  it("maps container, empty, zero-frame, corrupt-frame, and decode errors", () => {
    expect(describeIntegrityIssue("Empty audio file (0 bytes)", false).label).toBe("Empty file");
    expect(describeIntegrityIssue("Container or format probe failed: bad", false).label).toBe(
      "Unrecognized format",
    );
    expect(describeIntegrityIssue("Zero audio frames decoded from stream", false).label).toBe(
      "No playable audio",
    );
    expect(describeIntegrityIssue("Corrupt FLAC audio frame: bad crc", false).label).toBe(
      "Damaged audio data",
    );
    expect(describeIntegrityIssue("Audio stream packet error: io", false).label).toBe(
      "Unreadable audio stream",
    );
  });

  it("falls back to raw error_detail for unrecognized messages", () => {
    expect(describeIntegrityIssue("Custom error", false)).toEqual({
      label: "Custom error",
      description: "",
    });
  });
});
