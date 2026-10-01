import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import ParametricBandStrip from "./ParametricBandStrip.svelte";
import type { EqRanges, ParametricBand } from "../types/equalizer";
import { i18n } from "../stores/i18n.svelte";

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
    onselect: vi.fn(),
    onchange: vi.fn(async () => {}),
    onadd: vi.fn(),
    onremove: vi.fn(),
  };
  return { ...render(ParametricBandStrip, { props }), props };
}

describe("ParametricBandStrip.svelte", () => {
  beforeEach(() => {
    i18n.currentLocale = "en";
  });

  it("bounds each input by the backend ranges", () => {
    const { getByLabelText } = setup();
    const freq = getByLabelText("Frequency 1");
    const gain = getByLabelText("Gain 1");
    const q = getByLabelText(/^Q.* 1$/);
    expect(freq).toHaveAttribute("min", "20");
    expect(freq).toHaveAttribute("max", "20000");
    expect(gain).toHaveAttribute("min", "-12");
    expect(gain).toHaveAttribute("max", "12");
    expect(q).toHaveAttribute("min", "0.1");
    expect(q).toHaveAttribute("max", "10");
  });

  it("shows f32 values at a readable precision in English", () => {
    i18n.currentLocale = "en";
    const { getByLabelText } = setup([{ kind: "low_shelf", freq: 31.25, gain_db: -0.800000011920929, q: 0.70710677, enabled: true }]);
    expect(getByLabelText("Frequency 1")).toHaveValue("31.3");
    expect(getByLabelText("Gain 1")).toHaveValue("-0.8");
    expect(getByLabelText(/^Q.* 1$/)).toHaveValue("0.71");
  });

  it("shows f32 values localized in French with commas", () => {
    i18n.currentLocale = "fr";
    const { getByLabelText } = setup([{ kind: "low_shelf", freq: 31.25, gain_db: -0.800000011920929, q: 0.70710677, enabled: true }]);
    expect(getByLabelText(/Fréquence.* 1/)).toHaveValue("31,3");
    expect(getByLabelText(/Gain.* 1/)).toHaveValue("-0,8");
    expect(getByLabelText(/^Q.* 1$/)).toHaveValue("0,71");
  });

  it("accepts comma as decimal separator when committing in French", async () => {
    i18n.currentLocale = "fr";
    const { getByLabelText, props } = setup([peak(60, 0), peak(1000)]);
    const gain = getByLabelText("Gain 1") as HTMLInputElement;
    gain.value = "2,5";
    await fireEvent.change(gain);
    expect(props.onchange).toHaveBeenCalledWith(0, peak(60, 2.5));
  });

  it("commits on Enter and shows the value the backend echoed", async () => {
    // The backend clamps 99 dB to the +12 dB the band already had, so no prop
    // change arrives; the input must still drop the typed 99.
    const { getByLabelText, props } = setup([peak(60, 12), peak(1000)]);
    const gain = getByLabelText("Gain 1") as HTMLInputElement;
    gain.value = "99";
    await fireEvent.keyDown(gain, { key: "Enter" });
    expect(props.onchange).toHaveBeenCalledWith(0, peak(60, 99));
    await waitFor(() => expect(gain.value).toBe("12"));
  });

  it("toggles and retypes a band", async () => {
    const { getByLabelText, getAllByRole, props } = setup();
    await fireEvent.click(getByLabelText("Enable band 2"));
    expect(props.onchange).toHaveBeenLastCalledWith(1, { ...peak(1000), enabled: false });
    const kind = getAllByRole("combobox")[0] as HTMLSelectElement;
    await fireEvent.change(kind, { target: { value: "low_shelf" } });
    expect(props.onchange).toHaveBeenLastCalledWith(0, { ...peak(60), kind: "low_shelf" });
  });

  it("adds a band in the widest free gap", async () => {
    const { getByRole, props } = setup();
    await fireEvent.click(getByRole("button", { name: "Add Band" }));
    // Widest log gap is 1 kHz-20 kHz; its geometric midpoint is 4472 Hz.
    expect(props.onadd).toHaveBeenCalledWith(4472);
  });

  it("disables add at max_bands and remove at min_bands", async () => {
    const full = setup([peak(60), peak(1000), peak(8000)]);
    expect(full.getByRole("button", { name: "Add Band" })).toBeDisabled();
    await fireEvent.click(full.getByRole("button", { name: "Remove band 3" }));
    expect(full.props.onremove).toHaveBeenCalledWith(2);
    full.unmount();

    const one = setup([peak(1000)]);
    expect(one.getByRole("button", { name: "Remove band 1" })).toBeDisabled();
    expect(one.getByRole("button", { name: "Add Band" })).toBeEnabled();
  });
});
