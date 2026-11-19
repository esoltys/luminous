<script lang="ts">
  import { onMount } from "svelte";
  import { BookmarkSimpleIcon as Bookmark } from "phosphor-svelte";
  import { radioStore } from "../../stores/radio.svelte";
  import { i18n } from "../../stores/i18n.svelte";

  let {
    stationuuid,
    size = "sm",
    showLabel = false,
  }: {
    stationuuid: string;
    size?: "sm" | "md";
    showLabel?: boolean;
  } = $props();

  const saved = $derived(radioStore.isSaved(stationuuid));
  const label = $derived(saved ? i18n.t("radio.unsaveStation") : i18n.t("radio.saveStation"));

  onMount(() => {
    void radioStore.ensureSavedLoaded();
  });
</script>

<button
  type="button"
  data-testid="radio-playbar-save-btn"
  onclick={(e) => {
    e.stopPropagation();
    void radioStore.toggleSaveByUuid(stationuuid);
  }}
  aria-pressed={saved}
  aria-label={label}
  title={label}
  class="inline-flex items-center justify-center gap-1.5 rounded-lg text-xs font-medium transition-colors duration-150 {showLabel
    ? 'px-2.5 py-1.5'
    : size === 'md'
      ? 'p-1.5'
      : 'p-1'} {saved
    ? showLabel
      ? 'bg-brand-accent/20 text-brand-accent-text hover:bg-brand-accent/30'
      : 'text-brand-accent-text hover:opacity-80'
    : showLabel
      ? 'text-brand-text-secondary hover:bg-brand-surface hover:text-brand-text-primary'
      : 'text-brand-text-secondary hover:text-brand-text-primary'}"
>
  <Bookmark class={size === "md" ? "w-5 h-5" : "w-4 h-4"} weight={saved ? "fill" : "regular"} />
  {#if showLabel}
    <span class="hidden @md:inline">
      {saved ? i18n.t("radio.saved") : i18n.t("radio.save")}
    </span>
  {/if}
</button>
