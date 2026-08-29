<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    tags: string[];
    variant?: "compact" | "full";
    class?: string;
  }

  let { tags, variant = "compact", class: className = "" }: Props = $props();

  const chipClass =
    "inline-flex items-center pl-2 pr-2 py-0.5 rounded-full bg-brand-accent/15 text-brand-text-primary border border-brand-accent/25 text-xs font-medium hover:bg-brand-accent/25 hover:border-brand-accent/50 transition-colors";

  function goToTag(e: MouseEvent, tag: string) {
    e.stopPropagation();
    collectionStore.searchQuery = `artist-tag:${tag}`;
    navigationStore.selectedArtistName = null;
    navigationStore.selectedAlbumName = null;
    navigationStore.activeTab = "collection";
    navigationStore.activeSubTab = "songs";
  }
</script>

{#if tags && tags.length > 0}
  {#if variant === "compact"}
    <button
      type="button"
      onclick={(e) => goToTag(e, tags[0])}
      title={i18n.t('artistProfileEditor.goToTagTooltip', { tag: tags[0] }, `Browse ${tags[0]}`)}
      class="{chipClass} gap-1 min-w-0 max-w-full {className}"
    >
      <span class="truncate min-w-0">{tags[0]}</span>
      {#if tags.length > 1}
        <span class="text-brand-text-secondary shrink-0">+{tags.length - 1}</span>
      {/if}
    </button>
  {:else}
    <div class="flex flex-wrap gap-1 {className}">
      {#each tags as tag (tag)}
        <button
          type="button"
          onclick={(e) => goToTag(e, tag)}
          title={i18n.t('artistProfileEditor.goToTagTooltip', { tag }, `Browse ${tag}`)}
          class="{chipClass} max-w-64"
        >
          <span class="truncate">{tag}</span>
        </button>
      {/each}
    </div>
  {/if}
{/if}
