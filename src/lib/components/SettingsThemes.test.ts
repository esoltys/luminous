import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render } from "@testing-library/svelte";
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
  });

  it("renders Dynamic Themes row and footnotes for Luminous and System", async () => {
    const { findByText } = render(SettingsThemes);
    expect(await findByText("Dynamic Themes")).toBeInTheDocument();
    expect(await findByText("Colors shift to match whatever album art is playing now")).toBeInTheDocument();
    expect(await findByText("Switches between light and dark to match your OS")).toBeInTheDocument();
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
});
