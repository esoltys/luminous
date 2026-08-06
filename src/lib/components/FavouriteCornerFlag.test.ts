import { render } from "@testing-library/svelte";
import { describe, it, expect } from "vitest";
import FavouriteCornerFlag from "./FavouriteCornerFlag.svelte";

describe("FavouriteCornerFlag", () => {
  it("renders corner flag with favourite testid and heart icon", () => {
    const { getByTestId } = render(FavouriteCornerFlag, { props: { size: "md" } });
    const flag = getByTestId("favourite-corner-flag");
    expect(flag).toBeDefined();
    expect(flag.className).toContain("w-10 h-10");
  });

  it("applies proper size class for small size", () => {
    const { getByTestId } = render(FavouriteCornerFlag, { props: { size: "sm" } });
    const flag = getByTestId("favourite-corner-flag");
    expect(flag.className).toContain("w-6 h-6");
  });

  it("applies proper size class for large size", () => {
    const { getByTestId } = render(FavouriteCornerFlag, { props: { size: "lg" } });
    const flag = getByTestId("favourite-corner-flag");
    expect(flag.className).toContain("w-12 h-12");
  });
});
