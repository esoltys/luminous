import "@testing-library/jest-dom";
import { describe, it, expect, beforeEach } from "vitest";
import { render, screen } from "@testing-library/svelte";
import SongContextMenu from "./SongContextMenu.svelte";
import { picardStore } from "../stores/picard.svelte";
import type { Song } from "../types";

describe("SongContextMenu.svelte", () => {
  const baseSong: Omit<Song, "id" | "source"> = {
    filetype: "FLAC",
    title: "Paranoid Android",
    artist: "Radiohead",
    album: "OK Computer",
    art_embedded: false,
    art_unset: false,
    compilation: false,
    beginning_nanosec: 0,
    end_nanosec: 0,
    rating: 5,
    playcount: 42,
    skipcount: 1,
    length_nanosec: 387_000_000_000,
    added: 1_700_000_000,
    year: 1997,
    unavailable: false,
  };

  const localSong: Song = { ...baseSong, id: 1, source: "local_file" };
  const webdavSong: Song = { ...baseSong, id: 2, source: "web_dav" };

  beforeEach(() => {
    picardStore.path = "/usr/bin/picard";
  });

  it("enables Open in Picard for a local song", async () => {
    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: localSong,
      onPlay: () => {},
      onOpenInPicard: () => {},
      onClose: () => {},
    });

    const item = await screen.findByText("Open in Picard");
    expect(item.closest("button")).not.toBeDisabled();
  });

  it("disables Open in Picard for a single WebDAV song, with a tooltip", async () => {
    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: webdavSong,
      onPlay: () => {},
      onOpenInPicard: () => {},
      onClose: () => {},
    });

    const item = await screen.findByText("Open in Picard");
    const button = item.closest("button");
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", "Open in Picard isn't available for songs on a remote server");
  });

  it("disables Open in Picard for a song synced from an OpenSubsonic server", async () => {
    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: { ...baseSong, id: 3, source: "subsonic" },
      onPlay: () => {},
      onOpenInPicard: () => {},
      onClose: () => {},
    });

    const item = await screen.findByText("Open in Picard");
    expect(item.closest("button")).toBeDisabled();
  });

  it("disables Open in Picard when every selected song is WebDAV", async () => {
    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: webdavSong,
      selectedCount: 2,
      selectedSongIds: [2, 3],
      selectedSongs: [webdavSong, { ...webdavSong, id: 3 }],
      onPlay: () => {},
      onOpenInPicard: () => {},
      onClose: () => {},
    });

    const item = await screen.findByText("Open in Picard");
    expect(item.closest("button")).toBeDisabled();
  });

  it("keeps Open in Picard enabled for a mixed local + WebDAV selection", async () => {
    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: localSong,
      selectedCount: 2,
      selectedSongIds: [1, 2],
      selectedSongs: [localSong, webdavSong],
      onPlay: () => {},
      onOpenInPicard: () => {},
      onClose: () => {},
    });

    const item = await screen.findByText("Open in Picard");
    expect(item.closest("button")).not.toBeDisabled();
  });

  it("disables Open in Picard when Picard itself isn't found, regardless of source", async () => {
    picardStore.path = null;
    render(SongContextMenu, {
      x: 0,
      y: 0,
      song: localSong,
      onPlay: () => {},
      onOpenInPicard: () => {},
      onClose: () => {},
    });

    const item = await screen.findByText("Open in Picard");
    const button = item.closest("button");
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute(
      "title",
      "MusicBrainz Picard not found. Install it from picard.musicbrainz.org, or set a custom path in Settings."
    );
  });
});
