import { describe, it, expect, beforeEach, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AddonsStore, type AddonStateChange, type AddonTheme } from "./addons.svelte";
import { LUMINOUS_DARK_COLORS, ThemeStore } from "./theme.svelte";

// Synthetic fixture — real add-on assets never live in this repo.
const FIXTURE: AddonTheme = {
  id: "fixture-addon",
  name: "Fixture Add-on",
  colors: { ...LUMINOUS_DARK_COLORS, "color-accent": "#12ab34" },
  overlayEntry: "overlay.html"
};

describe("AddonsStore", () => {
  let addons: AddonsStore;
  beforeEach(() => {
    addons = new AddonsStore();
  });

  it("reports unavailable for unknown ids and is not usable until owned", () => {
    expect(addons.stateOf("nope")).toBe("unavailable");
    addons.register(FIXTURE);
    expect(addons.isUsable(FIXTURE.id)).toBe(false);
    addons.applyEvent({ id: FIXTURE.id, state: "downloading" });
    expect(addons.isUsable(FIXTURE.id)).toBe(false);
    addons.applyEvent({ id: FIXTURE.id, state: "owned" });
    expect(addons.isUsable(FIXTURE.id)).toBe(true);
  });

  it("requires registration even when the backend says owned", () => {
    addons.applyEvent({ id: "ghost", state: "owned" });
    expect(addons.isUsable("ghost")).toBe(false);
  });

  it("ignores unknown states and keeps the error message", () => {
    addons.register(FIXTURE);
    addons.applyEvent({ id: FIXTURE.id, state: "bogus" as never });
    expect(addons.stateOf(FIXTURE.id)).toBe("unavailable");
    addons.applyEvent({ id: FIXTURE.id, state: "error", error: "boom" });
    expect(addons.errorOf(FIXTURE.id)).toBe("boom");
  });

  it("changes state and registers themes only from backend events", async () => {
    const handlers: Record<string, (e: { payload: never }) => void> = {};
    vi.mocked(listen).mockImplementation((async (name: string, cb: never) => {
      handlers[name] = cb;
      return () => {};
    }) as never);
    await addons.init();
    handlers["addon-theme-defined"]({ payload: FIXTURE as never });
    expect(addons.isAddonId(FIXTURE.id)).toBe(true);
    expect(addons.isUsable(FIXTURE.id)).toBe(false);
    handlers["addon-state-changed"]({ payload: { id: FIXTURE.id, state: "owned" } as never });
    expect(addons.stateOf(FIXTURE.id)).toBe("owned");
  });

  it("keeps the Store's price from the backend's price event", async () => {
    const handlers: Record<string, (e: { payload: never }) => void> = {};
    vi.mocked(listen).mockImplementation((async (name: string, cb: never) => {
      handlers[name] = cb;
      return () => {};
    }) as never);
    await addons.init();
    expect(addons.prices[FIXTURE.id]).toBeUndefined();
    handlers["addon-price-defined"]({
      payload: { id: FIXTURE.id, formatted: "$4.99", isFree: false } as never
    });
    expect(addons.prices[FIXTURE.id]).toEqual({ id: FIXTURE.id, formatted: "$4.99", isFree: false });
  });
});

describe("ThemeStore with add-ons", () => {
  let addons: AddonsStore;
  let theme: ThemeStore;
  beforeEach(() => {
    vi.clearAllMocks();
    addons = new AddonsStore();
    theme = new ThemeStore(addons);
  });

  it("refuses to select an add-on that isn't owned", async () => {
    addons.register(FIXTURE);
    await theme.setTheme(FIXTURE.id);
    expect(theme.activeThemeId).not.toBe(FIXTURE.id);
  });

  it("selects an owned add-on and resolves its palette and overlay", async () => {
    addons.register(FIXTURE);
    addons.applyEvent({ id: FIXTURE.id, state: "owned" });
    await theme.setTheme(FIXTURE.id);
    expect(theme.activeThemeId).toBe(FIXTURE.id);
    expect(theme.currentTheme.colors["color-accent"]).toBe("#12ab34");
    expect(theme.activeAddon?.id).toBe(FIXTURE.id);
  });

  it("keeps a persisted add-on id pending, then applies it once owned", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ active_theme_id: FIXTURE.id } as never);
    await theme.init();
    expect(theme.activeThemeId).not.toBe(FIXTURE.id);
    expect(theme.pendingAddonThemeId).toBe(FIXTURE.id);

    addons.register(FIXTURE);
    addons.applyEvent({ id: FIXTURE.id, state: "owned" });
    expect(theme.activeThemeId).toBe(FIXTURE.id);
    expect(theme.pendingAddonThemeId).toBeNull();
  });

  it("falls back to system in memory without rewriting the saved choice when ownership is lost", async () => {
    addons.register(FIXTURE);
    addons.applyEvent({ id: FIXTURE.id, state: "owned" });
    await theme.init();
    await theme.setTheme(FIXTURE.id);
    vi.mocked(invoke).mockClear();

    addons.applyEvent({ id: FIXTURE.id, state: "unowned" });
    expect(theme.activeThemeId).toBe("system");
    expect(theme.activeAddon).toBeNull();
    expect(theme.pendingAddonThemeId).toBe(FIXTURE.id);
    expect(invoke).not.toHaveBeenCalledWith(
      "set_app_setting",
      expect.objectContaining({ key: "active_theme_id" })
    );
  });

  describe("provisional palette while a saved add-on waits for ownership (#1438)", () => {
    async function launchWithSaved(id: string) {
      vi.mocked(invoke).mockResolvedValueOnce({ active_theme_id: id } as never);
      await theme.init();
    }

    it("paints the add-on's public palette, without its overlay, before the backend reports", async () => {
      await launchWithSaved("mothman");
      expect(theme.currentTheme.id).toBe("mothman");
      expect(theme.resolvedColors["color-accent"]).toBe("#c6133d");
      expect(theme.activeAddon).toBeNull();
      expect(theme.pendingAddonThemeId).toBe("mothman");
    });

    it("keeps the palette while the add-on is on its way to owned", async () => {
      await launchWithSaved("mothman");
      for (const state of ["purchasing", "downloading", "owned"] as const) {
        addons.applyEvent({ id: "mothman", state });
        expect(theme.currentTheme.id).toBe("mothman");
      }
    });

    it("falls back to System and repaints once the backend rules the add-on out", async () => {
      await launchWithSaved("mothman");
      for (const state of ["unowned", "unavailable", "error"] as const) {
        addons.applyEvent({ id: "mothman", state });
        expect(theme.currentTheme.id).toBe("system");
      }
      const repaint = vi.spyOn(theme, "applyActiveTheme");
      addons.applyEvent({ id: "mothman", state: "unowned" });
      // Already painted as System after the first ruling, so nothing more to repaint.
      expect(repaint).not.toHaveBeenCalled();
    });

    it("repaints exactly once when a painted provisional palette is ruled out", async () => {
      await launchWithSaved("mothman");
      const repaint = vi.spyOn(theme, "applyActiveTheme");
      addons.applyEvent({ id: "mothman", state: "downloading" });
      expect(repaint).not.toHaveBeenCalled();
      theme.applyActiveTheme();
      repaint.mockClear();
      addons.applyEvent({ id: "mothman", state: "unowned" });
      expect(repaint).toHaveBeenCalledTimes(1);
    });

    it("never overrides a theme chosen since launch", async () => {
      await launchWithSaved("mothman");
      await theme.setTheme("nordic-blue");
      expect(theme.currentTheme.id).toBe("nordic-blue");
      expect(theme.pendingAddonThemeId).toBeNull();

      await launchWithSaved("mothman");
      await theme.addCustomTheme({
        id: "mine",
        name: "Mine",
        colors: { ...LUMINOUS_DARK_COLORS },
        isCustom: true
      });
      expect(theme.currentTheme.id).toBe("mine");
      expect(theme.pendingAddonThemeId).toBeNull();
    });

    it("ignores a saved id the catalog does not know", async () => {
      await launchWithSaved("not-in-catalog");
      expect(theme.currentTheme.id).toBe("system");
    });
  });
});
