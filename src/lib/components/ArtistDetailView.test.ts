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
    // Extended-artwork cache is on the singleton store — must not leak
    // between tests (#98/#761).
    collectionStore.extendedArtworkByArtist = {};
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

  describe("extended artist artwork (#98/#761)", () => {
    it("renders a discovered artist portrait and band logo instead of the album-art composite and text heading", async () => {
      const invokeMock = vi.mocked(invoke);
      invokeMock.mockImplementation((cmd: string, args?: any) => {
        if (cmd === "get_songs_by_artist") return Promise.resolve([]);
        if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
        if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
        if (cmd === "get_artist_profile") {
          return Promise.resolve({
            artist_key: args?.artist || "Shania Twain",
            website: "https://www.shaniatwain.com",
            tags: [],
            social_links: [],
            bio: null,
          });
        }
        if (cmd === "get_extended_artwork_for_artist") {
          return Promise.resolve({
            count: 2,
            primary_uri: null,
            artist_portrait_uri: "luminous-art://local/C:/Music/Shania Twain/artist.jpg",
            band_logo_uri: "luminous-art://local/C:/Music/Shania Twain/logo.png",
            fanart_uri: null,
            items: [],
          });
        }
        return Promise.resolve();
      });

      render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

      const images = await screen.findAllByAltText("Shania Twain");
      expect(images.length).toBe(2); // portrait + band logo
      expect(screen.queryByRole("heading", { name: "Shania Twain" })).toBeNull();
    });

    it("falls back to the plain text heading when no band logo was discovered", async () => {
      render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

      expect(await screen.findByRole("heading", { name: "Shania Twain" })).toBeTruthy();
    });

    it("shows a derived fanart.tv link when a MusicBrainz link with a recognizable MBID is present", async () => {
      const invokeMock = vi.mocked(invoke);
      invokeMock.mockImplementation((cmd: string, args?: any) => {
        if (cmd === "get_songs_by_artist") return Promise.resolve([]);
        if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
        if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
        if (cmd === "get_artist_profile") {
          return Promise.resolve({
            artist_key: args?.artist || "Shania Twain",
            website: null,
            tags: [],
            social_links: [
              { platform: "musicbrainz", handle_or_url: "https://musicbrainz.org/artist/7249b899-8db8-43e7-9e6e-22f1e736024e" },
            ],
            bio: null,
          });
        }
        return Promise.resolve();
      });
      collectionStore.artistProfiles = {
        "shania twain": {
          artist_key: "Shania Twain",
          tags: [],
          social_links: [
            { platform: "musicbrainz", handle_or_url: "https://musicbrainz.org/artist/7249b899-8db8-43e7-9e6e-22f1e736024e" },
          ],
        },
      };

      render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

      expect(await screen.findByText("Fanart.tv")).toBeTruthy();
    });
  });
});
