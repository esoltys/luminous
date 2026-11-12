import "@testing-library/jest-dom";
import { describe, it, expect } from "vitest";
import { createRawSnippet } from "svelte";
import { render } from "@testing-library/svelte";
import PlaylistCardShell from "./PlaylistCardShell.svelte";
import { cardAppearanceContext, type CardAppearance } from "../utils/cardAppearance";

const cover = createRawSnippet(() => ({ render: () => "<span data-testid='cover'></span>" }));

function renderShell(appearance?: CardAppearance) {
  return render(PlaylistCardShell, {
    props: {
      title: "Evening Mix",
      subtitleLabel: "Moment mix",
      updatedLabel: "2 days ago",
      trackCount: 299,
      onClick: () => {},
      cover,
    },
    context: appearance ? cardAppearanceContext(appearance) : undefined,
  });
}

describe("PlaylistCardShell appearance", () => {
  it("draws a filled, bordered box by default", () => {
    const { container } = renderShell();
    const root = container.firstElementChild as HTMLElement;
    expect(root.classList).toContain("bg-brand-sidebar");
    expect(root.classList).toContain("border");
  });

  it("drops the box and folds the kind and song count onto one line when plain", () => {
    const { container, getByText, queryByText } = renderShell("plain");
    const root = container.firstElementChild as HTMLElement;
    expect(root.classList).not.toContain("bg-brand-sidebar");
    expect(root.classList).not.toContain("border");
    expect(getByText("Moment mix, 299 songs")).toBeInTheDocument();
    expect(queryByText("2 days ago")).not.toBeInTheDocument();
  });

  it("keeps the cover's hover outline when plain, since the box no longer carries it", () => {
    const { getByTestId } = renderShell("plain");
    expect(getByTestId("cover").parentElement!.className).toContain(
      "group-hover:outline-brand-accent",
    );
  });
});
