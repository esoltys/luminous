<script lang="ts">
  import { playerStore } from "../stores/player.svelte";
  import { navigationStore, type SettingsTab } from "../stores/navigation.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { rememberScroll } from "../utils/scrollMemory";
  import SettingsGeneral from "./SettingsGeneral.svelte";
  import SettingsSystem from "./SettingsSystem.svelte";
  import SettingsSources from "./SettingsSources.svelte";
  import SettingsIntegrations from "./SettingsIntegrations.svelte";
  import SettingsThemes from "./SettingsThemes.svelte";
  import SettingsAbout from "./SettingsAbout.svelte";
  import Equalizer from "./Equalizer.svelte";

  let settingsTab = $state<SettingsTab>(navigationStore.settingsSubTab || "general");

  const TABS: { value: SettingsTab; label: () => string }[] = [
    { value: "general", label: () => i18n.t('settings.tabGeneral') },
    { value: "system", label: () => i18n.t('settings.tabSystem') },
    { value: "sources", label: () => i18n.t('settings.tabSources') },
    { value: "integrations", label: () => i18n.t('settings.tabIntegrations') },
    { value: "themes", label: () => i18n.t('settings.tabThemes') },
    { value: "equalizer", label: () => i18n.t('settings.tabEqualizer') },
    { value: "about", label: () => i18n.t('settings.tabAbout') }
  ];

  $effect(() => {
    if (navigationStore.settingsSubTab && settingsTab !== navigationStore.settingsSubTab) {
      settingsTab = navigationStore.settingsSubTab;
    }
  });

  // Only a change is saved, never the initial value: SettingsView can mount before the
  // startup restore in +page.svelte has read the backend, and must not overwrite it.
  let savedTab = untrack(() => settingsTab);
  $effect(() => {
    if (settingsTab !== savedTab) {
      savedTab = settingsTab;
      invoke("set_app_setting", { key: "active_settings_tab", value: settingsTab });
    }
  });
</script>

<div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full">
  <div class="flex-1 overflow-y-scroll px-6 pb-6" class:pb-28={!!playerStore.currentSong} use:rememberScroll={`settings:${settingsTab}`}>
    <div class="pt-4 pb-4 flex items-center gap-2" role="tablist" aria-label={i18n.t('settings.title')}>
      {#each TABS as tab (tab.value)}
        <button
          onclick={() => { settingsTab = tab.value; navigationStore.settingsSubTab = tab.value; }}
          role="tab"
          aria-selected={settingsTab === tab.value}
          class="px-3 py-1.5 rounded-lg text-sm font-medium transition-colors {settingsTab === tab.value ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20' : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'}"
        >
          {tab.label()}
        </button>
      {/each}
    </div>

    <div class="max-w-3xl mx-auto space-y-6">
      {#if settingsTab === "general"}
        <SettingsGeneral />
      {:else if settingsTab === "system"}
        <SettingsSystem />
      {:else if settingsTab === "sources"}
        <SettingsSources />
      {:else if settingsTab === "integrations"}
        <SettingsIntegrations />
      {:else if settingsTab === "themes"}
        <SettingsThemes />
      {:else if settingsTab === "equalizer"}
        <Equalizer />
      {:else if settingsTab === "about"}
        <SettingsAbout />
      {/if}
    </div>
  </div>
</div>
