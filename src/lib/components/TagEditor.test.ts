import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import TagEditor from "./TagEditor.svelte";
import { invoke } from "@tauri-apps/api/core";
import type { SongTagHistory } from "../types";
import { formatDate } from "../utils/formatters";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("TagEditor.svelte", () => {
  const mockSongDetails = {
    id: 10,
    path: "/music/rock/song.flac",
    title: "Original Title",
    artist: "Original Artist",
    album: "Original Album",
    album_artist: "Original Album Artist",
    composer: "Original Composer",
    genre: "Rock",
    language: "en",
    track: 3,
    disc: 1,
    year: 2020,
    originalyear: null,
    grouping: "Original Grouping",
    bpm: 120,
    initial_key: "Cmaj",
    rating: 3,
    compilation: false,
    art_embedded: false,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") return mockSongDetails;
      if (cmd === "save_song_tags") return null;
      if (cmd === "clear_song_cover_art") return null;
      if (cmd === "set_song_rating") return 5;
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      if (cmd === "get_all_artist_profiles" || cmd === "get_all_album_profiles") return [];
      if (cmd === "get_playlists") return [{ id: 1, name: "Queue", dynamic_enabled: false, created: 0, updated: 0, track_count: 0, is_queue: true }];
      return null;
    });
  });

  it("loads and populates initial song metadata fields", async () => {
    const onClose = vi.fn();
    const { getByLabelText, getByText } = render(TagEditor, { songId: 10, onClose });

    await waitFor(() => {
      expect(getByText("/music/rock/song.flac")).toBeInTheDocument();
    });

    const titleInput = getByLabelText("Song Title") as HTMLInputElement;
    const albumInput = getByLabelText("Album") as HTMLInputElement;

    expect(titleInput.value).toBe("Original Title");
    expect(albumInput.value).toBe("Original Album");
    // Artist, Album Artist, and Composer are chip inputs — the initial
    // values render as chips, not as the value of the (empty, ready-for-new-
    // input) labeled text field.
    expect(getByText("Original Artist")).toBeInTheDocument();
    expect(getByText("Original Composer")).toBeInTheDocument();
    expect((getByLabelText("Language") as HTMLInputElement).value).toBe("en");
  });

  it("shows a remote-source note for a WebDAV song and not for a local one (#682)", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") {
        return { ...mockSongDetails, path: "http://user:pass@127.0.0.1:8080/Music/song.mp3" };
      }
      return null;
    });
    const { findByText } = render(TagEditor, { songId: 10, onClose: vi.fn() });
    expect(
      await findByText(/changes are saved in Luminous only/i)
    ).toBeInTheDocument();
  });

  it("shows no remote-source note for a local song", async () => {
    const { findByText, queryByText } = render(TagEditor, { songId: 10, onClose: vi.fn() });
    await findByText("/music/rock/song.flac");
    expect(queryByText(/changes are saved in Luminous only/i)).not.toBeInTheDocument();
  });

  it("hides the Clear Artwork button for a WebDAV song even with embedded art (#682)", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") {
        return {
          ...mockSongDetails,
          path: "http://user:pass@127.0.0.1:8080/Music/song.mp3",
          art_embedded: true,
        };
      }
      return null;
    });
    const { findByText, queryByRole } = render(TagEditor, { songId: 10, onClose: vi.fn() });
    await findByText(/changes are saved in Luminous only/i);
    expect(queryByRole("button", { name: /clear artwork/i })).not.toBeInTheDocument();
  });

  it("shows a read-only Various Artists pill instead of the Album Artist input when the song is part of a compilation", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") {
        return { ...mockSongDetails, album_artist: "Various Artists", compilation: true };
      }
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });

    const onClose = vi.fn();
    const { getByText, queryByLabelText } = render(TagEditor, { songId: 10, onClose });

    await waitFor(() => {
      expect(getByText("Various Artists")).toBeInTheDocument();
    });
    expect(queryByLabelText("Album Artist")).not.toBeInTheDocument();
  });

  it("shows the real Album Artist in the read-only pill for a compilation-flagged track whose artist isn't literally Various Artists", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") {
        return { ...mockSongDetails, album_artist: "Artist A", compilation: true };
      }
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });

    const onClose = vi.fn();
    const { getByText, queryByText, queryByLabelText } = render(TagEditor, { songId: 10, onClose });

    await waitFor(() => {
      expect(getByText("Artist A")).toBeInTheDocument();
    });
    expect(queryByText("Various Artists")).not.toBeInTheDocument();
    expect(queryByLabelText("Album Artist")).not.toBeInTheDocument();
  });

  it("calls onClose when cancel button is clicked", async () => {
    const onClose = vi.fn();
    const { getByRole } = render(TagEditor, { songId: 10, onClose });

    await waitFor(() => {
      expect(getByRole("button", { name: "Cancel" })).toBeInTheDocument();
    });

    await fireEvent.click(getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalled();
  });

  it("invokes save_song_tags with modified fields when Save Tags is clicked", async () => {
    const onClose = vi.fn();
    const onSave = vi.fn();
    const { getByLabelText, getByRole } = render(TagEditor, { songId: 10, onClose, onSave });

    await waitFor(() => {
      expect(getByLabelText("Song Title")).toBeInTheDocument();
    });

    const titleInput = getByLabelText("Song Title") as HTMLInputElement;
    await fireEvent.input(titleInput, { target: { value: "Updated Track Title" } });

    const saveBtn = getByRole("button", { name: /save tags/i });
    await fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_song_tags", expect.objectContaining({
        songId: 10,
        title: "Updated Track Title",
      }));
      expect(onSave).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("adds a chip to the Artist field and saves it delimited alongside the original", async () => {
    const onClose = vi.fn();
    const onSave = vi.fn();
    const { getByLabelText, getByRole, getByText } = render(TagEditor, { songId: 10, onClose, onSave });

    await waitFor(() => {
      expect(getByText("Original Artist")).toBeInTheDocument();
    });

    const artistInput = getByLabelText("Artist") as HTMLInputElement;
    await fireEvent.input(artistInput, { target: { value: "Second Artist" } });
    await fireEvent.keyDown(artistInput, { key: "Enter" });

    expect(getByText("Second Artist")).toBeInTheDocument();

    const saveBtn = getByRole("button", { name: /save tags/i });
    await fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_song_tags", expect.objectContaining({
        songId: 10,
        artist: "Original Artist; Second Artist",
      }));
      expect(onSave).toHaveBeenCalled();
      expect(onClose).toHaveBeenCalled();
    });
  });

  it("disables the Clear Artwork button when the song has no embedded art", async () => {
    const onClose = vi.fn();
    const { getByRole } = render(TagEditor, { songId: 10, onClose });

    await waitFor(() => {
      expect(getByRole("button", { name: /clear artwork/i })).toBeDisabled();
    });
  });

  it("clears embedded artwork after confirming, and disables the button afterward", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") return { ...mockSongDetails, art_embedded: true };
      if (cmd === "clear_song_cover_art") return null;
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });

    const onClose = vi.fn();
    const { getByRole, getAllByRole } = render(TagEditor, { songId: 10, onClose });

    const clearBtn = await waitFor(() => {
      const btn = getByRole("button", { name: /clear artwork/i });
      expect(btn).not.toBeDisabled();
      return btn;
    });

    await fireEvent.click(clearBtn);

    // The confirm dialog's own "Clear Artwork" button is now on top of the
    // (still-rendered, still-disabled-pending) trigger button, so there are
    // two matches — the confirm dialog's is the one added last.
    const confirmBtn = await waitFor(() => {
      const btns = getAllByRole("button", { name: "Clear Artwork" });
      expect(btns).toHaveLength(2);
      return btns[btns.length - 1];
    });
    await fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("clear_song_cover_art", { songId: 10 });
    });

    await waitFor(() => {
      expect(getByRole("button", { name: /clear artwork/i })).toBeDisabled();
    });

    // The modal itself stays open (unlike Save), so the user can keep editing.
    expect(onClose).not.toHaveBeenCalled();
  });

  it("loads and saves metadata for an unindexed file by path (#1887)", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_file_song_details") return { ...mockSongDetails, id: 0, path: "C:/Music/unscanned.flac", title: "Unscanned Track" };
      if (cmd === "save_file_tags") return null;
      if (cmd === "scan_source_folder") return null;
      return null;
    });

    const onClose = vi.fn();
    const onSave = vi.fn();
    const { getByLabelText, getByRole } = render(TagEditor, {
      filePath: "C:/Music/unscanned.flac",
      source: { kind: "folder", id: 1 },
      folderPath: ["Music"],
      folderName: "Music",
      fileName: "unscanned.flac",
      onClose,
      onSave,
    });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_file_song_details", { path: "C:/Music/unscanned.flac" });
    });

    const titleInput = getByLabelText("Song Title") as HTMLInputElement;
    expect(titleInput.value).toBe("Unscanned Track");

    await fireEvent.input(titleInput, { target: { value: "Updated Unscanned Track" } });
    await fireEvent.click(getByRole("button", { name: /save/i }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_file_tags", expect.objectContaining({
        path: "C:/Music/unscanned.flac",
        title: "Updated Unscanned Track",
      }));
    });
    expect(invoke).toHaveBeenCalledWith("scan_source_folder", {
      source: { kind: "folder", id: 1 },
      path: ["Music"],
    });
    expect(onClose).toHaveBeenCalled();
  });

  it("renders History section with source facts and empty state when entries is empty", async () => {
    const mockHistory: SongTagHistory = {
      songId: 10,
      source: {
        path: "/music/rock/song.flac",
        filesize: 12582912,
        format: "FLAC",
        added: 1700000000,
        isRemote: false,
        isCueTrack: false,
      },
      entries: [],
    };

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") return mockSongDetails;
      if (cmd === "get_song_tag_history") return mockHistory;
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });

    const { getByText, findByText } = render(TagEditor, { songId: 10, onClose: vi.fn() });

    await findByText("History");
    expect(getByText("Source")).toBeInTheDocument();
    expect(getByText("FLAC")).toBeInTheDocument();
    expect(getByText("12.0 MB")).toBeInTheDocument();
    expect(getByText(new RegExp(`No recorded changes since ${formatDate(1700000000)}`))).toBeInTheDocument();
  });

  it("renders luminous and scan change events with old_value → new_value and batch note", async () => {
    const mockHistory: SongTagHistory = {
      songId: 10,
      source: {
        path: "/music/rock/song.flac",
        filesize: 12582912,
        format: "FLAC",
        added: 1700000000,
        isRemote: false,
        isCueTrack: false,
      },
      entries: [
        {
          id: 1,
          songId: 10,
          at: 1700001000,
          actor: "luminous",
          field: "genre",
          oldValue: "Classic Rock",
          newValue: "Hard Rock",
          batchId: "batch-1",
          batchOtherTracks: 2,
        },
        {
          id: 2,
          songId: 10,
          at: 1700002000,
          actor: "scan",
          field: "title",
          oldValue: null,
          newValue: "New Song Title",
          batchId: null,
          batchOtherTracks: 0,
        },
      ],
    };

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") return mockSongDetails;
      if (cmd === "get_song_tag_history") return mockHistory;
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });

    const { getByText, findByText } = render(TagEditor, { songId: 10, onClose: vi.fn() });

    await findByText("History");
    expect(getByText("Luminous")).toBeInTheDocument();
    expect(getByText("Detected on scan")).toBeInTheDocument();
    expect(getByText("Classic Rock")).toBeInTheDocument();
    expect(getByText("Hard Rock")).toBeInTheDocument();
    expect(getByText("(empty)")).toBeInTheDocument();
    expect(getByText("New Song Title")).toBeInTheDocument();
    expect(getByText("edited with 2 other songs")).toBeInTheDocument();
    expect(getByText("Clear History")).toBeInTheDocument();
  });

  it("filters history entries when Detected on scan filter is toggled", async () => {
    const mockHistory: SongTagHistory = {
      songId: 10,
      source: {
        path: "/music/rock/song.flac",
        filesize: 12582912,
        format: "FLAC",
        added: 1700000000,
        isRemote: false,
        isCueTrack: false,
      },
      entries: [
        {
          id: 1,
          songId: 10,
          at: 1700001000,
          actor: "luminous",
          field: "genre",
          oldValue: "Classic Rock",
          newValue: "Hard Rock",
          batchId: null,
          batchOtherTracks: 0,
        },
        {
          id: 2,
          songId: 10,
          at: 1700002000,
          actor: "scan",
          field: "title",
          oldValue: "Old Title",
          newValue: "New Song Title",
          batchId: null,
          batchOtherTracks: 0,
        },
      ],
    };

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") return mockSongDetails;
      if (cmd === "get_song_tag_history") return mockHistory;
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });

    const { getByText, findByText, queryByText } = render(TagEditor, { songId: 10, onClose: vi.fn() });

    await findByText("History");
    expect(getByText("All (2)")).toBeInTheDocument();
    expect(getByText("Detected on scan (1)")).toBeInTheDocument();
    expect(getByText("Hard Rock")).toBeInTheDocument();
    expect(getByText("New Song Title")).toBeInTheDocument();

    // Click "Detected on scan (1)" filter
    await fireEvent.click(getByText("Detected on scan (1)"));

    // Luminous entry should no longer be visible; Scan entry remains
    expect(queryByText("Hard Rock")).not.toBeInTheDocument();
    expect(getByText("New Song Title")).toBeInTheDocument();

    // Click "All (2)" filter
    await fireEvent.click(getByText("All (2)"));
    expect(getByText("Hard Rock")).toBeInTheDocument();
    expect(getByText("New Song Title")).toBeInTheDocument();
  });

  it("shows filter empty message when Detected on scan (0) is selected and allows switching back", async () => {
    const mockHistory: SongTagHistory = {
      songId: 10,
      source: {
        path: "/music/rock/song.flac",
        filesize: 12582912,
        format: "FLAC",
        added: 1700000000,
        isRemote: false,
        isCueTrack: false,
      },
      entries: [
        {
          id: 1,
          songId: 10,
          at: 1700001000,
          actor: "luminous",
          field: "genre",
          oldValue: "Classic Rock",
          newValue: "Hard Rock",
          batchId: null,
          batchOtherTracks: 0,
        },
      ],
    };

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") return mockSongDetails;
      if (cmd === "get_song_tag_history") return mockHistory;
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });

    const { getByText, findByText, queryByText } = render(TagEditor, { songId: 10, onClose: vi.fn() });

    await findByText("History");
    expect(getByText("All (1)")).toBeInTheDocument();
    expect(getByText("Detected on scan (0)")).toBeInTheDocument();
    expect(getByText("Hard Rock")).toBeInTheDocument();

    // Click "Detected on scan (0)" filter
    await fireEvent.click(getByText("Detected on scan (0)"));
    expect(queryByText("Hard Rock")).not.toBeInTheDocument();
    expect(getByText("No changes detected on scan.")).toBeInTheDocument();

    // Click "All (1)" filter to return
    await fireEvent.click(getByText("All (1)"));
    expect(getByText("Hard Rock")).toBeInTheDocument();
  });

  it("invokes clear_song_tag_history on Clear History click and transitions to empty state", async () => {
    const mockHistory: SongTagHistory = {
      songId: 10,
      source: {
        path: "/music/rock/song.flac",
        filesize: 12582912,
        format: "FLAC",
        added: 1700000000,
        isRemote: false,
        isCueTrack: false,
      },
      entries: [
        {
          id: 1,
          songId: 10,
          at: 1700001000,
          actor: "luminous",
          field: "genre",
          oldValue: "Rock",
          newValue: "Hard Rock",
          batchId: null,
          batchOtherTracks: 0,
        },
      ],
    };

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") return mockSongDetails;
      if (cmd === "get_song_tag_history") return mockHistory;
      if (cmd === "clear_song_tag_history") return null;
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });

    const { getByText, findByText, queryByText } = render(TagEditor, { songId: 10, onClose: vi.fn() });

    await findByText("History");
    expect(getByText("Clear History")).toBeInTheDocument();

    await fireEvent.click(getByText("Clear History"));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("clear_song_tag_history", { songId: 10 });
    });

    await waitFor(() => {
      expect(getByText(new RegExp(`No recorded changes since ${formatDate(1700000000)}`))).toBeInTheDocument();
    });
    expect(queryByText("Clear History")).not.toBeInTheDocument();
  });

  it("hides History section for remote (WebDAV/Subsonic) and CUE tracks", async () => {
    // 1. Remote track
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") {
        return { ...mockSongDetails, path: "http://user:pass@127.0.0.1:8080/Music/song.mp3" };
      }
      return null;
    });

    const { queryByText, findByText, unmount } = render(TagEditor, { songId: 10, onClose: vi.fn() });
    await findByText(/changes are saved in Luminous only/i);
    expect(queryByText("History")).not.toBeInTheDocument();
    unmount();

    // 2. CUE track
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") {
        return { ...mockSongDetails, is_cue_track: true };
      }
      return null;
    });

    const cueRender = render(TagEditor, { songId: 10, onClose: vi.fn() });
    await cueRender.findByText(/cue sheet/i);
    expect(cueRender.queryByText("History")).not.toBeInTheDocument();
  });

  it("refreshes tag history when Tag Clean-Up sub-dialog is closed", async () => {
    let historyCallCount = 0;
    const initialHistory: SongTagHistory = {
      songId: 10,
      source: {
        path: "/music/rock/song.flac",
        filesize: 12582912,
        format: "FLAC",
        added: 1700000000,
        isRemote: false,
        isCueTrack: false,
      },
      entries: [],
    };

    const updatedHistory: SongTagHistory = {
      ...initialHistory,
      entries: [
        {
          id: 99,
          songId: 10,
          at: 1700002000,
          actor: "luminous",
          field: "comment",
          oldValue: "promo spam",
          newValue: null,
          batchId: null,
          batchOtherTracks: 0,
        },
      ],
    };

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") return mockSongDetails;
      if (cmd === "get_song_tag_history") {
        historyCallCount++;
        return historyCallCount === 1 ? initialHistory : updatedHistory;
      }
      if (cmd === "preview_tag_cleanup") {
        return { songs: [], skipped: 0 };
      }
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });

    const { getByText, findByText, getAllByRole } = render(TagEditor, { songId: 10, onClose: vi.fn() });
    await findByText("History");
    expect(historyCallCount).toBe(1);

    // Open Tag Clean-up sub-dialog
    const cleanUpBtn = getByText("Clean Up Tags…");
    await fireEvent.click(cleanUpBtn);

    // Close the sub-dialog via its close button
    await findByText("No tag clutter found.");
    const closeButtons = getAllByRole("button", { name: "Close" });
    await fireEvent.click(closeButtons[0]);

    await waitFor(() => {
      expect(historyCallCount).toBe(2);
    });
    expect(await findByText("promo spam")).toBeInTheDocument();
  });

  it("renders Restore button on entries and Restore All button on batch entries", async () => {
    const historyWithBatch: SongTagHistory = {
      songId: 10,
      source: {
        path: "/music/rock/song.flac",
        filesize: 12582912,
        format: "FLAC",
        added: 1700000000,
        isRemote: false,
        isCueTrack: false,
      },
      entries: [
        {
          id: 101,
          songId: 10,
          at: 1700001000,
          actor: "luminous",
          field: "album",
          oldValue: "Old Album",
          newValue: "Original Album",
          batchId: "batch-42",
          batchOtherTracks: 3,
        },
      ],
    };

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") return mockSongDetails;
      if (cmd === "get_song_tag_history") return historyWithBatch;
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });

    const { findByText, getByText } = render(TagEditor, { songId: 10, onClose: vi.fn() });
    await findByText("History");
    expect(getByText("Restore")).toBeInTheDocument();
    expect(getByText("Restore All")).toBeInTheDocument();
  });

  it("invokes restore_tag_history when Restore button is clicked", async () => {
    const historyEntry: SongTagHistory = {
      songId: 10,
      source: {
        path: "/music/rock/song.flac",
        filesize: 12582912,
        format: "FLAC",
        added: 1700000000,
        isRemote: false,
        isCueTrack: false,
      },
      entries: [
        {
          id: 102,
          songId: 10,
          at: 1700001000,
          actor: "luminous",
          field: "album",
          oldValue: "Previous Album",
          newValue: "Original Album",
          batchId: null,
          batchOtherTracks: 0,
        },
      ],
    };

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") return mockSongDetails;
      if (cmd === "get_song_tag_history") return historyEntry;
      if (cmd === "restore_tag_history") {
        return { restoredTracks: 1, unchangedTracks: 0, skippedTracks: 0, errors: [] };
      }
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });

    const { findByText, getByText } = render(TagEditor, { songId: 10, onClose: vi.fn() });
    await findByText("History");

    const restoreBtn = getByText("Restore");
    await fireEvent.click(restoreBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("restore_tag_history", {
        target: { type: "entry", historyId: 102 },
      });
    });
  });

  it("invokes restore_tag_history when Restore All button is clicked", async () => {
    const historyWithBatch: SongTagHistory = {
      songId: 10,
      source: {
        path: "/music/rock/song.flac",
        filesize: 12582912,
        format: "FLAC",
        added: 1700000000,
        isRemote: false,
        isCueTrack: false,
      },
      entries: [
        {
          id: 103,
          songId: 10,
          at: 1700001000,
          actor: "luminous",
          field: "album",
          oldValue: "Previous Album",
          newValue: "Original Album",
          batchId: "batch-xyz",
          batchOtherTracks: 2,
        },
      ],
    };

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") return mockSongDetails;
      if (cmd === "get_song_tag_history") return historyWithBatch;
      if (cmd === "restore_tag_history") {
        return { restoredTracks: 3, unchangedTracks: 0, skippedTracks: 0, errors: [] };
      }
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });

    const { findByText, getByText } = render(TagEditor, { songId: 10, onClose: vi.fn() });
    await findByText("History");

    const restoreBatchBtn = getByText("Restore All");
    await fireEvent.click(restoreBatchBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("restore_tag_history", {
        target: { type: "batch", batchId: "batch-xyz" },
      });
    });
  });

  it("does not show Restore All button when batchOtherTracks is 0", async () => {
    const singleHistory: SongTagHistory = {
      songId: 10,
      source: {
        path: "/music/rock/song.flac",
        filesize: 12582912,
        format: "FLAC",
        added: 1700000000,
        isRemote: false,
        isCueTrack: false,
      },
      entries: [
        {
          id: 104,
          songId: 10,
          at: 1700001000,
          actor: "luminous",
          field: "album",
          oldValue: "Initial Album",
          newValue: "Intermediate Album",
          batchId: "single-track-batch",
          batchOtherTracks: 0,
        },
      ],
    };

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") return mockSongDetails;
      if (cmd === "get_song_tag_history") return singleHistory;
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });

    const { findByText, queryByText } = render(TagEditor, { songId: 10, onClose: vi.fn() });
    await findByText("History");
    expect(queryByText("Restore All")).not.toBeInTheDocument();
    expect(queryByText("Restore")).toBeInTheDocument();
  });

  it("disables Restore button when field already holds the oldValue", async () => {
    // Current album is "Original Album". Entry has oldValue "Original Album"
    const historyEntry: SongTagHistory = {
      songId: 10,
      source: {
        path: "/music/rock/song.flac",
        filesize: 12582912,
        format: "FLAC",
        added: 1700000000,
        isRemote: false,
        isCueTrack: false,
      },
      entries: [
        {
          id: 105,
          songId: 10,
          at: 1700001000,
          actor: "luminous",
          field: "album",
          oldValue: "Original Album",
          newValue: "Another Album",
          batchId: null,
          batchOtherTracks: 0,
        },
      ],
    };

    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_song_details") return mockSongDetails;
      if (cmd === "get_song_tag_history") return historyEntry;
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      return null;
    });

    const { findByText, getByText } = render(TagEditor, { songId: 10, onClose: vi.fn() });
    await findByText("History");

    const restoreBtn = getByText("Restore").closest("button");
    expect(restoreBtn).toBeDisabled();
    expect(restoreBtn).toHaveAttribute("title", "Tag value is already current");
  });
});
