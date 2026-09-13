<script lang="ts">
  import { playerStore } from "../stores/player.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { GearIcon as Settings } from "phosphor-svelte";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { rememberScroll } from "../utils/scrollMemory";
  import SettingsGeneral from "./SettingsGeneral.svelte";
  import SettingsFolders from "./SettingsFolders.svelte";
  import SettingsIntegrations from "./SettingsIntegrations.svelte";
  import SettingsTools from "./SettingsTools.svelte";
  import SettingsThemes from "./SettingsThemes.svelte";
  import SettingsAbout from "./SettingsAbout.svelte";
  import Equalizer from "./Equalizer.svelte";

  let settingsTab = $state<"general" | "folders" | "integrations" | "tools" | "themes" | "equalizer" | "about">("general");
  let isTabInitialized = $state(false);

  let tabElements = $state<Record<string, HTMLButtonElement>>({});
  let indicatorStyle = $state({ left: 0, width: 0, opacity: 0 });
  let indicatorMounted = $state(false);

  function updateIndicator() {
    const el = tabElements[settingsTab];
    if (el) {
      indicatorStyle = {
        left: el.offsetLeft,
        width: el.offsetWidth,
        opacity: 1
      };
    }
  }

  onMount(() => {
    (async () => {
      try {
        const settings = await invoke<Record<string, string>>("get_all_app_settings");
        if (settings && settings.active_settings_tab) {
          const savedTab = settings.active_settings_tab;
          if (savedTab === "general" || savedTab === "folders" || savedTab === "integrations" || savedTab === "tools" || savedTab === "themes" || savedTab === "equalizer" || savedTab === "about") {
            settingsTab = savedTab;
          }
        }
      } catch (e) {
        console.error("Failed to fetch settings on mount:", e);
      } finally {
        isTabInitialized = true;
      }
    })();

    const handleResize = () => updateIndicator();
    window.addEventListener("resize", handleResize);
    return () => {
      window.removeEventListener("resize", handleResize);
    };
  });

  $effect(() => {
    if (isTabInitialized) {
      invoke("set_app_setting", { key: "active_settings_tab", value: settingsTab });
    }
  });

  $effect(() => {
    // Re-measure when tab changes or element mounts
    if (tabElements[settingsTab]) {
      updateIndicator();
      if (!indicatorMounted) {
        requestAnimationFrame(() => {
          indicatorMounted = true;
        });
      }
    }
  });
</script>

<div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full">
  <div class="h-16 pl-6 pr-8 border-b border-brand-border flex items-center justify-between shrink-0">
    <div class="flex items-center gap-3">
      <Settings class="w-5 h-5 text-brand-accent-text" />
      <h2 class="text-base font-bold text-brand-text-primary">{i18n.t('settings.title')}</h2>
    </div>

    <div
      class="relative flex bg-brand-sidebar border border-brand-border rounded-xl p-0.5 text-xs shadow-sm"
      role="tablist"
      aria-label={i18n.t('settings.title')}
    >
      <!-- Sliding Active Indicator Pill with matching rounded-xl corner radius -->
      <div
        class="absolute top-0.5 bottom-0.5 bg-brand-accent rounded-xl shadow-md pointer-events-none {indicatorMounted ? 'transition-[left,width] duration-200 ease-out' : 'transition-none'}"
        style="left: {indicatorStyle.left}px; width: {indicatorStyle.width}px; opacity: {indicatorStyle.opacity};"
        aria-hidden="true"
      ></div>

      <button
        bind:this={tabElements["general"]}
        onclick={() => { settingsTab = "general"; }}
        role="tab"
        aria-selected={settingsTab === 'general'}
        class="relative z-10 px-4 py-1.5 rounded-xl font-semibold transition-colors duration-200 {settingsTab === 'general' ? 'text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabGeneral')}
      </button>
      <button
        bind:this={tabElements["folders"]}
        onclick={() => { settingsTab = "folders"; }}
        role="tab"
        aria-selected={settingsTab === 'folders'}
        class="relative z-10 px-4 py-1.5 rounded-xl font-semibold transition-colors duration-200 {settingsTab === 'folders' ? 'text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabFolders')}
      </button>
      <button
        bind:this={tabElements["integrations"]}
        onclick={() => { settingsTab = "integrations"; }}
        role="tab"
        aria-selected={settingsTab === 'integrations'}
        class="relative z-10 px-4 py-1.5 rounded-xl font-semibold transition-colors duration-200 {settingsTab === 'integrations' ? 'text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabIntegrations')}
      </button>
      <button
        bind:this={tabElements["tools"]}
        onclick={() => { settingsTab = "tools"; }}
        role="tab"
        aria-selected={settingsTab === 'tools'}
        class="relative z-10 px-4 py-1.5 rounded-xl font-semibold transition-colors duration-200 {settingsTab === 'tools' ? 'text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabTools')}
      </button>
      <button
        bind:this={tabElements["themes"]}
        onclick={() => { settingsTab = "themes"; }}
        role="tab"
        aria-selected={settingsTab === 'themes'}
        class="relative z-10 px-4 py-1.5 rounded-xl font-semibold transition-colors duration-200 {settingsTab === 'themes' ? 'text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabThemes')}
      </button>
      <button
        bind:this={tabElements["equalizer"]}
        onclick={() => { settingsTab = "equalizer"; }}
        role="tab"
        aria-selected={settingsTab === 'equalizer'}
        class="relative z-10 px-4 py-1.5 rounded-xl font-semibold transition-colors duration-200 {settingsTab === 'equalizer' ? 'text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        {i18n.t('settings.tabEqualizer')}
      </button>
      <button
        bind:this={tabElements["about"]}
        onclick={() => { settingsTab = "about"; }}
        role="tab"
        aria-selected={settingsTab === 'about'}
        class="relative z-10 px-4 py-1.5 rounded-xl font-semibold transition-colors duration-200 {settingsTab === 'about' ? 'text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
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
      {:else if settingsTab === "integrations"}
        <SettingsIntegrations />
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
