import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import HorizontalScrollRow from "./HorizontalScrollRow.svelte";
import { createRawSnippet } from "svelte";

describe("HorizontalScrollRow", () => {
  it("renders title and scroll buttons when title is provided", () => {
    const childrenSnippet = createRawSnippet(() => ({
      render: () => "<div><div>Item 1</div><div>Item 2</div></div>",
    }));

    const { getByText, getByTitle } = render(HorizontalScrollRow, {
      title: "Featured Albums",
      children: childrenSnippet,
    });

    expect(getByText("Featured Albums")).toBeInTheDocument();
    expect(getByTitle("Scroll left")).toBeInTheDocument();
    expect(getByTitle("Scroll right")).toBeInTheDocument();
  });

  it("renders children content inside scroll container", () => {
    const childrenSnippet = createRawSnippet(() => ({
      render: () => '<div data-testid="carousel-item">Test Item</div>',
    }));

    const { getByTestId } = render(HorizontalScrollRow, {
      children: childrenSnippet,
    });

    expect(getByTestId("carousel-item")).toBeInTheDocument();
  });
});
