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
    id: "custom",
    label: "Custom Link",
    placeholder: "https://...",
    example: "https://linktr.ee/shaniatwain",
  },
];

export function getPlatformInfo(platformId: string): SocialPlatformInfo {
  const found = SOCIAL_PLATFORMS.find((p) => p.id === platformId);
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

/**
 * Formats a clean human-readable label to display for a link in the UI.
 */
export function formatDisplayLabel(platformId: string, input: string): string {
  const trimmed = (input || "").trim();
  if (!trimmed) return "";

  const info = getPlatformInfo(platformId);

  if (platformId === "website") {
    // Show clean hostname or short path without http/https
    try {
      const url = resolveSocialUrl(platformId, trimmed);
      const parsed = new URL(url);
      return parsed.hostname.replace(/^www\./, "") + (parsed.pathname !== "/" ? parsed.pathname : "");
    } catch {
      return trimmed.replace(/^https?:\/\/(www\.)?/, "").replace(/\/$/, "");
    }
  }

  return info.label;
}
