import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import ParametricGraph from "./ParametricGraph.svelte";
import type { EqRanges, ParametricBand } from "../types/equalizer";

const ranges: EqRanges = {
  freq: { min: 20, max: 20000 },
  gain_db: { min: -12, max: 12 },
  q: { min: 0.1, max: 10 },
  preamp: { min: -12, max: 12 },
  max_bands: 3,
  min_bands: 1,
};

const peak = (freq: number, gain_db = 0, q = 1): ParametricBand => ({ kind: "peak", freq, gain_db, q, enabled: true });

function setup(bands: ParametricBand[] = [peak(60), peak(1000)]) {
  const props = {
    bands,
    selected: 0,
    ranges,
    active: true,
    response: [],
    bandResponse: [],
    onselect: vi.fn(),
    onchange: vi.fn(),
    onadd: vi.fn(),
    onremove: vi.fn(),
  };
  const utils = render(ParametricGraph, { props });
  const node = (idx: number) => utils.container.querySelector<HTMLElement>(`[data-node="${idx}"]`)!;
  return { ...utils, props, node };
}

function mockPlotBox(plot: HTMLElement) {
  plot.getBoundingClientRect = () =>
    ({ left: 0, top: 0, width: 300, height: 200, right: 300, bottom: 200, x: 0, y: 0, toJSON: () => ({}) }) as DOMRect;
  return plot;
}

describe("ParametricGraph.svelte", () => {
  it("labels each node with its kind and values", () => {
    const { getByRole } = setup([
      { kind: "low_shelf", freq: 105, gain_db: 3, q: 0.71, enabled: true },
      peak(1000, -2.5, 1.4),
    ]);
    expect(getByRole("button", { name: "Band 1: Low shelf, 105 Hz, +3.0 dB, Q 0.71" })).toBeInTheDocument();
    expect(getByRole("button", { name: "Band 2: Peak, 1000 Hz, -2.5 dB, Q 1.40" })).toBeInTheDocument();
  });

  it("nudges frequency by 1/24 octave, or 1/3 with Shift", async () => {
    const { node, props } = setup();
    await fireEvent.keyDown(node(1), { key: "ArrowRight" });
    expect(props.onchange).toHaveBeenLastCalledWith(1, peak(1029));
    await fireEvent.keyDown(node(1), { key: "ArrowLeft", shiftKey: true });
    expect(props.onchange).toHaveBeenLastCalledWith(1, peak(794));
  });

  it("nudges gain by 0.1 dB, or 1 dB with Shift, without float noise", async () => {
    const { node, props } = setup([peak(60, 0.2), peak(1000)]);
    await fireEvent.keyDown(node(0), { key: "ArrowUp" });
    expect(props.onchange).toHaveBeenLastCalledWith(0, peak(60, 0.3));
    await fireEvent.keyDown(node(0), { key: "ArrowDown", shiftKey: true });
    expect(props.onchange).toHaveBeenLastCalledWith(0, peak(60, -0.8));
  });

  it("clamps a gain nudge to the backend range", async () => {
    const { node, props } = setup([peak(60, 12), peak(1000)]);
    await fireEvent.keyDown(node(0), { key: "ArrowUp", shiftKey: true });
    expect(props.onchange).toHaveBeenLastCalledWith(0, peak(60, 12));
  });

  it("scales Q with Page Up/Down", async () => {
    const { node, props } = setup();
    await fireEvent.keyDown(node(0), { key: "PageUp" });
    expect(props.onchange).toHaveBeenLastCalledWith(0, peak(60, 0, 1.05));
    await fireEvent.keyDown(node(0), { key: "PageDown", shiftKey: true });
    expect(props.onchange).toHaveBeenLastCalledWith(0, peak(60, 0, 0.8));
  });

  it("removes the focused band with Delete, but not the last one", async () => {
    const two = setup();
    await fireEvent.keyDown(two.node(1), { key: "Delete" });
    expect(two.props.onremove).toHaveBeenCalledWith(1);
    two.unmount();

    const one = setup([peak(1000)]);
    await fireEvent.keyDown(one.node(0), { key: "Backspace" });
    expect(one.props.onremove).not.toHaveBeenCalled();
  });

  it("selects a node on pointer down", async () => {
    const { node, props } = setup();
    await fireEvent.pointerDown(node(1), { button: 0, pointerId: 1 });
    expect(props.onselect).toHaveBeenCalledWith(1);
  });

  it("adds a band at the double-clicked frequency", async () => {
    const { getByTestId, props } = setup();
    // Halfway along a 20 Hz-20 kHz log axis is sqrt(20 * 20000) = 632 Hz.
    await fireEvent.dblClick(mockPlotBox(getByTestId("eq-plot")), { clientX: 150, clientY: 100 });
    expect(props.onadd).toHaveBeenCalledWith(632);
  });

  it("does not add past the band limit, or when double-clicking a node", async () => {
    const full = setup([peak(60), peak(1000), peak(8000)]);
    await fireEvent.dblClick(mockPlotBox(full.getByTestId("eq-plot")), { clientX: 150, clientY: 100 });
    expect(full.props.onadd).not.toHaveBeenCalled();
    full.unmount();

    const some = setup();
    mockPlotBox(some.getByTestId("eq-plot"));
    await fireEvent.dblClick(some.node(0), { clientX: 150, clientY: 100 });
    expect(some.props.onadd).not.toHaveBeenCalled();
  });

  it("drags a captured node to the pointer position until release", async () => {
    const { getByTestId, node, props } = setup();
    mockPlotBox(getByTestId("eq-plot"));
    await fireEvent.pointerDown(node(0), { button: 0, pointerId: 1 });
    // The top edge of the plot is above +12 dB (the curve keeps headroom), so gain clamps to +12.
    await fireEvent.pointerMove(node(0), { pointerId: 1, clientX: 150, clientY: 0 });
    expect(props.onchange).toHaveBeenLastCalledWith(0, peak(632, 12));
    await fireEvent.pointerUp(node(0), { pointerId: 1 });
    props.onchange.mockClear();
    await fireEvent.pointerMove(node(0), { pointerId: 1, clientX: 10, clientY: 10 });
    expect(props.onchange).not.toHaveBeenCalled();
  });
});
