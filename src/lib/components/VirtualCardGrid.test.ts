import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render } from "@testing-library/svelte";
import { createRawSnippet, flushSync } from "svelte";
import VirtualCardGrid from "./VirtualCardGrid.svelte";

// jsdom has no layout, so the scroller's size, the grid's offset within it and
// a card's height are stubbed: a 1000x600 viewport, scrolled by `scrollY`.
// jsdom doesn't stretch cards to their row's height, which `measure`'s
// tallest-card reading doesn't need.
const VIEW_HEIGHT = 600;

let scroller: HTMLDivElement;
let viewWidth = 1000;
let scrollY = 0;
let cardHeight: number | ((n: number) => number) = 0;
const observers: Array<() => void> = [];

function rect(top: number, height: number): DOMRect {
  return { top, height, bottom: top + height, left: 0, right: viewWidth, width: viewWidth, x: 0, y: top, toJSON: () => ({}) };
}

beforeEach(() => {
  viewWidth = 1000;
  scrollY = 0;
  cardHeight = 0;
  observers.length = 0;
  scroller = document.createElement("div");
  scroller.style.overflowY = "auto";
  Object.defineProperty(scroller, "scrollTop", {
    get: () => scrollY,
    set: (y: number) => (scrollY = y),
  });
  document.body.append(scroller);

  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) {
    return this === scroller || this.parentElement === scroller ? viewWidth : 0;
  });
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this === scroller ? VIEW_HEIGHT : 0;
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this === scroller) return rect(0, VIEW_HEIGHT);
    if (this.parentElement === scroller) return rect(-scrollY, 0);
    if (this.dataset.card !== undefined) {
      return rect(0, typeof cardHeight === "number" ? cardHeight : cardHeight(Number(this.textContent)));
    }
    return rect(0, 0);
  });
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(private cb: () => void) {}
      observe() {
        observers.push(this.cb);
      }
      unobserve() {}
      disconnect() {}
    }
  );
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  scroller.remove();
});

const items = Array.from({ length: 100 }, (_, i) => i);
const item = createRawSnippet((n: () => number) => ({
  render: () => `<div data-card data-testid="card-${n()}">${n()}</div>`,
}));

function renderGrid(props: Partial<{ items: number[]; fit: boolean }> = {}) {
  return render(VirtualCardGrid<number>, {
    target: scroller,
    props: {
      items,
      key: (n: number) => String(n),
      item,
      minColumnWidth: 180,
      gap: 20,
      estimateRowHeight: () => 200,
      ...props,
    },
  });
}

function remeasure() {
  for (const cb of observers) cb();
  flushSync();
}

function scrollTo(y: number) {
  scrollY = y;
  scroller.dispatchEvent(new Event("scroll"));
  flushSync();
}

const mounted = () => [...scroller.querySelectorAll("[data-card]")].map((el) => Number(el.textContent));
const grid = () => scroller.firstElementChild as HTMLElement;
const rows = () => grid().firstElementChild as HTMLElement;

describe("VirtualCardGrid", () => {
  it("mounts only the rows near the viewport and reserves the full height", () => {
    renderGrid();
    // 1000px fits 5 columns of >=180px with 20px gaps; rows are 200 + 20 apart.
    expect(rows().style.gridTemplateColumns).toBe("repeat(5, minmax(0, 1fr))");
    expect(grid().style.height).toBe(`${20 * 220 - 20}px`);
    // Rows 0-2 are on screen, plus 2 overscan rows below.
    expect(mounted()).toEqual(items.slice(0, 25));
  });

  it("swaps rows as the view scrolls, and unmounts the ones scrolled past", () => {
    renderGrid();
    scrollTo(10 * 220);
    // Rows 10-12 on screen, 2 overscan rows each side: rows 8-14.
    expect(mounted()).toEqual(items.slice(40, 75));
    expect(rows().style.top).toBe(`${8 * 220}px`);

    scrollTo(20 * 220 - VIEW_HEIGHT);
    expect(mounted().at(-1)).toBe(99);
    expect(mounted()).not.toContain(0);

    scrollTo(0);
    expect(mounted()).toEqual(items.slice(0, 25));
  });

  it("lays rows out by a mounted card's measured height once it has one", () => {
    renderGrid();
    cardHeight = 280;
    for (const cb of observers) cb();
    flushSync();
    expect(grid().style.height).toBe(`${20 * 300 - 20}px`);
    scrollTo(10 * 300);
    // Rows 10-11 on screen at 300px apart, plus 2 overscan rows each side.
    expect(mounted()).toEqual(items.slice(40, 70));
  });

  it("holds every row to the tallest one, so mixed card heights share one stride", () => {
    // Odd rows have a taller card (an album with a genre chip), even rows don't.
    cardHeight = (n) => (Math.floor(n / 5) % 2 ? 292 : 280);
    renderGrid();
    remeasure();
    expect(rows().style.gridAutoRows).toBe("minmax(292px, auto)");
    const height = `${20 * 312 - 20}px`;
    expect(grid().style.height).toBe(height);
    // Whichever kind of row mounts first, the stride stays put (#1753).
    for (const row of [1, 2, 7, 10, 13]) {
      scrollTo(row * 312);
      remeasure();
      expect(grid().style.height).toBe(height);
      expect(rows().style.top).toBe(`${Math.max(0, row - 2) * 312}px`);
    }
  });

  it("only grows the stride, keeping the rows on screen in place as it does", () => {
    // Rows 0-9 are short; a taller row first mounts once scrolled down.
    cardHeight = (n) => (n < 50 ? 280 : 292);
    renderGrid();
    remeasure();
    expect(grid().style.height).toBe(`${20 * 300 - 20}px`);

    scrollTo(8 * 300);
    remeasure();
    // The rows above the view grew by 12px each: scroll by as much.
    expect(scrollY).toBe(8 * 312);
    expect(grid().style.height).toBe(`${20 * 312 - 20}px`);
    expect(rows().style.top).toBe(`${6 * 312}px`);

    // Back among the short rows, the stride doesn't shrink.
    scrollTo(0);
    remeasure();
    expect(grid().style.height).toBe(`${20 * 312 - 20}px`);
  });

  it("re-measures from scratch at a new column width", () => {
    cardHeight = 292;
    renderGrid();
    remeasure();
    expect(rows().style.gridAutoRows).toBe("minmax(292px, auto)");

    // 600px fits 3 columns; the narrower cards are shorter.
    viewWidth = 600;
    cardHeight = 250;
    remeasure();
    expect(rows().style.gridTemplateColumns).toBe("repeat(3, minmax(0, 1fr))");
    expect(rows().style.gridAutoRows).toBe("minmax(250px, auto)");
  });

  it("stretches a short list across the row with fit", () => {
    renderGrid({ items: [1, 2, 3], fit: true });
    expect(rows().style.gridTemplateColumns).toBe("repeat(3, minmax(0, 1fr))");
    expect(mounted()).toEqual([1, 2, 3]);
  });

  it("keeps the auto-fill columns for a short list without fit", () => {
    renderGrid({ items: [1, 2, 3] });
    expect(rows().style.gridTemplateColumns).toBe("repeat(5, minmax(0, 1fr))");
  });
});
