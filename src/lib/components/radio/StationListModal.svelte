<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    XIcon as X,
    SparkleIcon as Sparkles,
    CheckIcon as Check,
    BroadcastIcon as Radio,
  } from "phosphor-svelte";
  import { radioStore } from "../../stores/radio.svelte";
  import { toastStore } from "../../stores/toast.svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import type { Playlist, RadioDirectoryOrder, RadioStationListSpec } from "../../types";
  import Button from "../Button.svelte";
  import Input from "../Input.svelte";
  import Modal from "../Modal.svelte";
  import Select from "../Select.svelte";

  interface Props {
    editing?: { id: number; name: string; spec: RadioStationListSpec } | null;
    onClose: () => void;
    onSaved?: (playlist: Playlist) => void;
  }

  let { editing = null, onClose, onSaved }: Props = $props();

  const initialEditing = untrack(() => editing);
  let name = $state(initialEditing?.name ?? "");
  let nameTouched = $state(Boolean(initialEditing?.name));
  let selectedTags = $state<string[]>(initialEditing?.spec.tags ?? []);
  let country = $state(initialEditing?.spec.country ?? "");
  let lang = $state(initialEditing?.spec.lang ?? "");
  let minBitrate = $state(
    initialEditing?.spec.minBitrate ? String(initialEditing.spec.minBitrate) : "",
  );
  let order = $state<RadioDirectoryOrder>(initialEditing?.spec.order ?? "clickcount");
  let limit = $state(String(initialEditing?.spec.limit ?? 100));
  let tagSearch = $state("");
  let saving = $state(false);
  let errorMsg = $state<string | null>(null);

  onMount(() => {
    void radioStore.loadStationListTags();
    if (radioStore.facets.countries.length === 0 || radioStore.facets.languages.length === 0) {
      void radioStore.loadFacets();
    }
  });

  $effect(() => {
    if (!nameTouched && !initialEditing) {
      if (selectedTags.length > 0) {
        const base = selectedTags.slice(0, 2).join(", ");
        name = country ? `${base} (${country})` : base;
      } else {
        name = "";
      }
    }
  });

  const filteredTags = $derived.by(() => {
    const q = tagSearch.trim().toLowerCase();
    const list = radioStore.stationListTags;
    if (!q) return list.slice(0, 60);
    return list.filter((t) => t.name.toLowerCase().includes(q)).slice(0, 60);
  });

  function hasTag(tagName: string): boolean {
    const lower = tagName.toLowerCase();
    return selectedTags.some((t) => t.toLowerCase() === lower);
  }

  function toggleTag(tagName: string) {
    const lower = tagName.toLowerCase();
    if (hasTag(tagName)) {
      selectedTags = selectedTags.filter((t) => t.toLowerCase() !== lower);
    } else {
      selectedTags = [...selectedTags, tagName];
    }
  }

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    const cleanName = name.trim();
    if (!cleanName) {
      errorMsg = i18n.t("radio.stationListNameRequired");
      return;
    }
    saving = true;
    errorMsg = null;
    const spec: RadioStationListSpec = {
      tags: selectedTags,
      country: country.trim() ? country.trim().toUpperCase() : null,
      lang: lang.trim() ? lang.trim().toLowerCase() : null,
      minBitrate: minBitrate ? Number.parseInt(minBitrate, 10) : null,
      order,
      limit: Number.parseInt(limit, 10) || 100,
    };
    try {
      const playlist = editing
        ? await radioStore.updateStationList(editing.id, cleanName, spec)
        : await radioStore.createStationList(cleanName, spec);
      toastStore.show(
        editing ? i18n.t("radio.stationListUpdated") : i18n.t("radio.stationListCreated"),
        "success",
      );
      onSaved?.(playlist);
      onClose();
    } catch (err) {
      errorMsg = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  const selectClass =
    "w-full px-2.5 pr-8 py-2 rounded-lg bg-brand-main border border-brand-border hover:border-brand-accent/60 text-xs text-brand-text-primary focus:outline-none focus:border-brand-accent";
</script>

<Modal {onClose} maxWidth="max-w-xl" ariaLabelledby="station-list-modal-title">
  <form
    onsubmit={handleSubmit}
    data-testid="station-list-modal"
    class="w-full max-h-[85vh] flex flex-col overflow-hidden"
  >
    <div
      class="flex items-center justify-between px-5 py-4 border-b border-brand-border/60 bg-brand-main/50"
    >
      <div class="flex items-center gap-2.5">
        <Radio class="w-5 h-5 text-brand-accent-text" />
        <h2 id="station-list-modal-title" class="text-base font-semibold text-brand-text-primary">
          {editing ? i18n.t("radio.editStationList") : i18n.t("radio.newStationList")}
        </h2>
      </div>
      <button
        type="button"
        onclick={onClose}
        aria-label={i18n.t("common.close")}
        class="p-1.5 rounded-lg text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-surface-hover"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <div class="flex-1 overflow-y-auto p-5 space-y-5">
      <!-- Name -->
      <div class="space-y-1.5">
        <label for="station-list-name" class="block text-xs font-medium text-brand-text-secondary">
          {i18n.t("radio.stationListName")}
        </label>
        <Input
          id="station-list-name"
          class="w-full"
          value={name}
          oninput={(e) => {
            name = (e.currentTarget as HTMLInputElement).value;
            nameTouched = true;
          }}
          placeholder={i18n.t("radio.stationListNamePlaceholder")}
        />
      </div>

      <!-- Library Suggestions -->
      {#if radioStore.suggestedLibraryGenres.length > 0}
        <div class="space-y-2">
          <div class="flex items-center gap-1.5 text-xs font-medium text-brand-text-secondary">
            <Sparkles class="w-3.5 h-3.5 text-brand-accent-text" />
            <span>{i18n.t("radio.suggestFromLibrary")}</span>
          </div>
          <div class="flex flex-wrap gap-1.5" data-testid="station-list-suggestions">
            {#each radioStore.suggestedLibraryGenres as genre (genre.name)}
              {@const active = hasTag(genre.name)}
              <button
                type="button"
                onclick={() => toggleTag(genre.name)}
                class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium border transition-colors {active
                  ? 'bg-brand-accent text-white border-brand-accent'
                  : 'bg-brand-bg/60 text-brand-text-primary border-brand-border/60 hover:bg-brand-surface-hover'}"
              >
                <span>{genre.name}</span>
                <span class="opacity-70 text-[10px]">{genre.stationcount}</span>
              </button>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Genre / Tag Picker -->
      <div class="space-y-2">
        <div class="flex items-center justify-between">
          <label
            for="station-list-tag-search"
            class="text-xs font-medium text-brand-text-secondary"
          >
            {i18n.t("radio.genresLabel")}
          </label>
          {#if selectedTags.length > 0}
            <span class="text-[11px] text-brand-accent-text font-medium">
              {selectedTags.length}
              {i18n.t("radio.selectedCount")}
            </span>
          {/if}
        </div>

        {#if selectedTags.length > 0}
          <div class="flex flex-wrap gap-1.5" data-testid="station-list-selected-tags">
            {#each selectedTags as tag (tag)}
              <button
                type="button"
                onclick={() => toggleTag(tag)}
                class="inline-flex items-center gap-1 px-2.5 py-1 rounded-full text-xs font-medium bg-brand-accent/20 text-brand-accent-text border border-brand-accent/40 hover:bg-brand-accent/30"
              >
                <span>{tag}</span>
                <X class="w-3 h-3" />
              </button>
            {/each}
          </div>
        {/if}

        <Input
          id="station-list-tag-search"
          class="w-full"
          bind:value={tagSearch}
          placeholder={i18n.t("radio.searchGenresPlaceholder")}
        />

        <div
          class="max-h-40 overflow-y-auto rounded-xl border border-brand-border/50 bg-brand-bg/40 p-2 flex flex-wrap gap-1.5"
          data-testid="station-list-tag-picker"
        >
          {#if radioStore.stationListTagsLoading}
            <p class="text-xs text-brand-text-secondary p-2">{i18n.t("common.loading")}</p>
          {:else if filteredTags.length === 0}
            <p class="text-xs text-brand-text-secondary p-2">{i18n.t("radio.noMatchingGenres")}</p>
          {:else}
            {#each filteredTags as tag (tag.name)}
              {@const active = hasTag(tag.name)}
              <button
                type="button"
                onclick={() => toggleTag(tag.name)}
                class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs border transition-colors {active
                  ? 'bg-brand-accent text-white border-brand-accent'
                  : 'bg-brand-surface/70 text-brand-text-primary border-brand-border/40 hover:bg-brand-surface-hover'}"
              >
                {#if active}<Check class="w-3 h-3" />{/if}
                <span>{tag.name}</span>
                <span class="opacity-65 text-[10px]">{tag.stationcount}</span>
              </button>
            {/each}
          {/if}
        </div>
      </div>

      <!-- Optional Filters -->
      <div class="grid grid-cols-2 gap-3">
        <div class="space-y-1">
          <label
            for="station-list-country"
            class="block text-xs font-medium text-brand-text-secondary"
          >
            {i18n.t("radio.countryFilter")}
          </label>
          <Select
            id="station-list-country"
            class={selectClass}
            value={country}
            onchange={(e) => (country = e.currentTarget.value)}
          >
            <option value="">{i18n.t("radio.anyCountry")}</option>
            {#each radioStore.facets.countries as c (c.code ?? c.name)}
              <option value={c.code ?? c.name}>{c.name} ({c.stationcount})</option>
            {/each}
          </Select>
        </div>

        <div class="space-y-1">
          <label
            for="station-list-lang"
            class="block text-xs font-medium text-brand-text-secondary"
          >
            {i18n.t("radio.languageFilter")}
          </label>
          <Select
            id="station-list-lang"
            class={selectClass}
            value={lang}
            onchange={(e) => (lang = e.currentTarget.value)}
          >
            <option value="">{i18n.t("radio.anyLanguage")}</option>
            {#each radioStore.facets.languages as l (l.name)}
              <option value={l.name}>{l.name} ({l.stationcount})</option>
            {/each}
          </Select>
        </div>

        <div class="space-y-1">
          <label
            for="station-list-bitrate"
            class="block text-xs font-medium text-brand-text-secondary"
          >
            {i18n.t("radio.minBitrateFilter")}
          </label>
          <Select
            id="station-list-bitrate"
            class={selectClass}
            value={minBitrate}
            onchange={(e) => (minBitrate = e.currentTarget.value)}
          >
            <option value="">{i18n.t("radio.anyBitrate")}</option>
            <option value="64">64+ kbps</option>
            <option value="96">96+ kbps</option>
            <option value="128">128+ kbps</option>
            <option value="192">192+ kbps</option>
            <option value="256">256+ kbps</option>
          </Select>
        </div>

        <div class="space-y-1">
          <label
            for="station-list-order"
            class="block text-xs font-medium text-brand-text-secondary"
          >
            {i18n.t("radio.sortOrder")}
          </label>
          <Select
            id="station-list-order"
            class={selectClass}
            value={order}
            onchange={(e) => (order = e.currentTarget.value as RadioDirectoryOrder)}
          >
            <option value="clickcount">{i18n.t("radio.sortPopularity")}</option>
            <option value="votes">{i18n.t("radio.sortVotes")}</option>
            <option value="bitrate">{i18n.t("radio.sortBitrate")}</option>
            <option value="name">{i18n.t("radio.sortName")}</option>
          </Select>
        </div>
      </div>

      <div class="space-y-1">
        <label for="station-list-limit" class="block text-xs font-medium text-brand-text-secondary">
          {i18n.t("radio.stationLimit")}
        </label>
        <Select
          id="station-list-limit"
          class={selectClass}
          value={limit}
          onchange={(e) => (limit = e.currentTarget.value)}
        >
          <option value="25">25</option>
          <option value="50">50</option>
          <option value="100">100</option>
          <option value="200">200</option>
          <option value="500">500</option>
        </Select>
      </div>

      {#if errorMsg}
        <p class="text-xs text-brand-text-primary" role="alert">{errorMsg}</p>
      {/if}
    </div>

    <div
      class="flex items-center justify-end gap-2 px-5 py-3.5 border-t border-brand-border/60 bg-brand-main/30"
    >
      <Button variant="secondary" type="button" onclick={onClose} disabled={saving}>
        {i18n.t("common.cancel")}
      </Button>
      <Button variant="primary" type="submit" disabled={saving}>
        {saving ? i18n.t("playlists.saving") : i18n.t("common.save")}
      </Button>
    </div>
  </form>
</Modal>
