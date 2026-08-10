import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import Select from "./Select.svelte";
import { createRawSnippet } from "svelte";

describe("Select.svelte", () => {
  it("renders select element with passed props and classes", () => {
    const handleChange = vi.fn();
    const children = createRawSnippet(() => ({
      render: () => '<optgroup label="Opts"><option value="opt1">Option 1</option><option value="opt2">Option 2</option></optgroup>',
    }));

    const { getByRole } = render(Select, {
      props: {
        id: "test-select",
        value: "opt1",
        class: "rounded-full pl-3.5 pr-8",
        chevronPosition: "0.625rem",
        onchange: handleChange,
        children,
      },
    });

    const select = getByRole("combobox") as HTMLSelectElement;
    expect(select).toBeInTheDocument();
    expect(select.id).toBe("test-select");
    expect(select.value).toBe("opt1");
    expect(select.className).toContain("pl-3.5");
    expect(select.className).toContain("pr-8");
  });

  it("handles change events", async () => {
    const handleChange = vi.fn();
    const children = createRawSnippet(() => ({
      render: () => '<optgroup label="Opts"><option value="opt1">Option 1</option><option value="opt2">Option 2</option></optgroup>',
    }));

    const { getByRole } = render(Select, {
      props: {
        id: "test-select",
        value: "opt1",
        class: "rounded-full pl-3.5 pr-8",
        onchange: handleChange,
        children,
      },
    });

    const select = getByRole("combobox");
    await fireEvent.change(select, { target: { value: "opt2" } });
    expect(handleChange).toHaveBeenCalledTimes(1);
  });
});
