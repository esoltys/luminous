import "@testing-library/jest-dom";
import { describe, it, expect, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import FileDragGhost from "./FileDragGhost.svelte";
import { fileDrag } from "../../stores/fileDrag.svelte";
import { tick } from "svelte";
import type { FileEntry } from "../../types";

function mockEntry(name: string, songId: number | null): FileEntry {
  return {
    name,
    isDir: false,
    size: 1024,
    modified: null,
    fileType: "FLAC",
    kind: "audio",
    opensInApp: false,
    status: songId ? "inLibrary" : "notScanned",
    songId,
    missingTag: null,
    location: `/music/${name}`,
  };
}

describe("FileDragGhost.svelte", () => {
  beforeEach(() => {
    fileDrag.end();
  });

  it("renders nothing when fileDrag is not active", () => {
    const { queryByTestId } = render(FileDragGhost);
    expect(queryByTestId("file-drag-ghost")).toBeNull();
  });

  it("renders dragged single song name and positions following pointerPos", async () => {
    const { getByTestId, findByText } = render(FileDragGhost);
    fileDrag.start([mockEntry("01 - Track.flac", 10)], { x: 150, y: 300 });
    await tick();

    const ghost = getByTestId("file-drag-ghost");
    expect(ghost).toBeInTheDocument();
    expect(ghost.style.left).toBe("164px"); // 150 + 14
    expect(ghost.style.top).toBe("300px");
    expect(await findByText("01 - Track.flac")).toBeInTheDocument();
  });

  it("renders pluralized count when multiple files are dragged", async () => {
    const { findByText } = render(FileDragGhost);
    fileDrag.start([mockEntry("song1.flac", 1), mockEntry("song2.flac", 2)], { x: 100, y: 100 });

    expect(await findByText(/2 songs/i)).toBeInTheDocument();
  });

  it("displays drop target name when hovering a valid target", async () => {
    const { findByText } = render(FileDragGhost);
    fileDrag.start([mockEntry("song1.flac", 1)], { x: 100, y: 100 });

    fileDrag.updatePointer({ x: 100, y: 100 }, { kind: "queue" });
    await tick();
    expect(await findByText("Queue")).toBeInTheDocument();

    fileDrag.updatePointer({ x: 100, y: 100 }, { kind: "playlist", id: 5, name: "Chill Mix" });
    await tick();
    expect(await findByText("Chill Mix")).toBeInTheDocument();
  });
});
