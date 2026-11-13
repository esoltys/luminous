import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import CurationShelf from "./CurationShelf.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { picardStore } from "../stores/picard.svelte";
import type { CurationOverview } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

describe("CurationShelf.svelte", () => {
  const emptyOverview: CurationOverview = {
    unillustrated_albums: [],
    incomplete_albums: [],
    missing_core_tags_song_ids: [],
    suspected_provenance_songs: [],
    duplicate_groups: [],
    corrupt_songs: [],
  };

  beforeEach(() => {
    vi.clearAllMocks();
    picardStore.path = null;
  });

  it("renders nothing at all when total curation items is 0 (zero-noise rule)", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_curation_overview") {
        return Promise.resolve(emptyOverview);
      }
      return Promise.resolve(null);
    });

    const { container } = render(CurationShelf);

    await waitFor(() => {
      expect(container.querySelector("[data-testid]")).toBeNull();
      expect(container.textContent?.trim()).toBe("");
    });
  });

  it("renders only unillustrated albums section when unillustrated count > 0 and others are 0", async () => {
    const overviewWithArt: CurationOverview = {
      ...emptyOverview,
      unillustrated_albums: [
        {
          album_key: "album-1",
          album: "Amnesiac",
          artist: "Radiohead",
          year: 2001,
          track_count: 11,
          disc_count: 1,
          art_embedded: false,
          art_automatic: null,
          art_manual: null,
          genre: "Art Rock",
          rating: -1,
          sample_song_id: 101,
          song_ids: [101, 102],
        },
      ],
    };

    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_curation_overview") {
        return Promise.resolve(overviewWithArt);
      }
      return Promise.resolve(null);
    });

    const { findByTestId, queryByTestId, findByText } = render(CurationShelf);

    expect(await findByTestId("curation-unillustrated-section")).toBeInTheDocument();
    expect(await findByText("Amnesiac")).toBeInTheDocument();
    expect(await findByText("Add Artwork")).toBeInTheDocument();

    // Verify other 0-count sections are completely omitted (no empty placeholders)
    expect(queryByTestId("curation-incomplete-section")).toBeNull();
    expect(queryByTestId("curation-missing-core-section")).toBeNull();
    expect(queryByTestId("curation-duplicates-section")).toBeNull();
    expect(queryByTestId("curation-provenance-section")).toBeNull();
    expect(queryByTestId("curation-corrupt-section")).toBeNull();
  });

  it("renders incomplete albums with unnumbered badge and triggers navigation & picard handoff", async () => {
    picardStore.path = "C:\\Program Files\\MusicBrainz Picard\\picard.exe";
    const openSongsSpy = vi.spyOn(picardStore, "openSongs").mockResolvedValue();
    const viewAlbumSpy = vi.spyOn(navigationStore, "viewAlbum").mockImplementation(() => {});

    const overviewIncomplete: CurationOverview = {
      ...emptyOverview,
      incomplete_albums: [
        {
          album_key: "album-2",
          album: "In Rainbows",
          artist: "Radiohead",
          year: 2007,
          track_count: 10,
          disc_count: 1,
          art_embedded: true,
          art_automatic: null,
          art_manual: null,
          genre: "Art Rock",
          rating: 1.0,
          sample_song_id: 201,
          song_ids: [201, 202, 203],
          unnumbered_count: 3,
          duplicate_track_number_count: 0,
        },
      ],
    };

    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_curation_overview") {
        return Promise.resolve(overviewIncomplete);
      }
      return Promise.resolve(null);
    });

    const { findByTestId, findByText } = render(CurationShelf);

    const section = await findByTestId("curation-incomplete-section");
    expect(section).toBeInTheDocument();
    expect(await findByText("3 unnumbered tracks")).toBeInTheDocument();

    // Verify incomplete albums render in a single-column flex list to avoid suffix collision (#1934)
    const listContainer = section.querySelector(".flex.flex-col.gap-3");
    expect(listContainer).not.toBeNull();

    // Clicking Picard button triggers picardStore.openSongs
    const picardBtn = await findByText("Open in Picard");
    await fireEvent.click(picardBtn);
    expect(openSongsSpy).toHaveBeenCalledWith([201, 202, 203]);

    // Clicking album card navigates to album
    const albumName = await findByText("In Rainbows");
    await fireEvent.click(albumName);
    expect(viewAlbumSpy).toHaveBeenCalledWith("In Rainbows", undefined, "album-2");
  });

  it("renders missing core metadata card and clicking button calls navigationStore.viewAutoPlaylist", async () => {
    const viewAutoPlaylistSpy = vi
      .spyOn(navigationStore, "viewAutoPlaylist")
      .mockImplementation(() => {});

    const overviewMissingCore: CurationOverview = {
      ...emptyOverview,
      missing_core_tags_song_ids: Array.from({ length: 7 }, (_, i) => 9000 + i),
    };

    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_curation_overview") {
        return Promise.resolve(overviewMissingCore);
      }
      return Promise.resolve(null);
    });

    const { findByTestId, findByText } = render(CurationShelf);

    expect(await findByTestId("curation-missing-core-section")).toBeInTheDocument();
    expect(await findByText("7 tracks missing core tags")).toBeInTheDocument();

    const reviewBtn = await findByText("Review Missing Tags");
    await fireEvent.click(reviewBtn);

    expect(viewAutoPlaylistSpy).toHaveBeenCalledWith({ kind: "missing_metadata" });
  });

  it("renders duplicate recordings card and clicking button opens modal", async () => {
    const overviewDuplicates: CurationOverview = {
      ...emptyOverview,
      duplicate_groups: [
        {
          group_key: "dup-1",
          title: "Karma Police",
          artist: "Radiohead",
          album: "OK Computer",
          candidates: [
            {
              song: { id: 1, title: "Karma Police", artist: "Radiohead" } as any,
              playlist_count: 1,
              tags: ["Rock"],
              provenance: null,
            },
            {
              song: { id: 2, title: "Karma Police", artist: "Radiohead" } as any,
              playlist_count: 0,
              tags: [],
              provenance: null,
            },
          ],
        },
      ],
    };

    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_curation_overview") {
        return Promise.resolve(overviewDuplicates);
      }
      return Promise.resolve(null);
    });

    const { findByTestId, findByText } = render(CurationShelf);

    expect(await findByTestId("curation-duplicates-section")).toBeInTheDocument();
    expect(await findByText("1 duplicate recording group")).toBeInTheDocument();

    const compareBtn = await findByText("Compare Duplicates");
    await fireEvent.click(compareBtn);

    // Modal opens
    expect(await findByText("Compare Duplicate Pressings")).toBeInTheDocument();
  });

  it("renders audio provenance warnings and clicking View in Files calls locate_song_in_files", async () => {
    const openInFilesSpy = vi.spyOn(navigationStore, "openInFiles").mockImplementation(() => {});

    const overviewProvenance: CurationOverview = {
      ...emptyOverview,
      suspected_provenance_songs: [
        {
          id: 55,
          title: "Paranoid Android",
          artist: "Radiohead",
          album: "OK Computer",
          provenance: "suspected_lossy_transcode",
        } as any,
      ],
    };

    const mockTarget = {
      source: { type: "folder", id: "source-1", name: "Music" } as any,
      path: ["Radiohead", "OK Computer"],
      filename: "02 - Paranoid Android.flac",
    };

    vi.mocked(invoke).mockImplementation((cmd: string, args: any) => {
      if (cmd === "get_curation_overview") {
        return Promise.resolve(overviewProvenance);
      }
      if (cmd === "locate_song_in_files" && args?.songId === 55) {
        return Promise.resolve(mockTarget);
      }
      return Promise.resolve(null);
    });

    const { findByTestId, findByText } = render(CurationShelf);

    expect(await findByTestId("curation-provenance-section")).toBeInTheDocument();
    expect(await findByText("Paranoid Android")).toBeInTheDocument();
    expect(await findByText("suspected_lossy_transcode")).toBeInTheDocument();

    const viewFilesBtn = await findByText("View in Files");
    await fireEvent.click(viewFilesBtn);

    await waitFor(() => {
      expect(openInFilesSpy).toHaveBeenCalledWith(mockTarget);
    });
  });

  it("handles Show all / Show less toggle for lists with more than 6 albums", async () => {
    const albums = Array.from({ length: 8 }, (_, i) => ({
      album_key: `album-${i + 1}`,
      album: `Album ${i + 1}`,
      artist: "Test Artist",
      year: 2020 + i,
      track_count: 10,
      disc_count: 1,
      art_embedded: false,
      art_automatic: null,
      art_manual: null,
      genre: "Rock",
      rating: -1,
      sample_song_id: 100 + i,
      song_ids: [100 + i],
    }));

    const overviewMany: CurationOverview = {
      ...emptyOverview,
      unillustrated_albums: albums,
    };

    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_curation_overview") {
        return Promise.resolve(overviewMany);
      }
      return Promise.resolve(null);
    });

    const { findByText, queryByText } = render(CurationShelf);

    expect(await findByText("Album 1")).toBeInTheDocument();
    expect(await findByText("Album 6")).toBeInTheDocument();
    expect(queryByText("Album 7")).toBeNull();

    const showAllBtn = await findByText("Show all (8)");
    await fireEvent.click(showAllBtn);

    expect(await findByText("Album 7")).toBeInTheDocument();
    expect(await findByText("Album 8")).toBeInTheDocument();

    const showLessBtn = await findByText("Show less");
    await fireEvent.click(showLessBtn);

    expect(queryByText("Album 7")).toBeNull();
  });

  it("renders corrupt audio tracks and triggers view in files handoff", async () => {
    const openInFilesSpy = vi.spyOn(navigationStore, "openInFiles").mockImplementation(() => {});
    const mockTarget = { folder_id: 1, relative_path: "Broken Track.flac" };

    const overviewCorrupt: CurationOverview = {
      ...emptyOverview,
      corrupt_songs: [
        {
          id: 77,
          title: "Broken Track",
          artist: "Corrupt Artist",
          album: "Glitchy Album",
          path: "/music/Corrupt Artist/Glitchy Album/Broken Track.flac",
          format: "FLAC",
          error_detail: "FLAC stream checksum mismatch (corrupted audio frame)",
          verified_at: 1700000000,
        },
      ],
    };

    vi.mocked(invoke).mockImplementation((cmd: string, args: any) => {
      if (cmd === "get_curation_overview") {
        return Promise.resolve(overviewCorrupt);
      }
      if (cmd === "locate_song_in_files" && args?.songId === 77) {
        return Promise.resolve(mockTarget);
      }
      return Promise.resolve(null);
    });

    const { findByTestId, findByText } = render(CurationShelf);

    expect(await findByTestId("curation-corrupt-section")).toBeInTheDocument();
    expect(await findByText("Broken Track")).toBeInTheDocument();
    expect(
      await findByText("FLAC stream checksum mismatch (corrupted audio frame)"),
    ).toBeInTheDocument();

    const viewFilesBtn = await findByText("View in Files");
    await fireEvent.click(viewFilesBtn);

    await waitFor(() => {
      expect(openInFilesSpy).toHaveBeenCalledWith(mockTarget);
    });
  });
});
