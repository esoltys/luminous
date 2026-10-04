<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { i18n } from "../stores/i18n.svelte";
  import { addonsStore } from "../stores/addons.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { SWATCH_KEYS, type AddonCatalogEntry } from "../addons/catalog";
  import { CheckIcon as Check, SpinnerGapIcon as Spinner } from "phosphor-svelte";

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

  // The walk loop only runs while the window is visible.
  let windowVisible = $state(true);

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

<svelte:document onvisibilitychange={() => (windowVisible = !document.hidden)} />

<div
  class="bg-brand-main/50 border-2 rounded-xl flex flex-col overflow-hidden text-left {isActive
    ? 'border-brand-accent shadow-md shadow-brand-accent/5'
    : 'border-brand-border/60'}"
  data-addon-card={entry.id}
>
  <div
    class="hero h-[132px] flex items-center justify-center"
    style="background-color: {swatches[0]}; background-image: radial-gradient(closest-side at 50% 55%, {accent}4d, {accent}00)"
    aria-hidden="true"
  >
    <div
      class="walk"
      style="background-image: url({entry.heroFrames}); animation-play-state: {windowVisible ? 'running' : 'paused'}"
    ></div>
    <img class="still" src={entry.heroImage} alt="" width="104" height="102" />
  </div>

  <div class="p-4 flex flex-col gap-3">
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
      {#if state === "owned"}
        <span class="text-xs text-brand-text-secondary ml-auto">{i18n.t("settings.addonOwned")}</span>
      {/if}
    </div>

    <div class="flex gap-0.5 w-full h-8 rounded-lg overflow-hidden border border-brand-border/40 bg-black/10">
      {#each swatches as color}
        <div class="flex-1" style="background-color: {color}"></div>
      {/each}
    </div>

    <p class="text-xs leading-relaxed min-h-9 {errorMessage ? 'text-brand-gold' : 'text-brand-text-secondary'}" role={errorMessage ? "alert" : undefined}>
      {errorMessage ?? i18n.t(entry.descriptionKey)}
    </p>

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
    {:else if state === "owned" && isActive}
      <button
        disabled
        class="w-full py-2 px-3 rounded-md text-xs font-semibold border border-brand-border text-brand-text-primary flex items-center justify-center gap-1.5"
      >
        <Check class="w-3.5 h-3.5" />
        {i18n.t("settings.addonApplied")}
      </button>
    {:else if state === "owned"}
      <button
        onclick={apply}
        class="w-full py-2 px-3 rounded-md text-xs font-semibold bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast transition-colors"
      >
        {i18n.t("settings.addonApply")}
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
</div>

<style>
  .walk,
  .still {
    width: 104px;
    image-rendering: pixelated;
  }
  .walk {
    height: 100px;
    background-repeat: no-repeat;
    background-size: 312px 100px;
    animation: moth-walk 880ms steps(1, end) infinite;
  }
  .still {
    display: none;
    height: auto;
  }
  :global(.spin) {
    animation: moth-spin 1s linear infinite;
  }
  /* Walk cycle of the live overlay: frames 0, 1, 2, 1 at 220 ms each. */
  @keyframes moth-walk {
    0% {
      background-position: 0 0;
    }
    25% {
      background-position: -104px 0;
    }
    50% {
      background-position: -208px 0;
    }
    75% {
      background-position: -104px 0;
    }
  }
  @keyframes moth-spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .walk {
      display: none;
    }
    .still {
      display: block;
    }
    :global(.spin) {
      animation: none;
    }
  }
</style>
