import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import CardSection from "./CardSection.svelte";
import { createRawSnippet } from "svelte";

const items = createRawSnippet(() => ({
  render: () => "<div><div>Item 1</div><div>Item 2</div></div>",
}));

describe("CardSection", () => {
  it("renders the title and its children inside a responsive grid", () => {
    const { getByText, getByTestId } = render(CardSection, { title: "Featured Albums", children: items });

    expect(getByText("Featured Albums")).toBeInTheDocument();
    const grid = getByTestId("card-section-grid");
    expect(grid).toContainElement(getByText("Item 1"));
    expect(grid.className).toContain("auto-fill");
  });

  it("has no scroll controls", () => {
    const { queryByRole } = render(CardSection, { title: "Albums", children: items });

    expect(queryByRole("button")).not.toBeInTheDocument();
  });

  it("renders the title as a button when onHeaderClick is provided", async () => {
    const onHeaderClick = vi.fn();
    const { getByRole } = render(CardSection, { title: "Albums", onHeaderClick, children: items });

    await fireEvent.click(getByRole("button", { name: "Albums" }));

    expect(onHeaderClick).toHaveBeenCalledOnce();
  });

  it("renders without a header when no title is given", () => {
    const { getByText, queryByRole } = render(CardSection, { children: items });

    expect(getByText("Item 2")).toBeInTheDocument();
    expect(queryByRole("heading")).not.toBeInTheDocument();
  });
});
