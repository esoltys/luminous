import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import TagEditor from "./TagEditor.svelte";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

// Polyfill element.animate for jsdom environment used in Svelte transitions/animations
if (typeof Element !== "undefined" && !Element.prototype.animate) {
  Element.prototype.animate = vi.fn().mockReturnValue({
    finished: Promise.resolve(),
    cancel: () => {},
  }) as any;
}

describe("TagEditor.svelte", () => {
  const mockSongDetails = {
    id: 10,
    path: "/music/rock/song.flac",
    title: "Original Title",
    artist: "Original Artist",
    album: "Original Album",
    album_artist: "Original Artist",
    composer: "Original Composer",
    genre: "Rock",
    track: 3,
    disc: 1,
    year: 2020,
    rating: 3,
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
      if (cmd === "set_song_rating") return 5;
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
    const artistInput = getByLabelText("Artist") as HTMLInputElement;
    const albumInput = getByLabelText("Album") as HTMLInputElement;

    expect(titleInput.value).toBe("Original Title");
    expect(artistInput.value).toBe("Original Artist");
    expect(albumInput.value).toBe("Original Album");
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
