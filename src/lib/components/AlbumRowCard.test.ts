import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import AlbumRowCard from "./AlbumRowCard.svelte";
import { collectionStore } from "../stores/collection.svelte";
import type { AlbumItem } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe("AlbumRowCard.svelte", () => {
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
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders album title, artist, category, and year", () => {
    const { getByText } = render(AlbumRowCard, { props: { album: mockAlbum } });

    expect(getByText("Fake Nudes")).toBeInTheDocument();
    expect(getByText("Barenaked Ladies")).toBeInTheDocument();
    expect(getByText("Album")).toBeInTheDocument();
    expect(getByText("2017")).toBeInTheDocument();
  });

  it("shows Various Artists when the album has no artist", () => {
    const { getByText } = render(AlbumRowCard, {
      props: { album: { ...mockAlbum, artist: null } },
    });
    expect(getByText("Various Artists")).toBeInTheDocument();
  });

  it("navigates to the album on click by default", async () => {
    const viewAlbumSpy = vi.spyOn(collectionStore, "viewAlbum");
    const { getByText } = render(AlbumRowCard, { props: { album: mockAlbum } });

    await fireEvent.click(getByText("Fake Nudes"));

    expect(viewAlbumSpy).toHaveBeenCalledWith("Fake Nudes");
  });

  it("calls a custom click handler when passed", async () => {
    const handleClick = vi.fn();
    const { getByText } = render(AlbumRowCard, {
      props: { album: mockAlbum, onclick: handleClick },
    });

    await fireEvent.click(getByText("Fake Nudes"));

    expect(handleClick).toHaveBeenCalled();
  });

  it("forwards the context menu event", async () => {
    const handleContextMenu = vi.fn();
    const { getByText } = render(AlbumRowCard, {
      props: { album: mockAlbum, oncontextmenu: handleContextMenu },
    });

    await fireEvent.contextMenu(getByText("Fake Nudes"));

    expect(handleContextMenu).toHaveBeenCalled();
  });
});
