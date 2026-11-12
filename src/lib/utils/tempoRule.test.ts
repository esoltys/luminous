import { describe, it, expect } from "vitest";
import { TEMPO_BUCKET_OPTIONS, normalizeTempoToken } from "./tempoRule";
import { parseSearchRules } from "./filterParser";
import { en } from "../locales/en";

describe("tempoRule", () => {
  it("lists the six buckets slowest first", () => {
    expect(TEMPO_BUCKET_OPTIONS.map((o) => o.token)).toEqual([
      "slow",
      "down-tempo",
      "mid-tempo",
      "uptempo",
      "high-energy",
      "extreme",
    ]);
  });

  it("labels every bucket from an existing playlists locale string", () => {
    for (const { labelKey } of TEMPO_BUCKET_OPTIONS) {
      const key = labelKey.replace("playlists.", "") as keyof typeof en.playlists;
      expect(en.playlists[key], labelKey).toBeTruthy();
    }
  });

  it("normalizes a stored value to a known token, defaulting to the first bucket", () => {
    expect(normalizeTempoToken(" Down-Tempo ")).toBe("down-tempo");
    expect(normalizeTempoToken("")).toBe("slow");
    expect(normalizeTempoToken("blistering")).toBe("slow");
  });

  it("round-trips a serialised tempo rule back into the builder", () => {
    // The builder serialises `=` / `!=` rules as `field:<op>"value"`.
    expect(parseSearchRules('tempo:="down-tempo"')).toEqual([
      { field: "tempo", op: "=", value: "down-tempo" },
    ]);
    expect(parseSearchRules('tempo:!="extreme"')).toEqual([
      { field: "tempo", op: "!=", value: "extreme" },
    ]);
  });
});
