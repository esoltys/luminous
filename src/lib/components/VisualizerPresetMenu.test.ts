import { fireEvent, render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { visualizerStore } from "../stores/visualizer.svelte";
import { vuMeterSettings } from "../stores/vuMeterSettings.svelte";
import { autoReferenceDbfs } from "../visualizers/vuMeter";
import VisualizerPresetMenu from "./VisualizerPresetMenu.svelte";

function openMenu() {
  return render(VisualizerPresetMenu, { x: 10, y: 10, onClose: () => {} });
}

describe("VisualizerPresetMenu.svelte VU meter calibration", () => {
  beforeEach(() => {
    localStorage.clear();
    vuMeterSettings.setAuto();
    visualizerStore.select("voxel");
  });

  afterEach(() => {
    vuMeterSettings.setAuto();
    visualizerStore.select("voxel");
  });

  it("offers no calibration for other visualizers", () => {
    openMenu();
    expect(screen.queryByTestId("vu-reference-auto")).toBeNull();
  });

  it("shows Auto, checked, with the level it is using", () => {
    visualizerStore.select("vu-meter");
    openMenu();
    const auto = screen.getByTestId("vu-reference-auto");
    expect(auto).toHaveAttribute("aria-checked", "true");
    expect(auto).toHaveTextContent(String(Math.round(autoReferenceDbfs())).replace("-", "−"));
    expect(screen.queryByTestId("vu-reference-slider")).toBeNull();
  });

  it("leaves Auto at the level it was using, then follows the slider", async () => {
    visualizerStore.select("vu-meter");
    openMenu();
    await fireEvent.click(screen.getByTestId("vu-reference-auto"));
    await tick();

    expect(screen.getByTestId("vu-reference-auto")).toHaveAttribute("aria-checked", "false");
    expect(vuMeterSettings.referenceDbfs).toBe(Math.round(autoReferenceDbfs()));

    const slider = screen.getByTestId("vu-reference-slider") as HTMLInputElement;
    await fireEvent.input(slider, { target: { value: "-9" } });
    expect(vuMeterSettings.referenceDbfs).toBe(-9);
  });

  it("returns to Auto and drops the slider", async () => {
    visualizerStore.select("vu-meter");
    vuMeterSettings.setManual(-9);
    openMenu();
    expect(screen.getByTestId("vu-reference-slider")).toBeInTheDocument();

    await fireEvent.click(screen.getByTestId("vu-reference-auto"));
    await tick();

    expect(vuMeterSettings.referenceDbfs).toBeNull();
    expect(screen.queryByTestId("vu-reference-slider")).toBeNull();
  });
});
