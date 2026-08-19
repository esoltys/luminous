import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import ArtistDetailView from "./ArtistDetailView.svelte";
import { collectionStore } from "../stores/collection.svelte";

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
    expect(collectionStore.activeSubTab).toBe("artists");
  });

  it("clicking Edit button opens the ArtistProfileEditor modal", async () => {
    render(ArtistDetailView, { props: { artistName: "Shania Twain" } });

    const editBtn = screen.getByRole("button", { name: /Edit/i });
    await fireEvent.click(editBtn);

    expect(screen.getByRole("dialog")).toBeTruthy();
  });
});
