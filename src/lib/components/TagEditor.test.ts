import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import TagEditor from "./TagEditor.svelte";
import { invoke } from "@tauri-apps/api/core";

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
      if (cmd === "lookup_acoustid_tags") {
        return {
          title: "Fetched Title",
          artist: "Fetched Artist",
          album: "Fetched Album",
          year: 2021,
        };
      }
      if (cmd === "save_song_tags") return null;
      if (cmd === "clear_song_cover_art") return null;
      if (cmd === "set_song_rating") return 5;
      if (cmd === "get_library_snapshot") return { songs: [], albums: [], artists: [] };
      if (cmd === "get_all_artist_profiles") return [];
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

  it("handles AcoustID fingerprint lookup to suggest tags", async () => {
    const onClose = vi.fn();
    const { getByRole, getByLabelText } = render(TagEditor, { songId: 10, onClose });

    await waitFor(() => {
      expect(getByRole("button", { name: /lookup acoustid/i })).toBeInTheDocument();
    });

    const lookupBtn = getByRole("button", { name: /lookup acoustid/i });
    await fireEvent.click(lookupBtn);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("lookup_acoustid_tags", { songId: 10 });
      const titleInput = getByLabelText("Song Title") as HTMLInputElement;
      expect(titleInput.value).toBe("Fetched Title");
    });
  });
});
