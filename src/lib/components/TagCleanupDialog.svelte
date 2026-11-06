<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import {
    BroomIcon as Broom,
    CircleNotchIcon as LoaderCircle,
    WarningIcon as AlertTriangle,
    XIcon as X,
  } from "phosphor-svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import { i18n, formatNumber } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import type { CruftField, CruftKind, TagCleanupPreview, TagCleanupResult } from "../types";

  interface Props {
    /** Songs to check; empty means the whole local library. */
    songIds: number[];
    onClose: () => void;
  }

  // The backend emits `library-changed` after a clean-up, so views refresh
  // from that rather than from a callback here.
  let { songIds, onClose }: Props = $props();

  const KIND_LABEL_KEYS: Record<CruftKind, string> = {
    promo_comment: "tagCleanup.kindPromoComment",
    encoder: "tagCleanup.kindEncoder",
    encoded_by: "tagCleanup.kindEncodedBy",
    itunes_normalization: "tagCleanup.kindItunesNormalization",
    id3v1_duplicate: "tagCleanup.kindId3v1Duplicate",
  };

  /** Section order: the fields most likely to be someone's note first. */
  const KIND_ORDER: CruftKind[] = ["promo_comment", "encoded_by", "encoder", "itunes_normalization", "id3v1_duplicate"];

  /**
   * Kinds whose value is one row per track (an ID3v1 summary, an iTunNORM hex
   * blob) collapse into a single row with an explanation; the rest get one
   * row per distinct value.
   */
  const KIND_DESCRIPTION_KEYS: Partial<Record<CruftKind, string>> = {
    itunes_normalization: "tagCleanup.itunesNormalizationDesc",
    id3v1_duplicate: "tagCleanup.id3v1DuplicateDesc",
  };

  interface CruftGroup {
    key: string;
    kind: CruftKind;
    /** `null` for collapsed kinds. */
    value: string | null;
    entries: { songId: number; field: CruftField }[];
  }

  let preview = $state<TagCleanupPreview | null>(null);
  let errorMsg = $state<string | null>(null);
  let isApplying = $state(false);
  let progress = $state<{ checked: number; total: number } | null>(null);
  /** Keys of groups the user unticked; every other group is removed. */
  let kept = $state<Record<string, boolean>>({});

  let sections = $derived.by(() => {
    const groups = new Map<string, CruftGroup>();
    for (const song of preview?.songs ?? []) {
      for (const field of song.fields) {
        const collapsed = field.kind in KIND_DESCRIPTION_KEYS;
        const key = collapsed ? field.kind : `${field.kind}\u0000${field.value}`;
        let group = groups.get(key);
        if (!group) {
          group = { key, kind: field.kind, value: collapsed ? null : field.value, entries: [] };
          groups.set(key, group);
        }
        group.entries.push({ songId: song.song_id, field });
      }
    }
    return KIND_ORDER.map((kind) => ({
      kind,
      groups: [...groups.values()]
        .filter((g) => g.kind === kind)
        .sort((a, b) => b.entries.length - a.entries.length || (a.value ?? "").localeCompare(b.value ?? "")),
    })).filter((section) => section.groups.length > 0);
  });

  let selectedCount = $derived(
    sections.reduce(
      (total, section) =>
        total + section.groups.reduce((n, g) => n + (kept[g.key] ? 0 : g.entries.length), 0),
      0,
    ),
  );

  onMount(() => {
    const unlisten = listen<{ checked: number; total: number }>("tag-cleanup-progress", (event) => {
      progress = event.payload;
    });
    invoke<TagCleanupPreview>("preview_tag_cleanup", { songIds })
      .then((result) => { preview = result; })
      .catch((e) => { errorMsg = String(e); });
    return () => {
      unlisten.then((fn) => fn());
      // Closing before the check finishes stops the backend reading files.
      if (!preview && !errorMsg) invoke("cancel_tag_cleanup").catch(() => {});
    };
  });

  async function handleApply() {
    if (!preview || selectedCount === 0) return;
    isApplying = true;
    const bySong = new Map<number, CruftField[]>();
    for (const section of sections) {
      for (const group of section.groups) {
        if (kept[group.key]) continue;
        for (const { songId, field } of group.entries) {
          bySong.set(songId, [...(bySong.get(songId) ?? []), field]);
        }
      }
    }
    const selections = [...bySong].map(([song_id, fields]) => ({ song_id, fields }));
    try {
      const result = await invoke<TagCleanupResult>("apply_tag_cleanup", { selections });
      if (result.cleaned_songs > 0) {
        toastStore.show(
          i18n.t("tagCleanup.done", { count: result.removed_fields, songs: result.cleaned_songs }),
          "success",
        );
      }
      if (result.errors.length > 0) {
        console.warn("Tag clean-up errors:", result.errors);
        toastStore.show(i18n.t("tagCleanup.partialErrors", { count: result.errors.length }), "error");
      }
      onClose();
    } catch (e) {
      toastStore.show(i18n.t("tagCleanup.failedPrefix") + String(e), "error");
    } finally {
      isApplying = false;
    }
  }
</script>

<Modal onClose={isApplying ? () => {} : onClose} maxWidth="max-w-xl" ariaLabelledby="tag-cleanup-title">
  <div class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main">
    <div class="flex items-center gap-2">
      <Broom class="w-4 h-4 text-brand-accent-text" />
      <h3 id="tag-cleanup-title" class="text-sm font-bold">{i18n.t("tagCleanup.title")}</h3>
    </div>
    <button
      onclick={onClose}
      disabled={isApplying}
      aria-label={i18n.t("tagCleanup.closeBtn")}
      class="text-brand-text-secondary hover:text-brand-text-primary transition-colors disabled:opacity-50"
    >
      <X class="w-4 h-4" />
    </button>
  </div>

  <div class="flex-1 overflow-y-auto p-6 max-h-[calc(100vh-200px)] text-xs">
    {#if errorMsg}
      <div class="py-10 flex flex-col items-center gap-3 text-center">
        <AlertTriangle class="w-8 h-8 text-brand-text-primary" />
        <p class="text-brand-text-primary">{i18n.t("tagCleanup.failedPrefix")}{errorMsg}</p>
      </div>
    {:else if !preview}
      <div class="py-12 flex flex-col items-center justify-center gap-3">
        <LoadingSpinner
          label={progress && progress.total > 1
            ? i18n.t("tagCleanup.checkingProgress", { checked: progress.checked, total: progress.total })
            : i18n.t("tagCleanup.checking")}
          size="sm"
        />
      </div>
    {:else if preview.songs.length === 0}
      <p class="py-10 text-center text-brand-text-secondary">{i18n.t("tagCleanup.nothingFound")}</p>
    {:else}
      <p class="text-brand-text-secondary mb-4">{i18n.t("tagCleanup.intro")}</p>
      <div class="flex flex-col gap-4">
        {#each sections as section (section.kind)}
          <section class="flex flex-col gap-1.5">
            <h4 class="font-semibold text-brand-text-primary">{i18n.t(KIND_LABEL_KEYS[section.kind])}</h4>
            {#each section.groups as group (group.key)}
              {@const descriptionKey = KIND_DESCRIPTION_KEYS[group.kind]}
              <label class="flex items-start gap-2.5 rounded-lg border border-brand-border bg-brand-main px-3 py-2 cursor-pointer">
                <input
                  type="checkbox"
                  class="mt-0.5 rounded accent-brand-accent"
                  checked={!kept[group.key]}
                  disabled={isApplying}
                  onchange={(e) => { kept[group.key] = !(e.currentTarget as HTMLInputElement).checked; }}
                />
                <span class="flex-1 min-w-0 break-all select-text {group.value === '' ? 'italic text-brand-text-secondary' : 'text-brand-text-primary'}">
                  {#if descriptionKey}
                    {i18n.t(descriptionKey)}
                  {:else if group.value === ""}
                    {i18n.t("tagCleanup.emptyValue")}
                  {:else}
                    {group.value}
                  {/if}
                </span>
                {#if group.entries.length > 1}
                  <span class="shrink-0 tabular-nums text-brand-text-secondary">×{formatNumber(group.entries.length)}</span>
                {/if}
              </label>
            {/each}
          </section>
        {/each}
      </div>
    {/if}
    {#if preview && preview.skipped > 0}
      <p class="mt-4 text-brand-text-secondary">{i18n.t("tagCleanup.skipped", { count: preview.skipped })}</p>
    {/if}
  </div>

  <div class="h-16 flex items-center justify-end gap-2 px-6 border-t border-brand-border shrink-0 bg-brand-main">
    {#if preview && preview.songs.length > 0}
      <Button onclick={onClose} disabled={isApplying} variant="secondary" size="sm">
        {i18n.t("tagCleanup.cancelBtn")}
      </Button>
      <Button onclick={handleApply} disabled={isApplying || selectedCount === 0} variant="primary" size="sm">
        {#if isApplying}
          <LoaderCircle class="w-3.5 h-3.5 animate-spin" />
          {i18n.t("tagCleanup.removing")}
        {:else}
          {i18n.t("tagCleanup.removeBtn")}
        {/if}
      </Button>
    {:else}
      <Button onclick={onClose} variant="secondary" size="sm">{i18n.t("tagCleanup.closeBtn")}</Button>
    {/if}
  </div>
</Modal>
