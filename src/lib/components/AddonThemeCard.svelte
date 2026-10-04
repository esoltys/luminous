<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { i18n } from "../stores/i18n.svelte";
  import { addonsStore } from "../stores/addons.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { SWATCH_KEYS, type AddonCatalogEntry } from "../addons/catalog";
  import { SpinnerGapIcon as Spinner } from "phosphor-svelte";

  let { entry }: { entry: AddonCatalogEntry } = $props();

  const ERROR_CODES = [
    "offline",
    "not_entitled",
    "store",
    "upstream",
    "rate_limited",
    "bundle",
    "unsupported_api",
    "internal"
  ];

  let state = $derived(addonsStore.stateOf(entry.id));
  let errorCode = $derived(addonsStore.errorOf(entry.id));
  let isActive = $derived(themeStore.activeThemeId === entry.id);
  let busy = $derived(state === "purchasing" || state === "downloading");
  // Once owned the whole card selects the theme, like every other theme card.
  let selectable = $derived(state === "owned");

  // The registered theme is the source of truth once the bundle is verified.
  let swatches = $derived.by(() => {
    const colors = addonsStore.themes[entry.id]?.colors;
    return colors ? SWATCH_KEYS.map((k) => colors[k] ?? "transparent") : entry.swatches;
  });
  let accent = $derived(swatches[3]);

  let errorMessage = $derived(
    state === "error"
      ? i18n.t(`settings.addonError_${ERROR_CODES.includes(errorCode ?? "") ? errorCode : "internal"}`)
      : null
  );

  let cardClass = $derived(
    `bg-brand-main/50 border-2 rounded-xl flex flex-col overflow-hidden text-left w-full transition-colors duration-200 ${
      isActive ? "border-brand-accent shadow-md shadow-brand-accent/5" : "border-brand-border/60"
    } ${selectable ? "hover:border-brand-accent/40" : ""}`
  );

  function get() {
    void invoke("acquire_addon", { id: entry.id });
  }

  // refresh re-checks ownership: an owned add-on re-downloads, an unowned one goes back to Get.
  function retry() {
    void invoke("refresh_addons");
  }

  function apply() {
    void themeStore.setTheme(entry.id);
  }
</script>

{#snippet content()}
  <div class="p-4 flex flex-col gap-3">
    <!-- The moth stands on top of the palette strip; the title sits beside it. -->
    <div>
      <div class="relative h-24 flex flex-col items-start justify-end gap-1 pb-2">
        <div class="flex items-center gap-2">
          <span
            class="font-semibold text-sm text-brand-text-primary"
            style="text-shadow: 0 0 12px {accent}a6, 0 0 3px {accent}b3"
          >
            {entry.name}
          </span>
          <span class="text-[11px] font-semibold px-2 py-0.5 rounded-full bg-brand-accent/15 text-brand-accent-text">
            {i18n.t("settings.addonBadge")}
          </span>
        </div>
        {#if state === "owned"}
          <span class="text-xs text-brand-text-secondary">{i18n.t("settings.addonOwned")}</span>
        {/if}
        <div
          class="absolute right-2 bottom-0 w-32 h-24 pointer-events-none"
          style="background-image: radial-gradient(closest-side at 50% 60%, {accent}4d, {accent}00)"
          aria-hidden="true"
        ></div>
        <img
          src={entry.art}
          alt=""
          width="104"
          height="102"
          class="art absolute right-4 -bottom-0.5 z-10 pointer-events-none"
        />
      </div>

      <div class="flex gap-0.5 w-full h-8 rounded-lg overflow-hidden border border-brand-border/40 bg-black/10">
        {#each swatches as color}
          <div class="flex-1" style="background-color: {color}"></div>
        {/each}
      </div>
    </div>

    <div class="text-xs leading-relaxed min-h-9 {errorMessage ? 'text-brand-gold' : 'text-brand-text-secondary'}" role={errorMessage ? "alert" : undefined}>
      {errorMessage ?? i18n.t(entry.descriptionKey)}
    </div>

    {#if state === "unowned"}
      <button
        onclick={get}
        class="w-full py-2 px-3 rounded-md text-xs font-semibold bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast transition-colors"
      >
        {i18n.t("settings.addonGet")}
      </button>
    {:else if busy}
      <button
        disabled
        aria-busy="true"
        class="w-full py-2 px-3 rounded-md text-xs font-semibold bg-brand-border text-brand-text-primary flex items-center justify-center gap-2"
      >
        <Spinner class="w-3.5 h-3.5 spin" />
        {state === "purchasing" ? i18n.t("settings.addonWaitingStore") : i18n.t("settings.addonDownloading")}
      </button>
    {:else if state === "error"}
      <button
        onclick={retry}
        class="w-full py-2 px-3 rounded-md text-xs font-semibold border border-brand-accent text-brand-accent-text hover:bg-brand-accent/10 transition-colors"
      >
        {i18n.t("settings.addonTryAgain")}
      </button>
    {/if}
  </div>
{/snippet}

{#if selectable}
  <button onclick={apply} aria-pressed={isActive} class={cardClass} data-addon-card={entry.id}>
    {@render content()}
  </button>
{:else}
  <div class={cardClass} data-addon-card={entry.id}>
    {@render content()}
  </div>
{/if}

<style>
  .art {
    width: 104px;
    height: 102px;
    image-rendering: pixelated;
  }
  :global(.spin) {
    animation: moth-spin 1s linear infinite;
  }
  @keyframes moth-spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    :global(.spin) {
      animation: none;
    }
  }
</style>
