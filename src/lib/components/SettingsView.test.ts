import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import SettingsView from "./SettingsView.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "get_all_app_settings") {
      return Promise.resolve({ active_settings_tab: "general" });
    }
    return Promise.resolve([]);
  }),
}));

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("0.75.0"),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));

describe("SettingsView.svelte", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    const { navigationStore } = await import("../stores/navigation.svelte");
    navigationStore.settingsSubTab = "general";
  });

  it("defaults to the General tab and renders its content", async () => {
    const { findByText } = render(SettingsView);
    expect(await findByText("General Settings")).toBeInTheDocument();
  });

  it("persists the active tab via set_app_setting when switching tabs", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { findByText, getByText } = render(SettingsView);
    await findByText("General Settings");

    await fireEvent.click(getByText("UI Themes"));

    expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "active_settings_tab", value: "themes" });
  });

  it("persists active tab as integrations when clicking Integrations tab", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { findByText, getByText } = render(SettingsView);
    await findByText("General Settings");

    await fireEvent.click(getByText("Integrations"));

    expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "active_settings_tab", value: "integrations" });
  });

  it("persists active tab as system when clicking System tab", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { findByText, getByText } = render(SettingsView);
    await findByText("General Settings");

    await fireEvent.click(getByText("System"));

    expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "active_settings_tab", value: "system" });
  });

  it("persists active tab as sources when clicking Sources tab", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { findByText, getByText } = render(SettingsView);
    await findByText("General Settings");

    await fireEvent.click(getByText("Sources"));

    expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "active_settings_tab", value: "sources" });
  });

  it("migrates legacy folders setting to sources on mount", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_all_app_settings") {
        return Promise.resolve({ active_settings_tab: "folders" });
      }
      return Promise.resolve([]);
    });

    const { findByText } = render(SettingsView);
    await findByText("Watched Folders");

    expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "active_settings_tab", value: "sources" });
  });
});
