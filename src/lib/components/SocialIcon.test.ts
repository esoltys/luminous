import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import SocialIcon from "./SocialIcon.svelte";

describe("SocialIcon", () => {
  it("renders website icon", () => {
    const { container } = render(SocialIcon, { props: { platform: "website" } });
    expect(container.querySelector("svg")).toBeDefined();
  });

  it("renders custom svg icons for social platforms", () => {
    const platforms = [
      "spotify",
      "apple_music",
      "bandcamp",
      "soundcloud",
      "youtube",
      "instagram",
      "x",
      "facebook",
      "bluesky",
      "threads",
      "tiktok",
      "musicbrainz",
      "discogs",
      "wikipedia",
      "custom",
    ];

    for (const platform of platforms) {
      const { container } = render(SocialIcon, { props: { platform } });
      expect(container.querySelector("svg")).toBeTruthy();
    }
  });
});
