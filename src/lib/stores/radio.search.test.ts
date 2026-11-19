import { describe, it, expect } from "vitest";
import { mergeStationSuggestions } from "./radio.svelte";
import type { RadioDirectoryStation } from "../types";

function station(stationuuid: string, overrides: Partial<RadioDirectoryStation> = {}) {
  return {
    stationuuid,
    name: stationuuid,
    urlResolved: "",
    homepage: "",
    favicon: "",
    tags: "",
    country: "",
    countrycode: "",
    language: "",
    codec: "MP3",
    bitrate: 128,
    hls: false,
    votes: 0,
    clickcount: 0,
    lastcheckok: true,
    changeuuid: "",
    updatedAt: 0,
    playable: true,
    reducedQuality: false,
    ...overrides,
  } satisfies RadioDirectoryStation;
}

describe("mergeStationSuggestions (#2007)", () => {
  const saved = [
    station("s-folk", { name: "Folk Alley", tags: "folk,acoustic", country: "United States" }),
    station("s-jazz", { name: "Late Night", tags: "jazz", language: "french" }),
  ];

  it("matches saved stations on every query word across name, tags, country and language", () => {
    expect(mergeStationSuggestions(saved, [], "folk", 3).map((s) => s.stationuuid)).toEqual([
      "s-folk",
    ]);
    expect(mergeStationSuggestions(saved, [], "jazz fren", 3).map((s) => s.stationuuid)).toEqual([
      "s-jazz",
    ]);
    expect(mergeStationSuggestions(saved, [], "folk jazz", 3)).toEqual([]);
  });

  it("puts saved matches first, drops directory rows that are already saved, and caps the list", () => {
    const directory = [
      station("d-1", { name: "Jazz One" }),
      station("s-jazz", { name: "Late Night", tags: "jazz" }),
      station("d-2", { name: "Jazz Two" }),
      station("d-3", { name: "Jazz Three" }),
    ];
    expect(mergeStationSuggestions(saved, directory, "jazz", 3).map((s) => s.stationuuid)).toEqual([
      "s-jazz",
      "d-1",
      "d-2",
    ]);
  });

  it("returns nothing for a query with no words", () => {
    expect(mergeStationSuggestions(saved, [station("d-1")], "  ::  ", 3)).toEqual([]);
  });
});
