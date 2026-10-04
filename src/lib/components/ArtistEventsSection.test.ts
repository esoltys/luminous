import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import ArtistEventsSection from "./ArtistEventsSection.svelte";
import type { ArtistEvent } from "../types";

describe("ArtistEventsSection", () => {
  const sampleEvents: ArtistEvent[] = [
    {
      id: "evt-1",
      name: "Music of the Spheres: London",
      event_type: "Concert",
      begin_date: "2099-08-15",
      end_date: "2099-08-15",
      time: "20:00",
      cancelled: false,
      venue_name: "Wembley Stadium",
      venue_address: "London HA9 0WS",
      venue_city: "London",
      venue_country: "United Kingdom",
      venue_latitude: 51.556,
      venue_longitude: -0.279,
      ticket_urls: ["https://tickets.example.com/london"],
      event_urls: ["https://example.com/london"],
      disambiguation: "night 1",
    },
    {
      id: "evt-2",
      name: "Past Tour 2020",
      event_type: "Festival",
      begin_date: "2020-05-01",
      end_date: "2020-05-01",
      cancelled: false,
      venue_name: "Old Arena",
      venue_city: "Paris",
      venue_country: "France",
      ticket_urls: [],
      event_urls: ["https://example.com/paris"],
    },
  ];

  it("renders upcoming events and platform links", () => {
    const handleOpenUrl = vi.fn();
    const { container } = render(ArtistEventsSection, {
      props: {
        events: sampleEvents,
        artistName: "Coldplay",
        songkickUrl: "https://www.songkick.com/artists/coldplay",
        onOpenUrl: handleOpenUrl,
      },
    });

    expect(screen.getByText("Upcoming Concerts & Events")).toBeTruthy();
    expect(screen.getByText("Music of the Spheres: London")).toBeTruthy();
    expect(screen.getByText("(night 1)")).toBeTruthy();
    expect(screen.getByText("Wembley Stadium")).toBeTruthy();
    expect(container.textContent).toContain("London, United Kingdom");
    expect(screen.getByText("Tickets")).toBeTruthy();
    expect(screen.getByText("Songkick")).toBeTruthy();
    expect(screen.getByText("Bandsintown")).toBeTruthy();
    expect(screen.getByText("Setlist.fm")).toBeTruthy();
  });

  it("handles ticket click action", async () => {
    const handleOpenUrl = vi.fn();
    render(ArtistEventsSection, {
      props: {
        events: sampleEvents,
        artistName: "Coldplay",
        onOpenUrl: handleOpenUrl,
      },
    });

    const ticketBtn = screen.getByText("Tickets");
    await fireEvent.click(ticketBtn);
    expect(handleOpenUrl).toHaveBeenCalledWith("https://tickets.example.com/london");
  });

  it("displays no events message when upcoming list is empty", () => {
    const { container } = render(ArtistEventsSection, {
      props: {
        events: [],
        artistName: "Radiohead",
      },
    });

    expect(screen.getByText("No upcoming concerts found")).toBeTruthy();
  });

  it("toggles past events display", async () => {
    render(ArtistEventsSection, {
      props: {
        events: sampleEvents,
        artistName: "Coldplay",
      },
    });

    expect(screen.queryByText("Past Tour 2020")).toBeNull();
    const toggleBtn = screen.getByText(/Show past events/i);
    expect(toggleBtn).toBeTruthy();

    await fireEvent.click(toggleBtn);
    expect(screen.getByText("Past Tour 2020")).toBeTruthy();
    expect(screen.getByText(/Hide past events/i)).toBeTruthy();
  });

  it("sorts past events descending with most recent on top", async () => {
    const pastList: ArtistEvent[] = [
      { id: "e1", name: "Old Show 2017", begin_date: "2017-05-01", cancelled: false, ticket_urls: [], event_urls: [] },
      { id: "e2", name: "Recent Show 2023", begin_date: "2023-09-01", cancelled: false, ticket_urls: [], event_urls: [] },
      { id: "e3", name: "Mid Show 2020", begin_date: "2020-01-15", cancelled: false, ticket_urls: [], event_urls: [] },
    ];

    const { container } = render(ArtistEventsSection, {
      props: {
        events: pastList,
        artistName: "Test Artist",
      },
    });

    const toggleBtn = screen.getByText(/Show past events/i);
    await fireEvent.click(toggleBtn);

    const eventNames = Array.from(container.querySelectorAll(".text-brand-text-primary.truncate"))
      .map((el) => el.textContent?.trim());

    expect(eventNames).toEqual(["Recent Show 2023", "Mid Show 2020", "Old Show 2017"]);
  });

  it("links header to MusicBrainz events page when musicbrainzUrl is provided", async () => {
    const handleOpenUrl = vi.fn();
    render(ArtistEventsSection, {
      props: {
        events: sampleEvents,
        artistName: "Coldplay",
        musicbrainzUrl: "https://musicbrainz.org/artist/cc197027-5f32-4d2b-add3-13eac73b1230/events",
        onOpenUrl: handleOpenUrl,
      },
    });

    const headerBtn = screen.getByRole("button", { name: /Upcoming Concerts & Events/i });
    expect(headerBtn).toBeTruthy();
    await fireEvent.click(headerBtn);
    expect(handleOpenUrl).toHaveBeenCalledWith(
      "https://musicbrainz.org/artist/cc197027-5f32-4d2b-add3-13eac73b1230/events"
    );
  });

  it("limits upcoming events to 5 per page with pagination controls", async () => {
    const manyEvents: ArtistEvent[] = Array.from({ length: 7 }, (_, i) => ({
      id: `evt-${i + 1}`,
      name: `Upcoming Tour Show ${i + 1}`,
      begin_date: `2099-01-0${i + 1}`,
      cancelled: false,
      ticket_urls: [],
      event_urls: [],
    }));

    render(ArtistEventsSection, {
      props: {
        events: manyEvents,
        artistName: "Busy Artist",
      },
    });

    // Page 1: shows shows 1 to 5
    expect(screen.getByText("Upcoming Tour Show 1")).toBeTruthy();
    expect(screen.getByText("Upcoming Tour Show 5")).toBeTruthy();
    expect(screen.queryByText("Upcoming Tour Show 6")).toBeNull();
    expect(screen.getByText("1–5 of 7")).toBeTruthy();

    // Click Next page
    const nextBtn = screen.getByRole("button", { name: "Next page" });
    await fireEvent.click(nextBtn);

    // Page 2: shows shows 6 and 7
    expect(screen.queryByText("Upcoming Tour Show 1")).toBeNull();
    expect(screen.getByText("Upcoming Tour Show 6")).toBeTruthy();
    expect(screen.getByText("Upcoming Tour Show 7")).toBeTruthy();
    expect(screen.getByText("6–7 of 7")).toBeTruthy();
  });
});
