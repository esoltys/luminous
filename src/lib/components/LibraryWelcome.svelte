<script lang="ts">
  import {
    FileAudioIcon as FileAudio,
    FolderOpenIcon as FolderOpen,
    FolderSimplePlusIcon as FolderPlus,
    ArrowsClockwiseIcon as RefreshCw,
  } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { walkthroughStore } from "../stores/walkthrough.svelte";
  import Button from "./Button.svelte";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import ReactiveLogoBrand from "./ReactiveLogoBrand.svelte";

  // A database last opened by a newer Luminous build looks identical to a
  // genuinely empty library (song queries naming a since-changed column just
  // fail) — this flag, set once at startup from get_db_schema_status, lets us
  // tell the two apart instead of misleadingly offering to "add a folder".
  let dbNewer = $derived(collectionStore.dbSchemaStatus?.db_newer_than_app ?? false);
</script>

<!-- Shown wherever a view would otherwise render an empty grid/list for want of
     a library: one a newer build wrote, one still loading, or none added yet —
     distinct from the "your search/filters matched nothing" empty states, which
     keep their own dashed-border treatment. -->
<div
  class="flex flex-col items-center justify-center max-w-md mx-auto text-center px-6 py-8 select-none"
>
  {#if dbNewer}
    <div
      class="w-14 h-14 rounded-full bg-brand-accent/15 border border-brand-accent/30 flex items-center justify-center mb-4"
    >
      <RefreshCw class="w-7 h-7 text-brand-accent-text" />
    </div>
    <h3 class="text-base font-semibold text-brand-text-primary mb-1.5 text-balance">
      {i18n.t("dbNewerThanApp.title")}
    </h3>
    <p class="text-xs text-brand-text-secondary mb-5 leading-relaxed text-pretty">
      {i18n.t("dbNewerThanApp.text", {
        dbVersion: collectionStore.dbSchemaStatus?.db_version,
        appVersion: collectionStore.dbSchemaStatus?.app_version,
      })}
    </p>
    <Button
      onclick={() => {
        navigationStore.activeTab = "settings";
      }}
      variant="primary"
      size="sm"
    >
      {i18n.t("sidebar.settings")}
    </Button>
  {:else if !collectionStore.libraryLoaded}
    <!-- The first library read hasn't landed yet: an empty view here means
         "still loading", so don't offer to create a library (#1968). -->
    <div class="flex flex-col items-center gap-3" role="status">
      <LoadingSpinner label={i18n.t("home.loading")} />
    </div>
  {:else}
    <!-- Built around the Luminous mark: adding a folder is the primary path,
         while playing files or a folder directly without a library stays
         one click away as secondary actions (#2047). -->
    <div class="anim-welcome-mark mb-5">
      <ReactiveLogoBrand size="2xl" />
    </div>

    <h2
      class="text-2xl @xl:text-3xl font-heading font-bold text-brand-text-primary leading-snug mb-2 text-balance"
    >
      {i18n.t("emptyLibrary.title")}
    </h2>
    <p class="text-sm text-brand-text-secondary leading-relaxed text-pretty mb-6">
      {i18n.t("emptyLibrary.subtitle")}
    </p>

    <Button onclick={() => collectionStore.addDirectoryDialog()} variant="primary" size="md">
      <FolderPlus class="w-4 h-4" />
      {i18n.t("emptyLibrary.addFolder")}
    </Button>

    <div class="flex flex-wrap items-center justify-center gap-2 mt-3">
      <Button onclick={() => playerStore.openFileDialog()} variant="secondary" size="sm">
        <FileAudio class="w-3.5 h-3.5" />
        {i18n.t("emptyLibrary.openFiles")}
      </Button>
      <Button onclick={() => playerStore.openFolderDialog()} variant="secondary" size="sm">
        <FolderOpen class="w-3.5 h-3.5" />
        {i18n.t("emptyLibrary.playFolder")}
      </Button>
    </div>

    <p class="text-xs text-brand-text-secondary/80 leading-relaxed text-pretty mt-5">
      {i18n.t("emptyLibrary.dropHint")}
    </p>

    <div class="flex items-center gap-3 mt-3 text-xs">
      <button
        type="button"
        onclick={() => {
          navigationStore.activeTab = "help";
        }}
        class="text-brand-text-secondary underline-offset-2 hover:underline hover:text-brand-accent-text transition-colors"
        >{i18n.t("sidebar.help")}</button
      >
      <span class="text-brand-border" aria-hidden="true">·</span>
      <button
        type="button"
        onclick={() => walkthroughStore.start()}
        class="text-brand-text-secondary underline-offset-2 hover:underline hover:text-brand-accent-text transition-colors"
        >{i18n.t("walkthrough.takeTour", {}, "Take a quick tour")}</button
      >
    </div>
  {/if}
</div>
