import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import CoverArt from "./CoverArt.svelte";
import { prefs } from "../stores/prefs.svelte";
import { collectionStore } from "../stores/collection.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  convertFileSrc: (path: string) => `asset://${path}`,
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onResized: vi.fn().mockResolvedValue(() => {}),
    onMoved: vi.fn().mockResolvedValue(() => {}),
  }),
}));

describe("CoverArt.svelte - fanart.tv cover fallback (#1277)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    prefs.onlineEnabled = true;
  });

  it("shows the fanart.tv cover once the iTunes lookup misses", async () => {
    let coverLookups = 0;
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_cover_art_uri") {
        // First lookup: nothing yet. After iTunes misses, the backend
        // resolves the song's fanart.tv cover as its fallback.
        return coverLookups++ === 0 ? null : "luminous-art://abc_fanart_cover.jpg";
      }
      return null;
    });

    const { container } = render(CoverArt, { props: { songId: 7 } });

    await vi.waitFor(() => {
      expect(container.querySelector("img")?.getAttribute("src")).toContain("abc_fanart_cover.jpg");
    });
    expect(invoke).toHaveBeenCalledWith("fetch_remote_cover", { songId: 7 });
  });

  it("shows the placeholder when neither iTunes nor fanart.tv has a cover", async () => {
    vi.mocked(invoke).mockResolvedValue(null);

    const { container } = render(CoverArt, { props: { songId: 7 } });

    await vi.waitFor(() => {
      expect(vi.mocked(invoke).mock.calls.filter(([c]) => c === "get_cover_art_uri")).toHaveLength(2);
    });
    expect(container.querySelector("img")).toBeNull();
  });

  it("never asks the backend to fetch a remote cover while Offline (#1398)", async () => {
    prefs.onlineEnabled = false;
    vi.mocked(invoke).mockResolvedValue(null);

    render(CoverArt, { props: { songId: 7 } });

    await vi.waitFor(() => {
      expect(vi.mocked(invoke).mock.calls.some(([c]) => c === "get_cover_art_uri")).toBe(true);
    });
    expect(vi.mocked(invoke).mock.calls.some(([c]) => c === "fetch_remote_cover")).toBe(false);
  });
});

describe("CoverArt.svelte - rescanned covers reload (#1755)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    collectionStore.coverArtVersion = 0;
  });

  it("changes the image URL when the cover-art version is bumped", async () => {
    const { container } = render(CoverArt, { props: { songId: 7, artAutomatic: "album-1.jpg" } });
    const src = () => container.querySelector("img")?.getAttribute("src");
    await vi.waitFor(() => expect(src()).toContain("album-1.jpg"));
    const before = src();

    collectionStore.coverArtVersion = 1;

    await vi.waitFor(() => expect(src()).not.toBe(before));
    expect(src()).toMatch(/[?&]v=1$/);
  });
});

describe("CoverArt.svelte - remote cover propagation (#1823)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    prefs.onlineEnabled = true;
    collectionStore.songs = [
      {
        id: 50,
        source: "web_dav",
        filetype: "FLAC",
        title: "Track 1",
        artist: "Remote Artist",
        album: "Remote Album",
        art_embedded: false,
        art_automatic: undefined,
        art_manual: undefined,
        art_unset: false,
        compilation: false,
        beginning_nanosec: 0,
        end_nanosec: 0,
        rating: 0,
        playcount: 0,
        skipcount: 0,
        unavailable: false,
      },
      {
        id: 51,
        source: "web_dav",
        filetype: "FLAC",
        title: "Track 2",
        artist: "Remote Artist",
        album: "Remote Album",
        art_embedded: false,
        art_automatic: undefined,
        art_manual: undefined,
        art_unset: false,
        compilation: false,
        beginning_nanosec: 0,
        end_nanosec: 0,
        rating: 0,
        playcount: 0,
        skipcount: 0,
        unavailable: false,
      },
    ];
    collectionStore.albums = [
      {
        album: "Remote Album",
        artist: "Remote Artist",
        year: 2024,
        track_count: 2,
        disc_count: 1,
        art_embedded: false,
        art_automatic: null,
        art_manual: null,
        rating: -1,
        total_duration_nanosec: 0,
        sample_song_id: 50,
      },
    ];
  });

  it("propagates fetched remote cover filename to collectionStore songs and albums", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_cover_art_uri") return null;
      if (cmd === "fetch_remote_cover") return "album-remote-123.jpg";
      return null;
    });

    const { container } = render(CoverArt, { props: { songId: 50 } });

    await vi.waitFor(() => {
      expect(container.querySelector("img")?.getAttribute("src")).toContain("album-remote-123.jpg");
    });

    expect(collectionStore.songs[0].art_automatic).toBe("album-remote-123.jpg");
    expect(collectionStore.songs[1].art_automatic).toBe("album-remote-123.jpg");
    expect(collectionStore.albums[0].art_automatic).toBe("album-remote-123.jpg");
  });
});
