<script lang="ts">
  import {
    TicketIcon as Ticket,
    CaretDownIcon as CaretDown,
    MapPinIcon as MapPin,
    ArrowSquareOutIcon as ExternalLink,
    ClockIcon as Clock,
  } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import type { ArtistEvent } from "../types";
  import SocialIcon from "./SocialIcon.svelte";

  interface Props {
    events: ArtistEvent[];
    loading?: boolean;
    artistName: string;
    songkickUrl?: string | null;
    setlistfmUrl?: string | null;
    bandsintownUrl?: string | null;
    onOpenUrl?: (url: string) => void;
    class?: string;
  }

  let {
    events = [],
    loading = false,
    artistName,
    songkickUrl,
    setlistfmUrl,
    bandsintownUrl,
    onOpenUrl,
    class: className = "",
  }: Props = $props();

  let showPast = $state(false);

  // Today in UTC YYYY-MM-DD
  const todayStr = new Date().toISOString().slice(0, 10);

  let upcomingEvents = $derived(
    events.filter((e) => !e.begin_date || e.begin_date >= todayStr)
  );

  let pastEvents = $derived(
    events.filter((e) => e.begin_date && e.begin_date < todayStr)
  );

  function parseDateParts(dateStr?: string | null) {
    if (!dateStr) return { month: "", day: "", year: "" };
    const parts = dateStr.trim().split("-");
    const year = parts[0] || "";
    let month = "";
    let day = "";

    if (parts.length >= 2) {
      const mNum = parseInt(parts[1], 10);
      if (!isNaN(mNum) && mNum >= 1 && mNum <= 12) {
        // Localized short month
        const d = new Date(Date.UTC(2000, mNum - 1, 1));
        month = new Intl.DateTimeFormat(i18n.currentLocale, {
          month: "short",
          timeZone: "UTC",
        }).format(d).toUpperCase().replace(/\.$/, "");
      }
    }

    if (parts.length >= 3) {
      const dNum = parseInt(parts[2], 10);
      if (!isNaN(dNum)) {
        day = String(dNum);
      }
    }

    return { month, day, year };
  }

  let effectiveSongkickUrl = $derived(
    songkickUrl || `https://www.songkick.com/search?query=${encodeURIComponent(artistName)}`
  );

  let effectiveBandsintownUrl = $derived(
    bandsintownUrl || `https://www.bandsintown.com/a/${encodeURIComponent(artistName)}`
  );

  let effectiveSetlistfmUrl = $derived(
    setlistfmUrl || `https://www.setlist.fm/search?query=${encodeURIComponent(artistName)}`
  );

  let hasEvents = $derived(events.length > 0);
  let totalCount = $derived(events.length);
</script>

<details
  open
  class="group border border-brand-border/60 rounded-lg bg-brand-sidebar/40 overflow-hidden {className}"
>
  <summary
    class="flex items-center justify-between px-3 py-2 text-xs font-semibold text-brand-text-secondary cursor-pointer select-none hover:text-brand-text-primary transition-colors"
  >
    <div class="flex items-center gap-2 min-w-0">
      <Ticket class="w-3.5 h-3.5 text-brand-accent shrink-0" />
      <span>{i18n.t("artistEvents.panelTitle", {}, "Upcoming Concerts & Events")}</span>
      {#if upcomingEvents.length > 0}
        <span
          class="px-1.5 py-0.2 rounded-full text-[10px] font-medium bg-brand-accent/20 text-brand-accent"
        >
          {upcomingEvents.length}
        </span>
      {/if}
    </div>
    <CaretDown
      class="w-3.5 h-3.5 text-brand-text-secondary/70 group-open:rotate-180 transition-transform shrink-0"
    />
  </summary>

  <div class="p-3 border-t border-brand-border/40 space-y-3 text-xs">
    {#if loading}
      <div class="py-3 text-center text-xs text-brand-text-secondary/70 animate-pulse">
        {i18n.t("artistEvents.loading", {}, "Loading upcoming events...")}
      </div>
    {:else}
      <!-- Upcoming Events List -->
      {#if upcomingEvents.length > 0}
        <div class="space-y-2">
          {#each upcomingEvents as event (event.id)}
            {@const { month, day, year } = parseDateParts(event.begin_date)}
            {@const primaryTicketUrl = event.ticket_urls[0]}
            {@const primaryEventUrl = event.event_urls[0]}
            <div
              class="flex items-start justify-between gap-3 p-2.5 rounded-lg border border-brand-border/50 bg-brand-main/40 hover:bg-brand-main/70 transition-colors"
            >
              <!-- Date Badge -->
              <div
                class="flex flex-col items-center justify-center shrink-0 w-11 h-12 rounded bg-brand-sidebar border border-brand-border text-center select-none"
              >
                {#if month}
                  <span class="text-[9px] font-bold tracking-wider text-brand-accent uppercase leading-none mt-1">
                    {month}
                  </span>
                {/if}
                {#if day}
                  <span class="text-sm font-extrabold text-brand-text-primary leading-tight">
                    {day}
                  </span>
                {:else if year}
                  <span class="text-[10px] font-semibold text-brand-text-secondary leading-tight">
                    {year}
                  </span>
                {/if}
                {#if day && year}
                  <span class="text-[8px] text-brand-text-secondary/60 leading-none mb-0.5">
                    {year}
                  </span>
                {/if}
              </div>

              <!-- Event Details -->
              <div class="flex-1 min-w-0 flex flex-col gap-1">
                <div class="flex items-center gap-1.5 flex-wrap">
                  <span class="font-medium text-brand-text-primary text-xs leading-snug">
                    {event.name}
                  </span>
                  {#if event.disambiguation}
                    <span class="text-[10px] text-brand-text-secondary/70">
                      ({event.disambiguation})
                    </span>
                  {/if}
                  {#if event.event_type}
                    <span
                      class="px-1.5 py-0.2 rounded text-[9px] font-medium bg-brand-border/60 text-brand-text-secondary uppercase tracking-wider"
                    >
                      {event.event_type}
                    </span>
                  {/if}
                  {#if event.cancelled}
                    <span
                      class="px-1.5 py-0.2 rounded text-[9px] font-medium bg-red-500/20 text-red-400 uppercase tracking-wider"
                    >
                      {i18n.t("artistEvents.cancelled", {}, "Cancelled")}
                    </span>
                  {/if}
                </div>

                <!-- Venue & Location -->
                {#if event.venue_name || event.venue_city || event.venue_country}
                  <div class="flex items-center gap-1 text-[11px] text-brand-text-secondary truncate">
                    <MapPin class="w-3 h-3 text-brand-text-secondary/70 shrink-0" />
                    <span class="truncate">
                      {#if event.venue_name}
                        <span class="font-medium text-brand-text-secondary">{event.venue_name}</span>
                      {/if}
                      {#if event.venue_name && (event.venue_city || event.venue_country)}
                        <span class="mx-1">•</span>
                      {/if}
                      {#if event.venue_city && event.venue_country}
                        <span>{event.venue_city}, {event.venue_country}</span>
                      {:else}
                        <span>{event.venue_city || event.venue_country || ""}</span>
                      {/if}
                    </span>
                  </div>
                {/if}

                {#if event.time}
                  <div class="flex items-center gap-1 text-[10px] text-brand-text-secondary/70">
                    <Clock class="w-2.5 h-2.5 shrink-0" />
                    <span>{event.time}</span>
                  </div>
                {/if}
              </div>

              <!-- Action button (Tickets or Event Link) -->
              <div class="shrink-0 flex items-center self-center">
                {#if primaryTicketUrl}
                  <button
                    type="button"
                    onclick={() => onOpenUrl?.(primaryTicketUrl)}
                    class="px-2.5 py-1 rounded bg-brand-accent/15 text-brand-accent hover:bg-brand-accent hover:text-white text-[11px] font-medium inline-flex items-center gap-1 transition-colors cursor-pointer"
                    title={primaryTicketUrl}
                  >
                    <Ticket class="w-3 h-3" />
                    <span>{i18n.t("artistEvents.viewTickets", {}, "Tickets")}</span>
                  </button>
                {:else if primaryEventUrl}
                  <button
                    type="button"
                    onclick={() => onOpenUrl?.(primaryEventUrl)}
                    class="px-2.5 py-1 rounded border border-brand-border text-brand-text-secondary hover:text-brand-text-primary text-[11px] font-medium inline-flex items-center gap-1 transition-colors cursor-pointer"
                    title={primaryEventUrl}
                  >
                    <ExternalLink class="w-3 h-3" />
                    <span>{i18n.t("artistEvents.viewDetails", {}, "Details")}</span>
                  </button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <div class="py-2 text-center text-xs text-brand-text-secondary/70">
          {i18n.t("artistEvents.noEvents", {}, "No upcoming concerts found")}
        </div>
      {/if}

      <!-- Past Events Toggle -->
      {#if pastEvents.length > 0}
        <div class="pt-1">
          <button
            type="button"
            onclick={() => (showPast = !showPast)}
            class="text-[11px] font-medium text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer inline-flex items-center gap-1"
          >
            <span>
              {showPast
                ? i18n.t("artistEvents.hidePastEvents", {}, "Hide past events")
                : i18n.t("artistEvents.showPastEvents", { count: pastEvents.length }, `Show past events (${pastEvents.length})`)}
            </span>
          </button>

          {#if showPast}
            <div class="mt-2 space-y-1.5 opacity-80">
              {#each pastEvents as event (event.id)}
                {@const { month, day, year } = parseDateParts(event.begin_date)}
                <div
                  class="flex items-center justify-between gap-3 px-2 py-1.5 rounded border border-brand-border/30 bg-brand-main/20 text-[11px]"
                >
                  <div class="flex items-center gap-2 min-w-0">
                    <span class="text-brand-text-secondary/60 shrink-0 font-mono text-[10px] w-20">
                      {event.begin_date || ""}
                    </span>
                    <span class="text-brand-text-primary truncate">
                      {event.name}
                    </span>
                    {#if event.venue_city || event.venue_name}
                      <span class="text-brand-text-secondary/60 truncate">
                        — {event.venue_city || event.venue_name}
                      </span>
                    {/if}
                  </div>
                  {#if event.ticket_urls[0] || event.event_urls[0]}
                    <button
                      type="button"
                      onclick={() => onOpenUrl?.(event.ticket_urls[0] || event.event_urls[0])}
                      class="text-brand-text-secondary hover:text-brand-text-primary shrink-0 cursor-pointer"
                      title={event.ticket_urls[0] || event.event_urls[0]}
                    >
                      <ExternalLink class="w-3 h-3" />
                    </button>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}

      <!-- Quick Platform Links -->
      <div class="pt-2 border-t border-brand-border/30 flex flex-wrap items-center justify-between gap-2 text-[11px]">
        <span class="text-brand-text-secondary/60">
          {i18n.t("artistEvents.otherPlatforms", {}, "Find tickets and tour dates on:")}
        </span>
        <div class="flex items-center gap-2">
          <button
            type="button"
            onclick={() => onOpenUrl?.(effectiveSongkickUrl)}
            class="inline-flex items-center gap-1 px-2 py-0.5 rounded border border-brand-border text-brand-text-secondary hover:text-brand-text-primary hover:border-brand-accent/40 transition-colors cursor-pointer"
            title="Songkick"
          >
            <SocialIcon platform="songkick" size={11} />
            <span>Songkick</span>
            <ExternalLink class="w-2.5 h-2.5 opacity-60" />
          </button>
          <button
            type="button"
            onclick={() => onOpenUrl?.(effectiveBandsintownUrl)}
            class="inline-flex items-center gap-1 px-2 py-0.5 rounded border border-brand-border text-brand-text-secondary hover:text-brand-text-primary hover:border-brand-accent/40 transition-colors cursor-pointer"
            title="Bandsintown"
          >
            <SocialIcon platform="bandsintown" size={11} />
            <span>Bandsintown</span>
            <ExternalLink class="w-2.5 h-2.5 opacity-60" />
          </button>
          <button
            type="button"
            onclick={() => onOpenUrl?.(effectiveSetlistfmUrl)}
            class="inline-flex items-center gap-1 px-2 py-0.5 rounded border border-brand-border text-brand-text-secondary hover:text-brand-text-primary hover:border-brand-accent/40 transition-colors cursor-pointer"
            title="Setlist.fm"
          >
            <ExternalLink class="w-3 h-3" />
            <span>Setlist.fm</span>
          </button>
        </div>
      </div>
    {/if}
  </div>
</details>
