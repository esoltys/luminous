import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import ConfirmDialog from "./ConfirmDialog.svelte";

describe("ConfirmDialog.svelte", () => {
  it("renders with theme text and colors, avoiding hardcoded red classes", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();

    const { getByText, container } = render(ConfirmDialog, {
      props: {
        title: "Confirm Edition Preservation",
        message: "Are you sure you want to preserve this edition?",
        confirmLabel: "Preserve Edition",
        cancelLabel: "Cancel",
        danger: true,
        onConfirm,
        onCancel,
      },
    });

    expect(getByText("Confirm Edition Preservation")).toBeInTheDocument();
    const confirmBtn = getByText("Preserve Edition");
    expect(confirmBtn).toBeInTheDocument();

    // Confirm button and icon must use theme classes, not hardcoded red
    expect(confirmBtn.className).not.toContain("text-red");
    expect(confirmBtn.className).not.toContain("bg-red");
    expect(confirmBtn.className).not.toContain("border-red");

    const svgIcons = container.querySelectorAll("svg");
    svgIcons.forEach((svg) => {
      expect(svg.className.baseVal || svg.getAttribute("class") || "").not.toContain("text-red");
    });
  });

  it("calls onConfirm and onCancel callbacks", async () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();

    const { getByText } = render(ConfirmDialog, {
      props: {
        title: "Test",
        message: "Test message",
        confirmLabel: "Confirm",
        cancelLabel: "Cancel",
        onConfirm,
        onCancel,
      },
    });

    await fireEvent.click(getByText("Confirm"));
    expect(onConfirm).toHaveBeenCalled();

    await fireEvent.click(getByText("Cancel"));
    expect(onCancel).toHaveBeenCalled();
  });
});
