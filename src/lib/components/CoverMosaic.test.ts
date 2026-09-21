import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import { render } from "@testing-library/svelte";
import CoverMosaic from "./CoverMosaic.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(""),
}));

const cover = (songId: number) => ({ songId, artAutomatic: `cover${songId}.jpg` });

function tiles(container: HTMLElement) {
  return container.querySelectorAll(".bg-brand-sidebar.border-brand-border");
}

function aspectRatio(container: HTMLElement) {
  return (container.firstElementChild as HTMLElement).style.aspectRatio;
}

describe("CoverMosaic.svelte", () => {
  it("shows the fallback initial when there are no covers", () => {
    const { getByText } = render(CoverMosaic, {
      props: { covers: [], fallbackName: "Radiohead" },
    });

    expect(getByText("R")).toBeInTheDocument();
  });

  it("renders a single square tile for one cover", () => {
    const { container } = render(CoverMosaic, {
      props: { covers: [cover(1)] },
    });

    expect(tiles(container).length).toBe(1);
    expect(aspectRatio(container)).toBe("1 / 1");
  });

  it("renders 1 full tile + 1 quarter tile for 2 covers (never two equal tiles)", () => {
    const { container } = render(CoverMosaic, {
      props: { covers: [cover(1), cover(2)] },
    });

    expect(tiles(container).length).toBe(2);
    expect(aspectRatio(container)).toBe("1.5 / 1");
  });

  it("renders 1 full tile + 2 stacked quarter tiles for 3 covers", () => {
    const covers = [1, 2, 3].map(cover);
    const { container } = render(CoverMosaic, { props: { covers } });

    expect(tiles(container).length).toBe(3);
    expect(aspectRatio(container)).toBe("1.5 / 1");
  });

  it("renders 1 full tile + 3 quarter tiles for 4 covers (never four equal tiles)", () => {
    const covers = [1, 2, 3, 4].map(cover);
    const { container } = render(CoverMosaic, { props: { covers } });

    expect(tiles(container).length).toBe(4);
    expect(aspectRatio(container)).toBe("2 / 1");
  });

  it("renders 1 full tile + a 2x2 block of 4 quarter tiles for 5 covers", () => {
    const covers = [1, 2, 3, 4, 5].map(cover);
    const { container } = render(CoverMosaic, { props: { covers } });

    expect(tiles(container).length).toBe(5);
    expect(aspectRatio(container)).toBe("2 / 1");
  });

  it("caps the mosaic at 5 covers even when more are supplied", () => {
    const covers = [1, 2, 3, 4, 5, 6, 7].map(cover);
    const { container } = render(CoverMosaic, { props: { covers } });

    expect(tiles(container).length).toBe(5);
  });

  it("respects a maxCovers prop lower than 5", () => {
    const covers = [1, 2, 3, 4, 5].map(cover);
    const { container } = render(CoverMosaic, { props: { covers, maxCovers: 3 } });

    expect(tiles(container).length).toBe(3);
    expect(aspectRatio(container)).toBe("1.5 / 1");
  });

  describe("heroImageUrl", () => {
    it("renders the hero image alone as a single square tile when there are no covers", () => {
      const { container, getByAltText } = render(CoverMosaic, {
        props: { heroImageUrl: "https://example.com/portrait.jpg", heroImageAlt: "Shania Twain", covers: [] },
      });

      expect(getByAltText("Shania Twain")).toHaveAttribute("src", "https://example.com/portrait.jpg");
      expect(aspectRatio(container)).toBe("1 / 1");
    });

    it("uses the hero image as the big tile and every cover as a quarter tile (none consumed as the big tile)", () => {
      const covers = [1, 2, 3, 4].map(cover);
      const { container, getByAltText } = render(CoverMosaic, {
        props: { heroImageUrl: "https://example.com/portrait.jpg", heroImageAlt: "Shania Twain", covers },
      });

      expect(getByAltText("Shania Twain")).toBeInTheDocument();
      // 1 hero tile + all 4 covers as quarters = 5 tiles total.
      expect(tiles(container).length).toBe(5);
      expect(aspectRatio(container)).toBe("2 / 1");
    });

    it("caps quarter tiles at 4 even with a hero image and 5+ covers", () => {
      const covers = [1, 2, 3, 4, 5, 6].map(cover);
      const { container } = render(CoverMosaic, {
        props: { heroImageUrl: "https://example.com/portrait.jpg", covers },
      });

      expect(tiles(container).length).toBe(5);
      expect(aspectRatio(container)).toBe("2 / 1");
    });
  });
});
