import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import SocialIcon from "./SocialIcon.svelte";
import {
  siThreads,
  siSpotify,
  siApplemusic,
  siBandcamp,
  siSoundcloud,
  siYoutube,
  siInstagram,
  siX,
  siFacebook,
  siBluesky,
  siTiktok,
  siMusicbrainz,
  siDiscogs,
  siWikipedia,
} from "simple-icons";

describe("SocialIcon", () => {
  it("renders the generic globe icon (no brand path) for website", () => {
    const { container } = render(SocialIcon, { props: { platform: "website" } });
    expect(container.querySelector("svg")).toBeTruthy();
    expect(container.querySelector("svg path")?.getAttribute("d")).not.toBe(siSpotify.path);
  });

  it.each([
    ["spotify", siSpotify],
    ["apple_music", siApplemusic],
    ["bandcamp", siBandcamp],
    ["soundcloud", siSoundcloud],
    ["youtube", siYoutube],
    ["instagram", siInstagram],
    ["x", siX],
    ["facebook", siFacebook],
    ["bluesky", siBluesky],
    ["threads", siThreads],
    ["tiktok", siTiktok],
    ["musicbrainz", siMusicbrainz],
    ["discogs", siDiscogs],
    ["wikipedia", siWikipedia],
  ])("renders the correct brand icon path for %s", (platform, icon) => {
    const { container } = render(SocialIcon, { props: { platform } });
    const path = container.querySelector("svg path");
    expect(path?.getAttribute("d")).toBe(icon.path);
  });

  it("falls back to the generic link icon for an unrecognized platform", () => {
    const { container } = render(SocialIcon, { props: { platform: "custom" } });
    const path = container.querySelector("svg path")?.getAttribute("d");
    const brandPaths = [
      siThreads,
      siSpotify,
      siApplemusic,
      siBandcamp,
      siSoundcloud,
      siYoutube,
      siInstagram,
      siX,
      siFacebook,
      siBluesky,
      siTiktok,
      siMusicbrainz,
      siDiscogs,
      siWikipedia,
    ].map((icon) => icon.path);
    expect(brandPaths).not.toContain(path);
  });
});
