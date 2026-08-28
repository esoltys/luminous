import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import ArtistDetailView from "./ArtistDetailView.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((cmd: string, args?: any) => {
    if (cmd === "get_songs_by_artist") return Promise.resolve([]);
    if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
    if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
    if (cmd === "get_artist_profile") {
      return Promise.resolve({
        artist_key: args?.artist || "Shania Twain",
        website: "https://www.shaniatwain.com",
        tags: ["country", "canadian", "pop"],
        social_links: [
          { platform: "instagram", handle_or_url: "@shaniatwain" },
        ],
        bio: "Canadian music icon",
      });
    }
    return Promise.resolve();
  }),
}));

describe("ArtistDetailView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    collectionStore.artistProfiles = {
      "shania twain": {
        artist_key: "Shania Twain",
        website: "https://www.shaniatwain.com",
        tags: ["country", "canadian", "pop"],
        social_links: [
          { platform: "instagram", handle_or_url: "@shaniatwain" },
        ],
        bio: "Canadian music icon",
      },
    };
  });

  it("renders artist name and action buttons including Edit", async () => {
    render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

    expect(screen.getByText("Shania Twain")).toBeTruthy();
    expect(screen.getByRole("button", { name: /^Play$/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /Shuffle Play/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /Edit/i })).toBeTruthy();
  });

  it("renders Profile Card with About, Tags, Bio, and Links", async () => {
    render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

    expect(screen.getByText("About")).toBeTruthy();
    expect(screen.getByText("country")).toBeTruthy();
    expect(screen.getByText("canadian")).toBeTruthy();
    expect(screen.getByText("pop")).toBeTruthy();
    expect(screen.getByText("Canadian music icon")).toBeTruthy();
    expect(screen.getByText("LINKS")).toBeTruthy();
    expect(screen.getByText("shaniatwain.com")).toBeTruthy();
    expect(screen.getByText("Instagram")).toBeTruthy();
  });

  it("clicking tag sets search query to artist-tag filter", async () => {
    render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

    const tagBtn = screen.getByText("canadian");
    await fireEvent.click(tagBtn);

    expect(collectionStore.searchQuery).toBe("artist-tag:canadian");
    expect(navigationStore.activeSubTab).toBe("artists");
  });

  it("clicking Edit button opens the ArtistProfileEditor modal", async () => {
    render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

    const editBtn = screen.getByRole("button", { name: /Edit/i });
    await fireEvent.click(editBtn);

    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("renders genre chips when artist has songs with multi-value genres", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockImplementation((cmd: string, args?: any) => {
      if (cmd === "get_songs_by_artist") {
        return Promise.resolve([
          { id: 1, title: "Song 1", artist: "Shania Twain", genre: "Country; Pop", length_nanosec: 180_000_000_000 } as any,
          { id: 2, title: "Song 2", artist: "Shania Twain", genre: "Country; Rock", length_nanosec: 200_000_000_000 } as any,
        ]);
      }
      if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
      if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
      if (cmd === "get_artist_profile") return Promise.resolve(null as any);
      return Promise.resolve();
    });

    render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

    // "Country", "Pop", "Rock" should be rendered as chips
    const countryChip = await screen.findByTitle("Browse Country");
    expect(countryChip).toBeTruthy();
    expect(screen.getByTitle("Browse Pop")).toBeTruthy();
    expect(screen.getByTitle("Browse Rock")).toBeTruthy();

    await fireEvent.click(countryChip);
    expect(navigationStore.selectedAutoPlaylist?.genre).toBe("Country");
    expect(navigationStore.activeTab).toBe("playlists");
  });

  it("renders Unknown genre when artist songs have no genre", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockImplementation((cmd: string, args?: any) => {
      if (cmd === "get_songs_by_artist") {
        return Promise.resolve([
          { id: 1, title: "Song 1", artist: "Shania Twain", genre: "", length_nanosec: 180_000_000_000 } as any,
        ]);
      }
      if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
      if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
      if (cmd === "get_artist_profile") return Promise.resolve(null as any);
      return Promise.resolve();
    });

    render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

    expect(await screen.findByText("Unknown genre")).toBeTruthy();
  });
});
