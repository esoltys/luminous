import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import DuplicateReviewModal from "./DuplicateReviewModal.svelte";
import { invoke } from "@tauri-apps/api/core";
import type { DuplicateTrackGroup } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("DuplicateReviewModal.svelte", () => {
  const mockGroups: DuplicateTrackGroup[] = [
    {
      group_key: "dup-radiohead-airbag-1",
      title: "Airbag",
      artist: "Radiohead",
      album: "OK Computer",
      candidates: [
        {
          song: {
            id: 1,
            path: "/music/Radiohead/OK Computer/01 - Airbag.flac",
            title: "Airbag",
            artist: "Radiohead",
            album: "OK Computer",
            track: 1,
            disc: 1,
            year: 1997,
            filetype: "FLAC",
            bitdepth: 16,
            samplerate: 44100,
            filesize: 30_000_000,
            length_nanosec: 284_000_000_000,
            rating: 0.8,
            loved: 0,
            playcount: 10,
            added: 1600000000,
            source: "local_file",
            art_embedded: true,
          } as any,
          playlist_count: 1,
          tags: ["Alternative Rock"],
          provenance: "authentic",
        },
        {
          song: {
            id: 2,
            path: "/music/Radiohead/OK Computer/Duplicates/01 - Airbag.mp3",
            title: "Airbag",
            artist: "Radiohead",
            album: "OK Computer",
            track: 1,
            disc: 1,
            year: 1997,
            filetype: "MP3",
            bitrate: 320,
            filesize: 11_000_000,
            length_nanosec: 284_000_000_000,
            rating: 1.0, // 5★ rating upgrade!
            loved: 1, // favourite upgrade!
            playcount: 25, // +25 plays carry-over!
            added: 1500000000,
            source: "local_file",
            art_embedded: false,
          } as any,
          playlist_count: 2, // 2 playlist spots!
          tags: ["Alternative Rock", "90s"], // "90s" tag carry-over!
          provenance: null,
        },
      ],
    },
    {
      group_key: "dup-radiohead-paranoid-android-3",
      title: "Paranoid Android",
      artist: "Radiohead",
      album: "OK Computer",
      candidates: [
        {
          song: {
            id: 3,
            path: "/music/Radiohead/OK Computer/02 - Paranoid Android.flac",
            title: "Paranoid Android",
            artist: "Radiohead",
            album: "OK Computer",
            track: 2,
            disc: 1,
            year: 1997,
            filetype: "FLAC",
            source: "local_file",
          } as any,
          playlist_count: 0,
          tags: [],
          provenance: null,
        },
        {
          song: {
            id: 4,
            path: "/music/Radiohead/OK Computer/Duplicates/02 - Paranoid Android.mp3",
            title: "Paranoid Android",
            artist: "Radiohead",
            album: "OK Computer",
            track: 2,
            disc: 1,
            year: 1997,
            filetype: "MP3",
            source: "local_file",
          } as any,
          playlist_count: 0,
          tags: [],
          provenance: null,
        },
      ],
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_duplicate_track_groups") return mockGroups;
      if (cmd === "resolve_duplicate_group") return null;
      if (cmd === "locate_song_in_files") return null;
      return null;
    });
  });

  it("renders side-by-side pressing cards and carry-over reassurance", async () => {
    const onClose = vi.fn();
    const { getByText, getAllByText } = render(DuplicateReviewModal, {
      groups: mockGroups,
      onClose,
    });

    // Check title and candidates
    expect(getAllByText("Airbag").length).toBeGreaterThan(0);
    expect(getByText("/music/Radiohead/OK Computer/01 - Airbag.flac")).toBeInTheDocument();
    expect(getByText("/music/Radiohead/OK Computer/Duplicates/01 - Airbag.mp3")).toBeInTheDocument();

    // Candidate 1 is FLAC and recommended by default
    expect(getByText("Recommended")).toBeInTheDocument();
    expect(getByText("FLAC")).toBeInTheDocument();
    expect(getByText("MP3")).toBeInTheDocument();

    // Reassurance summary should display carried over plays, rating, favourite, and tags
    expect(getByText("+25 plays from retired copies")).toBeInTheDocument();
    expect(getByText("Upgraded rating (5.0★)")).toBeInTheDocument();
    expect(getByText("Favourite heart will carry over")).toBeInTheDocument();
    expect(getByText("Favourite")).toBeInTheDocument(); // Favourite heart badge on candidate 2
    expect(getByText("2 playlist spots")).toBeInTheDocument();
    expect(getByText("Additional tags: 90s")).toBeInTheDocument();
  });

  it("switches selected edition when clicking the second card", async () => {
    const onClose = vi.fn();
    const { getByText, queryByText } = render(DuplicateReviewModal, {
      groups: mockGroups,
      onClose,
    });

    // Currently Candidate 1 is selected
    // Click candidate 2 (MP3 card)
    const mp3Path = getByText("/music/Radiohead/OK Computer/Duplicates/01 - Airbag.mp3");
    await fireEvent.click(mp3Path);

    // Now Candidate 2 is selected to preserve
    // Preserving MP3 will now carry over stats from FLAC:
    // FLAC had 10 plays, rating 0.8 (4.0★), 1 playlist spot
    expect(getByText("+10 plays from retired copies")).toBeInTheDocument();
    expect(getByText("1 playlist spot")).toBeInTheDocument();
    // MP3 already had rating 1.0 (5.0★), loved 1, and tag "90s", so no rating/loved upgrade from FLAC
    expect(queryByText("Favourite heart will carry over")).not.toBeInTheDocument();
  });

  it("navigates across multiple duplicate groups", async () => {
    const onClose = vi.fn();
    const { getByText, getAllByText, getByTitle } = render(DuplicateReviewModal, {
      groups: mockGroups,
      onClose,
    });

    expect(getByText("Duplicate 1 of 2")).toBeInTheDocument();

    // Click next group
    const nextBtn = getByTitle("Next duplicate");
    await fireEvent.click(nextBtn);

    expect(getByText("Duplicate 2 of 2")).toBeInTheDocument();
    expect(getAllByText("Paranoid Android").length).toBeGreaterThan(0);
  });

  it("focuses initialSongId group when provided", async () => {
    const onClose = vi.fn();
    const { getByText, getAllByText } = render(DuplicateReviewModal, {
      groups: mockGroups,
      initialSongId: 3, // Song in group 2
      onClose,
    });

    expect(getByText("Duplicate 2 of 2")).toBeInTheDocument();
    expect(getAllByText("Paranoid Android").length).toBeGreaterThan(0);
  });

  it("calls onEditTags when Keep Both is clicked", async () => {
    const onClose = vi.fn();
    const onEditTags = vi.fn();
    const { getByText } = render(DuplicateReviewModal, {
      groups: mockGroups,
      onClose,
      onEditTags,
    });

    const keepBothBtn = getByText("Keep Both — Edit Tags");
    await fireEvent.click(keepBothBtn);

    expect(onEditTags).toHaveBeenCalledWith(1);
  });

  it("opens ConfirmDialog and resolves duplicate via IPC when confirmed", async () => {
    const onClose = vi.fn();
    const onResolved = vi.fn();
    const { getByText } = render(DuplicateReviewModal, {
      groups: [mockGroups[0]], // Single group
      onClose,
      onResolved,
    });

    // Click "Preserve This Edition"
    const preserveBtn = getByText("Preserve This Edition");
    await fireEvent.click(preserveBtn);

    // Confirm dialog should appear
    expect(getByText("Confirm Edition Preservation")).toBeInTheDocument();

    // Confirm dialog confirm button
    const confirmBtn = getByText("Preserve Edition");
    await fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("resolve_duplicate_group", {
        keepSongId: 1,
        removeSongIds: [2],
        deleteFilesFromDisk: true,
      });
      expect(onResolved).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });
});
