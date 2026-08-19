import { describe, it, expect } from "vitest";
import { resolveSocialUrl, formatDisplayLabel, getPlatformInfo, SOCIAL_PLATFORMS } from "./artistSocials";

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
});
