<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { i18n } from "../stores/i18n.svelte";
  import { addonsStore } from "../stores/addons.svelte";
  import { ADDON_CATALOG } from "../addons/catalog";
  import AddonThemeCard from "./AddonThemeCard.svelte";
  import { ArrowsClockwiseIcon as Refresh } from "phosphor-svelte";

  // Hidden entirely when the Store isn't available (no package identity,
  // other platforms) rather than showing cards nobody can act on.
  let visible = $derived(ADDON_CATALOG.filter((e) => addonsStore.stateOf(e.id) !== "unavailable"));
</script>

{#if visible.length > 0}
  <div>
    <div class="flex items-center justify-between mb-3">
      <h4 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase">{i18n.t("settings.addonThemes")}</h4>
      <button
        onclick={() => void invoke("refresh_addons")}
        class="h-7 px-3 inline-flex items-center gap-1.5 rounded-lg border border-brand-border text-xs font-semibold text-brand-text-primary hover:bg-brand-border/40 transition-colors"
      >
        <Refresh class="w-3.5 h-3.5 text-brand-accent-text" />
        {i18n.t("settings.addonCheckPurchases")}
      </button>
    </div>
    <div class="grid grid-cols-1 @xl:grid-cols-2 gap-4">
      {#each visible as entry (entry.id)}
        <AddonThemeCard {entry} />
      {/each}
    </div>
  </div>
{/if}
