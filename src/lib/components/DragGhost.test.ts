import "@testing-library/jest-dom";
import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import { createRawSnippet } from "svelte";
import DragGhost from "./DragGhost.svelte";

const label = createRawSnippet(() => ({ render: () => "<span>Dragged item</span>" }));

describe("DragGhost.svelte", () => {
  it("sits beside the pointer, not under it, and shows its content", () => {
    const { getByTestId, getByText } = render(DragGhost, {
      props: { x: 150, y: 300, testId: "ghost", children: label },
    });
    const ghost = getByTestId("ghost");
    expect(ghost.style.left).toBe("164px");
    expect(ghost.style.top).toBe("300px");
    expect(getByText("Dragged item")).toBeInTheDocument();
  });

  it("marks itself only while over a valid drop target", () => {
    const off = render(DragGhost, { props: { x: 0, y: 0, testId: "off", children: label } });
    expect(off.getByTestId("off")).not.toHaveAttribute("data-over-target");

    const on = render(DragGhost, { props: { x: 0, y: 0, overTarget: true, testId: "on", children: label } });
    expect(on.getByTestId("on")).toHaveAttribute("data-over-target");
  });

  it("uses a caller-supplied border colour instead of the accent", () => {
    const { getByTestId } = render(DragGhost, {
      props: { x: 0, y: 0, color: "rgb(10, 20, 30)", testId: "ghost", children: label },
    });
    expect(getByTestId("ghost").style.borderColor).toBe("rgb(10, 20, 30)");
  });
});
