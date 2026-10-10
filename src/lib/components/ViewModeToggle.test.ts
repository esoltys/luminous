import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import ViewModeToggle from "./ViewModeToggle.svelte";

describe("ViewModeToggle", () => {
  it("marks the active mode as pressed", () => {
    const { getByRole } = render(ViewModeToggle, { mode: "rows", onChange: vi.fn() });

    expect(getByRole("button", { name: "Row view" })).toHaveAttribute("aria-pressed", "true");
    expect(getByRole("button", { name: "Card view" })).toHaveAttribute("aria-pressed", "false");
  });

  it("reports the clicked mode", async () => {
    const onChange = vi.fn();
    const { getByRole } = render(ViewModeToggle, { mode: "cards", onChange });

    await fireEvent.click(getByRole("button", { name: "Row view" }));
    await fireEvent.click(getByRole("button", { name: "Card view" }));

    expect(onChange.mock.calls).toEqual([["rows"], ["cards"]]);
  });

  it("exposes the walkthrough anchor when given one", () => {
    const { container } = render(ViewModeToggle, { mode: "cards", onChange: vi.fn(), walkthroughTarget: "collection-view" });

    expect(container.querySelector('[data-walkthrough-target="collection-view"]')).toBeInTheDocument();
  });
});
