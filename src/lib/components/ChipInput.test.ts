import "@testing-library/jest-dom";
import { describe, it, expect } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import ChipInput from "./ChipInput.svelte";

describe("ChipInput.svelte", () => {
  it("renders one chip per value in the delimited value", () => {
    const { getByText } = render(ChipInput, { value: "Rock; Jazz Fusion; Live" });
    expect(getByText("Rock")).toBeInTheDocument();
    expect(getByText("Jazz Fusion")).toBeInTheDocument();
    expect(getByText("Live")).toBeInTheDocument();
  });

  it("renders no chips for an empty value", () => {
    const { queryByRole } = render(ChipInput, { value: "" });
    expect(queryByRole("button")).not.toBeInTheDocument();
  });

  it("commits a typed value on Enter and clears the draft", async () => {
    const { getByRole, getByText } = render(ChipInput, { value: "Rock" });
    const input = getByRole("textbox") as HTMLInputElement;

    await fireEvent.input(input, { target: { value: "Blues" } });
    await fireEvent.keyDown(input, { key: "Enter" });

    expect(getByText("Blues")).toBeInTheDocument();
    expect(input.value).toBe("");
  });

  it("commits a typed value on comma", async () => {
    const { getByRole, getByText } = render(ChipInput, { value: "" });
    const input = getByRole("textbox") as HTMLInputElement;

    await fireEvent.input(input, { target: { value: "Metal" } });
    await fireEvent.keyDown(input, { key: "," });

    expect(getByText("Metal")).toBeInTheDocument();
    expect(input.value).toBe("");
  });

  it("commits the draft on blur, not just Enter/comma", async () => {
    const { getByRole, getByText } = render(ChipInput, { value: "" });
    const input = getByRole("textbox") as HTMLInputElement;

    await fireEvent.input(input, { target: { value: "Ambient" } });
    await fireEvent.blur(input);

    expect(getByText("Ambient")).toBeInTheDocument();
  });

  it("splits a comma-separated paste into multiple chips at once", async () => {
    const { getByRole, getByText } = render(ChipInput, { value: "" });
    const input = getByRole("textbox") as HTMLInputElement;

    await fireEvent.input(input, { target: { value: "Rock, Blues, Jazz" } });
    await fireEvent.keyDown(input, { key: "Enter" });

    expect(getByText("Rock")).toBeInTheDocument();
    expect(getByText("Blues")).toBeInTheDocument();
    expect(getByText("Jazz")).toBeInTheDocument();
  });

  it("removes a chip when its remove button is clicked", async () => {
    const { getByRole, getByText, queryByText } = render(ChipInput, {
      value: "Rock; Blues",
    });

    await fireEvent.click(getByRole("button", { name: "Remove Rock" }));

    expect(queryByText("Rock")).not.toBeInTheDocument();
    expect(getByText("Blues")).toBeInTheDocument();
  });

  it("removes the last chip on Backspace when the draft is empty", async () => {
    const { getByRole, queryByText } = render(ChipInput, { value: "Rock; Blues" });
    const input = getByRole("textbox") as HTMLInputElement;

    await fireEvent.keyDown(input, { key: "Backspace" });

    expect(queryByText("Blues")).not.toBeInTheDocument();
  });

  it("does not remove a chip on Backspace while the draft still has text", async () => {
    const { getByRole, getByText } = render(ChipInput, { value: "Rock" });
    const input = getByRole("textbox") as HTMLInputElement;

    await fireEvent.input(input, { target: { value: "Blu" } });
    await fireEvent.keyDown(input, { key: "Backspace" });

    expect(getByText("Rock")).toBeInTheDocument();
  });

  it("deduplicates case-insensitively, keeping the first-seen casing", async () => {
    const { getByRole, getByText, queryAllByText } = render(ChipInput, { value: "Rock" });
    const input = getByRole("textbox") as HTMLInputElement;

    await fireEvent.input(input, { target: { value: "rock" } });
    await fireEvent.keyDown(input, { key: "Enter" });

    expect(queryAllByText(/rock/i)).toHaveLength(1);
    expect(getByText("Rock")).toBeInTheDocument();
  });

  it("does not add an empty chip for whitespace-only input", async () => {
    const { getByRole, queryByRole } = render(ChipInput, { value: "" });
    const input = getByRole("textbox") as HTMLInputElement;

    await fireEvent.input(input, { target: { value: "   " } });
    await fireEvent.keyDown(input, { key: "Enter" });

    expect(queryByRole("button")).not.toBeInTheDocument();
  });

  it("hides remove buttons and dims the input when disabled", () => {
    const { getByRole, queryByRole } = render(ChipInput, {
      value: "Rock",
      disabled: true,
    });

    expect(queryByRole("button")).not.toBeInTheDocument();
    expect((getByRole("textbox") as HTMLInputElement).disabled).toBe(true);
  });

  it("is field-agnostic — works the same for a non-genre value list like artists", async () => {
    const { getByRole, getByText } = render(ChipInput, {
      value: "Artist A",
      placeholder: "Add an artist...",
    });
    const input = getByRole("textbox") as HTMLInputElement;

    expect(getByText("Artist A")).toBeInTheDocument();

    await fireEvent.input(input, { target: { value: "Artist B" } });
    await fireEvent.keyDown(input, { key: "Enter" });

    expect(getByText("Artist B")).toBeInTheDocument();
  });
});
