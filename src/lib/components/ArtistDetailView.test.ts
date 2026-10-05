import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import ArtistDetailView from "./ArtistDetailView.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { picardStore } from "../stores/picard.svelte";
import { windowLayoutStore } from "../stores/windowLayout.svelte";
import { prefs } from "../stores/prefs.svelte";
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
    windowLayoutStore.setOverviewExpanded(true);
  });

  it("renders artist name and action buttons including overflow menu", async () => {
    render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

    expect(screen.getByText("Shania Twain")).toBeTruthy();
    expect(screen.getByRole("button", { name: /^Play$/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^Shuffle$/i })).toBeTruthy();
    expect(screen.getByTitle("More actions")).toBeTruthy();
  });

  it("renders Profile Card with Tags, Bio, and Links", async () => {
    render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

    expect(screen.getByText("country")).toBeTruthy();
    expect(screen.getByText("canadian")).toBeTruthy();
    expect(screen.getByText("pop")).toBeTruthy();
    expect(screen.getByText("Canadian music icon")).toBeTruthy();
    expect(screen.getByText("shaniatwain.com")).toBeTruthy();
    expect(screen.getByText("Instagram")).toBeTruthy();
  });

  it("does not show blacklisted links (rateyourmusic.com, twitter.com, x.com) in Artist Info", async () => {
    collectionStore.artistProfiles = {
      "shania twain": {
        artist_key: "Shania Twain",
        website: "https://rateyourmusic.com/artist/shania-twain",
        tags: ["country"],
        social_links: [
          { platform: "other_databases", handle_or_url: "https://rateyourmusic.com/artist/shania-twain" },
          { platform: "twitter", handle_or_url: "https://twitter.com/shaniatwain" },
          { platform: "x", handle_or_url: "https://x.com/shaniatwain" },
          { platform: "instagram", handle_or_url: "@shaniatwain" },
        ],
        bio: "Bio",
      },
    };

    render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

    expect(screen.getByText("Instagram")).toBeTruthy();
    expect(screen.queryByText(/rateyourmusic/i)).toBeNull();
    expect(screen.queryByText(/twitter/i)).toBeNull();
    expect(screen.queryByText(/^x$/i)).toBeNull();
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

  it("clicking Refresh Artist in overflow menu invokes rescan_songs with artist songs, not full library scan", async () => {
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
      if (cmd === "rescan_songs") return Promise.resolve();
      if (cmd === "get_extended_artwork_for_artist") return Promise.resolve({} as any);
      return Promise.resolve();
    });

    const startScanSpy = vi.spyOn(collectionStore, "startScan");
    const refreshLibrarySpy = vi.spyOn(collectionStore, "refreshLibrary").mockResolvedValue(undefined as any);

    render(ArtistDetailView, { props: { artistName: "Shania Twain" } });
    await new Promise((resolve) => setTimeout(resolve, 50));

    const moreBtn = screen.getByTitle("More actions");
    await fireEvent.click(moreBtn);

    const refreshBtn = screen.getByText("Refresh Artist");
    expect(refreshBtn).toBeTruthy();
    await fireEvent.click(refreshBtn);

    expect(invokeMock).toHaveBeenCalledWith("rescan_songs", { songIds: [10, 11] });
    expect(refreshLibrarySpy).toHaveBeenCalled();
    expect(startScanSpy).not.toHaveBeenCalled();
  });

  it("never renders embedded genre chips in the header, even with multi-value genres", async () => {
    // The header used to mirror each song's embedded genre as its own chip
    // row, but that's redundant with the Genres page and every album/song
    // beneath this artist -- the header now shows curated artist-only tags
    // only, so with no artist profile there's nothing to show here at all.
    collectionStore.artistProfiles = {};
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

    const songsText = await screen.findByText(/2 songs/i);
    expect(songsText).toBeTruthy();

    expect(screen.queryByTitle("Browse Country")).toBeNull();
    expect(screen.queryByTitle("Browse Pop")).toBeNull();
    expect(screen.queryByTitle("Browse Rock")).toBeNull();
  });

  it("shows only artist-only curated tags in the header, hiding ones that duplicate an embedded genre", async () => {
    collectionStore.artistProfiles = {
      "shania twain": {
        artist_key: "Shania Twain",
        website: "https://www.shaniatwain.com",
        tags: ["Country Pop", "Canadian"],
        social_links: [],
      },
    };
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockImplementation((cmd: string, args?: any) => {
      if (cmd === "get_songs_by_artist") {
        return Promise.resolve([
          { id: 1, title: "Song 1", artist: "Shania Twain", genre: "Country; Canadian; Pop", length_nanosec: 180_000_000_000 } as any,
        ]);
      }
      if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
      if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
      if (cmd === "get_artist_profile") {
        return Promise.resolve(collectionStore.artistProfiles["shania twain"]);
      }
      if (cmd === "get_tags_overview") {
        // "Canadian" is a real genre elsewhere in the library, even though
        // it isn't part of the mocked getter above -- the filter checks
        // against every genre in the library, not just this artist's songs.
        return Promise.resolve({
          tags: [{ name: "Country", song_count: 1 }, { name: "Pop", song_count: 1 }, { name: "Canadian", song_count: 5 }],
          graph: [],
          no_genre_count: 0,
        });
      }
      return Promise.resolve();
    });

    render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

    // "Country Pop" isn't an embedded genre, so it's artist-only and shown.
    expect(await screen.findByTitle('Filter artists tagged "Country Pop"')).toBeTruthy();

    // "Canadian" duplicates a genre elsewhere in the library, so it's
    // excluded from the header once the global tag list has loaded.
    await waitFor(() => {
      expect(screen.queryByTitle('Filter artists tagged "Canadian"')).toBeNull();
    });

    // Embedded file genres never render here -- that's the Genres page's job.
    expect(screen.queryByTitle("Browse Country")).toBeNull();
    expect(screen.queryByTitle("Browse Pop")).toBeNull();

    // Clicking the artist-only tag sets artist-tag search query
    const curatedTag = screen.getByTitle('Filter artists tagged "Country Pop"');
    await fireEvent.click(curatedTag);
    expect(collectionStore.searchQuery).toBe("artist-tag:Country Pop");
    expect(navigationStore.activeTab).toBe("collection");
    expect(navigationStore.activeSubTab).toBe("artists");
  });

  it("shows every artist-only tag chip with no overflow limit when an artist has many", async () => {
    // Regression test: header chips used to cap at 4 with a "+N" overflow
    // badge (#817) -- there's room to show them all, so that cap was removed.
    collectionStore.artistProfiles = {
      nightwish: {
        artist_key: "Nightwish",
        tags: ["Symphonic Metal", "Gothic Metal", "Power Metal", "Heavy Metal", "Pop Rock", "Finnish"],
        social_links: [],
      },
    };
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockImplementation((cmd: string, args?: any) => {
      if (cmd === "get_songs_by_artist") {
        return Promise.resolve([
          { id: 1, title: "Song 1", artist: "Nightwish", genre: "Metal", length_nanosec: 180_000_000_000 } as any,
        ]);
      }
      if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
      if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
      if (cmd === "get_artist_profile") return Promise.resolve(collectionStore.artistProfiles["nightwish"]);
      return Promise.resolve();
    });

    render(ArtistDetailView, { props: { artistName: "Nightwish" } });

    expect(await screen.findByTitle('Filter artists tagged "Gothic Metal"')).toBeTruthy();
    expect(screen.getByTitle('Filter artists tagged "Heavy Metal"')).toBeTruthy();
    expect(screen.getByTitle('Filter artists tagged "Symphonic Metal"')).toBeTruthy();
    expect(screen.getByTitle('Filter artists tagged "Pop Rock"')).toBeTruthy();
    expect(screen.getByTitle('Filter artists tagged "Power Metal"')).toBeTruthy();
    expect(screen.getByTitle('Filter artists tagged "Finnish"')).toBeTruthy();
    expect(screen.queryByText("+2")).toBeNull();
  });

  describe("extended artist artwork (#98/#761)", () => {
    it("renders a discovered band logo beside the portrait while keeping the text heading", async () => {
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

      await waitFor(() => {
        expect(document.querySelector('img[src*="logo.png"]')).toBeTruthy();
      });
      expect(await screen.findByAltText("Shania Twain")).toBeTruthy(); // portrait
      expect(screen.getByRole("heading", { name: "Shania Twain" })).toBeTruthy();
    });

    it("shows the text heading when no band logo was discovered", async () => {
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

  describe("biography accordion", () => {
    const longBio = "Shania Twain is a Canadian singer and songwriter. She has sold over 100 million records, making her the best-selling female artist in country music history and one of the best-selling music artists of all time. Her success garnered her several titles including the Queen of Country Pop.";

    it("renders bio and links in an Artist Info accordion without inline show more buttons", async () => {
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

      expect(screen.getByText("Artist Info")).toBeTruthy();
      expect(screen.getByText(longBio)).toBeTruthy();
      expect(screen.queryByRole("button", { name: "Show more" })).toBeNull();
      expect(screen.queryByRole("button", { name: "Show less" })).toBeNull();
    });

    it("collapses to a corner button and expands the Artist Info accordion", async () => {
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

      const overviewHeader = screen.getByText("Artist Info");
      const detailsEl = overviewHeader.closest("details");
      expect(detailsEl).toBeTruthy();
      expect(detailsEl?.hasAttribute("open")).toBe(true);

      // Closing the accordion updates the global layout store and swaps
      // the inline accordion for a floating corner-tuck button
      windowLayoutStore.setOverviewExpanded(false);
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(windowLayoutStore.isOverviewExpanded).toBe(false);
      expect(screen.queryByRole("button", { name: /Artist Info/ })).toBeTruthy();
    });

    it("limits ArtistInformationPanel max-width to two columns of links and uses 4-column links when artist has no bio", async () => {
      const invokeMock = vi.mocked(invoke);
      invokeMock.mockImplementation((cmd: string) => {
        if (cmd === "get_songs_by_artist") return Promise.resolve([{ id: 1, title: "El Bueno Y El Malo", artist: "Hermanos Gutiérrez" }] as any);
        if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
        if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
        if (cmd === "get_artist_profile") {
          return Promise.resolve({
            artist_key: "Hermanos Gutiérrez",
            website: "https://hermanosgutierrez.ch",
            tags: [],
            social_links: [
              { platform: "bandcamp", handle_or_url: "https://hermanosgutierrez.bandcamp.com" },
              { platform: "discogs", handle_or_url: "https://discogs.com/artist/123" },
            ],
            bio: null,
          });
        }
        if (cmd === "get_song_context") {
          return Promise.resolve({
            artist_begin_date: "2015",
            artist_begin_area_name: "Zürich",
            artist_area_name: "Switzerland",
          });
        }
        return Promise.resolve();
      });

      const { container } = render(ArtistDetailView, { props: { artistName: "Hermanos Gutiérrez" } });
      await waitFor(() => {
        expect(screen.getByText("Formed")).toBeTruthy();
      });

      expect(screen.getByText("Artist Info")).toBeTruthy();
      expect(screen.getByText(/2015/)).toBeTruthy();

      // Facts panel container must have max-width constraint for two columns of links
      const factsContainer = container.querySelector(".space-y-2.text-xs");
      expect(factsContainer).toBeTruthy();
      expect(factsContainer?.className).toContain("@xl:max-w-[calc(50%-0.3125rem)]");

      // Links grid must expand to up to 4 columns when there is no bio
      const linksGrid = container.querySelector(".grid.grid-cols-1");
      expect(linksGrid).toBeTruthy();
      expect(linksGrid?.className).toContain("@xl:grid-cols-4");
    });

    it("limits ArtistInformationPanel max-width and renders 2-column links beside bio when bio is present", async () => {
      const invokeMock = vi.mocked(invoke);
      invokeMock.mockImplementation((cmd: string) => {
        if (cmd === "get_songs_by_artist") return Promise.resolve([{ id: 1, title: "Song 1", artist: "Shania Twain" }] as any);
        if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
        if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
        if (cmd === "get_artist_profile") {
          return Promise.resolve({
            artist_key: "Shania Twain",
            website: "https://www.shaniatwain.com",
            tags: ["country"],
            social_links: [{ platform: "instagram", handle_or_url: "@shaniatwain" }],
            bio: longBio,
          });
        }
        if (cmd === "get_song_context") {
          return Promise.resolve({
            artist_begin_date: "1965-08-28",
            artist_type: "Person",
            artist_gender: "female",
            artist_begin_area_name: "Windsor",
            artist_area_name: "Canada",
          });
        }
        return Promise.resolve();
      });

      const { container } = render(ArtistDetailView, { props: { artistName: "Shania Twain" } });
      await waitFor(() => {
        expect(screen.getByText("Born")).toBeTruthy();
      });

      expect(screen.getByText("Artist Info")).toBeTruthy();

      // Facts panel container in two-column mode also has the max-width constraint
      const factsContainer = container.querySelector(".space-y-2.text-xs");
      expect(factsContainer).toBeTruthy();
      expect(factsContainer?.className).toContain("@xl:max-w-[calc(50%-0.3125rem)]");

      // Links grid beside bio uses 2 columns
      const linksGrid = container.querySelector(".grid.grid-cols-1");
      expect(linksGrid).toBeTruthy();
      expect(linksGrid?.className).toContain("@sm:grid-cols-2");
      expect(linksGrid?.className).not.toContain("@xl:grid-cols-4");

      // Reset get_songs_by_artist to empty array so subsequent tests don't see mock songs
      invokeMock.mockImplementation((cmd: string) => {
        if (cmd === "get_songs_by_artist") return Promise.resolve([]);
        return Promise.resolve();
      });
    });
  });

    it("hides tags and bio/profile section while keeping action buttons visible when detail header is collapsed", async () => {
      const originalHeight = windowLayoutStore.viewportHeight;
      try {
        // Set viewportHeight to less than HEIGHT_BREAKPOINT_SHORT_PX (600)
        windowLayoutStore.viewportHeight = 500;
        expect(windowLayoutStore.isDetailHeaderCollapsed).toBe(true);

        render(ArtistDetailView, { props: { artistName: "Shania Twain" } });
        await new Promise((resolve) => setTimeout(resolve, 50));

        // Action buttons remain visible
        expect(screen.getByRole("button", { name: /^Play$/i })).toBeTruthy();
        expect(screen.getByRole("button", { name: /^Shuffle$/i })).toBeTruthy();
        expect(screen.getByTitle("More actions")).toBeTruthy();

        // Artist name, tags, and bio profile section are hidden
        expect(screen.queryByText("Shania Twain")).toBeNull();
        expect(screen.queryByText("country")).toBeNull();
        expect(screen.queryByText("canadian")).toBeNull();
        expect(screen.queryByText("pop")).toBeNull();
        expect(screen.queryByText("Canadian music icon")).toBeNull();
        expect(screen.queryByText("LINKS")).toBeNull();
      } finally {
        windowLayoutStore.viewportHeight = originalHeight;
      }
    });

    it("auto-fetches artist info, summary, and image on visit when flags are not set and context enrichment is enabled (#1143)", async () => {
      const invokeMock = vi.mocked(invoke);
      collectionStore.artistProfiles = {
        "shania twain": {
          artist_key: "Shania Twain",
          tags: [],
          social_links: [],
          details_fetched: false,
          image_fetched: false,
        },
      };

      invokeMock.mockImplementation((cmd: string, args?: any) => {
        if (cmd === "get_songs_by_artist") {
          return Promise.resolve([
            { id: 1, title: "Man! I Feel Like a Woman!", artist: "Shania Twain", musicbrainz_artist_id: "mbid-123" } as any,
          ]);
        }
        if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
        if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
        if (cmd === "get_artist_profile") return Promise.resolve(collectionStore.artistProfiles["shania twain"]);
        if (cmd === "is_context_enrichment_enabled") return Promise.resolve(true);
        if (cmd === "retrieve_artist_details") return Promise.resolve({ added_count: 2 });
        if (cmd === "get_song_context") return Promise.resolve({ wikipedia_extract: "Bio summary" });
        if (cmd === "retrieve_artist_image") {
          return Promise.resolve({
            uri: "luminous-art://shania.jpg",
            source: "fanart",
            logo_uri: null,
            background_uri: null,
            profile: { ...collectionStore.artistProfiles["shania twain"], image_fetched: true, logo_fetched: true, background_fetched: true },
          });
        }
        return Promise.resolve();
      });

      render(ArtistDetailView, { props: { artistName: "Shania Twain" } });
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith("retrieve_artist_details", expect.objectContaining({ artist: "Shania Twain" }));
        expect(invokeMock).toHaveBeenCalledWith("retrieve_artist_image", expect.objectContaining({ artist: "Shania Twain" }));
      });
    });

    it("does not auto-fetch artist info or images when details and every image type are already fetched (#1143)", async () => {
      const invokeMock = vi.mocked(invoke);
      invokeMock.mockClear();
      collectionStore.artistProfiles = {
        "shania twain": {
          artist_key: "Shania Twain",
          tags: [],
          social_links: [],
          details_fetched: true,
          image_fetched: true,
          logo_fetched: true,
          background_fetched: true,
        },
      };

      invokeMock.mockImplementation((cmd: string, args?: any) => {
        if (cmd === "get_songs_by_artist") {
          return Promise.resolve([
            { id: 1, title: "Man! I Feel Like a Woman!", artist: "Shania Twain", musicbrainz_artist_id: "mbid-123" } as any,
          ]);
        }
        if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
        if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
        if (cmd === "get_artist_profile") return Promise.resolve(collectionStore.artistProfiles["shania twain"]);
        if (cmd === "is_context_enrichment_enabled") return Promise.resolve(true);
        return Promise.resolve();
      });

      render(ArtistDetailView, { props: { artistName: "Shania Twain" } });
      await new Promise((resolve) => setTimeout(resolve, 50));

      expect(invokeMock).not.toHaveBeenCalledWith("retrieve_artist_details", expect.anything());
      expect(invokeMock).not.toHaveBeenCalledWith("retrieve_artist_image", expect.anything());
    });
    it("backfills only the missing image types, without re-fetching details, when details are already fetched (#1276)", async () => {
      const invokeMock = vi.mocked(invoke);
      invokeMock.mockClear();
      collectionStore.artistProfiles = {
        "shania twain": {
          artist_key: "Shania Twain",
          tags: [],
          social_links: [],
          details_fetched: true,
          image_fetched: true,
          logo_fetched: false,
          background_fetched: false,
        },
      };

      invokeMock.mockImplementation((cmd: string) => {
        if (cmd === "get_songs_by_artist") {
          return Promise.resolve([
            { id: 1, title: "Man! I Feel Like a Woman!", artist: "Shania Twain", musicbrainz_artist_id: "mbid-123" } as any,
          ]);
        }
        if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
        if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
        if (cmd === "get_artist_profile") return Promise.resolve(collectionStore.artistProfiles["shania twain"]);
        if (cmd === "is_context_enrichment_enabled") return Promise.resolve(true);
        if (cmd === "retrieve_artist_image") {
          return Promise.resolve({
            uri: null,
            source: null,
            logo_uri: "luminous-art://shania_logo.png",
            background_uri: null,
            profile: { ...collectionStore.artistProfiles["shania twain"], logo_fetched: true, background_fetched: true, fetched_logo_filename: "shania_logo.png" },
          });
        }
        return Promise.resolve();
      });

      render(ArtistDetailView, { props: { artistName: "Shania Twain" } });
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith("retrieve_artist_image", expect.objectContaining({ artist: "Shania Twain", onlyMissing: true }));
      });
      expect(invokeMock).not.toHaveBeenCalledWith("retrieve_artist_details", expect.anything());
    });

    it("does not backfill the logo or background while they're turned off (#1276)", async () => {
      const invokeMock = vi.mocked(invoke);
      invokeMock.mockClear();
      prefs.fanartFetchLogo = false;
      prefs.fanartFetchBackground = false;
      collectionStore.artistProfiles = {
        "shania twain": {
          artist_key: "Shania Twain",
          tags: [],
          social_links: [],
          details_fetched: true,
          image_fetched: true,
          logo_fetched: false,
          background_fetched: false,
        },
      };

      invokeMock.mockImplementation((cmd: string) => {
        if (cmd === "get_songs_by_artist") {
          return Promise.resolve([
            { id: 1, title: "Man! I Feel Like a Woman!", artist: "Shania Twain", musicbrainz_artist_id: "mbid-123" } as any,
          ]);
        }
        if (cmd === "get_playlists_by_artist") return Promise.resolve([]);
        if (cmd === "get_compilations_by_artist") return Promise.resolve([]);
        if (cmd === "get_artist_profile") return Promise.resolve(collectionStore.artistProfiles["shania twain"]);
        if (cmd === "is_context_enrichment_enabled") return Promise.resolve(true);
        return Promise.resolve();
      });

      try {
        render(ArtistDetailView, { props: { artistName: "Shania Twain" } });
        await new Promise((resolve) => setTimeout(resolve, 50));
        expect(invokeMock).not.toHaveBeenCalledWith("retrieve_artist_image", expect.anything());
      } finally {
        prefs.fanartFetchLogo = true;
        prefs.fanartFetchBackground = true;
      }
    });
});
