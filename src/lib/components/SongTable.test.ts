import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import SongTable, { type SongTableRow } from "./SongTable.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { inspectorStore } from "../stores/inspector.svelte";
import { playlistsStore } from "../stores/playlists.svelte";
import { songDrag } from "../stores/songDrag.svelte";
import { provenanceStore } from "../stores/provenance.svelte";
import type { Song } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

const baseSong: Song = {
  id: 1,
  source: "local_file",
  filetype: "MP3",
  path: "/music/test.mp3",
  title: "Test Track",
  artist: "Test Artist",
  album: "Test Album",
  album_artist: "",
  composer: "",
  genre: "",
  track: 1,
  disc: 1,
  year: 2024,
  compilation: false,
  length_nanosec: 180_000_000_000,
  beginning_nanosec: 0,
  end_nanosec: 180_000_000_000,
  rating: 0,
  playcount: 0,
  skipcount: 0,
  art_embedded: false,
  art_unset: false,
  unavailable: false,
};

function renderTable(rows: SongTableRow[]) {
  return render(SongTable, {
    props: {
      rows,
      mode: "track",
      leadingColumnWidth: "3rem",
      colDefaults: {},
      sortField: "title",
      sortAsc: true,
      onToggleSort: () => {},
      onRowDoubleClick: () => {},
      onRowContextMenu: () => {},
      onRate: () => {},
      onEditTags: () => {},
    },
  });
}

describe("SongTable.svelte — Album column (#428)", () => {
  beforeEach(() => {
    collectionStore.visibleColumns.album = true;
  });

  it("renders an em-dash for a song with no album", () => {
    const song: Song = { ...baseSong, album: "" };
    const { getByText, queryByText } = renderTable([{ key: "1", song }]);

    expect(getByText("—")).toBeInTheDocument();
    expect(queryByText(/unknown album/i)).not.toBeInTheDocument();
  });

  it("renders the album LinkButton when the album is present", () => {
    const { getByText } = renderTable([{ key: "1", song: baseSong }]);

    expect(getByText("Test Album")).toBeInTheDocument();
  });
});

describe("SongTable.svelte — Last Played column", () => {
  const baseSong: Song = {
    id: 1,
    source: "local_file",
    filetype: "MP3",
    path: "/music/test.mp3",
    title: "Test Track",
    artist: "Test Artist",
    album: "Test Album",
    album_artist: "",
    composer: "",
    genre: "",
    track: 1,
    disc: 1,
    year: 2024,
    compilation: false,
    length_nanosec: 180_000_000_000,
    beginning_nanosec: 0,
    end_nanosec: 180_000_000_000,
    rating: 0,
    playcount: 0,
    skipcount: 0,
    art_embedded: false,
    art_unset: false,
    unavailable: false,
  };

  beforeEach(() => {
    collectionStore.visibleColumns.lastplayed = true;
    // Relative times depend on the clock: pin it to midday so "N minutes ago" can't
    // cross midnight into "Yesterday" and results don't vary between runs.
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2024, 5, 15, 12, 0, 0));
  });

  function renderTable(rows: SongTableRow[]) {
    return render(SongTable, {
      props: {
        rows,
        mode: "track",
        leadingColumnWidth: "3rem",
        colDefaults: {},
        sortField: "title",
        sortAsc: true,
        onToggleSort: () => {},
        onRowDoubleClick: () => {},
        onRowContextMenu: () => {},
        onRate: () => {},
        onEditTags: () => {},
      },
    });
  }

  it("renders an em-dash for a song with no last played timestamp", () => {
    const song: Song = { ...baseSong, lastplayed: undefined };
    const { getByText } = renderTable([{ key: "1", song }]);

    expect(getByText("—")).toBeInTheDocument();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("renders relative time for a recently played song", () => {
    const nowSec = Math.floor(Date.now() / 1000);
    const song: Song = { ...baseSong, lastplayed: nowSec - 15 * 60 }; // 15 mins ago
    const { getByText } = renderTable([{ key: "1", song }]);

    expect(getByText("15 minutes ago")).toBeInTheDocument();
  });

  it("renders absolute date for a song played over 6 days ago", () => {
    const tenDaysAgoSec = Math.floor(Date.now() / 1000) - 10 * 24 * 3600;
    const song: Song = { ...baseSong, lastplayed: tenDaysAgoSec };
    const expectedDate = new Date(tenDaysAgoSec * 1000).toLocaleDateString();
    const { getByText } = renderTable([{ key: "1", song }]);

    expect(getByText(expectedDate)).toBeInTheDocument();
  });
});

describe("SongTable.svelte — Library column (#1842)", () => {
  const song = (id: number, source: Song["source"], path: string) =>
    ({ id, title: `Song ${id}`, source, filetype: "MP3", path }) as Song;

  beforeEach(() => {
    collectionStore.visibleColumns.album = false;
    collectionStore.visibleColumns.lastplayed = false;
    collectionStore.visibleColumns.library = true;
    collectionStore.directories = [];
    collectionStore.webdavServers = [];
  });
  afterEach(() => {
    collectionStore.visibleColumns.library = false;
  });

  function renderTable(rows: SongTableRow[]) {
    return render(SongTable, {
      props: {
        rows,
        mode: "track",
        leadingColumnWidth: "3rem",
        colDefaults: {},
        sortField: "title",
        sortAsc: true,
        onToggleSort: () => {},
        onRowDoubleClick: () => {},
        onRowContextMenu: () => {},
        onRate: () => {},
        onEditTags: () => {},
      },
    });
  }

  it("labels a file opened from outside the library", () => {
    const { getByText } = renderTable([{ key: "1", song: song(1, "opened_file", "E:/Lorde/a.flac") }]);
    expect(getByText("Not in library")).toBeInTheDocument();
  });

  it("doesn't label a library song outside any known folder", () => {
    const { queryByText } = renderTable([{ key: "1", song: song(1, "local_file", "/elsewhere/a.flac") }]);
    expect(queryByText("Not in library")).not.toBeInTheDocument();
  });
});

describe("SongTable.svelte — info sidebar selection", () => {
  const song = (id: number) => ({ id, title: `Song ${id}`, source: "local_file", filetype: "MP3" }) as Song;

  function renderTable(rows: SongTableRow[], selectedKeys: Set<string>) {
    return render(SongTable, {
      props: {
        rows,
        mode: "track",
        leadingColumnWidth: "3rem",
        colDefaults: {},
        sortField: "title",
        sortAsc: true,
        onToggleSort: () => {},
        onRowDoubleClick: () => {},
        onRowContextMenu: () => {},
        onRate: () => {},
        onEditTags: () => {},
        selectedKeys,
      },
    });
  }

  beforeEach(() => inspectorStore.clearAll());

  it("reports a single selected song and clears when the table unmounts", () => {
    const rows = [{ key: "1", song: song(1) }, { key: "2", song: song(2) }];
    const { unmount } = renderTable(rows, new Set(["2"]));
    expect(inspectorStore.subject).toMatchObject({ source: "selection", song: { id: 2 } });
    unmount();
    expect(inspectorStore.subject).toBeNull();
  });

  it("reports nothing for a multi-selection", () => {
    const rows = [{ key: "1", song: song(1) }, { key: "2", song: song(2) }];
    renderTable(rows, new Set(["1", "2"]));
    expect(inspectorStore.subject).toBeNull();
  });
});


describe("SongTable.svelte — windowing (#1898)", () => {
  const song = (id: number): Song => ({
    id,
    source: "local_file",
    filetype: "MP3",
    path: `/music/${id}.mp3`,
    title: `Track ${id}`,
    artist: "Artist",
    album: "Album",
    album_artist: "",
    composer: "",
    genre: "",
    track: id,
    disc: 1,
    year: 2024,
    compilation: false,
    length_nanosec: 180_000_000_000,
    beginning_nanosec: 0,
    end_nanosec: 180_000_000_000,
    rating: 0,
    playcount: 0,
    skipcount: 0,
    art_embedded: false,
    art_unset: false,
    unavailable: false,
  });

  function renderInScroller(rows: SongTableRow[]) {
    const scroller = document.createElement("div");
    scroller.style.overflowY = "auto";
    document.body.appendChild(scroller);
    const result = render(SongTable, {
      target: scroller,
      props: {
        rows,
        mode: "position",
        leadingColumnWidth: "3rem",
        colDefaults: {},
        sortField: "position",
        sortAsc: true,
        onToggleSort: () => {},
        onRowDoubleClick: () => {},
        onRowContextMenu: () => {},
        onRate: () => {},
        onEditTags: () => {},
      },
    });
    return { ...result, scroller };
  }

  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("draws only a window of a long list, with spacers holding its full height", () => {
    const rows = Array.from({ length: 5000 }, (_, i) => ({ key: String(i), song: song(i) }));
    const { container } = renderInScroller(rows);

    const drawn = container.querySelectorAll("[data-song-row]");
    expect(drawn.length).toBeGreaterThan(0);
    expect(drawn.length).toBeLessThan(100);

    const body = drawn[0].parentElement!;
    const spacers = [body.firstElementChild as HTMLElement, body.lastElementChild as HTMLElement];
    const rowHeight = parseFloat((drawn[0] as HTMLElement).style.height);
    const spacerTotal = spacers.reduce((sum, el) => sum + parseFloat(el.style.height), 0);
    expect(spacerTotal + drawn.length * rowHeight).toBe(5000 * rowHeight);
  });

  it("moves the window when its scrolling ancestor scrolls", async () => {
    const rows = Array.from({ length: 5000 }, (_, i) => ({ key: String(i), song: song(i) }));
    const { container, scroller } = renderInScroller(rows);
    // jsdom has no layout: place the table body as a real scroller would.
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      const top = this === scroller ? 0 : -scroller.scrollTop;
      return { top, bottom: top, left: 0, right: 0, width: 0, height: 0, x: 0, y: top, toJSON() {} } as DOMRect;
    });
    const rowHeight = parseFloat((container.querySelector("[data-song-row]") as HTMLElement).style.height);

    scroller.scrollTop = 3000 * rowHeight;
    scroller.dispatchEvent(new Event("scroll"));
    await tick();

    expect(container.querySelector('[data-key="0"]')).toBeNull();
    expect(container.querySelector('[data-key="3000"]')).not.toBeNull();
    vi.restoreAllMocks();
  });

  it("draws a divider row together with its divider", () => {
    const rows: SongTableRow[] = [
      { key: "0", song: song(0) },
      { key: "1", song: song(1), dividerBefore: "Auto Continue" },
    ];
    const { getByRole } = renderInScroller(rows);
    expect(getByRole("separator", { name: "Auto Continue" })).toBeInTheDocument();
  });

  it("scrollToKey scrolls to a row that is not drawn yet", async () => {
    const rows = Array.from({ length: 5000 }, (_, i) => ({ key: String(i), song: song(i) }));
    const { component, container, scroller } = renderInScroller(rows);
    expect(container.querySelector('[data-key="3000"]')).toBeNull();

    (component as unknown as { scrollToKey: (key: string) => void }).scrollToKey("3000");
    const rowHeight = parseFloat((container.querySelector("[data-song-row]") as HTMLElement).style.height);
    expect(scroller.scrollTop).toBeGreaterThan(2999 * rowHeight);
  });
});

describe("SongTable.svelte — dragging songs out (#1974)", () => {
  const makeSong = (id: number, title: string): Song => ({
    id,
    source: "local_file",
    filetype: "MP3",
    path: `/music/${id}.mp3`,
    title,
    artist: "Artist",
    album: "Album",
    compilation: false,
    length_nanosec: 180_000_000_000,
    beginning_nanosec: 0,
    end_nanosec: 180_000_000_000,
    rating: 0,
    playcount: 0,
    skipcount: 0,
    art_embedded: false,
    art_unset: false,
    unavailable: false,
  });
  const rows: SongTableRow[] = [
    { key: "a", song: makeSong(1, "One"), underlyingIndex: 0 },
    { key: "b", song: makeSong(2, "Two"), underlyingIndex: 1 },
    { key: "c", song: makeSong(3, "Three"), underlyingIndex: 2 },
  ];

  // jsdom has no layout: drop targets get a fixed box at (10..110, 20..60), and each test
  // points elementFromPoint at the row under the cursor where it needs one.
  function placeTarget(attrs: Record<string, string>) {
    const el = document.createElement("button");
    Object.assign(el.dataset, attrs);
    el.getBoundingClientRect = () => ({ left: 10, right: 110, top: 20, bottom: 60, width: 100, height: 40 }) as DOMRect;
    document.body.appendChild(el);
  }
  const OVER_TARGET = { clientX: 50, clientY: 40 };
  const originalElementFromPoint = document.elementFromPoint;

  let addToQueue: ReturnType<typeof vi.spyOn>;
  let addToPlaylist: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    addToQueue = vi.spyOn(playlistsStore, "addSongsToQueue").mockResolvedValue();
    addToPlaylist = vi.spyOn(playlistsStore, "addSongsToPlaylist").mockResolvedValue();
    document.elementFromPoint = () => null;
  });
  afterEach(() => {
    document.elementFromPoint = originalElementFromPoint;
    document.body.innerHTML = "";
    songDrag.end();
    vi.restoreAllMocks();
  });

  function renderTable(extra: Record<string, unknown> = {}) {
    return render(SongTable, {
      props: {
        rows,
        mode: "position",
        leadingColumnWidth: "3rem",
        colDefaults: {},
        sortField: "title",
        sortAsc: true,
        onToggleSort: () => {},
        onRowDoubleClick: () => {},
        onRowContextMenu: () => {},
        onRate: () => {},
        onEditTags: () => {},
        ...extra,
      },
    });
  }
  const row = (key: string) => document.querySelector<HTMLElement>(`[data-key="${key}"]`)!;
  const isSelected = (key: string) => row(key).className.includes("bg-brand-accent/20");

  it("adds the selected songs to the Queue and leaves the selection as it was", async () => {
    placeTarget({ dropTarget: "queue" });
    renderTable({ selectedKeys: new Set(["a", "c"]) });

    await fireEvent.pointerDown(row("a"), { button: 0, clientX: 300, clientY: 300 });
    await fireEvent.pointerMove(window, OVER_TARGET);
    expect(songDrag.payload?.songIds).toEqual([1, 3]);
    expect(songDrag.dropTarget).toEqual({ kind: "queue" });

    await fireEvent.pointerUp(window, OVER_TARGET);
    await fireEvent.click(row("a")); // the click the release produces
    await tick();

    expect(addToQueue).toHaveBeenCalledWith([1, 3]);
    expect(isSelected("a") && isSelected("c")).toBe(true);
    expect(isSelected("b")).toBe(false);
  });

  it("drags only the pressed row when it isn't part of the selection", async () => {
    placeTarget({ dropTarget: "playlist", playlistId: "9", playlistName: "Road Trip" });
    renderTable({ selectedKeys: new Set(["a", "c"]) });

    await fireEvent.pointerDown(row("b"), { button: 0, clientX: 300, clientY: 300 });
    await fireEvent.pointerMove(window, OVER_TARGET);
    await fireEvent.pointerUp(window, OVER_TARGET);

    expect(addToPlaylist).toHaveBeenCalledWith(9, [2]);
  });

  it("still selects on a plain click, and a click after an earlier drag isn't swallowed", async () => {
    renderTable();
    // A drag released away from any target, with no click following it.
    await fireEvent.pointerDown(row("a"), { button: 0, clientX: 300, clientY: 300 });
    await fireEvent.pointerMove(window, { clientX: 500, clientY: 500 });
    await fireEvent.pointerUp(window, { clientX: 500, clientY: 500 });

    await fireEvent.pointerDown(row("b"), { button: 0, clientX: 0, clientY: 0 });
    await fireEvent.pointerUp(window, { clientX: 0, clientY: 0 });
    await fireEvent.click(row("b"));
    await tick();

    expect(isSelected("b")).toBe(true);
    expect(addToQueue).not.toHaveBeenCalled();
  });

  it("reorders when released over a row of a reorderable table", async () => {
    placeTarget({ dropTarget: "queue" });
    const onReorder = vi.fn();
    renderTable({ onReorder });

    await fireEvent.pointerDown(row("a"), { button: 0, clientX: 300, clientY: 300 });
    document.elementFromPoint = () => row("c");
    await fireEvent.pointerMove(window, { clientX: 300, clientY: 340 });
    await fireEvent.pointerUp(window, { clientX: 300, clientY: 340 });

    expect(onReorder).toHaveBeenCalledWith(0, 2, ["a"]);
    expect(addToQueue).not.toHaveBeenCalled();
  });

  it("adds instead of reordering when a reorderable row is released on a target", async () => {
    placeTarget({ dropTarget: "queue" });
    const onReorder = vi.fn();
    renderTable({ onReorder, dragSourcePlaylistId: 9 });

    await fireEvent.pointerDown(row("a"), { button: 0, clientX: 300, clientY: 300 });
    document.elementFromPoint = () => row("c"); // passes over another row on the way
    await fireEvent.pointerMove(window, { clientX: 300, clientY: 340 });
    await fireEvent.pointerMove(window, OVER_TARGET);
    await fireEvent.pointerUp(window, OVER_TARGET);

    expect(addToQueue).toHaveBeenCalledWith([1]);
    expect(onReorder).not.toHaveBeenCalled();
  });

  it("does nothing when a playlist's songs are released on that same playlist", async () => {
    placeTarget({ dropTarget: "playlist", playlistId: "9", playlistName: "Road Trip" });
    const onReorder = vi.fn();
    renderTable({ onReorder, dragSourcePlaylistId: 9 });

    await fireEvent.pointerDown(row("a"), { button: 0, clientX: 300, clientY: 300 });
    await fireEvent.pointerMove(window, OVER_TARGET);
    expect(songDrag.dropTarget).toBeNull();
    await fireEvent.pointerUp(window, OVER_TARGET);

    expect(addToPlaylist).not.toHaveBeenCalled();
    expect(onReorder).not.toHaveBeenCalled();
  });
});

describe("SongTable.svelte — Format column and verification icon", () => {
  it("renders ShieldCheck icon next to FLAC for verified songs", () => {
    collectionStore.visibleColumns.format = true;
    provenanceStore.verdicts = new Map([[42, "authentic"]]);

    const flacSong: Song = {
      ...baseSong,
      id: 42,
      filetype: "FLAC",
    };

    const { getByText, container } = renderTable([{ key: "42", song: flacSong }]);

    expect(getByText("FLAC")).toBeInTheDocument();
    const icon = container.querySelector('span[title="Authentic Lossless"] svg');
    expect(icon).not.toBeNull();
  });

  it("omits ShieldCheck icon for suspected lossy songs", () => {
    collectionStore.visibleColumns.format = true;
    provenanceStore.verdicts = new Map([[42, "suspected_lossy"]]);

    const flacSong: Song = {
      ...baseSong,
      id: 42,
      filetype: "FLAC",
    };

    const { getByText, container } = renderTable([{ key: "42", song: flacSong }]);

    expect(getByText("FLAC")).toBeInTheDocument();
    const icon = container.querySelector('span[title="Authentic Lossless"] svg');
    expect(icon).toBeNull();
  });

  it("omits ShieldCheck icon for unverified FLAC songs", () => {
    collectionStore.visibleColumns.format = true;
    provenanceStore.verdicts = new Map();

    const flacSong: Song = {
      ...baseSong,
      id: 99,
      filetype: "FLAC",
    };

    const { getByText, container } = renderTable([{ key: "99", song: flacSong }]);

    expect(getByText("FLAC")).toBeInTheDocument();
    const icon = container.querySelector('span[title="Authentic Lossless"] svg');
    expect(icon).toBeNull();
  });
});
