<script lang="ts">
  import { i18n, type Locale } from "../stores/i18n.svelte";
  import { MANUAL_LANGUAGES, localeLabel, localePickerGroups, isManualLanguage } from "../locales";
  import { prefs, type RatingStyle, type WeekStart } from "../stores/prefs.svelte";
  import Select from "./Select.svelte";
  import { GearIcon as Settings } from "phosphor-svelte";

  const languageGroups = localePickerGroups();
</script>

<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6">
  <div class="pb-3 flex items-center justify-between">
    <div class="flex items-center gap-3">
      <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
        <Settings class="w-5 h-5" />
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.generalTitle')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">{i18n.t('settings.generalSubtitle', {}, 'Configure application language and formatting preferences.')}</p>
      </div>
    </div>
  </div>

  <div class="flex items-center justify-between gap-4 py-4">
    <div class="flex flex-col gap-0.5 min-w-0">
      <label for="language-select" class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.selectLanguage')}</label>
      <p class="text-xs text-brand-text-secondary">{i18n.t('settings.uiLanguageHint')}</p>
    </div>
    <Select
      id="language-select"
      value={i18n.currentLocale}
      onchange={(e) => i18n.setLocale(e.currentTarget.value as Locale)}
      class="shrink-0 bg-brand-main border border-brand-border hover:border-brand-accent/60 text-brand-text-primary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
    >
      {#each languageGroups.pinned as tag (tag)}
        <option value={tag} lang={tag}>{localeLabel(tag)}</option>
      {/each}
      {#if languageGroups.rest.length > 0}
        <!-- Non-focusable separator: a disabled option is skipped by keyboard navigation. -->
        <option disabled>──────────</option>
        {#each languageGroups.rest as tag (tag)}
          <option value={tag} lang={tag}>{localeLabel(tag)}</option>
        {/each}
      {/if}
    </Select>
  </div>

  <div class="flex items-center justify-between gap-4 py-4">
    <div class="flex flex-col gap-0.5 min-w-0">
      <label for="manual-language-select" class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.manualLanguage')}</label>
      <p class="text-xs text-brand-text-secondary">{i18n.t('settings.manualLanguageHint')}</p>
    </div>
    <Select
      id="manual-language-select"
      value={i18n.manualLanguage}
      onchange={(e) => {
        const code = e.currentTarget.value;
        if (isManualLanguage(code)) void i18n.setManualLanguage(code);
      }}
      class="shrink-0 bg-brand-main border border-brand-border hover:border-brand-accent/60 text-brand-text-primary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
    >
      {#each MANUAL_LANGUAGES as m (m.code)}
        <option value={m.code} lang={m.code.toLowerCase()}>{m.label}</option>
      {/each}
    </Select>
  </div>

  <div class="flex items-center justify-between gap-4 py-4">
    <div class="flex flex-col gap-0.5 min-w-0">
      <label for="week-start-select" class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.weekStart')}</label>
      <p class="text-xs text-brand-text-secondary">{i18n.t('settings.weekStartHint')}</p>
    </div>
    <Select
      id="week-start-select"
      value={prefs.weekStart}
      onchange={(e) => prefs.setWeekStart(e.currentTarget.value as WeekStart)}
      class="shrink-0 bg-brand-main border border-brand-border hover:border-brand-accent/60 text-brand-text-primary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
    >
      <option value="sunday">{i18n.t('settings.weekStartSunday')}</option>
      <option value="monday">{i18n.t('settings.weekStartMonday')}</option>
    </Select>
  </div>

  <div class="flex items-center justify-between gap-4 py-4">
    <div class="flex flex-col gap-0.5 min-w-0">
      <label for="rating-style-select" class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.ratingStyle')}</label>
      <p class="text-xs text-brand-text-secondary">{i18n.t('settings.ratingStyleHint')}</p>
    </div>
    <Select
      id="rating-style-select"
      value={prefs.ratingStyle}
      onchange={(e) => prefs.setRatingStyle(e.currentTarget.value as RatingStyle)}
      class="shrink-0 bg-brand-main border border-brand-border hover:border-brand-accent/60 text-brand-text-primary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
    >
      <option value="heart">{i18n.t('settings.ratingStyleHeart')}</option>
      <option value="stars">{i18n.t('settings.ratingStyleStars')}</option>
      <option value="both">{i18n.t('settings.ratingStyleBoth')}</option>
    </Select>
  </div>
</div>
