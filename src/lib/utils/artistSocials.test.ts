import { describe, it, expect } from "vitest";
import {
  resolveSocialUrl,
  formatDisplayLabel,
  getPlatformInfo,
  SOCIAL_PLATFORMS,
  deriveFanartTvUrl,
  resolveArtistMbid,
  deriveMusicbrainzArtistUrl,
  deriveListenbrainzArtistUrl,
  deriveFanartTvUrlFromMbid,
  normalizeWebsitePlatform,
} from "./artistSocials";

describe("artistSocials", () => {
  it("provides info for all known platforms", () => {
    expect(SOCIAL_PLATFORMS.length).toBeGreaterThan(10);
    const bandcamp = getPlatformInfo("bandcamp");
    expect(bandcamp.label).toBe("Bandcamp");
    expect(bandcamp.placeholder).toContain("username");
  });

  it("resolves website URLs", () => {
    expect(resolveSocialUrl("website", "www.shaniatwain.com")).toBe("https://www.shaniatwain.com");
    expect(resolveSocialUrl("website", "http://example.com")).toBe("http://example.com");
    expect(resolveSocialUrl("website", "https://shaniatwain.com/tour")).toBe("https://shaniatwain.com/tour");
  });

  it("resolves social handles to full URLs", () => {
    expect(resolveSocialUrl("bandcamp", "shaniatwain")).toBe("https://shaniatwain.bandcamp.com");
    expect(resolveSocialUrl("soundcloud", "shaniatwain")).toBe("https://soundcloud.com/shaniatwain");
    expect(resolveSocialUrl("youtube", "@ShaniaTwain")).toBe("https://youtube.com/@ShaniaTwain");
    expect(resolveSocialUrl("youtube", "ShaniaTwain")).toBe("https://youtube.com/@ShaniaTwain");
    expect(resolveSocialUrl("instagram", "@shaniatwain")).toBe("https://instagram.com/shaniatwain");
    expect(resolveSocialUrl("instagram", "shaniatwain")).toBe("https://instagram.com/shaniatwain");
    expect(resolveSocialUrl("x", "@ShaniaTwain")).toBe("https://x.com/ShaniaTwain");
    expect(resolveSocialUrl("facebook", "ShaniaTwain")).toBe("https://facebook.com/ShaniaTwain");
    expect(resolveSocialUrl("bluesky", "shaniatwain.bsky.social")).toBe("https://bsky.app/profile/shaniatwain.bsky.social");
    expect(resolveSocialUrl("threads", "@shaniatwain")).toBe("https://threads.net/@shaniatwain");
    expect(resolveSocialUrl("tiktok", "@shaniatwain")).toBe("https://tiktok.com/@shaniatwain");
  });

  it("leaves full URLs untouched", () => {
    const fullIg = "https://instagram.com/custom_artist_page";
    expect(resolveSocialUrl("instagram", fullIg)).toBe(fullIg);
    const spotify = "https://open.spotify.com/artist/4Z8W4fKeB5YxbusRsdQVPb";
    expect(resolveSocialUrl("spotify", spotify)).toBe(spotify);
  });

  it("formats display labels cleanly", () => {
    expect(formatDisplayLabel("website", "https://www.shaniatwain.com")).toBe("shaniatwain.com");
    expect(formatDisplayLabel("website", "https://shaniatwain.com/tour")).toBe("shaniatwain.com/tour");
    expect(formatDisplayLabel("instagram", "@shaniatwain")).toBe("Instagram");
    expect(formatDisplayLabel("youtube", "@ShaniaTwain")).toBe("YouTube");
  });

  it("labels a web.archive.org website link as 'Internet Archive' instead of its unreadable path (#1123)", () => {
    expect(
      formatDisplayLabel(
        "website",
        "https://web.archive.org/web/19970131155102/http://www.vmg.co.uk/massive/index.html"
      )
    ).toBe("Internet Archive");
  });

  describe("normalizeWebsitePlatform (#1123)", () => {
    it("swaps 'website' for 'internet_archive' when the URL is a web.archive.org snapshot", () => {
      expect(
        normalizeWebsitePlatform(
          "website",
          "https://web.archive.org/web/19970131155102/http://www.vmg.co.uk/massive/index.html"
        )
      ).toBe("internet_archive");
    });

    it("does the same for 'lyrics' and 'other_databases'", () => {
      expect(normalizeWebsitePlatform("lyrics", "https://web.archive.org/web/2020/https://genius.com/x")).toBe(
        "internet_archive"
      );
      expect(
        normalizeWebsitePlatform("other_databases", "https://web.archive.org/web/2020/https://rateyourmusic.com/x")
      ).toBe("internet_archive");
    });

    it("leaves a normal website URL's platform unchanged", () => {
      expect(normalizeWebsitePlatform("website", "https://massiveattack.com")).toBe("website");
    });

    it("leaves other platform ids unchanged even against a web.archive.org URL", () => {
      expect(normalizeWebsitePlatform("discogs", "https://web.archive.org/web/2020/https://discogs.com/x")).toBe(
        "discogs"
      );
    });
  });

  describe("resolveArtistMbid / derived MusicBrainz links (#1123)", () => {
    const mbid = "7249b899-8db8-43e7-9e6e-22f1e736024e";

    it("prefers ArtistProfile.musicbrainz_artist_id over a stored social link", () => {
      expect(
        resolveArtistMbid(mbid, [
          { platform: "musicbrainz", handle_or_url: "https://musicbrainz.org/artist/other-id-not-a-real-mbid" },
        ])
      ).toBe(mbid);
    });

    it("falls back to a stored 'musicbrainz' social link when the profile field is unset", () => {
      expect(
        resolveArtistMbid(null, [
          { platform: "musicbrainz", handle_or_url: `https://musicbrainz.org/artist/${mbid}` },
        ])
      ).toBe(mbid);
    });

    it("returns null when neither source has a recognizable MBID", () => {
      expect(resolveArtistMbid(null, null)).toBeNull();
      expect(resolveArtistMbid(undefined, [])).toBeNull();
      expect(resolveArtistMbid("not-a-valid-mbid", [{ platform: "discogs", handle_or_url: "https://discogs.com/artist/1" }])).toBeNull();
    });

    it("derives MusicBrainz/ListenBrainz/Fanart.tv URLs from the resolved MBID", () => {
      expect(deriveMusicbrainzArtistUrl(mbid)).toBe(`https://musicbrainz.org/artist/${mbid}`);
      expect(deriveListenbrainzArtistUrl(mbid)).toBe(`https://listenbrainz.org/artist/${mbid}/`);
      expect(deriveFanartTvUrlFromMbid(mbid)).toBe(`https://fanart.tv/artist/${mbid}`);
    });

    it("returns null from all three derivers when there is no MBID", () => {
      expect(deriveMusicbrainzArtistUrl(null)).toBeNull();
      expect(deriveListenbrainzArtistUrl(null)).toBeNull();
      expect(deriveFanartTvUrlFromMbid(null)).toBeNull();
    });
  });

  describe("deriveFanartTvUrl (#98/#761)", () => {
    it("derives a fanart.tv URL from a stored MusicBrainz link's MBID", () => {
      const url = deriveFanartTvUrl([
        { platform: "musicbrainz", handle_or_url: "https://musicbrainz.org/artist/7249b899-8db8-43e7-9e6e-22f1e736024e" },
      ]);
      expect(url).toBe("https://fanart.tv/artist/7249b899-8db8-43e7-9e6e-22f1e736024e");
    });

    it("lowercases a mixed-case MBID", () => {
      const url = deriveFanartTvUrl([
        { platform: "musicbrainz", handle_or_url: "https://musicbrainz.org/artist/7249B899-8DB8-43E7-9E6E-22F1E736024E" },
      ]);
      expect(url).toBe("https://fanart.tv/artist/7249b899-8db8-43e7-9e6e-22f1e736024e");
    });

    it("returns null when there is no musicbrainz link", () => {
      expect(deriveFanartTvUrl([{ platform: "discogs", handle_or_url: "https://discogs.com/artist/82343" }])).toBeNull();
      expect(deriveFanartTvUrl([])).toBeNull();
      expect(deriveFanartTvUrl(undefined)).toBeNull();
      expect(deriveFanartTvUrl(null)).toBeNull();
    });

    it("returns null when the musicbrainz link has no recognizable MBID", () => {
      expect(deriveFanartTvUrl([{ platform: "musicbrainz", handle_or_url: "not-a-valid-mbid" }])).toBeNull();
    });
  });
});
