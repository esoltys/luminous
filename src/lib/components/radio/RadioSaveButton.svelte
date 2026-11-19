<script lang="ts">
  import { onMount } from "svelte";
  import { BookmarkSimpleIcon as Bookmark } from "phosphor-svelte";
  import { radioStore } from "../../stores/radio.svelte";
  import { i18n } from "../../stores/i18n.svelte";

  let {
    stationuuid,
    size = "sm",
  }: {
    stationuuid: string;
    size?: "sm" | "md";
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
  class="inline-flex items-center justify-center rounded-md transition-colors duration-150 {size ===
  'md'
    ? 'p-1.5'
    : 'p-1'} {saved
    ? 'text-brand-accent-text hover:opacity-80'
    : 'text-brand-text-secondary hover:text-brand-text-primary'}"
>
  <Bookmark class={size === "md" ? "w-5 h-5" : "w-4 h-4"} weight={saved ? "fill" : "regular"} />
</button>
