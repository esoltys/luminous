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
    art_embedded: false,
    art_automatic: null,
    art_manual: null,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders album title, artist, year, and track count", () => {
    const { getByText } = render(AlbumCard, { props: { album: mockAlbum } });

    expect(getByText("Fake Nudes")).toBeInTheDocument();
    expect(getByText("Barenaked Ladies")).toBeInTheDocument();
    expect(getByText("2017")).toBeInTheDocument();
    expect(getByText(/14 songs/i)).toBeInTheDocument();
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
