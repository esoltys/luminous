import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import PinnedSongRowCard from "./PinnedSongRowCard.svelte";
import type { Song } from "../types";

const song = {
  id: 101,
  source: "local_file",
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
  playcount: 0,
  skipcount: 0,
  length_nanosec: 387_000_000_000,
  added: 1_700_000_000,
  year: 1997,
  unavailable: false,
} as Song;

describe("PinnedSongRowCard", () => {
  it("shows the title and artist", () => {
    const { getByText } = render(PinnedSongRowCard, { song });

    expect(getByText("Paranoid Android")).toBeInTheDocument();
    expect(getByText("Radiohead")).toBeInTheDocument();
  });

  it("falls back to unknown labels for missing tags", () => {
    const { getByText } = render(PinnedSongRowCard, { song: { ...song, title: undefined, artist: undefined } as unknown as Song });

    expect(getByText("Unknown Song")).toBeInTheDocument();
    expect(getByText("Unknown Artist")).toBeInTheDocument();
  });

  it("opens on click and Enter, and forwards the context menu", async () => {
    const onclick = vi.fn();
    const oncontextmenu = vi.fn();
    const { getByRole } = render(PinnedSongRowCard, { song, onclick, oncontextmenu });
    const row = getByRole("button");

    await fireEvent.click(row);
    await fireEvent.keyDown(row, { key: "Enter" });
    await fireEvent.contextMenu(row);

    expect(onclick).toHaveBeenCalledTimes(2);
    expect(oncontextmenu).toHaveBeenCalledOnce();
  });
});
