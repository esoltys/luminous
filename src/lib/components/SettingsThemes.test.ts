import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import SettingsThemes from "./SettingsThemes.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

describe("SettingsThemes.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders Dynamic Themes row and footnotes for Luminous and System", async () => {
    const { findByText } = render(SettingsThemes);
    expect(await findByText("Dynamic Themes")).toBeInTheDocument();
    expect(await findByText("Colors shift to match whatever album art is playing now")).toBeInTheDocument();
    expect(await findByText("Switches between light and dark to match your OS")).toBeInTheDocument();
  });
});
