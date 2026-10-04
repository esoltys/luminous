import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import AddonThemesSection from "./AddonThemesSection.svelte";
import { addonsStore, type AddonState } from "../stores/addons.svelte";
import { themeStore } from "../stores/theme.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined)
}));

function setState(state: AddonState, error?: string) {
  addonsStore.applyEvent({ id: "mothman", state, error });
}

describe("AddonThemesSection.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    addonsStore.statuses = {};
    addonsStore.themes = {};
    addonsStore.prices = {};
    themeStore.activeThemeId = "system";
  });

  it("renders nothing while the Store is unavailable or has not reported yet", () => {
    const { container, rerender } = render(AddonThemesSection);
    expect(container.querySelector("[data-addon-card]")).toBeNull();
    expect(container.textContent).not.toContain("Add-on Themes");

    setState("unavailable");
    rerender({});
    expect(container.querySelector("[data-addon-card]")).toBeNull();
  });

  it("offers Get for an add-on that is not owned and starts the purchase on click", async () => {
    setState("unowned");
    const { findByRole, getByText } = render(AddonThemesSection);
    expect(getByText("Add-on Themes")).toBeInTheDocument();
    expect(getByText("Mothman")).toBeInTheDocument();

    await fireEvent.click(await findByRole("button", { name: "Get" }));
    expect(invoke).toHaveBeenCalledWith("acquire_addon", { id: "mothman" });
  });

  it("shows a busy, non-clickable button while the Store dialog is open and while downloading", async () => {
    setState("purchasing");
    const { findByRole, rerender } = render(AddonThemesSection);
    const waiting = await findByRole("button", { name: "Waiting for the Store…" });
    expect(waiting).toBeDisabled();
    expect(waiting).toHaveAttribute("aria-busy", "true");

    setState("downloading");
    rerender({});
    expect(await findByRole("button", { name: "Downloading…" })).toBeDisabled();
  });

  it("makes the whole card select the theme once owned, with no separate Apply button", async () => {
    const setThemeSpy = vi.spyOn(themeStore, "setTheme").mockResolvedValue(undefined as never);
    setState("owned");
    const { findByRole, getByText, queryByRole } = render(AddonThemesSection);
    expect(getByText("Owned")).toBeInTheDocument();
    expect(queryByRole("button", { name: "Apply" })).toBeNull();

    const card = await findByRole("button", { name: /Mothman/ });
    expect(card).toHaveAttribute("aria-pressed", "false");
    await fireEvent.click(getByText("A tiny moth wanders along the top of the player bar."));
    expect(setThemeSpy).toHaveBeenCalledWith("mothman");
  });

  it("marks the card pressed when it is the active theme", async () => {
    setState("owned");
    themeStore.activeThemeId = "mothman";
    const { findByRole } = render(AddonThemesSection);
    expect(await findByRole("button", { name: /Mothman/ })).toHaveAttribute("aria-pressed", "true");
  });

  it("is not a clickable card before the add-on is owned", () => {
    setState("unowned");
    const { container } = render(AddonThemesSection);
    const card = container.querySelector("[data-addon-card]") as HTMLElement;
    expect(card.tagName).toBe("DIV");
    expect(card).not.toHaveAttribute("aria-pressed");
  });

  it("shows the message for the error code and re-checks ownership on Try Again", async () => {
    setState("error", "not_entitled");
    const { findByRole, getByRole } = render(AddonThemesSection);
    expect(getByRole("alert")).toHaveTextContent("The Store couldn't confirm you own this add-on.");

    await fireEvent.click(await findByRole("button", { name: "Try Again" }));
    expect(invoke).toHaveBeenCalledWith("refresh_addons");
  });

  it("falls back to the generic message for an unknown error code", async () => {
    setState("error", "something_new");
    const { getByRole } = render(AddonThemesSection);
    expect(getByRole("alert")).toHaveTextContent("Something went wrong. Try again.");
  });

  it("shows the Store price on the button when the add-on is not free", async () => {
    setState("unowned");
    addonsStore.prices["mothman"] = { id: "mothman", formatted: "$4.99", isFree: false };
    const { findByRole, queryByRole } = render(AddonThemesSection);
    const buy = await findByRole("button", { name: "Buy for $4.99" });
    expect(queryByRole("button", { name: "Get" })).toBeNull();

    await fireEvent.click(buy);
    expect(invoke).toHaveBeenCalledWith("acquire_addon", { id: "mothman" });
  });

  it("says Buy for Free for a free add-on and keeps Get when the Store reported no price", async () => {
    setState("unowned");
    addonsStore.prices["mothman"] = { id: "mothman", formatted: "$0.00", isFree: true };
    const { findByRole, rerender } = render(AddonThemesSection);
    expect(await findByRole("button", { name: "Buy for Free" })).toBeInTheDocument();

    addonsStore.prices = {};
    rerender({});
    expect(await findByRole("button", { name: "Get" })).toBeInTheDocument();
  });

  it("Check Purchases asks the backend to refresh every add-on", async () => {
    setState("unowned");
    const { findByRole } = render(AddonThemesSection);
    await fireEvent.click(await findByRole("button", { name: "Check Purchases" }));
    expect(invoke).toHaveBeenCalledWith("refresh_addons");
  });

  it("takes the swatches from the registered theme once the bundle is verified", () => {
    setState("owned");
    addonsStore.register({
      id: "mothman",
      name: "Mothman",
      overlayEntry: "overlay.html",
      colors: {
        "bg-main": "#111111",
        "bg-sidebar": "#222222",
        "bg-playerbar": "#333333",
        "color-accent": "#444444",
        "color-accent-hover": "#555555",
        "color-border": "#666666",
        "color-text-primary": "#ffffff",
        "color-text-secondary": "#eeeeee"
      }
    });
    const { container } = render(AddonThemesSection);
    const swatches = [...container.querySelectorAll("[data-addon-card] .h-8 > div")].map(
      (el) => (el as HTMLElement).style.backgroundColor
    );
    expect(swatches).toEqual([
      "rgb(17, 17, 17)",
      "rgb(34, 34, 34)",
      "rgb(51, 51, 51)",
      "rgb(68, 68, 68)",
      "rgb(85, 85, 85)",
      "rgb(102, 102, 102)"
    ]);
  });
});
