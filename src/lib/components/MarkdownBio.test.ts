import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/svelte";
import MarkdownBio from "./MarkdownBio.svelte";

describe("MarkdownBio.svelte", () => {
  it("renders plain bio text without markdown", () => {
    render(MarkdownBio, { props: { text: "Just a plain bio text without links." } });
    expect(screen.getByText("Just a plain bio text without links.")).toBeTruthy();
  });

  it("renders markdown links [label](url)", () => {
    render(MarkdownBio, {
      props: {
        text: "Check out [Wikipedia](https://en.wikipedia.org/wiki/Gunship_(band)) for details.",
      },
    });

    const link = screen.getByRole("button", { name: /Wikipedia/i });
    expect(link).toBeTruthy();
    expect(link.getAttribute("title")).toBe("https://en.wikipedia.org/wiki/Gunship_(band)");
  });

  it("handles balanced parentheses in URLs", () => {
    render(MarkdownBio, {
      props: {
        text: "More at https://en.wikipedia.org/wiki/Gunship_(band).",
      },
    });

    const link = screen.getByRole("button", { name: /https:\/\/en\.wikipedia\.org\/wiki\/Gunship_\(band\)/i });
    expect(link).toBeTruthy();
    expect(link.getAttribute("title")).toBe("https://en.wikipedia.org/wiki/Gunship_(band)");
  });

  it("renders **bold** text as <strong>", () => {
    render(MarkdownBio, { props: { text: "This is **very** important." } });
    const strong = screen.getByText("very");
    expect(strong.tagName).toBe("STRONG");
  });

  it("renders *italic* text as <em>", () => {
    render(MarkdownBio, { props: { text: "This is *quite* nice." } });
    const em = screen.getByText("quite");
    expect(em.tagName).toBe("EM");
  });

  it("renders bold and italic alongside a markdown link", () => {
    render(MarkdownBio, {
      props: {
        text: "**Bold** intro with *italic* flair and a [link](https://example.com).",
      },
    });

    expect(screen.getByText("Bold").tagName).toBe("STRONG");
    expect(screen.getByText("italic").tagName).toBe("EM");
    expect(screen.getByRole("button", { name: /link/i })).toBeTruthy();
  });

  it("does not render inline show more buttons when disableClamp is true", () => {
    render(MarkdownBio, {
      props: {
        text: "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8",
        disableClamp: true,
      },
    });

    expect(screen.queryByRole("button", { name: /Show more/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /Show less/i })).toBeNull();
  });
});
