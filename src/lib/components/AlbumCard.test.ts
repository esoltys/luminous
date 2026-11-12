import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import AlbumCard from "./AlbumCard.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { prefs } from "../stores/prefs.svelte";
import { invoke } from "@tauri-apps/api/core";
import type { AlbumItem } from "../types";

describe("AlbumCard.svelte", () => {
  const mockAlbum: AlbumItem = {
    album: "Fake Nudes",
    artist: "Barenaked Ladies",
    year: 2017,
    track_count: 14,
    disc_count: 1,
    art_embedded: false,
    art_automatic: null,
    art_manual: null,
    genre: "Alternative",
    rating: -1,
    total_duration_nanosec: 0,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    prefs.ratingStyle = "stars";
  });

  it("renders album title, artist, and year without genre chips or inline rating", () => {
    const { getByText, queryByText, queryByTestId } = render(AlbumCard, {
      props: { album: mockAlbum },
    });

    expect(getByText("Fake Nudes")).toBeInTheDocument();
    expect(getByText("Barenaked Ladies")).toBeInTheDocument();
    expect(getByText("2017")).toBeInTheDocument();
    expect(queryByText("Alternative")).not.toBeInTheDocument();
    expect(queryByTestId("album-card-genre-row")).toBeNull();
  });

  it("navigates to album when album title is clicked", async () => {
    const viewAlbumSpy = vi.spyOn(navigationStore, "viewAlbum");
    const { getByText } = render(AlbumCard, { props: { album: mockAlbum } });

    const albumTitleBtn = getByText("Fake Nudes");
    await fireEvent.click(albumTitleBtn);

    expect(viewAlbumSpy).toHaveBeenCalledWith("Fake Nudes");
  });

  it("navigates to artist when artist name is clicked", async () => {
    const viewArtistSpy = vi.spyOn(navigationStore, "viewArtist");
    const { getByText } = render(AlbumCard, { props: { album: mockAlbum } });

    const artistBtn = getByText("Barenaked Ladies");
    await fireEvent.click(artistBtn);

    expect(viewArtistSpy).toHaveBeenCalledWith("Barenaked Ladies");
  });

  it("calls custom click handler when passed", async () => {
    const handleClick = vi.fn();
    const { getByText } = render(AlbumCard, {
      props: { album: mockAlbum, onclick: handleClick },
    });

    const card = getByText("Fake Nudes").closest("div.bg-brand-sidebar")!;
    await fireEvent.click(card);

    expect(handleClick).toHaveBeenCalled();
  });

  it("renders favourite corner flag when album rating is 5", () => {
    const favoritedAlbum = { ...mockAlbum, rating: 5 };
    const { getByTestId } = render(AlbumCard, { props: { album: favoritedAlbum } });
    expect(getByTestId("favourite-corner-flag")).toBeInTheDocument();
  });

  it("does not render favourite corner flag when album rating is not 5", () => {
    const unratedAlbum = { ...mockAlbum, rating: 3 };
    const { queryByTestId } = render(AlbumCard, { props: { album: unratedAlbum } });
    expect(queryByTestId("favourite-corner-flag")).toBeNull();
  });

  it("defaults widthClass to w-full so it fills its grid cell", () => {
    const { getByText } = render(AlbumCard, { props: { album: mockAlbum } });
    const card = getByText("Fake Nudes").closest("div.bg-brand-sidebar")!;
    expect(card.className).toContain("w-full");
  });

  it("passes sample_song_id to CoverArt when art_automatic and art_manual are null (#1823)", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_cover_art_uri") return "luminous-art://webdav-cover.jpg";
      return null;
    });

    const { container } = render(AlbumCard, {
      props: { album: { ...mockAlbum, sample_song_id: 77 } },
    });

    await vi.waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_cover_art_uri", {
        songId: 77,
        fullResolution: false,
      });
      expect(container.querySelector("img")?.getAttribute("src")).toContain("webdav-cover.jpg");
    });
  });
});
