import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import ArtistDetailView from "./ArtistDetailView.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { picardStore } from "../stores/picard.svelte";
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

  it("renders artist name and action buttons including overflow menu", async () => {
    render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

    expect(screen.getByText("Shania Twain")).toBeTruthy();
    expect(screen.getByRole("button", { name: /^Play$/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /Shuffle Play/i })).toBeTruthy();
    expect(screen.getByTitle("More actions")).toBeTruthy();
  });

  it("renders Profile Card with Tags, Bio, and Links", async () => {
    render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

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

  it("clicking Edit Artist Details in overflow menu opens the ArtistProfileEditor modal", async () => {
    render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

    const moreBtn = screen.getByTitle("More actions");
    await fireEvent.click(moreBtn);

    const editBtn = screen.getByText("Edit Artist Details");
    await fireEvent.click(editBtn);

    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("clicking Open All in Picard in overflow menu invokes open_in_picard", async () => {
    picardStore.path = "/mock/picard";
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockImplementation((cmd: string, args?: any) => {
      if (cmd === "get_songs_by_artist") {
        return Promise.resolve([
          { id: 10, title: "Song 1", artist: "Shania Twain", genre: "Country", length_nanosec: 180_000_000_000 } as any,
          { id: 11, title: "Song 2", artist: "Shania Twain", genre: "Pop", length_nanosec: 200_000_000_000 } as any,
        ]);
      }
      if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
      if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
      if (cmd === "get_artist_profile") return Promise.resolve(null as any);
      return Promise.resolve();
    });

    render(ArtistDetailView, { props: { artistName: "Shania Twain" } });
    await new Promise((resolve) => setTimeout(resolve, 50));

    const moreBtn = screen.getByTitle("More actions");
    await fireEvent.click(moreBtn);

    const picardBtn = screen.getByText("Open All in Picard");
    await fireEvent.click(picardBtn);

    expect(invokeMock).toHaveBeenCalledWith("open_in_picard", { songIds: [10, 11] });
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

    // Verify genre container is distinct from the metadata container with songs count
    const genreContainer = countryChip.closest("div.flex.flex-wrap.gap-1");
    expect(genreContainer).toBeTruthy();

    const songsText = screen.getByText(/2 songs/i);
    expect(songsText).toBeTruthy();
    expect(genreContainer?.contains(songsText)).toBe(false);

    const metadataRow = songsText.closest("div.flex.flex-wrap.items-center");
    expect(metadataRow).toBeTruthy();
    expect(metadataRow?.firstElementChild).toBe(songsText);

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

  it("limits header genre chips to 4 and shows overflow badge when artist has many genres (#817)", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockImplementation((cmd: string, args?: any) => {
      if (cmd === "get_songs_by_artist") {
        return Promise.resolve([
          {
            id: 1,
            title: "Song 1",
            artist: "Nightwish",
            genre: "Metal; Symphonic Metal; Gothic Metal; Power Metal; Heavy Metal; Pop Rock",
            length_nanosec: 180_000_000_000,
          } as any,
        ]);
      }
      if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
      if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
      if (cmd === "get_artist_profile") return Promise.resolve(null as any);
      return Promise.resolve();
    });

    render(ArtistDetailView, { props: { artistName: "Nightwish" } });

    expect(await screen.findByTitle("Browse Gothic Metal")).toBeTruthy();
    expect(screen.getByTitle("Browse Heavy Metal")).toBeTruthy();
    expect(screen.getByTitle("Browse Metal")).toBeTruthy();
    expect(screen.getByTitle("Browse Pop Rock")).toBeTruthy();

    expect(screen.queryByTitle("Browse Power Metal")).toBeNull();
    expect(screen.queryByTitle("Browse Symphonic Metal")).toBeNull();
    const badge = screen.getByText("+2");
    expect(badge).toBeTruthy();
    expect(badge.getAttribute("title")).toBe("Power Metal, Symphonic Metal");
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

  describe("biography 'Show more' truncation", () => {
    const longBio = "Shania Twain is a Canadian singer and songwriter. She has sold over 100 million records, making her the best-selling female artist in country music history and one of the best-selling music artists of all time. Her success garnered her several titles including the Queen of Country Pop.";

    it("does not show 'Show more' when the bio text is not truncated even if length > 200", async () => {
      const invokeMock = vi.mocked(invoke);
      invokeMock.mockImplementation((cmd: string, args?: any) => {
        if (cmd === "get_songs_by_artist") return Promise.resolve([]);
        if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
        if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
        if (cmd === "get_artist_profile") {
          return Promise.resolve({
            artist_key: "Shania Twain",
            website: "https://www.shaniatwain.com",
            tags: ["country"],
            social_links: [],
            bio: longBio,
          });
        }
        return Promise.resolve();
      });
      collectionStore.artistProfiles = {
        "shania twain": {
          artist_key: "Shania Twain",
          website: "https://www.shaniatwain.com",
          tags: ["country"],
          social_links: [],
          bio: longBio,
        },
      };

      render(ArtistDetailView, { props: { artistName: "Shania Twain" } });
      await new Promise((resolve) => setTimeout(resolve, 50));

      expect(screen.getByText(longBio)).toBeTruthy();
      expect(screen.queryByRole("button", { name: "Show more" })).toBeNull();
      expect(screen.queryByRole("button", { name: "Show less" })).toBeNull();
    });

    it("shows 'Show more' when bio text is truncated, and toggles expansion on click", async () => {
      const invokeMock = vi.mocked(invoke);
      invokeMock.mockImplementation((cmd: string, args?: any) => {
        if (cmd === "get_songs_by_artist") return Promise.resolve([]);
        if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
        if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
        if (cmd === "get_artist_profile") {
          return Promise.resolve({
            artist_key: "Shania Twain",
            website: "https://www.shaniatwain.com",
            tags: ["country"],
            social_links: [],
            bio: longBio,
          });
        }
        return Promise.resolve();
      });
      collectionStore.artistProfiles = {
        "shania twain": {
          artist_key: "Shania Twain",
          website: "https://www.shaniatwain.com",
          tags: ["country"],
          social_links: [],
          bio: longBio,
        },
      };

      // Mock scrollHeight to exceed the available room baseline (~140px)
      Object.defineProperty(HTMLParagraphElement.prototype, "scrollHeight", {
        configurable: true,
        get: () => 300,
      });

      try {
        render(ArtistDetailView, { props: { artistName: "Shania Twain" } });
        await new Promise((resolve) => setTimeout(resolve, 50));

        const showMoreBtn = screen.getByRole("button", { name: "Show more" });
        expect(showMoreBtn).toBeTruthy();

        await fireEvent.click(showMoreBtn);

        const showLessBtn = screen.getByRole("button", { name: "Show less" });
        expect(showLessBtn).toBeTruthy();

        await fireEvent.click(showLessBtn);
        expect(screen.getByRole("button", { name: "Show more" })).toBeTruthy();
      } finally {
        delete (HTMLParagraphElement.prototype as any).scrollHeight;
      }
    });

    it("does not show 'Show more' when bio fits within the available height of the Links column", async () => {
      const invokeMock = vi.mocked(invoke);
      invokeMock.mockImplementation((cmd: string, args?: any) => {
        if (cmd === "get_songs_by_artist") return Promise.resolve([]);
        if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
        if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
        if (cmd === "get_artist_profile") {
          return Promise.resolve({
            artist_key: "Shania Twain",
            website: "https://www.shaniatwain.com",
            tags: ["country"],
            social_links: [
              { platform: "threads", handle_or_url: "shaniatwain" },
              { platform: "instagram", handle_or_url: "shaniatwain" },
            ],
            bio: longBio,
          });
        }
        return Promise.resolve();
      });
      collectionStore.artistProfiles = {
        "shania twain": {
          artist_key: "Shania Twain",
          website: "https://www.shaniatwain.com",
          tags: ["country"],
          social_links: [
            { platform: "threads", handle_or_url: "shaniatwain" },
            { platform: "instagram", handle_or_url: "shaniatwain" },
          ],
          bio: longBio,
        },
      };

      // Bio is 160px, but card width is >= 768 and links column is 220px tall
      Object.defineProperty(HTMLParagraphElement.prototype, "scrollHeight", {
        configurable: true,
        get: () => 160,
      });
      Object.defineProperty(HTMLDivElement.prototype, "clientWidth", {
        configurable: true,
        get() {
          return 800;
        },
      });
      Object.defineProperty(HTMLDivElement.prototype, "offsetHeight", {
        configurable: true,
        get() {
          return 220;
        },
      });

      try {
        render(ArtistDetailView, { props: { artistName: "Shania Twain" } });
        await new Promise((resolve) => setTimeout(resolve, 50));

        expect(screen.queryByRole("button", { name: "Show more" })).toBeNull();
        expect(screen.queryByRole("button", { name: "Show less" })).toBeNull();
      } finally {
        delete (HTMLParagraphElement.prototype as any).scrollHeight;
        delete (HTMLDivElement.prototype as any).clientWidth;
        delete (HTMLDivElement.prototype as any).offsetHeight;
      }
    });
  });
});


