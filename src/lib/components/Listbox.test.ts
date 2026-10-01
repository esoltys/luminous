import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import Listbox, { type ListboxItem, type ListboxGroup } from "./Listbox.svelte";

describe("Listbox.svelte", () => {
  const sampleItems: ListboxItem[] = [
    { id: "item-1", value: "opt1", label: "Option One", group: "g1" },
    { id: "item-2", value: "opt2", label: "Option Two", group: "g1" },
    { id: "item-3", value: "opt3", label: "Option Three", group: "g2" },
    { id: "item-disabled", value: "dis", label: "Disabled Option", disabled: true },
  ];

  const sampleGroups: ListboxGroup[] = [
    { id: "g1", label: "Group One" },
    { id: "g2", label: "Group Two" },
  ];

  it("renders trigger button with placeholder when value is null", () => {
    const onselect = vi.fn();
    const { getByRole } = render(Listbox, {
      props: {
        id: "test-listbox",
        value: null,
        items: sampleItems,
        placeholder: "Select an option",
        onselect,
      },
    });

    const trigger = getByRole("combobox");
    expect(trigger).toHaveTextContent("Select an option");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
  });

  it("renders selected item label when value is set", () => {
    const onselect = vi.fn();
    const { getByRole } = render(Listbox, {
      props: {
        id: "test-listbox",
        value: "opt2",
        items: sampleItems,
        placeholder: "Select an option",
        onselect,
      },
    });

    const trigger = getByRole("combobox");
    expect(trigger).toHaveTextContent("Option Two");
  });

  it("opens listbox on click and displays options and groups", async () => {
    const onselect = vi.fn();
    const { getByRole, getAllByRole } = render(Listbox, {
      props: {
        id: "test-listbox",
        value: "opt1",
        items: sampleItems,
        groups: sampleGroups,
        placeholder: "Select an option",
        onselect,
      },
    });

    const trigger = getByRole("combobox");
    await fireEvent.click(trigger);

    expect(trigger).toHaveAttribute("aria-expanded", "true");
    const listbox = getByRole("listbox");
    expect(listbox).toBeInTheDocument();

    const groups = getAllByRole("group");
    expect(groups).toHaveLength(2);
    expect(groups[0]).toHaveAttribute("aria-label", "Group One");
    expect(groups[1]).toHaveAttribute("aria-label", "Group Two");
  });

  it("selects an option when clicked and calls onselect", async () => {
    const onselect = vi.fn();
    const { getByRole } = render(Listbox, {
      props: {
        id: "test-listbox",
        value: "opt1",
        items: sampleItems,
        placeholder: "Select an option",
        onselect,
      },
    });

    const trigger = getByRole("combobox");
    await fireEvent.click(trigger);

    const opt3 = getByRole("option", { name: "Option Three" });
    await fireEvent.click(opt3);

    expect(onselect).toHaveBeenCalledWith("opt3", sampleItems[2]);
    expect(trigger).toHaveAttribute("aria-expanded", "false");
  });

  it("does not select disabled options", async () => {
    const onselect = vi.fn();
    const { getByRole } = render(Listbox, {
      props: {
        id: "test-listbox",
        value: "opt1",
        items: sampleItems,
        placeholder: "Select an option",
        onselect,
      },
    });

    const trigger = getByRole("combobox");
    await fireEvent.click(trigger);

    const disabledOpt = getByRole("option", { name: "Disabled Option" });
    expect(disabledOpt).toHaveAttribute("aria-disabled", "true");
    await fireEvent.click(disabledOpt);

    expect(onselect).not.toHaveBeenCalled();
    expect(trigger).toHaveAttribute("aria-expanded", "true");
  });

  it("navigates options via keyboard arrows and selects with Enter", async () => {
    const onselect = vi.fn();
    const { getByRole } = render(Listbox, {
      props: {
        id: "test-listbox",
        value: "opt1",
        items: sampleItems,
        placeholder: "Select an option",
        onselect,
      },
    });

    const trigger = getByRole("combobox");
    // Open via ArrowDown
    await fireEvent.keyDown(trigger, { key: "ArrowDown" });
    expect(trigger).toHaveAttribute("aria-expanded", "true");

    // Move to next option (Option Two)
    await fireEvent.keyDown(trigger, { key: "ArrowDown" });
    expect(trigger).toHaveAttribute("aria-activedescendant", "item-2");

    // Select with Enter
    await fireEvent.keyDown(trigger, { key: "Enter" });
    expect(onselect).toHaveBeenCalledWith("opt2", sampleItems[1]);
    expect(trigger).toHaveAttribute("aria-expanded", "false");
  });

  it("closes dropdown on Escape", async () => {
    const onselect = vi.fn();
    const { getByRole, queryByRole } = render(Listbox, {
      props: {
        id: "test-listbox",
        value: "opt1",
        items: sampleItems,
        placeholder: "Select an option",
        onselect,
      },
    });

    const trigger = getByRole("combobox");
    await fireEvent.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");

    await fireEvent.keyDown(trigger, { key: "Escape" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    await waitFor(() => expect(queryByRole("listbox")).toBeNull());
  });

  it("supports typeahead navigation", async () => {
    const onselect = vi.fn();
    const { getByRole } = render(Listbox, {
      props: {
        id: "test-listbox",
        value: null,
        items: [
          { id: "apple", value: "apple", label: "Apple" },
          { id: "banana", value: "banana", label: "Banana" },
          { id: "cherry", value: "cherry", label: "Cherry" },
        ],
        placeholder: "Choose fruit",
        onselect,
      },
    });

    const trigger = getByRole("combobox");
    await fireEvent.click(trigger);

    // Type 'c'
    await fireEvent.keyDown(trigger, { key: "c" });
    expect(trigger).toHaveAttribute("aria-activedescendant", "cherry");
  });
});
