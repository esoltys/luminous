<script lang="ts">
  import { untrack } from "svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { X, Plus, Sparkles, SlidersHorizontal } from "lucide-svelte";
  import type { Rule } from "../utils/filterParser";
  import type { QueuePopulationMode } from "../types";
  import PopulationModeTabs from "./PopulationModeTabs.svelte";
  import Toggle from "./Toggle.svelte";
  import Select from "./Select.svelte";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import { PLAYER_DOCK_CLEARANCE_PX } from "../constants";

  interface Props {
    initialRules?: Rule[];
    editing?: { id: number; name: string; autoPlay: boolean; populationMode?: QueuePopulationMode } | null;
    onClose: () => void;
  }

  let { initialRules = [], editing = null, onClose }: Props = $props();

  function generateSuggestedName(ruleList: Array<{ field: string; op: string; value: string }>): string {
    const activeRules = ruleList.filter((r) => r.value.trim() !== "");
    const fallbackName = i18n.t("smartPlaylistBuilder.fallbackName", {}, "Smart Playlist");
    if (activeRules.length === 0) return fallbackName;

    // Detect a decade range: year >= X and year <= Y where Y - X === 9
    const yearGteIdx = activeRules.findIndex((r) => r.field === "year" && (r.op === ">=" || r.op === ">"));
    const yearLteIdx = activeRules.findIndex((r) => r.field === "year" && (r.op === "<=" || r.op === "<"));
    let decadeToken: string | null = null;
    const consumedIndices = new Set<number>();

    if (yearGteIdx !== -1 && yearLteIdx !== -1) {
      const yearGte = activeRules[yearGteIdx];
      const yearLte = activeRules[yearLteIdx];
      const lo = parseInt(yearGte.value.trim(), 10);
      const hi = parseInt(yearLte.value.trim(), 10);
      const loAdj = yearGte.op === ">" ? lo + 1 : lo;
      const hiAdj = yearLte.op === "<" ? hi - 1 : hi;
      if (!isNaN(loAdj) && !isNaN(hiAdj) && hiAdj - loAdj === 9 && loAdj % 10 === 0) {
        decadeToken = `${loAdj}s`;
        consumedIndices.add(yearGteIdx);
        consumedIndices.add(yearLteIdx);
      }
    }

    const parts: string[] = [];
    if (decadeToken) parts.push(decadeToken);

    activeRules.forEach((r, idx) => {
      if (consumedIndices.has(idx)) return;
      const val = r.value.trim().replace(/^["']|["']$/g, "");
      if (!val) return;

      if (r.field === "genre") {
        parts.push(val.charAt(0).toUpperCase() + val.slice(1));
      } else if (r.field === "artist") {
        parts.push(val);
      } else if (r.field === "album") {
        parts.push(i18n.t("collection.albumPlaylistName", { name: val }));
      } else if (r.field === "year") {
        if (r.op === ">=" || r.op === ">") parts.push(`${val}+`);
        else if (r.op === "<=" || r.op === "<") parts.push(i18n.t("smartPlaylistBuilder.yearPrefixPre", { value: val }));
        else parts.push(val);
      } else if (r.field === "rating") {
        parts.push(r.op === ">=" || r.op === ">" ? `${val}★+` : `${val}★`);
      } else {
        parts.push(val);
      }
    });

    const mixWord = i18n.t("smartPlaylistBuilder.mixWord", {}, "Mix");
    if (parts.length === 0) return fallbackName;

    if (parts.length === 1) {
      const single = parts[0];
      if (decadeToken && single === decadeToken) return `${decadeToken} ${mixWord}`;
      const firstNonYear = activeRules.find((r) => r.field !== "year");
      const firstField = firstNonYear?.field ?? activeRules[0].field;
      if (firstField === "genre") return `${single} ${mixWord}`;
      if (firstField === "artist") return `${single} ${i18n.t("smartPlaylistBuilder.selectionWord", {}, "Selection")}`;
      if (firstField === "rating") return `${single} ${i18n.t("smartPlaylistBuilder.songsWord", {}, "Songs")}`;
      return `${single} ${i18n.t("smartPlaylistBuilder.playlistWord", {}, "Playlist")}`;
    }

    if (decadeToken) {
      const rest = parts.filter((p) => p !== decadeToken);
      return `${decadeToken} ${rest.join(" · ")} ${mixWord}`;
    }

    return `${parts.join(" · ")} ${mixWord}`;
  }

  interface RuleItem {
    id: string;
    field: string;
    op: string;
    value: string;
  }

  function createInitialRules(): RuleItem[] {
    if (initialRules && initialRules.length > 0) {
      return initialRules.map((r, i) => ({ id: `rule_${i}_${Date.now()}`, ...r }));
    }
    return [{ id: `rule_0_${Date.now()}`, field: "genre", op: "contains", value: "" }];
  }

  let rules = $state<RuleItem[]>(createInitialRules());
  // When editing an existing playlist, its name was already chosen
  // deliberately (possibly by hand) — don't clobber it with a re-generated
  // suggestion as the user tweaks rules. untrack() signals to Svelte that
  // capturing the initial value only is intentional here.
  let userHasEditedName = $state(untrack(() => editing !== null));
  let playlistName = $state(untrack(() => editing?.name ?? generateSuggestedName(rules)));
  let autoPlay = $state(untrack(() => editing?.autoPlay ?? true));
  let populationMode = $state<QueuePopulationMode>(untrack(() => editing?.populationMode ?? "all"));

  $effect(() => {
    if (!userHasEditedName) {
      playlistName = generateSuggestedName(rules);
    }
  });

  const availableFields = [
    { key: "artist", label: i18n.t("smartPlaylistBuilder.fieldArtist", {}, "Artist"), type: "text" },
    { key: "album_artist", label: i18n.t("smartPlaylistBuilder.fieldAlbumArtist", {}, "Album Artist"), type: "text" },
    { key: "album", label: i18n.t("smartPlaylistBuilder.fieldAlbum", {}, "Album"), type: "text" },
    { key: "title", label: i18n.t("smartPlaylistBuilder.fieldTitle", {}, "Title"), type: "text" },
    { key: "genre", label: i18n.t("smartPlaylistBuilder.fieldGenre", {}, "Genre"), type: "text" },
    { key: "composer", label: i18n.t("smartPlaylistBuilder.fieldComposer", {}, "Composer"), type: "text" },
    { key: "key", label: i18n.t("smartPlaylistBuilder.fieldKey", {}, "Key"), type: "text" },
    { key: "bpm", label: i18n.t("smartPlaylistBuilder.fieldBpm", {}, "BPM"), type: "number" },
    { key: "year", label: i18n.t("smartPlaylistBuilder.fieldYear", {}, "Year"), type: "number" },
    { key: "originalyear", label: i18n.t("smartPlaylistBuilder.fieldOriginalYear", {}, "Original Year"), type: "number" },
    { key: "rating", label: i18n.t("smartPlaylistBuilder.fieldRating", {}, "Rating (Stars)"), type: "number" },
    { key: "playcount", label: i18n.t("smartPlaylistBuilder.fieldPlayCount", {}, "Play Count"), type: "number" },
    { key: "skipcount", label: i18n.t("smartPlaylistBuilder.fieldSkipCount", {}, "Skip Count"), type: "number" },
    { key: "lastplayed", label: i18n.t("smartPlaylistBuilder.fieldLastPlayed", {}, "Last Played (Unix Timestamp)"), type: "number" },
    { key: "added", label: i18n.t("smartPlaylistBuilder.fieldAdded", {}, "Date Added (Unix Timestamp)"), type: "number" },
    { key: "track", label: i18n.t("smartPlaylistBuilder.fieldTrack", {}, "Track #"), type: "number" },
    { key: "disc", label: i18n.t("smartPlaylistBuilder.fieldDisc", {}, "Disc #"), type: "number" },
    { key: "bitrate", label: i18n.t("smartPlaylistBuilder.fieldBitrate", {}, "Bitrate"), type: "number" },
    { key: "samplerate", label: i18n.t("smartPlaylistBuilder.fieldSampleRate", {}, "Sample Rate (Hz)"), type: "number" },
    { key: "bitdepth", label: i18n.t("smartPlaylistBuilder.fieldBitDepth", {}, "Bit Depth"), type: "number" },
    { key: "channels", label: i18n.t("smartPlaylistBuilder.fieldChannels", {}, "Channels"), type: "number" },
    { key: "compilation", label: i18n.t("smartPlaylistBuilder.fieldCompilation", {}, "Compilation (1/0)"), type: "number" },
    { key: "duration", label: i18n.t("smartPlaylistBuilder.fieldDuration", {}, "Duration (MM:SS or Sec)"), type: "text" },
  ];

  function getOperatorsForField(fieldKey: string) {
    const fieldObj = availableFields.find((f) => f.key === fieldKey);
    if (fieldObj && fieldObj.type === "number") {
      return [
        { op: "=", label: i18n.t("smartPlaylistBuilder.opEquals", {}, "=") },
        { op: "!=", label: i18n.t("smartPlaylistBuilder.opNotEquals", {}, "!=") },
        { op: ">=", label: i18n.t("smartPlaylistBuilder.opGte", {}, ">=") },
        { op: "<=", label: i18n.t("smartPlaylistBuilder.opLte", {}, "<=") },
        { op: ">", label: i18n.t("smartPlaylistBuilder.opGt", {}, ">") },
        { op: "<", label: i18n.t("smartPlaylistBuilder.opLt", {}, "<") },
      ];
    }
    return [
      { op: "contains", label: i18n.t("smartPlaylistBuilder.opContains", {}, "contains") },
      { op: "=", label: i18n.t("smartPlaylistBuilder.opTextEquals", {}, "equals") },
      { op: "!=", label: i18n.t("smartPlaylistBuilder.opTextNotEquals", {}, "does not equal") },
    ];
  }

  function addRule() {
    rules = [
      ...rules,
      { id: `rule_${Date.now()}_${Math.random()}`, field: "artist", op: "contains", value: "" },
    ];
  }

  function removeRule(id: string) {
    rules = rules.filter((r) => r.id !== id);
  }

  async function handleSubmit(e: Event) {
    e.preventDefault();
    const name = playlistName.trim();
    if (!name) return;

    const validRules = rules.filter((r) => r.value.trim() !== "");
    // Serialise rules into the filter_parser format:
    //   text contains  → "field:value"      (parser defaults to LIKE for text)
    //   text =         → "field:=value"
    //   text !=        → "field:!=value"
    //   numeric op     → "field:>=value" etc.
    const specString = validRules
      .map((r) => {
        const val = r.value.trim();
        const opPrefix = r.op === "contains" ? "" : r.op;
        return `${r.field}:${opPrefix}${val}`;
      })
      .join("; ");

    try {
      if (editing) {
        if (name !== editing.name) {
          await playlistsStore.renamePlaylist(editing.id, name);
        }
        await playlistsStore.updatePlaylistSpec(editing.id, specString, autoPlay, populationMode);
        collectionStore.closeSmartBuilder();
        collectionStore.viewPlaylist(editing.id);
        return;
      }

      const playlist = await playlistsStore.createPlaylist(name);
      if (playlist && specString) {
        await playlistsStore.updatePlaylistSpec(playlist.id, specString, autoPlay, populationMode);
      }
      collectionStore.closeSmartBuilder();
      if (playlist) {
        collectionStore.viewPlaylist(playlist.id);
      }
    } catch (err) {
      console.error("Failed to save smart playlist:", err);
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      onClose();
    }
  }

  // Reserve room for the floating PlayerBar dock so the modal shrinks and its
  // form scrolls internally instead of sliding underneath it.
  let dockClearance = $derived(playerStore.currentSong ? PLAYER_DOCK_CLEARANCE_PX : 0);
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4"
  style="padding-bottom: {dockClearance + 16}px"
>
  <div
    class="bg-brand-sidebar border border-brand-border rounded-2xl w-full max-w-xl shadow-2xl overflow-hidden flex flex-col animate-in fade-in zoom-in-95 duration-150"
    style="max-height: min(90vh, calc(100vh - {dockClearance + 32}px))"
  >
    <!-- Header -->
    <div class="flex items-center justify-between px-6 py-4 border-b border-brand-border/60 bg-brand-main/50">
      <div class="flex items-center gap-2.5">
        <div class="p-2 rounded-xl bg-brand-accent/20 text-brand-accent-text">
          <Sparkles class="w-5 h-5" />
        </div>
        <div>
          <h2 class="text-base font-bold text-brand-text-primary">{editing ? i18n.t("smartPlaylistBuilder.editTitle") : i18n.t("smartPlaylistBuilder.createTitle")}</h2>
          <p class="text-xs text-brand-text-secondary/70">{i18n.t("smartPlaylistBuilder.subtitle")}</p>
        </div>
      </div>
      <button
        onclick={onClose}
        class="text-brand-text-secondary hover:text-brand-text-primary p-1.5 rounded-lg hover:bg-brand-main/80 transition-colors cursor-pointer"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <!-- Form Content -->
    <form onsubmit={handleSubmit} class="p-6 flex-1 overflow-y-auto flex flex-col gap-5">
      <!-- Playlist Name -->
      <div>
        <label for="smart-playlist-name-input" class="block text-xs font-semibold text-brand-text-secondary uppercase tracking-wider mb-1.5">
          {i18n.t("smartPlaylistBuilder.nameLabel")}
        </label>
        <Input
          id="smart-playlist-name-input"
          type="text"
          bind:value={playlistName}
          oninput={() => { userHasEditedName = true; }}
          placeholder={i18n.t("smartPlaylistBuilder.namePlaceholder")}
          class="w-full"
          required
        />
      </div>

      <!-- Rules Section -->
      <div>
        <div class="flex items-center justify-between mb-2">
          <span class="text-xs font-semibold text-brand-text-secondary uppercase tracking-wider flex items-center gap-1.5">
            <SlidersHorizontal class="w-3.5 h-3.5 text-brand-accent-text" />
            {i18n.t("smartPlaylistBuilder.matchingRules")}
          </span>
          <button
            type="button"
            onclick={addRule}
            class="text-xs text-brand-accent-text hover:text-brand-accent-text-hover font-semibold flex items-center gap-1 transition-colors cursor-pointer"
          >
            <Plus class="w-3.5 h-3.5" />
            {i18n.t("smartPlaylistBuilder.addRule")}
          </button>
        </div>

        <div class="space-y-2.5">
          {#each rules as rule (rule.id)}
            <div class="flex items-center gap-2 bg-brand-main/60 p-2.5 rounded-xl border border-brand-border/40">
              <!-- Field Selector -->
              <Select
                value={rule.field}
                onchange={(e) => {
                  rule.field = e.currentTarget.value;
                  const ops = getOperatorsForField(rule.field);
                  rule.op = ops[0].op;
                }}
                class="bg-brand-sidebar border border-brand-border text-brand-text-primary text-xs rounded-full pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent font-medium"
              >
                {#each availableFields as f}
                  <option value={f.key}>{f.label}</option>
                {/each}
              </Select>

              <!-- Operator Selector -->
              <Select
                value={rule.op}
                onchange={(e) => { rule.op = e.currentTarget.value; }}
                class="bg-brand-sidebar border border-brand-border text-brand-text-primary text-xs rounded-full pl-2.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent font-medium"
              >
                {#each getOperatorsForField(rule.field) as opItem}
                  <option value={opItem.op}>{opItem.label}</option>
                {/each}
              </Select>

              <!-- Value Input -->
              <Input
                type="text"
                bind:value={rule.value}
                placeholder={i18n.t("smartPlaylistBuilder.valuePlaceholder")}
                size="sm"
                surface="sidebar"
                class="flex-1 min-w-0"
              />

              <!-- Delete Rule -->
              {#if rules.length > 1}
                <button
                  type="button"
                  onclick={() => removeRule(rule.id)}
                  class="text-brand-text-secondary/60 hover:text-rose-400 p-1 rounded-lg transition-colors cursor-pointer"
                  title={i18n.t("smartPlaylistBuilder.removeRuleTooltip")}
                >
                  <X class="w-4 h-4" />
                </button>
              {/if}
            </div>
          {/each}
        </div>
      </div>

      <!-- Auto-Refill Toggle -->
      <div class="flex items-center justify-between gap-3 pt-2">
        <div>
          <span class="text-xs font-semibold text-brand-text-primary block">{i18n.t("smartPlaylistBuilder.autoRefillLabel")}</span>
          <span class="text-[11px] text-brand-text-secondary/70">{i18n.t("smartPlaylistBuilder.autoRefillHint")}</span>
        </div>
        <Toggle
          checked={autoPlay}
          onchange={(v) => { autoPlay = v; }}
          label={i18n.t("smartPlaylistBuilder.autoRefillLabel")}
        />
      </div>

      <!-- Queue population mode tabs (#120): applied when this playlist (re)populates -->
      <div class="flex items-center gap-3 pt-1">
        <span class="text-xs font-semibold text-brand-text-primary shrink-0">{i18n.t("playlists.populationModeLabel")}</span>
        <PopulationModeTabs mode={populationMode} onChange={(m) => (populationMode = m)} />
      </div>

      <!-- Footer Buttons -->
      <div class="flex items-center justify-end gap-3 pt-4 border-t border-brand-border/60">
        <Button type="button" onclick={onClose} variant="secondary" size="sm">
          {i18n.t("smartPlaylistBuilder.cancel")}
        </Button>
        <Button type="submit" variant="primary" size="sm">
          <Sparkles class="w-3.5 h-3.5" />
          {editing ? i18n.t("smartPlaylistBuilder.updateBtn") : i18n.t("smartPlaylistBuilder.createBtn")}
        </Button>
      </div>
    </form>
  </div>
</div>
