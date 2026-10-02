import { describe, it, expect } from "vitest";
import { en } from "./en";
import { fr } from "./fr";

/**
 * Recursively flattens a nested object into dotted key paths.
 * E.g. { sidebar: { home: "Home" } } -> { "sidebar.home": "Home" }
 */
function flatten(obj: Record<string, any>, prefix = ""): Record<string, string> {
  const result: Record<string, string> = {};
  for (const [key, value] of Object.entries(obj)) {
    const fullKey = prefix ? `${prefix}.${key}` : key;
    if (value !== null && typeof value === "object" && !Array.isArray(value)) {
      Object.assign(result, flatten(value, fullKey));
    } else {
      result[fullKey] = String(value);
    }
  }
  return result;
}

/**
 * Extracts sorted list of placeholder tokens from a string.
 * E.g. "Showing {count} songs on {date}" -> ["{count}", "{date}"]
 */
function extractPlaceholders(str: string): string[] {
  const matches = str.match(/\{(\w+)\}/g) || [];
  return [...matches].sort();
}

/**
 * Allowlist of keys whose French translation is legitimately identical to English
 * (e.g. loanwords, shared musical terminology, technical acronyms, brand names, or symbols).
 */
const IDENTICAL_OK = new Set([
  "albumDetail.statsLine", // "{genre} · {year} · {duration}"
  "albumTagEditor.genreField", // "Genre"
  "artistDetail.albumsFilter", // "Albums ({count})"
  "auth.collectionsCount", // "Collections"
  "artistDetail.epsFilter", // "EPs ({count})"
  "artistDetail.singlesFilter", // "Singles ({count})"
  "artistDetail.tags", // "Tags"
  "audioPipeline.bitPerfect", // "Bit-perfect"
  "audioPipeline.codec", // "Codec"
  "audioPipeline.normalizationGain", // "{gain} dB ({source})"
  "audioPipeline.outputFormat", // "Format"
  "collection.albums", // "Albums ({count})"
  "collection.albumsCount", // "{count} albums"
  "collection.columnActions", // "Actions"
  "collection.columnAlbum", // "Album"
  "collection.columnBpm", // "BPM"
  "collection.columnFormat", // "Format"
  "collection.columnGenre", // "Genre"
  "collection.oneAlbum", // "1 album"
  "collection.tableHeaderActions", // "Actions"
  "collection.tableHeaderAlbum", // "Album"
  "collection.tableHeaderBpm", // "BPM"
  "collection.tableHeaderFormat", // "Format"
  "collection.tableHeaderGenre", // "Genre"
  "collection.tableHeaderMusicBrainzId", // "MBID"
  "collection.tableHeaderTrack", // "#"
  "discord.integrationTitle", // "Discord Rich Presence"
  "equalizer.gain", // "Gain"
  "equalizer.isoStandard", // "ISO 266:1997"
  "equalizer.jazzPreset", // "Jazz"
  "equalizer.modeLabel", // "Mode"
  "equalizer.popPreset", // "Pop"
  "equalizer.qFactor", // "Q"
  "equalizer.rockPreset", // "Rock"
  "loudness.modeAlbum", // "Album"
  "lyrics.instrumentalBadge", // "Instrumental"
  "organizer.placeholders", // "Variables"
  "organizer.presetAlternative", // "Alternative"
  "picard.customPathPlaceholder", // "C:\\Program Files\\MusicBrainz Picard\\picard.exe"
  "picard.customPathPlaceholderLinux", // "/var/lib/flatpak/exports/bin/org.musicbrainz.Picard"
  "picard.integrationTitle", // "MusicBrainz Picard"
  "playerBar.albumLabel", // "Album"
  "playerBar.channelsMono", // "Mono"
  "playerBar.critiquebrainzSectionLabel", // "CritiqueBrainz"
  "playerBar.dynamicRangeRms", // "RMS {value} dB"
  "playerBar.formatLabel", // "Format"
  "playerBar.genreLabel", // "Genre"
  "playerBar.listenbrainzAlbumLabel", // "Album"
  "playerBar.listenbrainzSectionLabel", // "ListenBrainz"
  "playerBar.mbRatingVotes", // "({count} votes)"
  "playerBar.musicbrainzReleaseTypeLabel", // "Type"
  "playerBar.musicbrainzSectionLabel", // "MusicBrainz"
  "playerBar.pause", // "Pause"
  "playerBar.repeatAlbum", // "Album"
  "playerBar.shuffleAlbums", // "Albums"
  "playerBar.volume", // "Volume"
  "playlists.activeBadgeLabel", // "Active"
  "playlists.bpmAutoPlaylist", // "BPM"
  "playlists.genreAutoPlaylist", // "Genre"
  "playlists.playlistTypeLabel", // "Playlist"
  "playlists.tableHeaderTrack", // "#"
  "settings.badgeColorCyan", // "Cyan"
  "settings.badgeColorIndigo", // "Indigo"
  "settings.badgeColorOrange", // "Orange"
  "settings.badgeIconArchive", // "Archive"
  "settings.badgeIconCloud", // "Cloud / NAS"
  "settings.badgeIconUsb", // "USB"
  "settings.formatMsix", // "Microsoft Store"
  "settings.languageEnglish", // "English"
  "settings.languageFrench", // "Français"
  "settings.simple", // "Simple"
  "settings.statsAlbums", // "Albums"
  "settings.tabSources", // "Sources"
  "settings.updateBuildLabel", // "build {hash}"
  "settings.webdavUrlPlaceholder", // "https://cloud.example.com/remote.php/webdav"
  "shortcuts.groupNavigation", // "Navigation"
  "sidebar.albums", // "Albums"
  "sidebar.collection", // "Collection"
  "sidebar.genres", // "Genres"
  "smartPlaylistBuilder.fieldAlbum", // "Album"
  "smartPlaylistBuilder.fieldBpm", // "BPM"
  "smartPlaylistBuilder.fieldCompilation", // "Compilation"
  "smartPlaylistBuilder.fieldGenre", // "Genre"
  "smartPlaylistBuilder.mixWord", // "Mix"
  "smartPlaylistBuilder.opEquals", // "="
  "smartPlaylistBuilder.opGt", // ">"
  "smartPlaylistBuilder.opGte", // ">="
  "smartPlaylistBuilder.opLt", // "<"
  "smartPlaylistBuilder.opLte", // "<="
  "smartPlaylistBuilder.opNotEquals", // "!="
  "songTags.viewGenre", // "Genre"
  "songTags.viewTags", // "Tags"
  "stats.heatmapStatus", // "{date} — {minutes} min"
  "stats.minuteCount", // "{count} min"
  "stats.minuteUnderOne", // "< 1 min"
  "tagEditor.albumField", // "Album"
  "tagEditor.bpmField", // "BPM"
  "tagEditor.genreField", // "Genre"
  "themes.dynamic-artwork", // "✨ Luminous"
  "themes.sabrina", // "Sabrina"
  "topNav.searchSuggestions", // "Suggestions"
]);

describe("Locale translation completeness and integrity", () => {
  const flatEn = flatten(en);
  const flatFr = flatten(fr);

  it("every key in en.ts has a corresponding translation in fr.ts", () => {
    const missingInFr = Object.keys(flatEn).filter((key) => !(key in flatFr));
    expect(
      missingInFr,
      `Missing French translations in src/lib/locales/fr.ts for the following keys:\n${missingInFr.map((k) => `  - ${k}`).join("\n")}`
    ).toEqual([]);
  });

  it("fr.ts contains no stale keys that do not exist in en.ts", () => {
    const staleInFr = Object.keys(flatFr).filter((key) => !(key in flatEn));
    expect(
      staleInFr,
      `Stale keys found in src/lib/locales/fr.ts that no longer exist in en.ts:\n${staleInFr.map((k) => `  - ${k}`).join("\n")}`
    ).toEqual([]);
  });

  it("fr.ts values differ from en.ts unless explicitly allowlisted in IDENTICAL_OK", () => {
    const untranslated = Object.keys(flatEn).filter((key) => {
      return key in flatFr && flatEn[key] === flatFr[key] && !IDENTICAL_OK.has(key);
    });

    expect(
      untranslated,
      `The following French translations are identical to English. If this is legitimately the same word in French (or a symbol/brand name), add the key to IDENTICAL_OK in src/lib/locales/locales.test.ts:\n${untranslated
        .map((k) => `  - ${k}: "${flatEn[k]}"`)
        .join("\n")}`
    ).toEqual([]);
  });

  it("every key in IDENTICAL_OK is still identical and exists in en.ts", () => {
    const unneededInAllowlist = [...IDENTICAL_OK].filter((key) => {
      return !(key in flatEn) || !(key in flatFr) || flatEn[key] !== flatFr[key];
    });

    expect(
      unneededInAllowlist,
      `The following keys in IDENTICAL_OK no longer match or no longer exist; remove them from IDENTICAL_OK in src/lib/locales/locales.test.ts:\n${unneededInAllowlist
        .map((k) => `  - ${k}`)
        .join("\n")}`
    ).toEqual([]);
  });

  it("interpolation placeholder tokens match between en.ts and fr.ts", () => {
    const mismatches: { key: string; enTokens: string[]; frTokens: string[] }[] = [];

    for (const key of Object.keys(flatEn)) {
      if (key in flatFr) {
        const enTokens = extractPlaceholders(flatEn[key]);
        const frTokens = extractPlaceholders(flatFr[key]);
        if (enTokens.join(",") !== frTokens.join(",")) {
          mismatches.push({ key, enTokens, frTokens });
        }
      }
    }

    expect(
      mismatches,
      `Interpolation placeholders mismatch between en.ts and fr.ts:\n${mismatches
        .map((m) => `  - ${m.key}: en has [${m.enTokens.join(", ")}], fr has [${m.frTokens.join(", ")}]`)
        .join("\n")}`
    ).toEqual([]);
  });
});

describe("Locale validation helper functions", () => {
  it("flatten flattens nested objects into dotted key paths", () => {
    const input = {
      section: {
        title: "Hello",
        nested: {
          deep: "World"
        }
      }
    };
    expect(flatten(input)).toEqual({
      "section.title": "Hello",
      "section.nested.deep": "World"
    });
  });

  it("extractPlaceholders extracts sorted placeholder names", () => {
    expect(extractPlaceholders("Hello {name}, you have {count} messages from {name}!")).toEqual([
      "{count}",
      "{name}",
      "{name}"
    ]);
    expect(extractPlaceholders("Plain string without variables")).toEqual([]);
  });
});
