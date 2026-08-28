import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/svelte";
import ArtistTagChips from "./ArtistTagChips.svelte";

describe("ArtistTagChips", () => {
  it("renders compact format with +N badge when multiple tags are passed", () => {
    render(ArtistTagChips, { props: { tags: ["canadian", "rock", "female vocalists"], variant: "compact" } });

    expect(screen.getByText("canadian")).toBeTruthy();
    expect(screen.getByText("+2")).toBeTruthy();
  });

  it("renders single tag without badge when only one tag exists", () => {
    render(ArtistTagChips, { props: { tags: ["canadian"], variant: "compact" } });

    expect(screen.getByText("canadian")).toBeTruthy();
    expect(screen.queryByText("+0")).toBeNull();
  });

  it("renders all tags in full variant", () => {
    render(ArtistTagChips, { props: { tags: ["canadian", "rock"], variant: "full" } });

    expect(screen.getByText("canadian")).toBeTruthy();
    expect(screen.getByText("rock")).toBeTruthy();
  });
});
