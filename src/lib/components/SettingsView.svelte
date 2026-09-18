<script lang="ts">
  import { playerStore } from "../stores/player.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { rememberScroll } from "../utils/scrollMemory";
  import SettingsGeneral from "./SettingsGeneral.svelte";
  import SettingsFolders from "./SettingsFolders.svelte";
  import SettingsIntegrations from "./SettingsIntegrations.svelte";
  import SettingsThemes from "./SettingsThemes.svelte";
  import SettingsAbout from "./SettingsAbout.svelte";
  import Equalizer from "./Equalizer.svelte";

  let settingsTab = $state<"general" | "folders" | "integrations" | "themes" | "equalizer" | "about">("general");
  let isTabInitialized = $state(false);

  const TABS: { value: typeof settingsTab; label: () => string }[] = [
    { value: "general", label: () => i18n.t('settings.tabGeneral') },
    { value: "folders", label: () => i18n.t('settings.tabFolders') },
    { value: "integrations", label: () => i18n.t('settings.tabIntegrations') },
    { value: "themes", label: () => i18n.t('settings.tabThemes') },
    { value: "equalizer", label: () => i18n.t('settings.tabEqualizer') },
    { value: "about", label: () => i18n.t('settings.tabAbout') }
  ];

  onMount(() => {
    (async () => {
      try {
        const settings = await invoke<Record<string, string>>("get_all_app_settings");
        if (settings && settings.active_settings_tab) {
          const savedTab = settings.active_settings_tab;
          if (savedTab === "general" || savedTab === "folders" || savedTab === "integrations" || savedTab === "themes" || savedTab === "equalizer" || savedTab === "about") {
            settingsTab = savedTab;
          }
        }
      } catch (e) {
        console.error("Failed to fetch settings on mount:", e);
      } finally {
        isTabInitialized = true;
      }
    })();
  });

  $effect(() => {
    if (isTabInitialized) {
      invoke("set_app_setting", { key: "active_settings_tab", value: settingsTab });
    }
  });
</script>

<div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full">
  <div class="flex-1 overflow-y-scroll px-6 pb-6" class:pb-28={!!playerStore.currentSong} use:rememberScroll={`settings:${settingsTab}`}>
    <div class="pt-4 pb-4 flex items-center gap-2" role="tablist" aria-label={i18n.t('settings.title')}>
      {#each TABS as tab (tab.value)}
        <button
          onclick={() => { settingsTab = tab.value; }}
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
      {:else if settingsTab === "folders"}
        <SettingsFolders />
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
