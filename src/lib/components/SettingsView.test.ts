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
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("defaults to the General tab and renders its content", async () => {
    const { findByText } = render(SettingsView);
    expect(await findByText(/v0\.75\.0/)).toBeInTheDocument();
  });

  it("persists the active tab via set_app_setting when switching tabs", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { findByText, getByText } = render(SettingsView);
    await findByText(/v0\.75\.0/);

    await fireEvent.click(getByText("UI Themes"));

    expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "active_settings_tab", value: "themes" });
  });
});
