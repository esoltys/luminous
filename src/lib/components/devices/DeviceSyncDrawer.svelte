<script lang="ts">
  import { cubicOut } from "svelte/easing";
  import { CaretDownIcon } from "phosphor-svelte";
  import { fly } from "../../utils/motion";
  import { devicesStore } from "../../stores/devices.svelte";
  import { playerStore } from "../../stores/player.svelte";
  import { i18n, formatNumber } from "../../stores/i18n.svelte";
  import { formatStorageSize, STORAGE_MB } from "../../utils/formatters";
  import { conversionLabel, deviceName } from "../../utils/devices";
  import type { SyncProgress } from "../../types";
  import Button from "../Button.svelte";

  /**
   * A running device sync, docked above the player bar (#839): never a
   * modal, so the app stays usable. Hiding it leaves the progress on the
   * device's sidebar row.
   */
  const DRAWER_MS = 250;

  const id = $derived(devicesStore.syncingId);
  const device = $derived(id ? devicesStore.device(id) : undefined);
  const progress = $derived<SyncProgress | undefined>(id ? devicesStore.progress[id] : undefined);
  const name = $derived(device ? deviceName(device) : "");
  const open = $derived(!!device && !devicesStore.drawerHidden);

  function songLabel(song: { artist: string | null; title: string | null }): string {
    return [song.artist, song.title].filter(Boolean).join(", ");
  }

  const step = $derived.by(() => {
    const s = progress?.step;
    if (!s) return i18n.t("devices.syncingStarting");
    switch (s.kind) {
      case "removing":
        return i18n.t("devices.stepRemoving");
      case "converting":
        return i18n.t("devices.stepConverting", {
          song: songLabel(s.song),
          target: conversionLabel(s.format, s.bitrateKbps),
        });
      case "copying":
        return i18n.t("devices.stepCopying", { song: songLabel(s.song) });
      case "writingPlaylists":
        return i18n.t("devices.stepPlaylists");
      case "writingFolderArt":
        return i18n.t("devices.stepFolderArt");
      case "finishing":
        return i18n.t("devices.stepFinishing");
    }
  });

  const timeLeft = $derived.by(() => {
    const secs = progress?.etaSecs;
    if (secs == null) return "";
    const minutes = Math.round(secs / 60);
    return minutes < 1
      ? i18n.t("devices.lessThanAMinute")
      : i18n.plural("devices.minutesLeft", minutes, { count: formatNumber(minutes) });
  });
</script>

{#if open && device}
  <div
    class="absolute inset-x-4 z-30 rounded-2xl border border-brand-border/50 acrylic-float shadow-2xl px-4 py-3 flex flex-col gap-2 {playerStore.currentSong
      ? 'bottom-[6.5rem]'
      : 'bottom-4'}"
    transition:fly={{ y: 16, duration: DRAWER_MS, easing: cubicOut }}
    role="status"
    data-testid="device-sync-drawer"
  >
    <div class="flex items-center justify-between gap-3">
      <p class="min-w-0 truncate text-sm">
        <span class="font-semibold text-brand-text-primary"
          >{i18n.t("devices.syncingTo", { storage: name })}</span
        >
        {#if progress}
          <span class="ml-2 text-brand-text-secondary"
            >{i18n.t("devices.songsOf", {
              done: formatNumber(progress.songsDone),
              total: formatNumber(progress.songsTotal),
            })}</span
          >
        {/if}
      </p>
      <div class="flex items-center gap-1 shrink-0">
        <Button size="sm" onclick={() => devicesStore.stopSync(device.id)}
          >{i18n.t("devices.stopSync")}</Button
        >
        <button
          class="w-8 h-8 flex items-center justify-center rounded-full text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10 transition-colors"
          onclick={() => (devicesStore.drawerHidden = true)}
          title={i18n.t("devices.hideProgress")}
          aria-label={i18n.t("devices.hideProgress")}
        >
          <CaretDownIcon class="w-4 h-4" />
        </button>
      </div>
    </div>
    <div class="h-1.5 rounded-full bg-brand-border/40 overflow-hidden" aria-hidden="true">
      <div
        class="h-full rounded-full bg-brand-accent-text"
        style:width="{progress && progress.bytesTotal > 0
          ? (progress.bytesDone / progress.bytesTotal) * 100
          : 0}%"
      ></div>
    </div>
    <div class="flex items-center justify-between gap-4 text-xs">
      <span class="min-w-0 truncate text-brand-text-secondary">{step}</span>
      {#if progress}
        <span class="flex items-center gap-4 shrink-0 tabular-nums text-brand-text-secondary">
          <span
            >{i18n.t("devices.bytesOf", {
              done: formatStorageSize(progress.bytesDone),
              total: formatStorageSize(progress.bytesTotal),
            })}</span
          >
          {#if progress.bytesPerSec}
            <span
              >{formatNumber(progress.bytesPerSec / STORAGE_MB, { maximumFractionDigits: 0 })} MB/s</span
            >
          {/if}
          {#if timeLeft}<span class="font-semibold text-brand-text-primary">{timeLeft}</span>{/if}
        </span>
      {/if}
    </div>
    <p class="text-xs text-brand-text-secondary">
      {i18n.t("devices.stoppingKeeps", { device: name })}
    </p>
  </div>
{/if}
