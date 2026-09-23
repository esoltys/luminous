import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import ArtistInformationPanel from "./ArtistInformationPanel.svelte";

describe("ArtistInformationPanel", () => {
  it("renders nothing when all fields are empty or null", () => {
    const { container } = render(ArtistInformationPanel, {
      props: {
        sortName: null,
        gender: null,
        beginDate: null,
        endDate: null,
        artistType: null,
      },
    });

    expect(container.textContent?.trim()).toBe("");
  });

  it("renders person details (Born, Born in, Gender, Died) and omits Sort Name and Type", () => {
    const { container } = render(ArtistInformationPanel, {
      props: {
        sortName: "Bowie, David",
        gender: "male",
        artistType: "Person",
        beginDate: "1947-01-08",
        endDate: "2016-01-10",
        beginAreaName: "Brixton",
        beginAreaMbid: "d9e80e14-d07f-4ca6-b8db-60cb1c07cb81",
        areaName: "United Kingdom",
        areaMbid: "8a754a16-0027-4a29-b6d7-2b40ea0481ed",
      },
    });

    // Check sort name is NOT displayed in the UI
    expect(screen.queryByText("Sort Name")).toBeNull();
    expect(screen.queryByText("Bowie, David")).toBeNull();

    // Check gender
    expect(screen.getByText("Gender")).toBeTruthy();
    expect(screen.getByText("Male")).toBeTruthy();

    // Check born label
    expect(screen.getByText("Born")).toBeTruthy();
    expect(container.textContent).toContain("January 8, 1947");

    // Check City/Region and Country labels
    expect(screen.getByText("City/Region")).toBeTruthy();
    expect(screen.getByText("Brixton")).toBeTruthy();
    expect(screen.getByText("Country")).toBeTruthy();
    expect(screen.getByText("United Kingdom")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Brixton/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /United Kingdom/i })).toBeNull();

    // Check died label
    expect(screen.getByText("Died")).toBeTruthy();
    expect(container.textContent).toContain("January 10, 2016");

    // Check row order: Gender, Born, Died, City/Region, Country
    const labels = Array.from(
      container.querySelectorAll(".flex.items-start.justify-between > span:first-child")
    ).map((el) => el.textContent?.trim());
    expect(labels).toEqual(["Gender", "Born", "Died", "City/Region", "Country"]);

    // CRITICAL: Type should NOT be rendered in the UI
    expect(screen.queryByText("Type")).toBeNull();
    expect(screen.queryByText("Person")).toBeNull();
  });

  it("renders group details (Formed, City/Region, Country, Disbanded) and omits gender and sort name", () => {
    const { container } = render(ArtistInformationPanel, {
      props: {
        sortName: "Beatles, The",
        gender: null,
        artistType: "Group",
        beginDate: "1960",
        endDate: "1970-04-10",
        beginAreaName: "Liverpool",
        beginAreaMbid: "e1e771b9-7a63-40bf-b5ef-139ec42b6a9b",
        areaName: "United Kingdom",
      },
    });

    // Sort name omitted
    expect(screen.queryByText("Sort Name")).toBeNull();
    expect(screen.queryByText("Beatles, The")).toBeNull();

    // Group should say Formed / Disbanded / City/Region / Country
    expect(screen.getByText("Formed")).toBeTruthy();
    expect(screen.getByText("Disbanded")).toBeTruthy();
    expect(screen.getByText("City/Region")).toBeTruthy();
    expect(screen.getByText("Liverpool")).toBeTruthy();
    expect(screen.getByText("Country")).toBeTruthy();
    expect(screen.getByText("United Kingdom")).toBeTruthy();

    // Check row order: Formed, Disbanded, City/Region, Country
    const labels = Array.from(
      container.querySelectorAll(".flex.items-start.justify-between > span:first-child")
    ).map((el) => el.textContent?.trim());
    expect(labels).toEqual(["Formed", "Disbanded", "City/Region", "Country"]);

    // No gender
    expect(screen.queryByText("Gender")).toBeNull();

    // No Type label
    expect(screen.queryByText("Type")).toBeNull();
    expect(screen.queryByText("Group")).toBeNull();
  });

  it("renders card variant with collapsible details tag", () => {
    const { container } = render(ArtistInformationPanel, {
      props: {
        gender: "female",
        artistType: "Person",
        variant: "card",
      },
    });

    const details = container.querySelector("details");
    expect(details).toBeTruthy();
    expect(screen.getByText("Artist Information")).toBeTruthy();
  });
});
