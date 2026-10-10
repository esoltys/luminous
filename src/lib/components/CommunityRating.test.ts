import "@testing-library/jest-dom";
import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import CommunityRating from "./CommunityRating.svelte";

describe("CommunityRating.svelte", () => {
  it("shows the label, stars and count as plain text, never a link", () => {
    const { getByText } = render(CommunityRating, { props: { rating: 4.5, count: 12 } });

    expect(getByText("Community Rating")).toBeInTheDocument();
    expect(getByText("(12)")).toBeInTheDocument();
    expect(getByText("Community Rating").closest("button, a")).toBeNull();
  });

  it("omits count when count is null, undefined, or 0", () => {
    const { getByText, queryByText } = render(CommunityRating, { props: { rating: 3.0, count: 0 } });

    expect(getByText("Community Rating")).toBeInTheDocument();
    expect(queryByText(/\(\d+\)/)).toBeNull();
  });
});
