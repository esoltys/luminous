import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import CommunityRating from "./CommunityRating.svelte";
import { openUrl } from "@tauri-apps/plugin-opener";

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

describe("CommunityRating.svelte", () => {
  it("renders CritiqueBrainz rating with link and CritiqueBrainz tooltip (#1387)", async () => {
    const { getByTitle, getByText } = render(CommunityRating, {
      props: {
        rating: 4.5,
        count: 12,
        releaseGroupMbid: "rg-cb-123",
        source: "critiquebrainz",
      },
    });

    expect(getByText("Community Rating")).toBeInTheDocument();
    expect(getByText("(12)")).toBeInTheDocument();
    const button = getByTitle("Open this album on CritiqueBrainz to read or write reviews");
    expect(button).toBeInTheDocument();

    await fireEvent.click(button);
    expect(openUrl).toHaveBeenCalledWith("https://critiquebrainz.org/release-group/rg-cb-123");
  });

  it("renders MusicBrainz fallback rating with link and MusicBrainz tooltip (#1571)", async () => {
    const { getByTitle, getByText } = render(CommunityRating, {
      props: {
        rating: 3.75,
        count: 4,
        releaseGroupMbid: "rg-mb-456",
        source: "musicbrainz",
      },
    });

    expect(getByText("Community Rating")).toBeInTheDocument();
    expect(getByText("(4)")).toBeInTheDocument();
    const button = getByTitle("Open this album on MusicBrainz");
    expect(button).toBeInTheDocument();

    await fireEvent.click(button);
    expect(openUrl).toHaveBeenCalledWith("https://musicbrainz.org/release-group/rg-mb-456");
  });

  it("renders static text when releaseGroupMbid is not provided", () => {
    const { getByText, queryByRole } = render(CommunityRating, {
      props: {
        rating: 4.0,
        count: 5,
      },
    });

    expect(getByText("Community Rating")).toBeInTheDocument();
    expect(getByText("(5)")).toBeInTheDocument();
    expect(getByText("Community Rating").closest("button")).toBeNull();
  });

  it("omits count when count is null or undefined", () => {
    const { getByText, queryByText } = render(CommunityRating, {
      props: {
        rating: 3.0,
      },
    });

    expect(getByText("Community Rating")).toBeInTheDocument();
    expect(queryByText(/\(\d+\)/)).toBeNull();
  });
});
