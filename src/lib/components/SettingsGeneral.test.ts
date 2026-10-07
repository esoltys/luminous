import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import SettingsGeneral from "./SettingsGeneral.svelte";
import { invoke } from "@tauri-apps/api/core";
import { i18n } from "../stores/i18n.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(null),
}));

describe("SettingsGeneral.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    i18n.currentLocale = "en-CA";
  });

  it("lists every registered UI language by its own name and switches on pick", async () => {
    const { findByLabelText } = render(SettingsGeneral);
    const select = (await findByLabelText(/Language/)) as HTMLSelectElement;
    expect([...select.options].map((o) => [o.value, o.textContent?.trim()])).toEqual([
      ["en-CA", "English (Canada)"],
      ["fr-CA", "Français (Canada)"],
      ["──────────", "──────────"],
      ["de", "Deutsch"],
      ["en-GB", "English (United Kingdom)"],
      ["en-US", "English (United States)"],
      ["es", "Español"],
      ["fr", "Français"],
      ["it", "Italiano"],
      ["ru", "Русский"],
      ["uk", "Українська"],
    ]);

    await fireEvent.change(select, { target: { value: "fr-CA" } });

    expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "language", value: "fr-CA" });
  });


  it("saves the manual language separately from the UI language", async () => {
    const { findByLabelText } = render(SettingsGeneral);
    const select = (await findByLabelText("User manual language")) as HTMLSelectElement;
    expect([...select.options].map((o) => [o.value, o.textContent?.trim()])).toEqual([
      ["EN", "English"],
      ["FR", "Français"],
    ]);

    await fireEvent.change(select, { target: { value: "FR" } });

    expect(invoke).toHaveBeenCalledWith("set_app_setting", { key: "manual_language", value: "FR" });
    expect(invoke).not.toHaveBeenCalledWith("set_app_setting", expect.objectContaining({ key: "language" }));
  });
});
