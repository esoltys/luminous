import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import SettingsThemes from "./SettingsThemes.svelte";
import { themeStore } from "../stores/theme.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

describe("SettingsThemes.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    themeStore.colorSchemeMode = "system";
  });

  it("renders Dynamic Themes row and footnotes for Luminous and System", async () => {
    const { findByText } = render(SettingsThemes);
    expect(await findByText("Dynamic Themes")).toBeInTheDocument();
    expect(await findByText("Colors shift to match whatever album art is playing now")).toBeInTheDocument();
    expect(await findByText("Switches between light and dark to match your OS")).toBeInTheDocument();
  });

  it("renders the Light/Dark/System segmented control on the System theme card (#692)", async () => {
    const { findByText, findByRole } = render(SettingsThemes);
    expect(await findByText("Select Theme")).toBeInTheDocument();

    const lightTab = await findByRole("tab", { name: "Light" });
    const darkTab = await findByRole("tab", { name: "Dark" });
    const systemTab = await findByRole("tab", { name: "System" });
    expect(lightTab).toBeInTheDocument();
    expect(darkTab).toBeInTheDocument();
    expect(systemTab).toBeInTheDocument();
    expect(systemTab).toHaveAttribute("aria-selected", "true");
  });

  it("clicking a segmented-control option pins colorSchemeMode without changing activeThemeId (#692)", async () => {
    const { findByRole } = render(SettingsThemes);
    const darkTab = await findByRole("tab", { name: "Dark" });

    await fireEvent.click(darkTab);

    expect(themeStore.colorSchemeMode).toBe("dark");
    expect(themeStore.activeThemeId).toBe("system");
  });
});
