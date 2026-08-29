<script lang="ts">
  import { playerStore } from "../stores/player.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { Settings } from "lucide-svelte";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { rememberScroll } from "../utils/scrollMemory";
  import SettingsGeneral from "./SettingsGeneral.svelte";
  import SettingsFolders from "./SettingsFolders.svelte";
  import SettingsTools from "./SettingsTools.svelte";
  import SettingsThemes from "./SettingsThemes.svelte";
  import SettingsAbout from "./SettingsAbout.svelte";
  import Equalizer from "./Equalizer.svelte";

  let settingsTab = $state<"general" | "folders" | "tools" | "themes" | "equalizer" | "about">("general");
  let isTabInitialized = $state(false);

  onMount(async () => {
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings && settings.active_settings_tab) {
        const savedTab = settings.active_settings_tab;
        if (savedTab === "general" || savedTab === "folders" || savedTab === "tools" || savedTab === "themes" || savedTab === "equalizer" || savedTab === "about") {
          settingsTab = savedTab;
        }
      }
    } catch (e) {
      console.error("Failed to fetch settings on mount:", e);
    } finally {
      isTabInitialized = true;
    }
  });

  $effect(() => {
    if (isTabInitialized) {
      invoke("set_app_setting", { key: "active_settings_tab", value: settingsTab });
    }
  });
</script>

<div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full">
  <div class="h-16 pl-6 pr-8 border-b border-brand-border flex items-center justify-between shrink-0">
    <div class="flex items-center gap-3">
      <Settings class="w-5 h-5 text-brand-accent-text" />
      <h2 class="text-base font-bold text-brand-text-primary">{i18n.t('settings.title')}</h2>
    </div>

    <div class="flex bg-brand-sidebar border border-brand-border rounded-xl p-0.5 text-xs shadow-sm">
      <button
        onclick={() => { settingsTab = "general"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all {settingsTab === 'general' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabGeneral')}
      </button>
      <button
        onclick={() => { settingsTab = "folders"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all {settingsTab === 'folders' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabFolders')}
      </button>
      <button
        onclick={() => { settingsTab = "tools"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all {settingsTab === 'tools' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabTools')}
      </button>
      <button
        onclick={() => { settingsTab = "themes"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all {settingsTab === 'themes' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabThemes')}
      </button>
      <button
        onclick={() => { settingsTab = "equalizer"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all {settingsTab === 'equalizer' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabEqualizer')}
      </button>
      <button
        onclick={() => { settingsTab = "about"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all {settingsTab === 'about' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabAbout')}
      </button>
    </div>
  </div>

  <div class="flex-1 overflow-y-scroll p-6" class:pb-28={!!playerStore.currentSong} use:rememberScroll={`settings:${settingsTab}`}>
    <div class="max-w-3xl mx-auto space-y-6">
      {#if settingsTab === "general"}
        <SettingsGeneral />
      {:else if settingsTab === "folders"}
        <SettingsFolders />
      {:else if settingsTab === "tools"}
        <SettingsTools />
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
