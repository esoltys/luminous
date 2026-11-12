import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import { createRawSnippet } from "svelte";
import InlineMessage from "./InlineMessage.svelte";

const text = createRawSnippet(() => ({ render: () => "<span>Could not save</span>" }));

describe("InlineMessage", () => {
  it("shows the message with a warning icon in the theme's primary text colour", () => {
    const { getByText, container } = render(InlineMessage, {
      props: { children: text, role: "alert", id: "m" },
    });
    const root = container.querySelector("#m")!;
    expect(getByText("Could not save")).toBeInTheDocument();
    expect(root).toHaveAttribute("role", "alert");
    expect(root).toHaveClass("text-brand-text-primary");
    expect(root.querySelector("svg")).not.toBeNull();
    expect(root.className).not.toMatch(/(red|amber)-\d/);
  });
});
