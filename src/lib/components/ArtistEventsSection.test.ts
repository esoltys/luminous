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
});
