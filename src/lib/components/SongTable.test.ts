import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render } from "@testing-library/svelte";
import SongTable, { type SongTableRow } from "./SongTable.svelte";
import { collectionStore } from "../stores/collection.svelte";
import { inspectorStore } from "../stores/inspector.svelte";
import type { Song } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

describe("SongTable.svelte — Album column (#428)", () => {
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
    collectionStore.visibleColumns.album = true;
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
