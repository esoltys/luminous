import "@testing-library/jest-dom";
import { describe, it, expect, afterEach } from "vitest";
import { render, fireEvent, screen } from "@testing-library/svelte";
import HelpTip from "./HelpTip.svelte";
import FormField from "./FormField.svelte";
import { createRawSnippet, tick } from "svelte";

const HINT = "The first value is treated as the main genre.";

// The popover is the only place the hint is *visible*; it's portalled to body.
function visibleHint(): HTMLElement | null {
  return Array.from(document.body.querySelectorAll<HTMLElement>("[aria-hidden='true']")).find((el) => el.textContent?.trim() === HINT) ?? null;
}

describe("HelpTip.svelte", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("is a focusable button described by the hint text, even while closed", () => {
    render(HelpTip, { props: { text: HINT, label: "About Genre" } });
    const button = screen.getByRole("button", { name: "About Genre" });
    expect(button).toHaveAccessibleDescription(HINT);
    expect(visibleHint()).toBeNull();
  });

  it("opens on mouse hover and closes when the pointer leaves", async () => {
    render(HelpTip, { props: { text: HINT, label: "About Genre" } });
    const button = screen.getByRole("button");
    await fireEvent.pointerEnter(button, { pointerType: "mouse" });
    expect(visibleHint()).not.toBeNull();
    await fireEvent.pointerLeave(button, { pointerType: "mouse" });
    expect(visibleHint()).toBeNull();
  });

  it("opens on keyboard focus and closes on Escape", async () => {
    render(HelpTip, { props: { text: HINT, label: "About Genre" } });
    const button = screen.getByRole("button");
    // jsdom matches :focus-visible on any focus, so this covers the keyboard
    // path only; "a mouse click doesn't pin it open" needs a real browser.
    await fireEvent.keyDown(document.body, { key: "Tab" });
    button.focus();
    expect(button.matches(":focus-visible")).toBe(true);
    await tick();
    expect(visibleHint()).not.toBeNull();

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(visibleHint()).toBeNull();
  });

  it("closes on any press and on wheel", async () => {
    render(HelpTip, { props: { text: HINT, label: "About Genre" } });
    const button = screen.getByRole("button");
    await fireEvent.pointerEnter(button, { pointerType: "mouse" });
    await fireEvent.pointerDown(document.body);
    expect(visibleHint()).toBeNull();

    await fireEvent.pointerEnter(button, { pointerType: "mouse" });
    await fireEvent.wheel(document.body);
    expect(visibleHint()).toBeNull();
  });

  it("ignores touch/pen hover so a tap doesn't flash it open", async () => {
    render(HelpTip, { props: { text: HINT } });
    await fireEvent.pointerEnter(screen.getByRole("button"), { pointerType: "touch" });
    expect(visibleHint()).toBeNull();
  });
});

describe("FormField.svelte help", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("describes the field's input with its tooltip and exposes a keyboard-reachable help button", () => {
    const children = createRawSnippet(() => ({ render: () => '<input id="tag-genre" />' }));
    render(FormField, { props: { label: "Genre", for: "tag-genre", tooltip: HINT, children } });

    expect(screen.getByLabelText("Genre")).toHaveAccessibleDescription(HINT);
    const help = screen.getByRole("button", { name: "About Genre" });
    expect(help.tabIndex).toBe(0);
  });
});
