import type { ArtistSocialLink } from "../types";

export interface SocialPlatformInfo {
  id: string;
  label: string;
  placeholder: string;
  example: string;
  baseUrl?: string;
}

export const SOCIAL_PLATFORMS: SocialPlatformInfo[] = [
  {
    id: "website",
    label: "Website",
    placeholder: "https://www.artist.com or www.artist.com",
    example: "https://www.artist.com",
  },
  {
    id: "bandcamp",
    label: "Bandcamp",
    placeholder: "username or https://artist.bandcamp.com",
    example: "artist-name or https://artist.bandcamp.com",
    baseUrl: "https://{}.bandcamp.com",
  },
  {
    id: "soundcloud",
    label: "SoundCloud",
    placeholder: "username or https://soundcloud.com/artist",
    example: "artist or https://soundcloud.com/artist",
    baseUrl: "https://soundcloud.com/{}",
  },
  {
    id: "spotify",
    label: "Spotify",
    placeholder: "https://open.spotify.com/artist/...",
    example: "https://open.spotify.com/artist/4Z8W4fKeB5YxbusRsdQVPb",
  },
  {
    id: "apple_music",
    label: "Apple Music",
    placeholder: "https://music.apple.com/artist/...",
    example: "https://music.apple.com/us/artist/shania-twain/86782",
  },
  {
    id: "youtube",
    label: "YouTube",
    placeholder: "@channel or https://youtube.com/@channel",
    example: "@ShaniaTwain or https://youtube.com/@ShaniaTwain",
    baseUrl: "https://youtube.com/{}",
  },
  {
    id: "instagram",
    label: "Instagram",
    placeholder: "@username or https://instagram.com/username",
    example: "@shaniatwain or https://instagram.com/shaniatwain",
    baseUrl: "https://instagram.com/{}",
  },
  {
    id: "x",
    label: "X (Twitter)",
    placeholder: "@username or https://x.com/username",
    example: "@ShaniaTwain or https://x.com/ShaniaTwain",
    baseUrl: "https://x.com/{}",
  },
  {
    id: "facebook",
    label: "Facebook",
    placeholder: "username or https://facebook.com/username",
    example: "ShaniaTwain or https://facebook.com/ShaniaTwain",
    baseUrl: "https://facebook.com/{}",
  },
  {
    id: "bluesky",
    label: "Bluesky",
    placeholder: "handle.bsky.social or https://bsky.app/profile/...",
    example: "shaniatwain.bsky.social or https://bsky.app/profile/shaniatwain.bsky.social",
    baseUrl: "https://bsky.app/profile/{}",
  },
  {
    id: "threads",
    label: "Threads",
    placeholder: "@username or https://threads.net/@username",
    example: "@shaniatwain or https://threads.net/@shaniatwain",
    baseUrl: "https://threads.net/@{}",
  },
  {
    id: "tiktok",
    label: "TikTok",
    placeholder: "@username or https://tiktok.com/@username",
    example: "@shaniatwain or https://tiktok.com/@shaniatwain",
    baseUrl: "https://tiktok.com/@{}",
  },
  {
    id: "musicbrainz",
    label: "MusicBrainz",
    placeholder: "https://musicbrainz.org/artist/...",
    example: "https://musicbrainz.org/artist/042c0697-3948-4720-bf43-690240aeac43",
  },
  {
    id: "discogs",
    label: "Discogs",
    placeholder: "https://discogs.com/artist/...",
    example: "https://discogs.com/artist/82343-Shania-Twain",
  },
  {
    id: "wikipedia",
    label: "Wikipedia",
    placeholder: "https://en.wikipedia.org/wiki/...",
    example: "https://en.wikipedia.org/wiki/Shania_Twain",
  },
  {
    id: "allmusic",
    label: "AllMusic",
    placeholder: "https://www.allmusic.com/artist/...",
    example: "https://www.allmusic.com/artist/shania-twain-mn0000869402",
  },
  {
    id: "wikidata",
    label: "Wikidata",
    placeholder: "https://www.wikidata.org/wiki/...",
    example: "https://www.wikidata.org/wiki/Q11649",
  },
  {
    id: "imdb",
    label: "IMDb",
    placeholder: "https://www.imdb.com/name/...",
    example: "https://www.imdb.com/name/nm0876013",
  },
  {
    id: "custom",
    label: "Custom Link",
    placeholder: "https://...",
    example: "https://linktr.ee/shaniatwain",
  },
];

export function getPlatformInfo(platformId: string): SocialPlatformInfo {
  const found =
    SOCIAL_PLATFORMS.find((p) => p.id === platformId) ??
    ALBUM_LINK_PLATFORMS.find((p) => p.id === platformId);
  if (found) return found;
  return {
    id: platformId,
    label: platformId.charAt(0).toUpperCase() + platformId.slice(1),
    placeholder: "https://...",
    example: "https://...",
  };
}

/**
 * Resolves user input (which may be a username, handle, or full URL) into
 * a valid external URL.
 */
export function resolveSocialUrl(platformId: string, input: string): string {
  const trimmed = (input || "").trim();
  if (!trimmed) return "";

  if (/^https?:\/\//i.test(trimmed)) {
    return trimmed;
  }

  // Handle protocol-relative or bare domain URL
  if (/^[a-z0-9-]+(\.[a-z0-9-]+)+\/?/i.test(trimmed) && (platformId === "website" || platformId === "custom" || trimmed.includes("/"))) {
    return `https://${trimmed}`;
  }

  const cleanHandle = trimmed.replace(/^@/, "");

  switch (platformId) {
    case "website":
    case "custom":
    case "spotify":
    case "apple_music":
    case "musicbrainz":
    case "discogs":
    case "wikipedia":
      return trimmed.startsWith("http") ? trimmed : `https://${trimmed}`;

    case "bandcamp":
      return `https://${cleanHandle}.bandcamp.com`;

    case "soundcloud":
      return `https://soundcloud.com/${cleanHandle}`;

    case "youtube":
      return trimmed.startsWith("@")
        ? `https://youtube.com/${trimmed}`
        : `https://youtube.com/@${cleanHandle}`;

    case "instagram":
      return `https://instagram.com/${cleanHandle}`;

    case "x":
      return `https://x.com/${cleanHandle}`;

    case "facebook":
      return `https://facebook.com/${cleanHandle}`;

    case "bluesky":
      return `https://bsky.app/profile/${cleanHandle}`;

    case "threads":
      return `https://threads.net/@${cleanHandle}`;

    case "tiktok":
      return `https://tiktok.com/@${cleanHandle}`;

    default:
      return trimmed.startsWith("http") ? trimmed : `https://${trimmed}`;
  }
}

const KNOWN_FIXED_PLATFORMS = new Set([
  "bandcamp",
  "soundcloud",
  "spotify",
  "apple_music",
  "apple",
  "youtube",
  "instagram",
  "x",
  "twitter",
  "facebook",
  "bluesky",
  "threads",
  "tiktok",
  "musicbrainz",
  "discogs",
  "wikipedia",
  "allmusic",
  "wikidata",
  "imdb",
  "listenbrainz",
]);

/**
 * Formats a clean human-readable label to display for a link in the UI.
 * URLs from unrecognized sites or generic platforms ("website", "lyrics",
 * "other_databases", "custom") display only their domain rather than the
 * full URL path (#1133).
 */
export function formatDisplayLabel(platformId: string, input: string): string {
  const trimmed = (input || "").trim();
  if (!trimmed) return "";

  const info = getPlatformInfo(platformId);

  if (platformId === "internet_archive") {
    return "Internet Archive";
  }

  // "website", "lyrics", "other_databases", "custom", and any unrecognized
  // platforms cover many different sites rather than one fixed destination —
  // show only the domain so buttons remain compact and distinguishable.
  if (!KNOWN_FIXED_PLATFORMS.has(platformId)) {
    try {
      const url = resolveSocialUrl(platformId, trimmed);
      const parsed = new URL(url);
      const hostname = parsed.hostname.replace(/^www\./i, "");
      // web.archive.org's own path is a timestamp plus the entire archived
      // URL, which is unreadable as a label — name it plainly (#1123).
      if (hostname === "web.archive.org") return "Internet Archive";
      if (hostname) return hostname;
    } catch {
      const fallback = trimmed
        .replace(/^https?:\/\//i, "")
        .replace(/^www\./i, "")
        .split(/[/?#]/)[0]
        .trim();
      if (fallback) return fallback;
    }
  }

  return info.label;
}

/**
 * Swaps a website-like platform id ("website", "lyrics", "other_databases", "custom")
 * for "internet_archive" when the resolved URL is actually a
 * web.archive.org snapshot, so it renders with the Internet Archive icon
 * instead of a generic globe (#1123) — MusicBrainz's "official homepage"
 * relation sometimes points at an archived snapshot of a site that's since
 * gone offline. Any other platform id (or URL) is returned unchanged.
 */
export function normalizeWebsitePlatform(platformId: string, url: string): string {
  if (
    platformId !== "website" &&
    platformId !== "lyrics" &&
    platformId !== "other_databases" &&
    platformId !== "custom"
  ) {
    return platformId;
  }
  try {
    const hostname = new URL(url).hostname.replace(/^www\./i, "");
    if (hostname === "web.archive.org") return "internet_archive";
  } catch {
    // Not a parseable absolute URL — fall through unchanged.
  }
  return platformId;
}

const MBID_PATTERN = /([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})/i;

/**
 * Derives a fanart.tv artist page link from a stored MusicBrainz social
 * link's MBID (#98/#761) — fanart.tv keys its artist pages by the same
 * MusicBrainz artist id. Not a fetch: it's the same "link" pattern already
 * used for the user-entered MusicBrainz/Discogs/Wikipedia links above, just
 * computed from an id we already have on hand instead of typed in by the
 * user. Not registered in {@link SOCIAL_PLATFORMS} — it's a derived,
 * read-only link, not something users pick from the "add link" editor.
 * Returns null when there's no MusicBrainz link, or its value doesn't
 * contain a recognizable MBID.
 */
export function deriveFanartTvUrl(socialLinks: ArtistSocialLink[] | undefined | null): string | null {
  const mbLink = socialLinks?.find((l) => l.platform === "musicbrainz");
  if (!mbLink?.handle_or_url) return null;
  const match = mbLink.handle_or_url.match(MBID_PATTERN);
  if (!match) return null;
  return `https://fanart.tv/artist/${match[1].toLowerCase()}`;
}

/**
 * Resolves the artist's MusicBrainz MBID for the derived MusicBrainz/
 * ListenBrainz/Fanart.tv links: prefers `ArtistProfile.musicbrainz_artist_id`
 * (#1123, captured from "Retrieve Album/Artist Details" or a tagged song),
 * falling back to a manually-stored "musicbrainz" social link's MBID — the
 * only source available before #1123 added the dedicated field. Returns
 * `null` when neither is present or recognizable.
 */
export function resolveArtistMbid(
  musicbrainzArtistId: string | null | undefined,
  socialLinks: ArtistSocialLink[] | undefined | null
): string | null {
  const directMatch = musicbrainzArtistId?.trim().match(MBID_PATTERN);
  if (directMatch) return directMatch[1].toLowerCase();

  const mbLink = socialLinks?.find((l) => l.platform === "musicbrainz");
  const linkMatch = mbLink?.handle_or_url?.match(MBID_PATTERN);
  return linkMatch ? linkMatch[1].toLowerCase() : null;
}

/** Derives a MusicBrainz artist page URL from a resolved MBID (#1123). */
export function deriveMusicbrainzArtistUrl(mbid: string | null | undefined): string | null {
  return mbid ? `https://musicbrainz.org/artist/${mbid}` : null;
}

/** Derives a ListenBrainz artist page URL from a resolved MBID (#1123). */
export function deriveListenbrainzArtistUrl(mbid: string | null | undefined): string | null {
  return mbid ? `https://listenbrainz.org/artist/${mbid}/` : null;
}

/** Derives a fanart.tv artist page URL from a resolved MBID (#1123) — same
 * destination as {@link deriveFanartTvUrl}, computed from the resolved MBID
 * directly instead of re-deriving it from `socialLinks`. */
export function deriveFanartTvUrlFromMbid(mbid: string | null | undefined): string | null {
  return mbid ? `https://fanart.tv/artist/${mbid}` : null;
}

/**
 * Derives a ListenBrainz album/release page URL from a representative song's
 * MusicBrainz release group or release ID (#950).
 */
export function deriveListenbrainzAlbumUrl(song: {
  musicbrainz_release_group_id?: string | null;
  musicbrainz_album_id?: string | null;
} | null | undefined): string | null {
  if (!song) return null;
  const releaseGroupMbid = song.musicbrainz_release_group_id?.trim();
  if (releaseGroupMbid && MBID_PATTERN.test(releaseGroupMbid)) {
    return `https://listenbrainz.org/album/${releaseGroupMbid}/`;
  }
  const albumMbid = song.musicbrainz_album_id?.trim();
  if (albumMbid && MBID_PATTERN.test(albumMbid)) {
    return `https://listenbrainz.org/release/${albumMbid}/`;
  }
  return null;
}

/**
 * Curated list of link platforms specifically relevant to album releases (#950).
 * Prevents artist-level social channels (e.g. personal Instagram, Twitter, TikTok)
 * from cluttering album liner notes.
 */
export const ALBUM_LINK_PLATFORMS: SocialPlatformInfo[] = [
  {
    id: "website",
    label: "Official Page",
    placeholder: "https://artist.com/album or www.artist.com/album",
    example: "https://artist.com/music/album-name",
  },
  {
    id: "bandcamp",
    label: "Bandcamp",
    placeholder: "https://artist.bandcamp.com/album/...",
    example: "https://artist.bandcamp.com/album/album-name",
  },
  {
    id: "discogs",
    label: "Discogs",
    placeholder: "https://www.discogs.com/release/... or .../master/...",
    example: "https://www.discogs.com/master/12345-Album-Name",
  },
  {
    id: "wikipedia",
    label: "Wikipedia",
    placeholder: "https://en.wikipedia.org/wiki/...",
    example: "https://en.wikipedia.org/wiki/Album_Name",
  },
  {
    id: "musicbrainz",
    label: "MusicBrainz",
    placeholder: "https://musicbrainz.org/release-group/...",
    example: "https://musicbrainz.org/release-group/042c0697-3948-4720-bf43-690240aeac43",
  },
  {
    id: "spotify",
    label: "Spotify",
    placeholder: "https://open.spotify.com/album/...",
    example: "https://open.spotify.com/album/4Z8W4fKeB5YxbusRsdQVPb",
  },
  {
    id: "apple_music",
    label: "Apple Music",
    placeholder: "https://music.apple.com/album/...",
    example: "https://music.apple.com/us/album/album-name/123456",
  },
  {
    id: "youtube",
    label: "YouTube",
    placeholder: "https://youtube.com/playlist?list=... or video URL",
    example: "https://www.youtube.com/playlist?list=OLAK5uy_...",
  },
  {
    id: "soundcloud",
    label: "SoundCloud",
    placeholder: "https://soundcloud.com/artist/sets/...",
    example: "https://soundcloud.com/artist/sets/album-name",
  },
  {
    id: "allmusic",
    label: "AllMusic",
    placeholder: "https://www.allmusic.com/album/...",
    example: "https://www.allmusic.com/album/mw0000123456",
  },
  {
    id: "wikidata",
    label: "Wikidata",
    placeholder: "https://www.wikidata.org/wiki/...",
    example: "https://www.wikidata.org/wiki/Q11649",
  },
  {
    id: "lyrics",
    label: "Lyrics",
    placeholder: "https://... (e.g. Genius, Musixmatch)",
    example: "https://genius.com/albums/Artist/Album-name",
  },
  {
    id: "other_databases",
    label: "Other Databases",
    placeholder: "https://... (e.g. Rate Your Music, VGMdb)",
    example: "https://rateyourmusic.com/release/album/...",
  },
  {
    id: "custom",
    label: "Custom Link",
    placeholder: "https://... (e.g. Pitchfork review, liner notes, blog)",
    example: "https://pitchfork.com/reviews/albums/...",
  },
];
