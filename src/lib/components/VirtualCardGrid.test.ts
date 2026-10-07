import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render } from "@testing-library/svelte";
import { createRawSnippet, flushSync } from "svelte";
import VirtualCardGrid from "./VirtualCardGrid.svelte";

// jsdom has no layout, so the scroller's size, the grid's offset within it and
// a card's height are stubbed: a 1000x600 viewport, scrolled by `scrollY`.
const VIEW_WIDTH = 1000;
const VIEW_HEIGHT = 600;

let scroller: HTMLDivElement;
let scrollY = 0;
let cardHeight = 0;
const observers: Array<() => void> = [];

function rect(top: number, height: number): DOMRect {
  return { top, height, bottom: top + height, left: 0, right: VIEW_WIDTH, width: VIEW_WIDTH, x: 0, y: top, toJSON: () => ({}) };
}

beforeEach(() => {
  scrollY = 0;
  cardHeight = 0;
  observers.length = 0;
  scroller = document.createElement("div");
  scroller.style.overflowY = "auto";
  document.body.append(scroller);

  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) {
    return this === scroller || this.parentElement === scroller ? VIEW_WIDTH : 0;
  });
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this === scroller ? VIEW_HEIGHT : 0;
  });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this === scroller) return rect(0, VIEW_HEIGHT);
    if (this.parentElement === scroller) return rect(-scrollY, 0);
    if (this.dataset.card !== undefined) return rect(0, cardHeight);
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
