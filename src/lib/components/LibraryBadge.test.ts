import "@testing-library/jest-dom";
import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import LibraryBadge from "./LibraryBadge.svelte";
import type { MusicDirectory } from "../types";

describe("LibraryBadge.svelte", () => {
  const baseDirectory: MusicDirectory = {
    id: 1,
    path: "C:\\Music\\HighRes",
    is_available: true,
    nickname: null,
    icon: null,
    color: null,
  };

  it("renders folder name extracted from path by default", () => {
    const { getByText } = render(LibraryBadge, {
      props: { directory: baseDirectory },
    });
    expect(getByText("HighRes")).toBeInTheDocument();
  });

  it("renders custom nickname when specified", () => {
    const customDir: MusicDirectory = {
      ...baseDirectory,
      nickname: "Studio FLACs",
    };
    const { getByText } = render(LibraryBadge, {
      props: { directory: customDir },
    });
    expect(getByText("Studio FLACs")).toBeInTheDocument();
  });

  it("applies custom color style when specified", () => {
    const coloredDir: MusicDirectory = {
      ...baseDirectory,
      color: "#8b5cf6",
    };
    const { container } = render(LibraryBadge, {
      props: { directory: coloredDir },
    });
    const badge = container.querySelector("span");
    expect(badge).toBeInTheDocument();
    expect(badge?.getAttribute("style")).toMatch(/139,\s*92,\s*246/);
  });

  it("shows disconnected state and tooltip when is_available is false", () => {
    const disconnectedDir: MusicDirectory = {
      ...baseDirectory,
      nickname: "External USB",
      is_available: false,
    };
    const { container, getByText } = render(LibraryBadge, {
      props: { directory: disconnectedDir },
    });
    expect(getByText("External USB")).toBeInTheDocument();
    const badge = container.querySelector("span");
    expect(badge?.getAttribute("title")).toContain("(Disconnected)");
  });

  it("supports hiding the text when showName is false", () => {
    const { queryByText } = render(LibraryBadge, {
      props: { directory: baseDirectory, showName: false },
    });
    expect(queryByText("HighRes")).toBeNull();
  });
});
