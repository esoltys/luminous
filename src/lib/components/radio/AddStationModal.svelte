<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import {
    XIcon as X,
    FileArrowUpIcon as FileUp,
    WarningCircleIcon as AlertCircle,
  } from "phosphor-svelte";
  import { radioStore } from "../../stores/radio.svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import type { RadioDirectoryStation } from "../../types";
  import Modal from "../Modal.svelte";

  interface Props {
    station?: RadioDirectoryStation | null;
    onClose: () => void;
  }

  let { station = null, onClose }: Props = $props();

  const isEditing = $derived(station !== null);

  let name = $state("");
  let url = $state("");
  let tags = $state("");
  let submitting = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    name = station?.name ?? "";
    url = station?.urlResolved ?? "";
    tags = station?.tags ?? "";
  });

  const canSubmit = $derived(!submitting && name.trim().length > 0 && url.trim().length > 0);

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (!canSubmit) return;
    submitting = true;
    error = null;
    try {
      if (station) {
        await radioStore.updateCustomStation(
          station.stationuuid,
          name.trim(),
          url.trim(),
          tags.trim(),
        );
      } else {
        await radioStore.addCustomStation(name.trim(), url.trim(), tags.trim());
      }
      onClose();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }

  async function handleImportFile() {
    if (submitting) return;
    error = null;
    try {
      const selected = await open({
        multiple: false,
        title: i18n.t("radio.importStationFileTitle"),
        filters: [
          {
            name: "Station playlists (*.pls, *.m3u, *.m3u8, *.xspf)",
            extensions: ["pls", "m3u", "m3u8", "xspf"],
          },
        ],
      });
      if (selected && typeof selected === "string") {
        submitting = true;
        await radioStore.importCustomStations(selected);
        onClose();
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      submitting = false;
    }
  }
</script>

<Modal {onClose} closeOnBackdropClick ariaLabelledby="add-station-title">
  <div class="flex items-center justify-between px-5 py-4 border-b border-brand-border/50">
    <h2 id="add-station-title" class="text-base font-semibold text-brand-text-primary">
      {isEditing ? i18n.t("radio.editStationTitle") : i18n.t("radio.addStationTitle")}
    </h2>
    <button
      type="button"
      onclick={onClose}
      aria-label={i18n.t("common.close")}
      class="p-1 rounded-lg text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-surface transition-colors"
    >
      <X class="w-4 h-4" />
    </button>
  </div>

  <form onsubmit={handleSubmit} class="p-5 space-y-4">
    {#if error}
      <div
        role="alert"
        data-testid="add-station-error"
        class="flex items-start gap-2 px-3 py-2 rounded-lg border border-brand-border bg-brand-surface text-xs text-brand-text-primary"
      >
        <AlertCircle class="w-4 h-4 shrink-0 text-brand-accent-text mt-0.5" />
        <span>{error}</span>
      </div>
    {/if}

    <div class="space-y-1.5">
      <label for="station-name" class="block text-xs font-medium text-brand-text-secondary">
        {i18n.t("radio.stationNameLabel")}
      </label>
      <input
        id="station-name"
        data-testid="add-station-name"
        type="text"
        bind:value={name}
        required
        placeholder={i18n.t("radio.stationNamePlaceholder")}
        class="w-full px-3 py-2 rounded-lg bg-brand-surface border border-brand-border text-sm text-brand-text-primary placeholder:text-brand-text-secondary/50 focus:outline-none focus:border-brand-accent"
      />
    </div>

    <div class="space-y-1.5">
      <label for="station-url" class="block text-xs font-medium text-brand-text-secondary">
        {i18n.t("radio.stationUrlLabel")}
      </label>
      <input
        id="station-url"
        data-testid="add-station-url"
        type="url"
        bind:value={url}
        required
        placeholder="https://example.com/stream.mp3"
        class="w-full px-3 py-2 rounded-lg bg-brand-surface border border-brand-border text-sm text-brand-text-primary placeholder:text-brand-text-secondary/50 focus:outline-none focus:border-brand-accent"
      />
      <p class="text-[11px] text-brand-text-secondary">
        {i18n.t("radio.stationUrlHint")}
      </p>
    </div>

    <div class="space-y-1.5">
      <label for="station-tags" class="block text-xs font-medium text-brand-text-secondary">
        {i18n.t("radio.stationTagsLabel")}
      </label>
      <input
        id="station-tags"
        data-testid="add-station-tags"
        type="text"
        bind:value={tags}
        placeholder={i18n.t("radio.stationTagsPlaceholder")}
        class="w-full px-3 py-2 rounded-lg bg-brand-surface border border-brand-border text-sm text-brand-text-primary placeholder:text-brand-text-secondary/50 focus:outline-none focus:border-brand-accent"
      />
    </div>

    <div class="flex items-center justify-between gap-2 pt-2 border-t border-brand-border/40">
      {#if !isEditing}
        <button
          type="button"
          disabled={submitting}
          onclick={() => void handleImportFile()}
          data-testid="add-station-import-file"
          class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-brand-border text-xs font-medium text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-surface disabled:opacity-50 transition-colors"
        >
          <FileUp class="w-3.5 h-3.5" />
          <span>{i18n.t("radio.importStationFileBtn")}</span>
        </button>
      {:else}
        <span></span>
      {/if}

      <div class="flex items-center gap-2">
        <button
          type="button"
          onclick={onClose}
          class="px-3.5 py-1.5 rounded-lg text-xs font-medium text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-surface transition-colors"
        >
          {i18n.t("common.cancel")}
        </button>
        <button
          type="submit"
          disabled={!canSubmit}
          data-testid="add-station-submit"
          class="px-4 py-1.5 rounded-lg bg-brand-accent text-brand-accent-contrast text-xs font-medium hover:opacity-90 disabled:opacity-50 transition-opacity"
        >
          {#if submitting}
            {isEditing ? i18n.t("radio.savingStation") : i18n.t("radio.addingStation")}
          {:else}
            {isEditing ? i18n.t("common.save") : i18n.t("radio.addStationBtn")}
          {/if}
        </button>
      </div>
    </div>
  </form>
</Modal>
