import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import EqPresetPicker from "./EqPresetPicker.svelte";
import type { EqPresetList } from "../types/equalizer";

describe("EqPresetPicker.svelte", () => {
  const samplePresets: EqPresetList = {
    builtin: ["Flat", "Rock", "Pop"],
    user: [{ id: 10, name: "Acoustic" }],
  };

  const samplePreviews = new Map<string, number[]>([
    ["Flat", [0, 0, 0, 0]],
    ["Rock", [4, 2, -1, 3]],
    ["Pop", [-1, 2, 3, 1]],
    ["user:10", [2, 1, 0, -1]],
  ]);

  it("renders trigger with Custom when value is null", () => {
    const onselect = vi.fn();
    const { getByRole } = render(EqPresetPicker, {
      props: {
        value: null,
        presets: samplePresets,
        mode: "graphic10",
        previews: samplePreviews,
        onselect,
      },
    });

    const trigger = getByRole("combobox");
    expect(trigger).toHaveTextContent("Custom");
  });

  it("renders selected preset and sparkline in trigger button", () => {
    const onselect = vi.fn();
    const { getByRole } = render(EqPresetPicker, {
      props: {
        value: "Rock",
        presets: samplePresets,
        mode: "graphic10",
        previews: samplePreviews,
        onselect,
      },
    });

    const trigger = getByRole("combobox");
    expect(trigger).toHaveTextContent("Rock");
    const svg = trigger.querySelector("svg[aria-hidden='true']");
    expect(svg).toBeInTheDocument();
  });

  it("opens listbox on click and renders built-in presets with sparklines", async () => {
    const onselect = vi.fn();
    const { getByRole, getAllByRole } = render(EqPresetPicker, {
      props: {
        value: "Rock",
        presets: samplePresets,
        mode: "graphic10",
        previews: samplePreviews,
        onselect,
      },
    });

    const trigger = getByRole("combobox");
    await fireEvent.click(trigger);

    const listbox = getByRole("listbox");
    expect(listbox).toBeInTheDocument();

    const options = getAllByRole("option");
    // In graphic mode: 3 built-in options
    expect(options).toHaveLength(3);
    expect(options[0]).toHaveTextContent("Flat");
    expect(options[1]).toHaveTextContent("Rock");
    expect(options[2]).toHaveTextContent("Pop");

    // Each option has an aria-hidden sparkline SVG
    const sparklines = listbox.querySelectorAll("svg[aria-hidden='true']");
    expect(sparklines.length).toBeGreaterThanOrEqual(3);
  });

  it("shows user presets group in parametric mode", async () => {
    const onselect = vi.fn();
    const { getByRole, getAllByRole } = render(EqPresetPicker, {
      props: {
        value: "user:10",
        presets: samplePresets,
        mode: "parametric",
        previews: samplePreviews,
        onselect,
      },
    });

    const trigger = getByRole("combobox");
    await fireEvent.click(trigger);

    const groups = getAllByRole("group");
    expect(groups).toHaveLength(2);
    expect(groups[0]).toHaveAttribute("aria-label", "Built-in");
    expect(groups[1]).toHaveAttribute("aria-label", "My presets");

    const userOpt = getByRole("option", { name: /acoustic/i });
    expect(userOpt).toBeInTheDocument();
  });

  it("selects a preset when clicked and invokes onselect", async () => {
    const onselect = vi.fn();
    const { getByRole } = render(EqPresetPicker, {
      props: {
        value: "Flat",
        presets: samplePresets,
        mode: "parametric",
        previews: samplePreviews,
        onselect,
      },
    });

    const trigger = getByRole("combobox");
    await fireEvent.click(trigger);

    const userOpt = getByRole("option", { name: /acoustic/i });
    await fireEvent.click(userOpt);

    expect(onselect).toHaveBeenCalledWith("user:10");
  });
});
