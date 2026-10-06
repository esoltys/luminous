import { describe, it, expect } from "vitest";
import { en } from "./en";
import { es } from "./es";
import { fr } from "./fr";
import { it as itMessages } from "./it";
import { BASE_LOCALE, LOCALES, catalogChain, isLocale, legacyLanguageToLocale, localeLabel, localePickerGroups, manualLanguageForLocale } from "./index";

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
const IDENTICAL_OK_FR = new Set([
  "equalizer.importPlaceholder", // Equalizer APO sample lines, same syntax in every language
  "listenbrainz.critiquebrainzUserPlaceholder", // a URL
  "albumDetail.statsLine", // "{genre} · {year} · {duration}"
  "albumTagEditor.genreField", // "Genre"
  "artistDetail.albumsFilter", // "Albums ({count})"
  "artistDetail.epsFilter", // "EPs ({count})"
  "artistEvents.concert", // "Concert"
  "artistEvents.festival", // "Festival"
  "audioPipeline.bitPerfect", // "Bit-perfect"
  "audioPipeline.codec", // "Codec"
  "audioPipeline.normalizationGain", // "{gain} dB ({source})"
  "audioPipeline.outputFormat", // "Format"
  "auth.scrobbling", // "Scrobbling"
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
  "playlists.tableHeaderTrack", // "#"
  "settings.badgeColorCyan", // "Cyan"
  "settings.badgeColorIndigo", // "Indigo"
  "settings.badgeColorOrange", // "Orange"
  "settings.badgeIconArchive", // "Archive"
  "settings.badgeIconUsb", // "USB"
  "settings.formatMsix", // "Microsoft Store"
  "settings.simple", // "Simple"
  "settings.statsAlbums", // "Albums"
  "settings.tabSources", // "Sources"
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
  "smartPlaylistBuilder.fieldBpm", // "BPM"
  "smartPlaylistBuilder.opEquals", // "="
  "smartPlaylistBuilder.opGt", // ">"
  "smartPlaylistBuilder.opGte", // ">="
  "smartPlaylistBuilder.opLt", // "<"
  "smartPlaylistBuilder.opLte", // "<="
  "smartPlaylistBuilder.opNotEquals", // "!="
  "songTags.viewGenre", // "Genre"
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

const IDENTICAL_OK_IT = new Set<string>([
  "sidebar.home", // "Home"
  "collection.tableHeaderTrack", // "#"
  "collection.tableHeaderAlbum", // "Album"
  "collection.oneAlbum", // "1 album"
  "collection.albumPlaylistName", // "Album: {name}"
  "collection.columnBitrate", // "Bitrate"
  "collection.columnBpm", // "BPM"
  "collection.booleanNo", // "No"
  "collection.columnAlbum", // "Album"
  "collection.tableHeaderBitrate", // "Bitrate"
  "collection.tableHeaderBpm", // "BPM"
  "collection.tableHeaderMusicBrainzId", // "MBID"
  "settings.aboutAppName", // "Luminous Music Player"
  "settings.formatMsix", // "Microsoft Store"
  "settings.updateBuildLabel", // "build {hash}"
  "settings.badgeIconCloud", // "Cloud / NAS"
  "settings.badgeIconComputer", // "Computer"
  "settings.badgeIconUsb", // "USB"
  "settings.webdavUrlPlaceholder", // "https://cloud.example.com/remote.php/webdav"
  "settings.subsonicUrlPlaceholder", // "https://music.example.com"
  "settings.subsonicPassword", // "Password"
  "settings.subsonicPasswordPlaceholder", // "Password"
  "settings.subsonicAuthPassword", // "Password (legacy)"
  "playlists.tableHeaderTrack", // "#"
  "playlists.populationModeTitleFormat", // "{base} {suffix}"
  "playlists.bpmAutoPlaylist", // "BPM"
  "playlists.playlistTypeLabel", // "Playlist"
  "stats.minuteCount", // "{count} min"
  "stats.minuteUnderOne", // "< 1 min"
  "stats.heatmapStatus", // "{date} — {minutes} min"
  "playerBar.repeatAlbum", // "Album"
  "playerBar.repeatPlaylist", // "Playlist"
  "playerBar.volume", // "Volume"
  "playerBar.volumeWithValue", // "Volume: {value}%"
  "playerBar.albumLabel", // "Album"
  "playerBar.bitrateLabel", // "Bitrate"
  "playerBar.channelsMono", // "Mono"
  "playerBar.channelsStereo", // "Stereo"
  "playerBar.dynamicRangeRms", // "RMS {value} dB"
  "playerBar.musicbrainzSectionLabel", // "MusicBrainz"
  "playerBar.wikipediaSectionLabel", // "Wikipedia"
  "playerBar.critiquebrainzSectionLabel", // "CritiqueBrainz"
  "playerBar.listenbrainzSectionLabel", // "ListenBrainz"
  "playerBar.listenbrainzAlbumLabel", // "Album"
  "miniplayer.title", // "Miniplayer"
  "tagEditor.albumField", // "Album"
  "tagEditor.bpmField", // "BPM"
  "equalizer.presetLabel", // "Preset"
  "equalizer.popPreset", // "Pop"
  "equalizer.rockPreset", // "Rock"
  "equalizer.jazzPreset", // "Jazz"
  "equalizer.importPlaceholder", // "Preamp: -6.2 dB\nFilter 1: ON LSC Fc 105 Hz Gain 5.5 dB Q 0
  "equalizer.qFactor", // "Q"
  "equalizer.kindPeak", // "Peak"
  "equalizer.kindLowShelf", // "Low shelf"
  "equalizer.kindHighShelf", // "High shelf"
  "equalizer.isoStandard", // "ISO 266:1997"
  "loudness.modeAlbum", // "Album"
  "themes.dynamic-artwork", // "✨ Luminous"
  "themes.sabrina", // "Sabrina"
  "artistEvents.festival", // "Festival"
  "artistEvents.tour", // "Tour"
  "markdownEditor.linkText", // "link"
  "albumDetail.statsLine", // "{genre} · {year} · {duration}"
  "picard.integrationTitle", // "MusicBrainz Picard"
  "picard.customPathPlaceholder", // "C:\\Program Files\\MusicBrainz Picard\\picard.exe"
  "picard.customPathPlaceholderLinux", // "/var/lib/flatpak/exports/bin/org.musicbrainz.Picard"
  "listenbrainz.critiquebrainzUserPlaceholder", // "https://critiquebrainz.org/user/..."
  "discord.integrationTitle", // "Discord Rich Presence"
  "smartPlaylistBuilder.mixWord", // "Mix"
  "smartPlaylistBuilder.playlistWord", // "Playlist"
  "smartPlaylistBuilder.fieldAlbum", // "Album"
  "smartPlaylistBuilder.fieldBitrate", // "Bitrate"
  "smartPlaylistBuilder.fieldBpm", // "BPM"
  "smartPlaylistBuilder.fieldCompilation", // "Compilation"
  "smartPlaylistBuilder.fieldBpm", // "BPM"
  "smartPlaylistBuilder.opEquals", // "="
  "smartPlaylistBuilder.opNotEquals", // "!="
  "smartPlaylistBuilder.opGte", // ">="
  "smartPlaylistBuilder.opLte", // "<="
  "smartPlaylistBuilder.opGt", // ">"
  "smartPlaylistBuilder.opLt", // "<"
  "audioPipeline.codec", // "Codec"
  "audioPipeline.bitrate", // "Bitrate"
  "audioPipeline.normalizationGain", // "{gain} dB ({source})"
  "audioPipeline.outputBackend", // "Backend"
  "audioPipeline.bitPerfect", // "Bit-perfect"
  "auth.scrobbling", // "Scrobbling"
]);

const IDENTICAL_OK_ES = new Set<string>([
  "collection.tableHeaderTrack", // "#"
  "collection.columnBpm", // "BPM"
  "collection.booleanNo", // "No"
  "collection.tableHeaderBpm", // "BPM"
  "collection.tableHeaderMusicBrainzId", // "MBID"
  "settings.tabGeneral", // "General"
  "settings.aboutAppName", // "Luminous Music Player"
  "settings.formatMsix", // "Microsoft Store"
  "settings.badgeIconUsb", // "USB"
  "settings.webdavUrlPlaceholder", // a URL
  "settings.subsonicUrlPlaceholder", // a URL
  "playlists.tableHeaderTrack", // "#"
  "playlists.populationModeTitleFormat", // "{base} {suffix}"
  "playlists.bpmAutoPlaylist", // "BPM"
  "lyrics.instrumentalBadge", // "Instrumental"
  "stats.minuteCount", // "{count} min"
  "stats.minuteUnderOne", // "< 1 min"
  "stats.heatmapStatus", // "{date} — {minutes} min"
  "playerBar.channelsMono", // "Mono"
  "playerBar.dynamicRangeRms", // "RMS {value} dB"
  "playerBar.musicbrainzSectionLabel", // "MusicBrainz"
  "playerBar.wikipediaSectionLabel", // "Wikipedia"
  "playerBar.critiquebrainzSectionLabel", // "CritiqueBrainz"
  "playerBar.listenbrainzSectionLabel", // "ListenBrainz"
  "tagEditor.bpmField", // "BPM"
  "equalizer.popPreset", // "Pop"
  "equalizer.rockPreset", // "Rock"
  "equalizer.jazzPreset", // "Jazz"
  "equalizer.importPlaceholder", // Equalizer APO sample lines, same syntax in every language
  "equalizer.qFactor", // "Q"
  "equalizer.isoStandard", // "ISO 266:1997"
  "themes.dynamic-artwork", // "✨ Luminous"
  "themes.sabrina", // "Sabrina"
  "artistEvents.festival", // "Festival"
  "albumDetail.statsLine", // "{genre} · {year} · {duration}"
  "organizer.placeholders", // "Variables"
  "organizer.statusError", // "Error"
  "picard.integrationTitle", // "MusicBrainz Picard"
  "picard.customPathPlaceholder", // a Windows path
  "picard.customPathPlaceholderLinux", // a Linux path
  "listenbrainz.integrationTitle", // "ListenBrainz Scrobbler"
  "listenbrainz.critiquebrainzUserPlaceholder", // a URL
  "discord.integrationTitle", // "Discord Rich Presence"
  "smartPlaylistBuilder.fieldBpm", // "BPM"
  "smartPlaylistBuilder.opEquals", // "="
  "smartPlaylistBuilder.opNotEquals", // "!="
  "smartPlaylistBuilder.opGte", // ">="
  "smartPlaylistBuilder.opLte", // "<="
  "smartPlaylistBuilder.opGt", // ">"
  "smartPlaylistBuilder.opLt", // "<"
  "audioPipeline.normalizationGain", // "{gain} dB ({source})"
  "audioPipeline.bitPerfect", // "Bit-perfect, a term of art"
  "auth.scrobbling", // "Scrobbling"
]);

const CATALOGS = [
  { name: "Spanish", file: "es.ts", messages: es, identicalOk: IDENTICAL_OK_ES },
  { name: "French", file: "fr.ts", messages: fr, identicalOk: IDENTICAL_OK_FR },
  { name: "Italian", file: "it.ts", messages: itMessages, identicalOk: IDENTICAL_OK_IT },
];

describe.each(CATALOGS)("Locale translation completeness and integrity: $name", ({ name, file, messages, identicalOk }) => {
  const flatEn = flatten(en);
  const flatLoc = flatten(messages);

  it(`every key in en.ts has a corresponding translation in ${file}`, () => {
    const missing = Object.keys(flatEn).filter((key) => !(key in flatLoc));
    expect(
      missing,
      `Missing ${name} translations in src/lib/locales/${file} for the following keys:\n${missing.map((k) => `  - ${k}`).join("\n")}`
    ).toEqual([]);
  });

  it(`${file} contains no stale keys that do not exist in en.ts`, () => {
    const stale = Object.keys(flatLoc).filter((key) => !(key in flatEn));
    expect(
      stale,
      `Stale keys found in src/lib/locales/${file} that no longer exist in en.ts:\n${stale.map((k) => `  - ${k}`).join("\n")}`
    ).toEqual([]);
  });

  it(`${file} values differ from en.ts unless explicitly allowlisted`, () => {
    const untranslated = Object.keys(flatEn).filter((key) => {
      return key in flatLoc && flatEn[key] === flatLoc[key] && !identicalOk.has(key);
    });

    expect(
      untranslated,
      `The following ${name} translations are identical to English. If this is legitimately the same word in ${name} (or a symbol/brand name), add the key to that locale's identical-text allowlist in src/lib/locales/locales.test.ts:\n${untranslated
        .map((k) => `  - ${k}: "${flatEn[k]}"`)
        .join("\n")}`
    ).toEqual([]);
  });

  it("every allowlisted key is still identical and exists in en.ts", () => {
    const unneeded = [...identicalOk].filter((key) => {
      return !(key in flatEn) || !(key in flatLoc) || flatEn[key] !== flatLoc[key];
    });

    expect(
      unneeded,
      `The following ${name} allowlist keys no longer match or no longer exist; remove them from the allowlist in src/lib/locales/locales.test.ts:\n${unneeded
        .map((k) => `  - ${k}`)
        .join("\n")}`
    ).toEqual([]);
  });

  it(`interpolation placeholder tokens match between en.ts and ${file}`, () => {
    const mismatches: { key: string; enTokens: string[]; locTokens: string[] }[] = [];

    for (const key of Object.keys(flatEn)) {
      if (key in flatLoc) {
        const enTokens = extractPlaceholders(flatEn[key]);
        const locTokens = extractPlaceholders(flatLoc[key]);
        if (enTokens.join(",") !== locTokens.join(",")) {
          mismatches.push({ key, enTokens, locTokens });
        }
      }
    }

    expect(
      mismatches,
      `Interpolation placeholders mismatch between en.ts and ${file}:\n${mismatches
        .map((m) => `  - ${m.key}: en has [${m.enTokens.join(", ")}], ${name} has [${m.locTokens.join(", ")}]`)
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

describe("Locale registry", () => {
  it("lists the base locale and has unique BCP 47 tags", () => {
    const tags = LOCALES.map((l) => l.tag);
    expect(tags).toContain(BASE_LOCALE);
    expect(new Set(tags).size).toBe(tags.length);
    for (const tag of tags) {
      expect(() => new Intl.Locale(tag)).not.toThrow();
    }
  });

  it("every declared fallback names a registered locale", () => {
    const tags = new Set<string>(LOCALES.map((l) => l.tag));
    for (const def of LOCALES as readonly { tag: string; fallback?: string }[]) {
      if (def.fallback) expect(tags.has(def.fallback), `${def.tag} -> ${def.fallback}`).toBe(true);
    }
  });

  it("isLocale accepts only registered tags", () => {
    expect(isLocale("fr-CA")).toBe(true);
    expect(isLocale("fr")).toBe(false);
    expect(isLocale(undefined)).toBe(false);
  });

  it("maps pre-registry values to tags", () => {
    expect(legacyLanguageToLocale("en")).toBe("en-CA");
    expect(legacyLanguageToLocale("fr")).toBe("fr-CA");
    expect(legacyLanguageToLocale("de")).toBeNull();
  });

  it("catalogChain ends at the base catalog and tolerates unknown tags", () => {
    expect(catalogChain("fr-CA")).toEqual([fr, en]);
    expect(catalogChain("en-CA")).toEqual([en]);
    expect(catalogChain("zz")).toEqual([en]);
  });

  it("labels locales in their own language", () => {
    expect(localeLabel("en-CA")).toBe("English (Canada)");
    expect(localeLabel("fr-CA")).toBe("Français (Canada)");
  });
});

describe("Locale picker groups", () => {
  it("pins English (Canada) and Français (Canada) first", () => {
    const { pinned } = localePickerGroups();
    expect(pinned).toEqual(["en-CA", "fr-CA"]);
  });

  it("lists every registered locale exactly once", () => {
    const { pinned, rest } = localePickerGroups();
    expect([...pinned, ...rest].sort()).toEqual(LOCALES.map((l) => l.tag).sort());
  });
});

describe("Manual language", () => {
  it("maps any fr* UI locale to French and everything else to English", () => {
    expect(manualLanguageForLocale("fr-CA")).toBe("FR");
    expect(manualLanguageForLocale("fr-FR")).toBe("FR");
    expect(manualLanguageForLocale("de")).toBe("EN");
    expect(manualLanguageForLocale("en-GB")).toBe("EN");
  });
});

describe("Inline-fallback keys", () => {
  it("every i18n.t call with an inline English fallback names a key that exists in en.ts", () => {
    // A key that is only in the call site's fallback can never be translated.
    const sources = import.meta.glob(["/src/**/*.svelte", "/src/**/*.ts", "!/src/**/*.test.ts", "!/src/lib/locales/**"], {
      query: "?raw",
      import: "default",
      eager: true,
    }) as Record<string, string>;
    const flatEn = flatten(en);
    const missing: string[] = [];
    for (const [file, text] of Object.entries(sources)) {
      for (const m of text.matchAll(/i18n\.t\(\s*["'`]([\w.-]+)["'`]\s*,\s*\{[^}]*\}\s*,\s*["'`]/g)) {
        if (!(m[1] in flatEn)) missing.push(`${file}: ${m[1]}`);
      }
    }
    expect(missing, `Keys used with an inline fallback but missing from en.ts:\n${missing.join("\n")}`).toEqual([]);
  });
});
