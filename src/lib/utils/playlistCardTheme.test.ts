import { describe, it, expect } from "vitest";
import { getPlaylistCardTheme, type PlaylistCardThemeKind } from "./playlistCardTheme";

const KINDS: PlaylistCardThemeKind[] = [
  "queue", "genre", "decade", "smart", "bpm", "artist_tag", "favourites", "recently_added",
  "most_played", "history", "missing_metadata", "missing_musicbrainz", "daypart",
];

describe("playlistCardTheme", () => {
  it("gives every kind an icon, tint and colours", () => {
    for (const kind of KINDS) {
      const t = getPlaylistCardTheme(kind);
      expect(t.icon, kind).toBeTruthy();
      expect(t.tintClass, kind).toContain("bg-");
      expect(t.iconColorClass, kind).toContain("text-");
    }
  });

  it("keeps every kind's icon colour distinct, except the intentionally shared Tag kinds' hues", () => {
    const colours = KINDS.map((k) => getPlaylistCardTheme(k).iconColorClass);
    expect(new Set(colours).size).toBe(colours.length);
  });

  it("uses the Tag icon for genre and the brand indigo for Moment Mix", () => {
    expect(getPlaylistCardTheme("genre").icon).toBe(getPlaylistCardTheme("artist_tag").icon);
    expect(getPlaylistCardTheme("daypart").iconColorClass).toBe("text-[#4F5BD5]");
  });
});
