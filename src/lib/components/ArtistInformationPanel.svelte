<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import {
    formatArtistLifeEvent,
    isArtistPerson,
  } from "../utils/artistInfo";

  interface Props {
    sortName?: string | null;
    gender?: string | null;
    beginDate?: string | null;
    endDate?: string | null;
    ended?: boolean | null;
    artistType?: string | null;
    beginAreaName?: string | null;
    beginAreaMbid?: string | null;
    areaName?: string | null;
    areaMbid?: string | null;
    onOpenUrl?: (url: string) => void;
    class?: string;
  }

  let {
    sortName: _sortName,
    gender,
    beginDate,
    endDate,
    ended: _ended = null,
    artistType,
    beginAreaName,
    beginAreaMbid,
    areaName,
    areaMbid,
    onOpenUrl: _onOpenUrl,
    class: className = ""
  }: Props = $props();

  let isPerson = $derived(isArtistPerson(artistType, gender));
  let formattedBeginDate = $derived(formatArtistLifeEvent(beginDate, i18n.currentLocale));
  let formattedEndDate = $derived(formatArtistLifeEvent(endDate, i18n.currentLocale));
  let city = $derived(beginAreaName?.trim() || "");
  let country = $derived(areaName?.trim() || "");
  let isSameCityCountry = $derived(
    !!city && !!country && city.toLowerCase() === country.toLowerCase()
  );

  let dateLabel = $derived(
    isPerson
      ? i18n.t("artistInfo.born", {}, "Born")
      : i18n.t("artistInfo.formed", {}, "Formed")
  );
  let cityLabel = $derived(i18n.t("artistInfo.cityRegion", {}, "City/Region"));
  let countryLabel = $derived(i18n.t("artistInfo.country", {}, "Country"));
  let endLabel = $derived(
    isPerson
      ? i18n.t("artistInfo.died", {}, "Died")
      : i18n.t("artistInfo.disbanded", {}, "Disbanded")
  );

  let hasContent = $derived(
    !!formattedBeginDate ||
      !!city ||
      !!country ||
      !!formattedEndDate
  );
</script>

{#snippet contentRows()}
  {#if formattedBeginDate}
    <div class="flex items-start justify-between gap-3">
      <span class="text-brand-text-secondary/60 shrink-0">{dateLabel}</span>
      <span class="text-brand-text-primary text-right break-words min-w-0">{formattedBeginDate}</span>
    </div>
  {/if}
  {#if formattedEndDate}
    <div class="flex items-start justify-between gap-3">
      <span class="text-brand-text-secondary/60 shrink-0">{endLabel}</span>
      <span class="text-brand-text-primary text-right break-words min-w-0">{formattedEndDate}</span>
    </div>
  {/if}
  {#if city}
    <div class="flex items-start justify-between gap-3">
      <span class="text-brand-text-secondary/60 shrink-0">{cityLabel}</span>
      <span class="text-brand-text-primary text-right break-words min-w-0">{city}</span>
    </div>
  {/if}
  {#if country && !isSameCityCountry}
    <div class="flex items-start justify-between gap-3">
      <span class="text-brand-text-secondary/60 shrink-0">{countryLabel}</span>
      <span class="text-brand-text-primary text-right break-words min-w-0">{country}</span>
    </div>
  {/if}
{/snippet}

{#if hasContent}
  <section class="border border-brand-border/60 rounded-lg bg-brand-sidebar/40 overflow-hidden {className}">
    <h3 class="px-3 py-2 text-xs font-semibold text-brand-text-secondary">{i18n.t("artistInfo.panelTitle", {}, "Artist Information")}</h3>
    <div class="px-3 pb-3 pt-2 border-t border-brand-border/40 space-y-2 text-xs">
      {@render contentRows()}
    </div>
  </section>
{/if}
