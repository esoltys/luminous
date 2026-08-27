import "@testing-library/jest-dom";
import { describe, it, expect, vi } from "vitest";
import { render } from "@testing-library/svelte";
import CoverStack from "./CoverStack.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(""),
}));

describe("CoverStack.svelte", () => {
  const mockCovers = [
    { songId: 1, artAutomatic: "cover1.jpg" },
    { songId: 2, artAutomatic: "cover2.jpg" },
  ];

  it("centers content by default when direction is right", () => {
    const { container } = render(CoverStack, {
      props: { covers: mockCovers, direction: "right" },
    });

    const rootDiv = container.firstElementChild as HTMLElement;
    expect(rootDiv.className).toContain("justify-center");
    expect(rootDiv.className).not.toContain("justify-end");
  });

  it("right-aligns content when direction is left", () => {
    const { container } = render(CoverStack, {
      props: { covers: mockCovers, direction: "left" },
    });

    const rootDiv = container.firstElementChild as HTMLElement;
    expect(rootDiv.className).toContain("justify-end");
    expect(rootDiv.className).not.toContain("justify-center");
  });

  it("renders single cover correctly with direction left", () => {
    const { container } = render(CoverStack, {
      props: { covers: [mockCovers[0]], direction: "left" },
    });

    const rootDiv = container.firstElementChild as HTMLElement;
    expect(rootDiv.className).toContain("justify-end");
  });
});
