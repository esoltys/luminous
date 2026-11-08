import "@testing-library/jest-dom";
import { describe, it, expect, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import SongDragGhost from "./SongDragGhost.svelte";
import { songDrag, songsPayload } from "../stores/songDrag.svelte";
import { tick } from "svelte";
import type { Song } from "../types";

const song = (id: number, title: string) => ({ id, title }) as Song;

describe("SongDragGhost.svelte", () => {
  beforeEach(() => {
    songDrag.end();
  });

  it("renders nothing when no drag is active", () => {
    const { queryByTestId } = render(SongDragGhost);
    expect(queryByTestId("song-drag-ghost")).toBeNull();
  });

  it("shows the dragged song's title beside the pointer", async () => {
    const { getByTestId, findByText } = render(SongDragGhost);
    songDrag.start(songsPayload([song(10, "Paranoid Android")]), { x: 150, y: 300 });
    await tick();

    const ghost = getByTestId("song-drag-ghost");
    expect(ghost.style.left).toBe("164px"); // 150 + 14
    expect(ghost.style.top).toBe("300px");
    expect(await findByText("Paranoid Android")).toBeInTheDocument();
  });

  it("shows a count when several songs are dragged", async () => {
    const { findByText } = render(SongDragGhost);
    songDrag.start(songsPayload([song(1, "A"), song(2, "B")]), { x: 100, y: 100 });

    expect(await findByText(/2 songs/i)).toBeInTheDocument();
  });

  it("names the drop target and lights up while over one", async () => {
    const { findByText, getByTestId } = render(SongDragGhost);
    songDrag.start(songsPayload([song(1, "A")]), { x: 100, y: 100 });
    await tick();
    expect(getByTestId("song-drag-ghost")).not.toHaveAttribute("data-over-target");

    songDrag.dropTarget = { kind: "queue" };
    await tick();
    expect(await findByText("Queue")).toBeInTheDocument();
    expect(getByTestId("song-drag-ghost")).toHaveAttribute("data-over-target");

    songDrag.dropTarget = { kind: "playlist", id: 5, name: "Chill Mix" };
    await tick();
    expect(await findByText("Chill Mix")).toBeInTheDocument();
  });
});
