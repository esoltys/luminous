import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import PlaylistCard from "./PlaylistCard.svelte";
import type { Playlist } from "../types";

const base: Playlist = {
  id: 7,
  name: "Road Trip",
  dynamic_enabled: false,
  created: 1700000000,
  updated: 1700000000,
  track_count: 0,
  is_queue: false,
  kind: "custom",
};

describe("PlaylistCard file badge (#2280)", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockResolvedValue([]);
  });

  it("marks a playlist kept in sync with a file and names the file", () => {
    render(PlaylistCard, {
      props: { playlist: { ...base, source_path: "/music/Road Trip.m3u" }, onClick: () => {} },
    });
    const badge = screen.getByText("File");
    expect(badge.getAttribute("title")).toBe("Kept in sync with /music/Road Trip.m3u");
  });

  it("says when the playlist and its file are in conflict", () => {
    render(PlaylistCard, {
      props: {
        playlist: { ...base, source_path: "/music/Road Trip.m3u", source_conflict: true },
        onClick: () => {},
      },
    });
    expect(screen.getByText("File").getAttribute("title")).toContain("Changed in Luminous");
  });

  it("shows no badge on a playlist that lives only in Luminous", () => {
    render(PlaylistCard, { props: { playlist: base, onClick: () => {} } });
    expect(screen.queryByText("File")).toBeNull();
  });

  it("omits Make Active on a file-backed station_list while keeping its File badge", () => {
    render(PlaylistCard, {
      props: {
        playlist: {
          ...base,
          kind: "station_list",
          source_path: "/music/soma_fm.m3u",
          track_count: 43,
        },
        onClick: () => {},
      },
    });
    expect(screen.getByText("File")).toBeTruthy();
    expect(screen.getByText("43 stations")).toBeTruthy();
    expect(screen.queryByTitle("Make Active")).toBeNull();
  });
});
