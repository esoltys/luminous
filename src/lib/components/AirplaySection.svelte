<!--
  "On the radio now" (#1998): internet radio stations playing the shown song right
  now, from TunePing. Opt-in: until the user agrees, it is a consent card and nothing
  is sent. The backend (`get_airplay_now`, `src-tauri/src/airplay.rs`) owns matching,
  caching and back-off and re-checks the opt-in itself; this component only debounces
  and renders. It mounts only while the Information tab shows a song, so it never
  fetches off-screen. TunePing is in public beta: removing the section means deleting
  this component, its `InfoSections` line and the backend module.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    ArrowsClockwiseIcon as RefreshCw,
    ArrowRightIcon as ArrowRight,
    RadioIcon as Radio,
  } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import { radioStore } from "../stores/radio.svelte";
  import type { AirplayNow, Song } from "../types";
  import { countryDisplayName } from "../utils/country";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import HelpTip from "./HelpTip.svelte";

  interface Props {
    song: Song;
  }

  let { song }: Props = $props();

  /** Wait this long on a song before asking, so skipping through a playlist sends nothing. */
  const DEBOUNCE_MS = 1500;
  /** Stations shown before "+N more". */
  const VISIBLE_STATIONS = 8;

  let result = $state<AirplayNow | null>(null);
  let resultSongId = $state<number | null>(null);
  let loading = $state(false);
  let now = $state(Date.now());

  let isStation = $derived(!!song.path?.startsWith("radio://"));
  let active = $derived(prefs.onlineEnabled && prefs.airplayEnabled && !isStation);
  let showConsent = $derived(
    prefs.onlineEnabled && !prefs.airplayEnabled && !prefs.airplayPromptDismissed && !isStation,
  );

  let current = $derived(resultSongId === song.id ? result : null);
  let stations = $derived(current?.stations ?? []);
  let hiddenCount = $derived(Math.max(0, stations.length - VISIBLE_STATIONS));
  let unavailable = $derived(current?.status.kind === "unavailable");
  /** Nothing to show for this song: a station, or a song with no artist or title. */
  let hidden = $derived(
    current?.status.kind === "unsupported" || current?.status.kind === "disabled",
  );

  async function load(songId: number, forceRefresh: boolean) {
    loading = true;
    try {
      const answer = await invoke<AirplayNow>("get_airplay_now", { songId, forceRefresh });
      if (songId !== song.id) return;
      result = answer;
      resultSongId = songId;
    } catch {
      // The command never fails by contract; an IPC error just leaves the last result.
    } finally {
      if (songId === song.id) loading = false;
    }
  }

  $effect(() => {
    const songId = song.id;
    if (!active) return;
    loading = true;
    const timer = setTimeout(() => void load(songId, false), DEBOUNCE_MS);
    return () => clearTimeout(timer);
  });

  // Keep "still playing as of …" honest while the section stays open.
  $effect(() => {
    if (!active) return;
    const tick = setInterval(() => (now = Date.now()), 15_000);
    return () => clearInterval(tick);
  });

  function ageLabel(confirmedAt: number): string {
    const seconds = Math.max(0, Math.round(now / 1000 - confirmedAt));
    const rtf = new Intl.RelativeTimeFormat(i18n.currentLocale, { numeric: "auto" });
    const age =
      seconds < 60
        ? rtf.format(-seconds, "second")
        : rtf.format(-Math.round(seconds / 60), "minute");
    return i18n.t("airplay.confirmedAgo", { age });
  }

  function details(station: AirplayNow["stations"][number]): string {
    const parts: string[] = [];
    if (station.country) {
      parts.push(countryDisplayName(station.country, i18n.currentLocale) || station.country);
    }
    if (station.bitrate) parts.push(i18n.t("airplay.bitrate", { bitrate: station.bitrate }));
    return parts.join(" · ");
  }
</script>

{#if showConsent}
  <section class="space-y-2 text-xs pt-1" aria-labelledby="airplay-consent-title">
    <div class="flex items-center gap-1.5 font-semibold text-brand-text-secondary">
      <Radio class="w-3.5 h-3.5 text-brand-accent shrink-0" />
      <h3 id="airplay-consent-title">{i18n.t("airplay.title")}</h3>
    </div>
    <p class="text-brand-text-secondary leading-relaxed text-pretty">
      {i18n.t("airplay.consentBody")}
    </p>
    <div class="flex flex-wrap gap-2">
      <button
        type="button"
        onclick={() => prefs.setAirplayEnabled(true)}
        class="py-1 px-2.5 rounded-lg bg-brand-accent text-brand-accent-contrast font-medium hover:opacity-95 active:opacity-90 transition-opacity cursor-pointer"
      >
        {i18n.t("airplay.consentAccept")}
      </button>
      <button
        type="button"
        onclick={() => prefs.dismissAirplayPrompt()}
        class="py-1 px-2.5 rounded-lg bg-brand-sidebar/40 hover:bg-brand-sidebar/70 text-brand-text-primary border border-brand-border/60 font-medium transition-colors cursor-pointer"
      >
        {i18n.t("airplay.consentDecline")}
      </button>
    </div>
  </section>
{:else if active && !hidden && stations.length > 0}
  <section class="space-y-1.5 text-xs pt-1" aria-labelledby="airplay-title" aria-busy={loading}>
    <div class="flex items-center gap-1.5 font-semibold text-brand-text-secondary">
      <Radio class="w-3.5 h-3.5 text-brand-accent shrink-0" />
      <h3 id="airplay-title" class="min-w-0 truncate">{i18n.t("airplay.title")}</h3>
      <HelpTip text={i18n.t("airplay.detectedHint")} label={i18n.t("airplay.detectedLabel")} />
      <button
        type="button"
        onclick={() => void load(song.id, true)}
        disabled={loading}
        class="ml-auto p-0.5 text-brand-text-secondary/70 hover:text-brand-accent disabled:opacity-50 transition-colors cursor-pointer disabled:cursor-default"
        aria-label={i18n.t("airplay.refresh")}
      >
        <RefreshCw class="w-3.5 h-3.5 {loading ? 'animate-spin' : ''}" />
      </button>
    </div>

    <div class="space-y-1">
      {#if !current && loading}
        <p class="text-brand-text-secondary/70">{i18n.t("airplay.loading")}</p>
      {:else if current}
        {#if unavailable}
          <p class="text-brand-text-secondary/70">
            {stations.length > 0
              ? i18n.t("airplay.unavailableStale")
              : i18n.t("airplay.unavailable")}
          </p>
        {/if}
        {#if stations.length > 0}
          <ul class="space-y-1" class:opacity-60={unavailable}>
            {#each stations.slice(0, VISIBLE_STATIONS) as station (station.id)}
              {@const meta = [details(station), ageLabel(station.confirmed_at)]
                .filter(Boolean)
                .join(" · ")}
              <li class="min-w-0">
                <button
                  type="button"
                  onclick={() => {
                    radioStore.openStationInDiscover(station.name);
                    navigationStore.activeTab = "radio";
                  }}
                  class="group/st inline-flex items-center gap-1 max-w-full text-left cursor-pointer"
                  title={meta || i18n.t("airplay.openInRadio", {}, "Open in Internet Radio")}
                >
                  <span
                    class="font-medium text-brand-text-primary truncate underline decoration-brand-text-secondary/40 group-hover/st:decoration-brand-accent"
                    >{station.name}</span
                  >
                  <ArrowRight
                    class="w-3 h-3 text-brand-text-secondary opacity-0 group-hover/st:opacity-100 group-focus-visible/st:opacity-100 transition-opacity shrink-0"
                  />
                </button>
              </li>
            {/each}
          </ul>
          {#if hiddenCount > 0}
            <p class="text-brand-text-secondary/70">{i18n.plural("airplay.more", hiddenCount)}</p>
          {/if}
        {:else if !unavailable}
          <p class="text-brand-text-secondary/70">{i18n.t("airplay.empty")}</p>
        {/if}
        <button
          type="button"
          onclick={() => current && openExternalUrl(current.attribution_url)}
          class="inline-flex items-center gap-1 text-[11px] text-brand-text-secondary/60 hover:text-brand-accent underline decoration-brand-text-secondary/30 transition-colors cursor-pointer"
        >
          {i18n.t("airplay.attribution")}
        </button>
      {/if}
    </div>
  </section>
{/if}
