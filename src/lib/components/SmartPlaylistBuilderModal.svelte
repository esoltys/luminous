<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { X, Plus, Sparkles, SlidersHorizontal } from "lucide-svelte";
  import type { Rule } from "../utils/filterParser";

  interface Props {
    initialRules?: Rule[];
    editing?: { id: number; name: string; autoPlay: boolean } | null;
    onClose: () => void;
  }

  let { initialRules = [], editing = null, onClose }: Props = $props();

  function generateSuggestedName(ruleList: Array<{ field: string; op: string; value: string }>): string {
    const activeRules = ruleList.filter((r) => r.value.trim() !== "");
    if (activeRules.length === 0) return "Smart Playlist";

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
        parts.push(`Album: ${val}`);
      } else if (r.field === "year") {
        if (r.op === ">=" || r.op === ">") parts.push(`${val}+`);
        else if (r.op === "<=" || r.op === "<") parts.push(`Pre-${val}`);
        else parts.push(val);
      } else if (r.field === "rating") {
        parts.push(r.op === ">=" || r.op === ">" ? `${val}★+` : `${val}★`);
      } else {
        parts.push(val);
      }
    });

    if (parts.length === 0) return "Smart Playlist";

    if (parts.length === 1) {
      const single = parts[0];
      if (decadeToken && single === decadeToken) return `${decadeToken} Mix`;
      const firstNonYear = activeRules.find((r) => r.field !== "year");
      const firstField = firstNonYear?.field ?? activeRules[0].field;
      if (firstField === "genre") return `${single} Mix`;
      if (firstField === "artist") return `${single} Selection`;
      if (firstField === "rating") return `${single} Songs`;
      return `${single} Playlist`;
    }

    if (decadeToken) {
      const rest = parts.filter((p) => p !== decadeToken);
      return `${decadeToken} ${rest.join(" · ")} Mix`;
    }

    return `${parts.join(" · ")} Mix`;
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
  // suggestion as the user tweaks rules.
  let userHasEditedName = $state(editing !== null);
  let playlistName = $state(editing?.name ?? generateSuggestedName(rules));
  let autoPlay = $state(editing?.autoPlay ?? true);

  $effect(() => {
    if (!userHasEditedName) {
      playlistName = generateSuggestedName(rules);
    }
  });

  const availableFields = [
    { key: "artist", label: "Artist", type: "text" },
    { key: "album", label: "Album", type: "text" },
    { key: "title", label: "Title", type: "text" },
    { key: "genre", label: "Genre", type: "text" },
    { key: "composer", label: "Composer", type: "text" },
    { key: "year", label: "Year", type: "number" },
    { key: "rating", label: "Rating (Stars)", type: "number" },
    { key: "playcount", label: "Play Count", type: "number" },
    { key: "skipcount", label: "Skip Count", type: "number" },
    { key: "bitrate", label: "Bitrate", type: "number" },
    { key: "duration", label: "Duration (MM:SS or Sec)", type: "text" },
  ];

  function getOperatorsForField(fieldKey: string) {
    const fieldObj = availableFields.find((f) => f.key === fieldKey);
    if (fieldObj && fieldObj.type === "number") {
      return [
        { op: "=", label: "=" },
        { op: "!=", label: "!=" },
        { op: ">=", label: ">=" },
        { op: "<=", label: "<=" },
        { op: ">", label: ">" },
        { op: "<", label: "<" },
      ];
    }
    return [
      { op: "contains", label: "contains" },
      { op: "=", label: "equals" },
      { op: "!=", label: "does not equal" },
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
        await playlistsStore.updatePlaylistSpec(editing.id, specString, autoPlay);
        collectionStore.closeSmartBuilder();
        collectionStore.viewPlaylist(editing.id);
        return;
      }

      const playlist = await playlistsStore.createPlaylist(name);
      if (playlist && specString) {
        await playlistsStore.updatePlaylistSpec(playlist.id, specString, autoPlay);
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
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
  <div
    class="bg-brand-sidebar border border-brand-border rounded-2xl w-full max-w-xl shadow-2xl overflow-hidden flex flex-col max-h-[90vh] animate-in fade-in zoom-in-95 duration-150"
  >
    <!-- Header -->
    <div class="flex items-center justify-between px-6 py-4 border-b border-brand-border/60 bg-brand-main/50">
      <div class="flex items-center gap-2.5">
        <div class="p-2 rounded-xl bg-brand-accent/20 text-brand-accent-text">
          <Sparkles class="w-5 h-5" />
        </div>
        <div>
          <h2 class="text-base font-bold text-brand-text-primary">{editing ? "Edit Smart Playlist" : "Create Smart Playlist"}</h2>
          <p class="text-xs text-brand-text-secondary/70">Build a dynamic playlist based on custom metadata rules</p>
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
          Playlist Name
        </label>
        <input
          id="smart-playlist-name-input"
          type="text"
          bind:value={playlistName}
          oninput={() => { userHasEditedName = true; }}
          placeholder="My Smart Playlist"
          class="w-full bg-brand-main border border-brand-border rounded-xl px-3.5 py-2 text-sm text-brand-text-primary focus:outline-none focus:border-brand-accent font-medium"
          required
        />
      </div>

      <!-- Rules Section -->
      <div>
        <div class="flex items-center justify-between mb-2">
          <span class="text-xs font-semibold text-brand-text-secondary uppercase tracking-wider flex items-center gap-1.5">
            <SlidersHorizontal class="w-3.5 h-3.5 text-brand-accent-text" />
            Matching Rules
          </span>
          <button
            type="button"
            onclick={addRule}
            class="text-xs text-brand-accent-text hover:text-brand-accent-text-hover font-semibold flex items-center gap-1 transition-colors cursor-pointer"
          >
            <Plus class="w-3.5 h-3.5" />
            Add Rule
          </button>
        </div>

        <div class="space-y-2.5">
          {#each rules as rule (rule.id)}
            <div class="flex items-center gap-2 bg-brand-main/60 p-2.5 rounded-xl border border-brand-border/40">
              <!-- Field Selector -->
              <select
                bind:value={rule.field}
                onchange={() => {
                  const ops = getOperatorsForField(rule.field);
                  rule.op = ops[0].op;
                }}
                class="bg-brand-sidebar border border-brand-border text-brand-text-primary text-xs rounded-lg px-2.5 py-1.5 focus:outline-none focus:border-brand-accent font-medium"
              >
                {#each availableFields as f}
                  <option value={f.key}>{f.label}</option>
                {/each}
              </select>

              <!-- Operator Selector -->
              <select
                bind:value={rule.op}
                class="bg-brand-sidebar border border-brand-border text-brand-text-primary text-xs rounded-lg px-2.5 py-1.5 focus:outline-none focus:border-brand-accent font-medium"
              >
                {#each getOperatorsForField(rule.field) as opItem}
                  <option value={opItem.op}>{opItem.label}</option>
                {/each}
              </select>

              <!-- Value Input -->
              <input
                type="text"
                bind:value={rule.value}
                placeholder="Value..."
                class="flex-1 bg-brand-sidebar border border-brand-border text-brand-text-primary text-xs rounded-lg px-2.5 py-1.5 focus:outline-none focus:border-brand-accent min-w-0"
              />

              <!-- Delete Rule -->
              {#if rules.length > 1}
                <button
                  type="button"
                  onclick={() => removeRule(rule.id)}
                  class="text-brand-text-secondary/60 hover:text-rose-400 p-1 rounded-lg transition-colors cursor-pointer"
                  title="Remove rule"
                >
                  <X class="w-4 h-4" />
                </button>
              {/if}
            </div>
          {/each}
        </div>
      </div>

      <!-- Auto-Refill Toggle -->
      <div class="flex items-center gap-3 pt-2">
        <label class="relative inline-flex items-center cursor-pointer">
          <input type="checkbox" bind:checked={autoPlay} class="sr-only peer" />
          <div class="w-9 h-5 bg-brand-main peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-brand-accent"></div>
        </label>
        <div>
          <span class="text-xs font-semibold text-brand-text-primary block">Auto-Refill Batch Playback</span>
          <span class="text-[11px] text-brand-text-secondary/70">Automatically queue matching tracks as playback nears the end</span>
        </div>
      </div>

      <!-- Footer Buttons -->
      <div class="flex items-center justify-end gap-3 pt-4 border-t border-brand-border/60">
        <button
          type="button"
          onclick={onClose}
          class="px-4 py-2 text-xs font-semibold text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
        >
          Cancel
        </button>
        <button
          type="submit"
          class="px-4 py-2 bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast text-xs font-semibold rounded-xl shadow-lg transition-all cursor-pointer flex items-center gap-1.5"
        >
          <Sparkles class="w-3.5 h-3.5" />
          {editing ? "Update Smart Playlist" : "Create Smart Playlist"}
        </button>
      </div>
    </form>
  </div>
</div>
