<script lang="ts">
  import { FolderClosed, RefreshCw } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import Button from "./Button.svelte";

  // A database last opened by a newer Luminous build looks identical to a
  // genuinely empty library (song queries naming a since-changed column just
  // fail) — this flag, set once at startup from get_db_schema_status, lets us
  // tell the two apart instead of misleadingly offering to "add a folder".
  let dbNewer = $derived(collectionStore.dbSchemaStatus?.db_newer_than_app ?? false);
</script>

<!-- Shown wherever a view would otherwise render an empty grid/list because no
     watched folder has been added yet — distinct from the "your search/filters
     matched nothing" empty states, which keep their own dashed-border treatment. -->
<div class="flex flex-col items-center justify-center max-w-sm mx-auto text-center p-8 bg-brand-sidebar/40 rounded-xl border border-brand-border select-none">
  <div class="w-14 h-14 flex items-center justify-center mb-4">
    {#if dbNewer}
      <div class="w-14 h-14 rounded-full bg-brand-accent/15 border border-brand-accent/30 flex items-center justify-center">
        <RefreshCw class="w-7 h-7 text-brand-accent-text" />
      </div>
    {:else}
      <img src="/luminous-mark.svg" alt="" class="w-14 h-14" />
    {/if}
  </div>
  {#if dbNewer}
    <h3 class="text-base font-semibold text-brand-text-primary mb-1.5 text-balance">{i18n.t('dbNewerThanApp.title')}</h3>
    <p class="text-xs text-brand-text-secondary mb-5 leading-relaxed text-pretty">
      {i18n.t('dbNewerThanApp.text', {
        dbVersion: collectionStore.dbSchemaStatus?.db_version,
        appVersion: collectionStore.dbSchemaStatus?.app_version,
      })}
    </p>
    <Button onclick={() => { navigationStore.activeTab = 'settings'; }} variant="primary" size="sm">
      {i18n.t('sidebar.settings')}
    </Button>
  {:else}
    <h3 class="text-base font-semibold text-brand-text-primary mb-1.5 text-balance">{i18n.t('emptyLibrary.title')}</h3>
    <p class="text-xs text-brand-text-secondary mb-5 leading-relaxed text-pretty">{i18n.t('emptyLibrary.text')}</p>
    <div class="flex items-center gap-2">
      <Button onclick={() => collectionStore.addDirectoryDialog()} variant="primary" size="sm">
        <FolderClosed class="w-3.5 h-3.5" />
        {i18n.t('settings.addFolder')}
      </Button>
      <Button onclick={() => { navigationStore.activeTab = 'help'; }} variant="secondary" size="sm">
        {i18n.t('sidebar.help')}
      </Button>
    </div>
  {/if}
</div>
