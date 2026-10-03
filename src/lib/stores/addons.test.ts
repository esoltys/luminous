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

  it("changes state only from the addon-state-changed event", async () => {
    let handler: ((e: { payload: AddonStateChange }) => void) | undefined;
    vi.mocked(listen).mockImplementationOnce((async (name: string, cb: typeof handler) => {
      expect(name).toBe("addon-state-changed");
      handler = cb;
      return () => {};
    }) as never);
    addons.register(FIXTURE);
    await addons.init();
    handler!({ payload: { id: FIXTURE.id, state: "owned" } });
    expect(addons.stateOf(FIXTURE.id)).toBe("owned");
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
});
