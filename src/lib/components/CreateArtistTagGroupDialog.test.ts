import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import CreateArtistTagGroupDialog from "./CreateArtistTagGroupDialog.svelte";

describe("CreateArtistTagGroupDialog", () => {
  it("renders Create Tag Group when no initial tags provided", () => {
    render(CreateArtistTagGroupDialog, {
      props: {
        onConfirm: vi.fn(),
        onCancel: vi.fn()
      }
    });

    expect(screen.getByText("Create Tag Group")).toBeTruthy();
    expect(screen.getByPlaceholderText("e.g. Award-Winning")).toBeTruthy();
  });

  it("renders Group Selected Tags when initial tags provided", () => {
    render(CreateArtistTagGroupDialog, {
      props: {
        initialTags: ["Juno Award", "Grammy Award"],
        onConfirm: vi.fn(),
        onCancel: vi.fn()
      }
    });

    expect(screen.getByText("Group Selected Tags")).toBeTruthy();
    expect(screen.getByText("Juno Award")).toBeTruthy();
    expect(screen.getByText("Grammy Award")).toBeTruthy();
    expect(screen.getByText("Group Selected")).toBeTruthy();
  });

  it("calls onConfirm with trimmed group name on submit", async () => {
    const onConfirm = vi.fn();
    render(CreateArtistTagGroupDialog, {
      props: {
        initialTags: ["Juno Award"],
        onConfirm,
        onCancel: vi.fn()
      }
    });

    const input = screen.getByPlaceholderText("e.g. Award-Winning");
    await fireEvent.input(input, { target: { value: "Award-Winning" } });

    const submitBtn = screen.getByText("Group Selected");
    await fireEvent.click(submitBtn);

    expect(onConfirm).toHaveBeenCalledWith("Award-Winning");
  });

  it("calls onCancel when Cancel button is clicked", async () => {
    const onCancel = vi.fn();
    render(CreateArtistTagGroupDialog, {
      props: {
        onConfirm: vi.fn(),
        onCancel
      }
    });

    const cancelBtn = screen.getByText("Cancel");
    await fireEvent.click(cancelBtn);

    expect(onCancel).toHaveBeenCalled();
  });
});
