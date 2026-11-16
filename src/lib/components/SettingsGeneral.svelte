<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import {
    prefs,
    type AutoPlaylistFamily,
    type RatingStyle,
    type WeekStart,
  } from "../stores/prefs.svelte";
  import LanguageSelect from "./LanguageSelect.svelte";
  import Select from "./Select.svelte";
  import Toggle from "./Toggle.svelte";
  import { GearIcon as Settings, SparkleIcon } from "phosphor-svelte";

  // Label and hint keys are spelled out, not built from a suffix, so they stay greppable.
  const AUTO_PLAYLIST_FAMILIES: { family: AutoPlaylistFamily; label: string; hint: string }[] = [
    {
      family: "genre",
      label: "settings.autoPlaylistsGenre",
      hint: "settings.autoPlaylistsGenreHint",
    },
    {
      family: "decade",
      label: "settings.autoPlaylistsDecade",
      hint: "settings.autoPlaylistsDecadeHint",
    },
    {
      family: "artist_tag",
      label: "settings.autoPlaylistsArtistTag",
      hint: "settings.autoPlaylistsArtistTagHint",
    },
    {
      family: "bpm",
      label: "settings.autoPlaylistsBpm",
      hint: "settings.autoPlaylistsBpmHint",
    },
    {
      family: "language",
      label: "settings.autoPlaylistsLanguage",
      hint: "settings.autoPlaylistsLanguageHint",
    },
    {
      family: "daypart",
      label: "settings.autoPlaylistsDaypart",
      hint: "settings.autoPlaylistsDaypartHint",
    },
  ];
</script>

<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6">
  <div class="pb-3 flex items-center justify-between">
    <div class="flex items-center gap-3">
      <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
        <Settings class="w-5 h-5" />
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t("settings.generalTitle")}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">
          {i18n.t(
            "settings.generalSubtitle",
            {},
            "Configure application language and formatting preferences.",
          )}
        </p>
      </div>
    </div>
  </div>

  <div class="flex items-center justify-between gap-4 py-4">
    <div class="flex flex-col gap-0.5 min-w-0">
      <label for="language-select" class="text-sm font-medium text-brand-text-primary"
        >{i18n.t("settings.selectLanguage")}</label
      >
      <p class="text-xs text-brand-text-secondary">{i18n.t("settings.uiLanguageHint")}</p>
    </div>
    <LanguageSelect
      id="language-select"
      class="shrink-0 bg-brand-main border border-brand-border hover:border-brand-accent/60 text-brand-text-primary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
    />
  </div>

  <div class="flex items-center justify-between gap-4 py-4">
    <div class="flex flex-col gap-0.5 min-w-0">
      <label for="week-start-select" class="text-sm font-medium text-brand-text-primary"
        >{i18n.t("settings.weekStart")}</label
      >
      <p class="text-xs text-brand-text-secondary">{i18n.t("settings.weekStartHint")}</p>
    </div>
    <Select
      id="week-start-select"
      value={prefs.weekStart}
      onchange={(e) => prefs.setWeekStart(e.currentTarget.value as WeekStart)}
      class="shrink-0 bg-brand-main border border-brand-border hover:border-brand-accent/60 text-brand-text-primary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
    >
      <option value="sunday">{i18n.t("settings.weekStartSunday")}</option>
      <option value="monday">{i18n.t("settings.weekStartMonday")}</option>
    </Select>
  </div>

  <div class="flex items-center justify-between gap-4 py-4">
    <div class="flex flex-col gap-0.5 min-w-0">
      <label for="rating-style-select" class="text-sm font-medium text-brand-text-primary"
        >{i18n.t("settings.ratingStyle")}</label
      >
      <p class="text-xs text-brand-text-secondary">{i18n.t("settings.ratingStyleHint")}</p>
    </div>
    <Select
      id="rating-style-select"
      value={prefs.ratingStyle}
      onchange={(e) => prefs.setRatingStyle(e.currentTarget.value as RatingStyle)}
      class="shrink-0 bg-brand-main border border-brand-border hover:border-brand-accent/60 text-brand-text-primary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
    >
      <option value="heart">{i18n.t("settings.ratingStyleHeart")}</option>
      <option value="stars">{i18n.t("settings.ratingStyleStars")}</option>
      <option value="both">{i18n.t("settings.ratingStyleBoth")}</option>
    </Select>
  </div>
</div>

<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 mt-4">
  <div class="pb-3 flex items-center gap-3">
    <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
      <SparkleIcon class="w-5 h-5" />
    </div>
    <div class="space-y-1 min-w-0">
      <h3 class="font-bold text-sm text-brand-text-primary">
        {i18n.t("settings.autoPlaylistsTitle")}
      </h3>
      <p class="text-xs text-brand-text-secondary leading-relaxed text-pretty">
        {i18n.t("settings.autoPlaylistsSubtitle")}
      </p>
    </div>
  </div>

  {#each AUTO_PLAYLIST_FAMILIES as { family, label, hint } (family)}
    <div class="flex items-center justify-between gap-4 py-4">
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-sm font-medium text-brand-text-primary">{i18n.t(label)}</span>
        <p class="text-xs text-brand-text-secondary text-pretty">
          {i18n.t(hint)}
        </p>
      </div>
      <Toggle
        checked={prefs.autoPlaylists[family]}
        onchange={(v) => prefs.setAutoPlaylistFamily(family, v)}
        label={i18n.t(label)}
      />
    </div>

    {#if family === "bpm"}
      <div
        class="flex items-center justify-between gap-4 py-4 ml-6 pl-4 border-l border-brand-border"
      >
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-sm font-medium text-brand-text-primary"
            >{i18n.t("settings.detectTempoKeyLabel")}</span
          >
          <p class="text-xs text-brand-text-secondary text-pretty">
            {i18n.t("settings.detectTempoKeyHint")}
          </p>
        </div>
        <Toggle
          checked={prefs.detectTempoAndKey}
          onchange={(v) => prefs.setDetectTempoAndKey(v)}
          label={i18n.t("settings.detectTempoKeyLabel")}
        />
      </div>
    {/if}
  {/each}
</div>
