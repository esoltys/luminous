import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import { flushSync } from "svelte";
import RightPanelHost from "./RightPanelHost.svelte";
import { windowLayoutStore } from "../stores/windowLayout.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

const OPEN_DELAY_MS = 120;
const CLOSE_DELAY_MS = 250;

describe("RightPanelHost.svelte", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    localStorage.clear();
    windowLayoutStore.viewportWidth = 1400;
    windowLayoutStore.rightPanelOpen = true;
    windowLayoutStore.setRightPanelPinned(true);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function renderUnpinned() {
    windowLayoutStore.setRightPanelPinned(false);
    windowLayoutStore.setRightPanelPeeking(false);
    const view = render(RightPanelHost);
    flushSync();
    return view;
  }

  function overlayOf(container: HTMLElement): HTMLElement {
    return container.ownerDocument.querySelector<HTMLElement>(".shadow-2xl")!;
  }

  it("docks the panel with an unpin button while pinned, and shows no rail", () => {
    const { getByRole, queryByRole } = render(RightPanelHost);
    expect(getByRole("button", { name: /unpin info sidebar/i })).toBeInTheDocument();
    expect(queryByRole("group", { name: /info sidebar/i })).toBeNull();
  });

  it("shows only the rail while unpinned, and its info button pins the sidebar", async () => {
    const { getByRole, queryByRole } = renderUnpinned();
    expect(getByRole("group", { name: /info sidebar/i })).toBeInTheDocument();
    expect(queryByRole("separator")).toBeNull();

    await fireEvent.click(getByRole("button", { name: /^pin info sidebar/i }));
    expect(windowLayoutStore.rightPanelPinned).toBe(true);
    expect(windowLayoutStore.rightPanelOpen).toBe(true);
  });

  it("opens the overlay after the pointer rests on the rail, not on a brief pass", async () => {
    const { getByRole } = renderUnpinned();
    const rail = getByRole("group", { name: /info sidebar/i });

    await fireEvent.pointerEnter(rail);
    await fireEvent.pointerLeave(rail);
    vi.advanceTimersByTime(OPEN_DELAY_MS * 2);
    expect(windowLayoutStore.rightPanelPeeking).toBe(false);

    await fireEvent.pointerEnter(rail);
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    expect(windowLayoutStore.rightPanelPeeking).toBe(true);
  });

  it("closes a hover-opened overlay after the pointer leaves it, unless it comes back", async () => {
    const { getByRole, container } = renderUnpinned();
    await fireEvent.pointerEnter(getByRole("group", { name: /info sidebar/i }));
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    flushSync();
    const overlay = overlayOf(container);

    await fireEvent.pointerLeave(overlay);
    vi.advanceTimersByTime(CLOSE_DELAY_MS - 50);
    await fireEvent.pointerEnter(overlay);
    vi.advanceTimersByTime(CLOSE_DELAY_MS);
    expect(windowLayoutStore.rightPanelPeeking).toBe(true);

    await fireEvent.pointerLeave(overlay);
    vi.advanceTimersByTime(CLOSE_DELAY_MS);
    expect(windowLayoutStore.rightPanelPeeking).toBe(false);
  });

  it("closes on Escape, or when the window loses focus", async () => {
    const { getByRole } = renderUnpinned();
    const rail = getByRole("group", { name: /info sidebar/i });
    await fireEvent.pointerEnter(rail);
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    expect(windowLayoutStore.rightPanelPeeking).toBe(true);
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(windowLayoutStore.rightPanelPeeking).toBe(false);

    windowLayoutStore.setRightPanelPeeking(true);
    flushSync();
    await fireEvent.blur(window);
    expect(windowLayoutStore.rightPanelPeeking).toBe(false);
  });

  it("does not reopen after Escape while the pointer is still resting on the rail", async () => {
    const { getByRole } = renderUnpinned();
    const rail = getByRole("group", { name: /info sidebar/i });
    await fireEvent.pointerEnter(rail);
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    expect(windowLayoutStore.rightPanelPeeking).toBe(true);

    await fireEvent.keyDown(window, { key: "Escape" });
    flushSync();
    // Removing the overlay uncovers the rail, which sees the pointer enter again.
    await fireEvent.pointerEnter(getByRole("group", { name: /info sidebar/i }));
    vi.advanceTimersByTime(OPEN_DELAY_MS * 2);
    expect(windowLayoutStore.rightPanelPeeking).toBe(false);

    await fireEvent.pointerMove(document.body);
    await fireEvent.pointerEnter(getByRole("group", { name: /info sidebar/i }));
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    expect(windowLayoutStore.rightPanelPeeking).toBe(true);
  });

  it("closes the overlay on a click outside it but not inside it", async () => {
    const { container } = renderUnpinned();
    windowLayoutStore.setRightPanelPeeking(true);
    flushSync();
    const overlay = overlayOf(container);

    await fireEvent.pointerDown(overlay);
    expect(windowLayoutStore.rightPanelPeeking).toBe(true);
    await fireEvent.pointerDown(container.ownerDocument.body);
    expect(windowLayoutStore.rightPanelPeeking).toBe(false);
  });

  it("does not close while the resize handle is being dragged", async () => {
    const { getByRole, container } = renderUnpinned();
    await fireEvent.pointerEnter(getByRole("group", { name: /info sidebar/i }));
    vi.advanceTimersByTime(OPEN_DELAY_MS);
    flushSync();
    const overlay = overlayOf(container);

    await fireEvent.pointerDown(getByRole("separator"), { clientX: 500 });
    await fireEvent.pointerLeave(overlay);
    vi.advanceTimersByTime(CLOSE_DELAY_MS * 2);
    expect(windowLayoutStore.rightPanelPeeking).toBe(true);
    await fireEvent.pointerUp(window);
  });

  it("renders nothing below the right-panel breakpoint", () => {
    windowLayoutStore.viewportWidth = 500;
    const { queryByRole } = renderUnpinned();
    expect(queryByRole("group", { name: /info sidebar/i })).toBeNull();
    expect(queryByRole("separator")).toBeNull();
  });
});
