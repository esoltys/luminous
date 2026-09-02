import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import SongTable, { type SongTableRow } from "./SongTable.svelte";
import { collectionStore } from "../stores/collection.svelte";
import type { Song } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

describe("SongTable.svelte — Album column (#428)", () => {
  const baseSong: Song = {
    id: 1,
    source: "local_file",
    filetype: "MP3",
    path: "/music/test.mp3",
    title: "Test Track",
    artist: "Test Artist",
    album: "Test Album",
    album_artist: "",
    composer: "",
    genre: "",
    track: 1,
    disc: 1,
    year: 2024,
    compilation: false,
    length_nanosec: 180_000_000_000,
    beginning_nanosec: 0,
    end_nanosec: 180_000_000_000,
    rating: 0,
    playcount: 0,
    skipcount: 0,
    art_embedded: false,
    art_unset: false,
    unavailable: false,
  };

  beforeEach(() => {
    collectionStore.visibleColumns.album = true;
  });

  function renderTable(rows: SongTableRow[]) {
    return render(SongTable, {
      props: {
        rows,
        mode: "track",
        leadingColumnWidth: "3rem",
        colDefaults: {},
        sortField: "title",
        sortAsc: true,
        onToggleSort: () => {},
        onRowDoubleClick: () => {},
        onRowContextMenu: () => {},
        onRate: () => {},
        onEditTags: () => {},
      },
    });
  }

  it("renders an em-dash for a song with no album", () => {
    const song: Song = { ...baseSong, album: "" };
    const { getByText, queryByText } = renderTable([{ key: "1", song }]);

    expect(getByText("—")).toBeInTheDocument();
    expect(queryByText(/unknown album/i)).not.toBeInTheDocument();
  });

  it("renders the album LinkButton when the album is present", () => {
    const { getByText } = renderTable([{ key: "1", song: baseSong }]);

    expect(getByText("Test Album")).toBeInTheDocument();
  });
});
