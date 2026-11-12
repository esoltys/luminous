import "@testing-library/jest-dom";
import { describe, it, expect, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import CollectionHeader from "./CollectionHeader.svelte";
import { windowLayoutStore } from "../stores/windowLayout.svelte";
import { HEIGHT_BREAKPOINT_SHORT_PX } from "../constants";

describe("CollectionHeader.svelte", () => {
  beforeEach(() => {
    windowLayoutStore.viewportHeight = HEIGHT_BREAKPOINT_SHORT_PX + 200;
  });

  it("renders the Expose heading and count subtitle", () => {
    const { getByRole, getByText } = render(CollectionHeader, {
      props: { title: "Albums", subtitle: "315 albums" },
    });

    const heading = getByRole("heading", { level: 1, name: "Albums" });
    expect(heading).toHaveClass("font-heading");
    expect(getByText("315 albums")).toBeInTheDocument();
  });

  it("uses full header padding at normal height and compacts when the window is short", () => {
    const { container, rerender } = render(CollectionHeader, {
      props: { title: "Albums", subtitle: "315 albums" },
    });

    const header = container.querySelector("header")!;
    expect(header).toHaveClass("px-6", "pt-6", "pb-4");

    windowLayoutStore.viewportHeight = HEIGHT_BREAKPOINT_SHORT_PX - 1;
    rerender({ title: "Albums", subtitle: "315 albums" });
    expect(header).toHaveClass("px-6", "py-3");
    expect(header).not.toHaveClass("pt-6");
  });
});
