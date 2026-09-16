import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, within } from "@testing-library/svelte";
import SettingsThemes from "./SettingsThemes.svelte";
import { themeStore, LUMINOUS_DARK_COLORS, type Theme } from "../stores/theme.svelte";
import { open, save } from "@tauri-apps/plugin-dialog";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
  save: vi.fn(),
}));

describe("SettingsThemes.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    themeStore.customThemes = [];
    themeStore.colorSchemeMode = "system";
  });

  it("renders Dynamic Themes row and footnotes for Luminous and System", async () => {
    const { findByText } = render(SettingsThemes);
    expect(await findByText("Dynamic Themes")).toBeInTheDocument();
    expect(await findByText("Colors shift to match whatever album art is playing now")).toBeInTheDocument();
    expect(await findByText("Switches between light and dark to match your OS")).toBeInTheDocument();
  });

  it("renders a Light/Dark/System segmented control scoped to the System theme card", async () => {
    const { findByRole } = render(SettingsThemes);
    const group = await findByRole("group", { name: "Select Theme" });
    expect(group).toBeInTheDocument();

    const scoped = within(group);
    const lightBtn = scoped.getByRole("button", { name: /light/i });
    const darkBtn = scoped.getByRole("button", { name: /dark/i });
    const systemBtn = scoped.getByRole("button", { name: "System" });
    expect(lightBtn).toBeInTheDocument();
    expect(darkBtn).toBeInTheDocument();
    expect(systemBtn).toBeInTheDocument();
    expect(systemBtn).toHaveAttribute("aria-pressed", "true");
  });

  it("clicking a segmented control option pins colorSchemeMode without changing activeThemeId or reselecting the theme", async () => {
    const setModeSpy = vi.spyOn(themeStore, "setColorSchemeMode");
    const setThemeSpy = vi.spyOn(themeStore, "setTheme");
    themeStore.activeThemeId = "dynamic-artwork";

    const { findByRole } = render(SettingsThemes);
    const group = await findByRole("group", { name: "Select Theme" });
    const darkBtn = within(group).getByRole("button", { name: /dark/i });
    await fireEvent.click(darkBtn);

    expect(setModeSpy).toHaveBeenCalledWith("dark");
    expect(setThemeSpy).not.toHaveBeenCalled();
    expect(themeStore.activeThemeId).toBe("dynamic-artwork");
  });

  it("triggers file dialog and themeStore.importTheme on Import Theme click", async () => {
    const importedTheme: Theme = {
      id: "custom-imported-999",
      name: "Retrowave",
      colors: { ...LUMINOUS_DARK_COLORS },
      isCustom: true,
    };
    vi.mocked(open).mockResolvedValueOnce("/path/to/Retrowave.json");
    const importSpy = vi.spyOn(themeStore, "importTheme").mockResolvedValueOnce(importedTheme);

    const { getAllByRole } = render(SettingsThemes);
    const importBtns = getAllByRole("button", { name: /import theme/i });
    expect(importBtns.length).toBeGreaterThan(0);

    await fireEvent.click(importBtns[0]);

    expect(open).toHaveBeenCalledWith(expect.objectContaining({
      multiple: false,
      filters: [{ name: "Theme (*.json)", extensions: ["json"] }],
    }));
    expect(importSpy).toHaveBeenCalledWith("/path/to/Retrowave.json");
  });

  it("triggers save dialog and themeStore.exportTheme on Export click in custom themes grid", async () => {
    const customTheme: Theme = {
      id: "custom-vapor",
      name: "Vaporwave",
      colors: { ...LUMINOUS_DARK_COLORS },
      isCustom: true,
    };
    themeStore.customThemes = [customTheme];

    vi.mocked(save).mockResolvedValueOnce("/path/to/exported-vapor.json");
    const exportSpy = vi.spyOn(themeStore, "exportTheme").mockResolvedValueOnce();

    const { getByTitle } = render(SettingsThemes);
    const exportBtn = getByTitle("Export Theme");
    expect(exportBtn).toBeInTheDocument();

    await fireEvent.click(exportBtn);

    expect(save).toHaveBeenCalledWith(expect.objectContaining({
      defaultPath: "Vaporwave.json",
      filters: [{ name: "Theme (*.json)", extensions: ["json"] }],
    }));
    expect(exportSpy).toHaveBeenCalledWith(customTheme, "/path/to/exported-vapor.json");
  });

  it("applies matching auto-fill responsive grid classes to predefined themes", async () => {
    const { findByText } = render(SettingsThemes);
    const predefinedHeading = await findByText("Predefined Themes");
    const predefinedGrid = predefinedHeading.nextElementSibling;
    expect(predefinedGrid).not.toBeNull();
    expect(predefinedGrid?.className).toContain("grid-cols-[repeat(auto-fill,minmax(160px,1fr))]");
  });

  it("applies container-responsive grid classes to custom theme builder", async () => {
    const { findByText } = render(SettingsThemes);
    const mainBgLabel = await findByText("Main Background");
    const colorGrid = mainBgLabel.closest(".grid");
    expect(colorGrid).not.toBeNull();
    expect(colorGrid?.className).toContain("grid-cols-1");
    expect(colorGrid?.className).toContain("@md:grid-cols-2");
    expect(colorGrid?.className).toContain("@2xl:grid-cols-3");
  });
});
