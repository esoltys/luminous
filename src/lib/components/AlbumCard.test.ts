import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import AlbumCard from "./AlbumCard.svelte";
import { collectionStore } from "../stores/collection.svelte";
import type { AlbumItem } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

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
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders album title, artist, year, and category", () => {
    const { getByText } = render(AlbumCard, { props: { album: mockAlbum } });

    expect(getByText("Fake Nudes")).toBeInTheDocument();
    expect(getByText("Barenaked Ladies")).toBeInTheDocument();
    expect(getByText("2017")).toBeInTheDocument();
    expect(getByText("Album")).toBeInTheDocument();
  });

  it("shows Single for a one-track release", () => {
    const { getByText } = render(AlbumCard, {
      props: { album: { ...mockAlbum, track_count: 1 } },
    });
    expect(getByText("Single")).toBeInTheDocument();
  });

  it("shows EP for a 2-6 track release", () => {
    const { getByText } = render(AlbumCard, {
      props: { album: { ...mockAlbum, track_count: 5 } },
    });
    expect(getByText("EP")).toBeInTheDocument();
  });

  it("shows an N-Disc Set label instead of Album when multi-disc, never both", () => {
    const { getByText, queryByText } = render(AlbumCard, {
      props: { album: { ...mockAlbum, track_count: 20, disc_count: 2 } },
    });
    expect(getByText("2-Disc Set")).toBeInTheDocument();
    expect(queryByText("Album")).not.toBeInTheDocument();
  });

  it("navigates to album when album title is clicked", async () => {
    const viewAlbumSpy = vi.spyOn(collectionStore, "viewAlbum");
    const { getByText } = render(AlbumCard, { props: { album: mockAlbum } });

    const albumTitleBtn = getByText("Fake Nudes");
    await fireEvent.click(albumTitleBtn);

    expect(viewAlbumSpy).toHaveBeenCalledWith("Fake Nudes");
  });

  it("navigates to artist when artist name is clicked", async () => {
    const viewArtistSpy = vi.spyOn(collectionStore, "viewArtist");
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
});
