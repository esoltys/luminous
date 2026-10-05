import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  BREAKPOINT_NARROW_PX,
  BREAKPOINT_MEDIUM_PX,
  BREAKPOINT_RIGHT_PANEL_PX,
  BREAKPOINT_EXPANDED_PX,
  COVER_STACK_FOURTH_COVER_PX,
  COVER_STACK_FIFTH_COVER_PX,
  COVER_STACK_SIXTH_COVER_PX,
} from "./constants";

// Reads the real stylesheet so a drift between the Tailwind tokens and the JS
// constants fails here instead of showing up as a layout that disagrees with
// itself at one window width.
const css = readFileSync(resolve(__dirname, "../app.css"), "utf-8");

function token(name: string): number {
  const match = css.match(new RegExp(String.raw`--breakpoint-${name}:\s*(\d+)px`));
  if (!match) throw new Error(`--breakpoint-${name} missing from app.css @theme (must be in px)`);
  return Number(match[1]);
}

describe("breakpoint tokens", () => {
  it("keeps CSS --breakpoint-* equal to the JS tier constants", () => {
    expect(token("xs")).toBe(BREAKPOINT_NARROW_PX);
    expect(token("sm")).toBe(BREAKPOINT_MEDIUM_PX);
    expect(token("md")).toBe(BREAKPOINT_RIGHT_PANEL_PX);
    expect(token("lg")).toBe(BREAKPOINT_EXPANDED_PX);
  });
});

describe("CoverStack container thresholds", () => {
  it("keeps the @container rules in CoverStack.svelte equal to the JS constants", () => {
    const src = readFileSync(resolve(__dirname, "components/CoverStack.svelte"), "utf-8");
    const widths = [...src.matchAll(/@container \(min-width: (\d+)px\)/g)].map((m) => Number(m[1]));
    expect(widths).toEqual([
      COVER_STACK_FOURTH_COVER_PX,
      COVER_STACK_FIFTH_COVER_PX,
      COVER_STACK_SIXTH_COVER_PX,
    ]);
  });
});
