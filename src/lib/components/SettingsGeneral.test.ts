import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import SettingsGeneral from "./SettingsGeneral.svelte";
import { invoke } from "@tauri-apps/api/core";
import { i18n } from "../stores/i18n.svelte";
import { prefs } from "../stores/prefs.svelte";

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
    const select = (await findByLabelText(/Language/, { selector: "select" })) as HTMLSelectElement;
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

  it("has no separate manual language picker", async () => {
    const { queryByLabelText } = render(SettingsGeneral);
    expect(queryByLabelText("User manual language")).toBeNull();
  });

  describe("auto-playlists card (#2131)", () => {
    it("lists a toggle per family, on by default, with tempo detection under BPM", () => {
      const { getByRole, getAllByRole } = render(SettingsGeneral);
      const labels = getAllByRole("switch").map((el) => el.getAttribute("aria-label"));
      expect(labels).toEqual([
        "Genre",
        "Decade",
        "Artist tags",
        "BPM",
        "Detect tempo and key",
        "Language",
        "Country",
        "Moment Mix",
      ]);
      expect(getByRole("switch", { name: "Genre" })).toBeChecked();
      expect(getByRole("switch", { name: "Detect tempo and key" })).not.toBeChecked();
    });

    it("persists a family toggle, then re-syncs the auto-playlists", async () => {
      prefs.autoPlaylists.decade = true;
      const { getByRole } = render(SettingsGeneral);
      await fireEvent.click(getByRole("switch", { name: "Decade" }));

      expect(prefs.autoPlaylists.decade).toBe(false);
      expect(invoke).toHaveBeenCalledWith(
        "set_ui_preferences",
        expect.objectContaining({
          prefs: expect.objectContaining({ auto_playlists_decade: false }),
        }),
      );
      await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("sync_all_auto_playlists"));
      const calls = vi.mocked(invoke).mock.calls.map((c) => c[0]);
      expect(calls.indexOf("set_ui_preferences")).toBeLessThan(
        calls.indexOf("sync_all_auto_playlists"),
      );
      prefs.autoPlaylists.decade = true;
    });
  });
});
