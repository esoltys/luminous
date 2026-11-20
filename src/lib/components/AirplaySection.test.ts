import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { fireEvent, render } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import { openExternalUrl } from "../utils/openExternalUrl";
import AirplaySection from "./AirplaySection.svelte";
import { navigationStore } from "../stores/navigation.svelte";
import { prefs } from "../stores/prefs.svelte";
import { radioStore } from "../stores/radio.svelte";
import type { AirplayNow, AirplayStation, Song } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));
vi.mock("../utils/openExternalUrl", () => ({
  openExternalUrl: vi.fn(),
}));

const mockInvoke = vi.mocked(invoke);

/** 2026-10-29T15:10:30Z, the fixed "now" of these tests. */
const NOW_SECS = 1_793_286_630;

const song = { id: 7, title: "Holiday", artist: "Madonna", path: "/music/holiday.flac" } as Song;

function station(n: number, overrides: Partial<AirplayStation> = {}): AirplayStation {
  return {
    id: `uuid-${n}`,
    name: `Station ${n}`,
    country: "NL",
    codec: "MP3",
    bitrate: 128,
    homepage: `https://station${n}.example`,
    confirmed_at: NOW_SECS - 30,
    ...overrides,
  };
}

function answer(status: AirplayNow["status"], stations: AirplayStation[] = []): AirplayNow {
  return { status, stations, fetched_at: NOW_SECS, attribution_url: "https://tuneping.com/" };
}

/** Resolves the airplay command with `result`, and every other command with nothing. */
function respondWith(result: AirplayNow) {
  mockInvoke.mockImplementation(async (cmd: string) =>
    cmd === "get_airplay_now" ? result : undefined,
  );
}

/** Runs past the debounce and lets the command's promise settle. */
async function settle() {
  await vi.advanceTimersByTimeAsync(1600);
}

describe("AirplaySection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    vi.setSystemTime(NOW_SECS * 1000);
    prefs.onlineEnabled = true;
    prefs.airplayEnabled = true;
    prefs.airplayPromptDismissed = false;
    respondWith(answer({ kind: "ready" }));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("asks before sending anything and sends nothing until the user agrees", async () => {
    prefs.airplayEnabled = false;
    const { getByRole, queryByText } = render(AirplaySection, { props: { song } });
    expect(getByRole("button", { name: "Show stations" })).toBeInTheDocument();
    await settle();
    expect(mockInvoke).not.toHaveBeenCalledWith("get_airplay_now", expect.anything());

    await fireEvent.click(getByRole("button", { name: "Show stations" }));
    expect(prefs.airplayEnabled).toBe(true);
    await settle();
    expect(mockInvoke).toHaveBeenCalledWith("get_airplay_now", { songId: 7, forceRefresh: false });
    expect(queryByText("Show stations")).toBeNull();
  });

  it("hides the consent card for good after Not now", async () => {
    prefs.airplayEnabled = false;
    const { getByRole, container } = render(AirplaySection, { props: { song } });
    await fireEvent.click(getByRole("button", { name: "Not now" }));
    expect(prefs.airplayPromptDismissed).toBe(true);
    expect(container.textContent).toBe("");
  });

  it("shows nothing while offline, and nothing for a station", async () => {
    prefs.onlineEnabled = false;
    const offline = render(AirplaySection, { props: { song } });
    expect(offline.container.textContent).toBe("");
    offline.unmount();

    prefs.onlineEnabled = true;
    const radio = { ...song, path: "radio://abc" } as Song;
    const { container } = render(AirplaySection, { props: { song: radio } });
    await settle();
    expect(container.textContent).toBe("");
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("waits for the debounce, then lists stations with details and attribution", async () => {
    respondWith(answer({ kind: "ready" }, [station(1), station(2, { bitrate: null })]));
    const { getByText, container } = render(AirplaySection, { props: { song } });

    await vi.advanceTimersByTimeAsync(1000);
    expect(mockInvoke).not.toHaveBeenCalled();
    expect(getByText("Checking stations…")).toBeInTheDocument();

    await settle();
    expect(getByText("Station 1")).toBeInTheDocument();
    expect(getByText("Station 2")).toBeInTheDocument();
    expect(container.textContent).toContain("128 kbps");
    expect(container.textContent).toContain("Confirmed 30 seconds ago");
    expect(getByText("Data by Tuneping")).toBeInTheDocument();
  });

  it("caps the list and counts the rest", async () => {
    respondWith(
      answer(
        { kind: "ready" },
        Array.from({ length: 11 }, (_, i) => station(i + 1)),
      ),
    );
    const { getByText, queryByText } = render(AirplaySection, { props: { song } });
    await settle();
    expect(getByText("Station 8")).toBeInTheDocument();
    expect(queryByText("Station 9")).toBeNull();
    expect(getByText("+3 more stations")).toBeInTheDocument();
  });

  it("says so when no station is playing the song", async () => {
    const { getByText } = render(AirplaySection, { props: { song } });
    await settle();
    expect(getByText("No stations are playing this right now.")).toBeInTheDocument();
  });

  it("serves the last result, marked stale, while Tuneping is unavailable", async () => {
    respondWith(answer({ kind: "unavailable", retry_after: 3600 }, [station(1)]));
    const { getByText } = render(AirplaySection, { props: { song } });
    await settle();
    expect(getByText("Couldn't reach Tuneping. Showing the last result.")).toBeInTheDocument();
    expect(getByText("Station 1")).toBeInTheDocument();
  });

  it("reports an outage with nothing cached", async () => {
    respondWith(answer({ kind: "unavailable", retry_after: 30 }));
    const { getByText } = render(AirplaySection, { props: { song } });
    await settle();
    expect(getByText("Couldn't reach Tuneping. Try again in a moment.")).toBeInTheDocument();
  });

  it("hides itself when the backend says the song can't be looked up", async () => {
    respondWith(answer({ kind: "unsupported" }));
    const { container } = render(AirplaySection, { props: { song } });
    await settle();
    expect(container.textContent).toBe("");
  });

  it("refreshes on demand, skipping the cache", async () => {
    const { getByRole } = render(AirplaySection, { props: { song } });
    await settle();
    await fireEvent.click(getByRole("button", { name: "Refresh stations" }));
    await vi.advanceTimersByTimeAsync(0);
    expect(mockInvoke).toHaveBeenLastCalledWith("get_airplay_now", {
      songId: 7,
      forceRefresh: true,
    });
  });

  it("navigates to the station in Internet Radio Discover when clicked", async () => {
    respondWith(answer({ kind: "ready" }, [station(1)]));
    const { getByText } = render(AirplaySection, { props: { song } });
    await settle();
    await fireEvent.click(getByText("Station 1"));
    expect(openExternalUrl).not.toHaveBeenCalled();
    expect(navigationStore.activeTab).toBe("radio");
    expect(radioStore.subView).toBe("discover");
    expect(radioStore.searchQuery).toBe("Station 1");
  });

  it("skipping songs inside the debounce sends one request, for the last song", async () => {
    const { rerender } = render(AirplaySection, { props: { song } });
    await vi.advanceTimersByTimeAsync(500);
    await rerender({ song: { ...song, id: 8 } as Song });
    await vi.advanceTimersByTimeAsync(500);
    await rerender({ song: { ...song, id: 9 } as Song });
    await settle();
    const calls = mockInvoke.mock.calls.filter(([cmd]) => cmd === "get_airplay_now");
    expect(calls).toEqual([["get_airplay_now", { songId: 9, forceRefresh: false }]]);
  });
});
