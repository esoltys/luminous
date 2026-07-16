<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { themeStore, PREDEFINED_THEMES, LUMINOUS_DARK_COLORS, LUMINOUS_LIGHT_COLORS, type ThemeColors, type Theme } from "../stores/theme.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { Folder, Plus, Trash2, HelpCircle, Palette, Settings, Check, Wand2 } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Equalizer from "./Equalizer.svelte";
  import DesignTools from "./DesignTools.svelte";

  let settingsTab = $state<"folders" | "themes" | "equalizer" | "formats">("folders");
  let isTabInitialized = $state(false);
  let editingThemeId = $state<string | null>(null);
  let useAdvancedBuilder = $state(false);

  onMount(async () => {
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings && settings.active_settings_tab) {
        const savedTab = settings.active_settings_tab;
        if (savedTab === "folders" || savedTab === "themes" || savedTab === "equalizer" || savedTab === "formats") {
          settingsTab = savedTab as any;
        }
      }
    } catch (e) {
      console.error("Failed to restore active_settings_tab:", e);
    } finally {
      isTabInitialized = true;
    }
  });

  $effect(() => {
    if (isTabInitialized) {
      invoke("set_app_setting", { key: "active_settings_tab", value: settingsTab }).catch((err) => {
        console.error("Failed to save active_settings_tab:", err);
      });
    }
  });

  // Folders operations
  async function handleAddDirectory() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select Music Directory",
      });
      if (selected && typeof selected === "string") {
        await collectionStore.addDirectory(selected);
      }
    } catch (err) {
      console.error("Failed to open directory dialog:", err);
    }
  }

  async function handleRemoveDirectory(path: string) {
    if (confirm(`Stop watching folder: ${path}?\nSongs from this folder will not be removed from your playlists but will be marked unavailable.`)) {
      await collectionStore.removeDirectory(path);
    }
  }

  // Custom Theme state
  let newThemeName = $state("");
  let customColors = $state<ThemeColors>({
    "bg-main": "#0d0b18",
    "bg-sidebar": "#07050e",
    "bg-playerbar": "#0a0813",
    "color-accent": "#8b5cf6",
    "color-accent-hover": "#a78bfa",
    "color-text-primary": "#f3f4f6",
    "color-text-secondary": "#9ca3af",
    "color-border": "#1f1b2e"
  });

  // The System theme's live colors depend on the OS light/dark preference,
  // not the static (dark) preview baked into its PREDEFINED_THEMES entry —
  // use the current system scheme so its swatch matches what's on screen.
  function getPreviewColors(theme: Theme): ThemeColors {
    if (theme.id === "system") {
      return themeStore.systemColorScheme === "dark" ? LUMINOUS_DARK_COLORS : LUMINOUS_LIGHT_COLORS;
    }
    return theme.colors;
  }

  function loadActiveThemeColors() {
    customColors = { ...themeStore.resolvedColors };
  }

  // Pre-fill theme builder with current active theme colors on mount and
  // updates — skipped while editing an existing custom theme, since that
  // case is seeded from the theme being edited instead (below).
  $effect(() => {
    const _themeId = themeStore.activeThemeId;
    const _songId = playerStore.currentSong?.id;
    if (!editingThemeId) {
      loadActiveThemeColors();
    }
  });

  let editingTheme = $derived(themeStore.customThemes.find(t => t.id === editingThemeId));

  // Seed the builder (Simple and Advanced share this same customColors/
  // newThemeName state) from the theme being edited whenever Edit is clicked.
  $effect(() => {
    if (editingTheme) {
      newThemeName = editingTheme.name;
      customColors = { ...editingTheme.colors };
    }
  });

  function handleLivePreview() {
    // Inject the colors live to the document head for instant feedback
    if (typeof document === "undefined") return;
    let styleEl = document.getElementById("luminous-theme-style");
    if (!styleEl) {
      styleEl = document.createElement("style");
      styleEl.id = "luminous-theme-style";
      document.head.appendChild(styleEl);
    }
    styleEl.innerHTML = `
      :root {
        --bg-main: ${customColors["bg-main"]};
        --bg-sidebar: ${customColors["bg-sidebar"]};
        --bg-playerbar: ${customColors["bg-playerbar"]};
        --color-accent: ${customColors["color-accent"]};
        --color-accent-hover: ${customColors["color-accent-hover"]};
        --color-text-primary: ${customColors["color-text-primary"]};
        --color-text-secondary: ${customColors["color-text-secondary"]};
        --color-border: ${customColors["color-border"]};
      }
    `;
  }

  async function saveCustomTheme() {
    if (newThemeName.trim() === "") {
      alert("Please enter a name for your custom theme.");
      return;
    }

    if (editingThemeId) {
      await themeStore.addCustomTheme({
        id: editingThemeId,
        name: newThemeName.trim(),
        colors: { ...customColors },
        isCustom: true
      });
      editingThemeId = null;
    } else {
      const id = "custom-" + newThemeName.toLowerCase().replace(/[^a-z0-9]/g, "-");
      await themeStore.addCustomTheme({
        id,
        name: newThemeName.trim(),
        colors: { ...customColors },
        isCustom: true
      });
      newThemeName = "";
    }
  }
</script>

<div class="flex-1 flex flex-col overflow-hidden bg-brand-main text-brand-text-secondary h-full">
  <!-- Top Header bar -->
  <div class="h-16 px-6 border-b border-brand-border flex items-center justify-between">
    <div class="flex items-center gap-3">
      <Settings class="w-5 h-5 text-brand-accent-text" />
      <h2 class="text-base font-bold text-brand-text-primary">Settings</h2>
    </div>

    <!-- Sleek Tab Control -->
    <div class="flex bg-brand-sidebar border border-brand-border rounded-xl p-0.5 text-xs shadow-sm">
      <button
        onclick={() => { settingsTab = "folders"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all cursor-pointer {settingsTab === 'folders' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        Watched Folders
      </button>
      <button
        onclick={() => { settingsTab = "themes"; editingThemeId = null; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all cursor-pointer {settingsTab === 'themes' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        UI Themes
      </button>
      <button
        onclick={() => { settingsTab = "equalizer"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all cursor-pointer {settingsTab === 'equalizer' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        Equalizer
      </button>
      <button
        onclick={() => { settingsTab = "formats"; }}
        class="px-4 py-1.5 rounded-lg font-semibold transition-all cursor-pointer {settingsTab === 'formats' ? 'bg-brand-accent text-brand-accent-contrast shadow-md' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
      >
        File Formats
      </button>
    </div>
  </div>

  <!-- Content Area -->
  <div class="flex-1 overflow-y-auto p-6 pb-24 max-w-4xl space-y-6">
    {#if settingsTab === "folders"}
      <!-- Watched Folders Section -->
      <div class="flex justify-between items-center">
        <h3 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase">Watched Folders</h3>
        <button
          onclick={handleAddDirectory}
          class="bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast px-3.5 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-colors shadow-lg shadow-brand-accent/20 cursor-pointer"
        >
          <Plus class="w-4 h-4" /> Add Folder
        </button>
      </div>

      <!-- Info Banner -->
      <div class="bg-brand-accent/5 border border-brand-accent/20 rounded-xl p-4 flex gap-3.5 text-sm text-brand-text-secondary">
        <HelpCircle class="w-5 h-5 text-brand-accent-text shrink-0 mt-0.5" />
        <div class="space-y-1">
          <h4 class="font-semibold text-brand-text-primary">How do Watched Folders work?</h4>
          <p class="text-xs text-brand-text-secondary/70 leading-relaxed">
            Luminous monitors these folders for audio files (MP3, FLAC, M4A, etc.). When you press "Rescan Library",
            the scanner recursively searches these folders and adds new/updated tracks to your collection.
            Luminous uses mtime-based hashing to perform fast incremental scans.
          </p>
        </div>
      </div>

      <!-- Folders List -->
      <div class="space-y-2">
        {#each collectionStore.directories as dir}
          <div class="flex items-center justify-between bg-brand-sidebar/40 border border-brand-border rounded-xl p-4 hover:border-brand-border/80 transition-colors">
            <div class="flex items-center gap-3.5 min-w-0">
              <div class="w-10 h-10 rounded-lg bg-brand-main border border-brand-border flex items-center justify-center text-brand-accent-text">
                <Folder class="w-5 h-5" />
              </div>
              <div class="min-w-0">
                <p class="text-sm font-medium text-brand-text-primary truncate" title={dir.path}>{dir.path}</p>
                <p class="text-[10px] text-brand-text-secondary/50 mt-0.5">Recursive scanning active</p>
              </div>
            </div>
            <button
              onclick={() => handleRemoveDirectory(dir.path)}
              class="p-2 rounded-lg bg-brand-main hover:bg-red-950/20 text-brand-text-secondary hover:text-red-400 border border-brand-border hover:border-red-900/30 transition-colors cursor-pointer"
              title="Stop watching this folder"
            >
              <Trash2 class="w-4 h-4" />
            </button>
          </div>
        {/each}

        {#if collectionStore.directories.length === 0}
          <div class="border border-dashed border-brand-border rounded-xl py-12 text-center text-brand-text-secondary/60">
            <Folder class="w-12 h-12 mx-auto mb-2 text-brand-text-secondary/30" />
            <h4 class="font-semibold text-brand-text-primary mb-1">No Watched Folders</h4>
            <p class="text-xs text-brand-text-secondary/60 mb-4">Click "Add Folder" above to add your music directory.</p>
          </div>
        {/if}
      </div>
    {:else if settingsTab === "themes"}
      <!-- UI Themes Section -->
      <div class="space-y-6">
        <div>
          <h3 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase mb-3">Predefined Themes</h3>
          <div class="grid grid-cols-1 md:grid-cols-3 lg:grid-cols-5 gap-4">
            {#each PREDEFINED_THEMES as theme}
              {@const previewColors = getPreviewColors(theme)}
              <button
                onclick={() => themeStore.setTheme(theme.id)}
                class="bg-brand-sidebar/40 border rounded-xl p-4 flex flex-col items-start gap-3 text-left transition-all duration-200 group hover:border-brand-accent/40 cursor-pointer w-full relative {themeStore.activeThemeId === theme.id ? 'border-2 border-brand-accent shadow-md shadow-brand-accent/5' : 'border-brand-border'}"
              >
                <div class="flex items-center justify-between w-full">
                  <span class="font-semibold text-sm text-brand-text-primary">{theme.name}</span>
                </div>
                <!-- Miniature colors preview -->
                <div class="flex gap-1 w-full h-8 rounded-lg overflow-hidden border border-brand-border/40 bg-black/10">
                  <div class="w-[30%]" style="background-color: {previewColors['bg-sidebar']}" title="Sidebar"></div>
                  <div class="w-[50%]" style="background-color: {previewColors['bg-main']}" title="Main View"></div>
                  <div class="w-[10%]" style="background-color: {previewColors['bg-playerbar']}" title="Player Bar"></div>
                  <div class="w-[10%]" style="background-color: {previewColors['color-accent']}" title="Accent"></div>
                </div>
              </button>
            {/each}
          </div>
        </div>

        {#if themeStore.customThemes.length > 0}
          <div>
            <h3 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase mb-3">Custom Themes</h3>
            <div class="grid grid-cols-1 md:grid-cols-3 lg:grid-cols-5 gap-4">
              {#each themeStore.customThemes as theme}
                <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <div
                  onclick={() => themeStore.setTheme(theme.id)}
                  role="button"
                  tabindex="0"
                  class="bg-brand-sidebar/40 border rounded-xl p-4 flex flex-col gap-3 text-left transition-colors cursor-pointer w-full {themeStore.activeThemeId === theme.id ? 'border-2 border-brand-accent shadow-md shadow-brand-accent/5' : 'border-brand-border hover:border-brand-border/80'}"
                >
                  <div class="flex items-center justify-between w-full">
                    <span class="font-semibold text-sm text-brand-text-primary truncate">{theme.name}</span>
                  </div>
                  <div class="flex gap-1 w-full h-8 rounded-lg overflow-hidden border border-brand-border/40 bg-black/10">
                    <div class="w-[30%]" style="background-color: {theme.colors['bg-sidebar']}" title="Sidebar"></div>
                    <div class="w-[50%]" style="background-color: {theme.colors['bg-main']}" title="Main View"></div>
                    <div class="w-[10%]" style="background-color: {theme.colors['bg-playerbar']}" title="Player Bar"></div>
                    <div class="w-[10%]" style="background-color: {theme.colors['color-accent']}" title="Accent"></div>
                  </div>
                  <div class="flex gap-2">
                    <button
                      onclick={(e) => { e.stopPropagation(); editingThemeId = theme.id; }}
                      class="flex-1 px-2 py-1 rounded text-xs font-semibold bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast transition-colors cursor-pointer"
                      title="Edit Theme"
                    >
                      Edit
                    </button>
                    <button
                      onclick={(e) => { e.stopPropagation(); themeStore.deleteCustomTheme(theme.id); }}
                      class="p-1 rounded bg-brand-main hover:bg-red-950/20 text-brand-text-secondary hover:text-red-400 border border-brand-border hover:border-red-900/30 transition-colors cursor-pointer"
                      title="Delete Theme"
                    >
                      <Trash2 class="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          </div>
        {/if}

        <!-- Custom Theme Builder Form -->
        <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
          <div class="flex items-center justify-between border-b border-brand-border pb-3">
            <div class="flex items-center gap-2">
              <Palette class="w-5 h-5 text-brand-accent-text" />
              <h4 class="font-bold text-sm text-brand-text-primary">
                {editingTheme ? `Editing "${editingTheme.name}"` : 'Custom Theme Builder'}
              </h4>
            </div>
            <div class="flex items-center gap-2">
              {#if editingThemeId}
                <button
                  onclick={() => { editingThemeId = null; }}
                  class="text-xs text-brand-text-secondary hover:text-brand-text-primary px-3 py-1.5 rounded-lg border border-brand-border hover:border-brand-accent/40 transition-colors cursor-pointer"
                >
                  Cancel
                </button>
              {/if}
              <!-- Simple/Advanced Toggle -->
              <div class="flex items-center gap-2 bg-brand-main rounded-full p-0.5 border border-brand-border">
                <button
                  onclick={() => { useAdvancedBuilder = false; }}
                  class="px-3 py-1.5 rounded-full text-xs font-semibold transition-all {!useAdvancedBuilder ? 'bg-brand-accent text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
                >
                  Simple
                </button>
                <button
                  onclick={() => { useAdvancedBuilder = true; }}
                  class="px-3 py-1.5 rounded-full text-xs font-semibold transition-all {useAdvancedBuilder ? 'bg-brand-accent text-brand-accent-contrast' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
                >
                  Advanced
                </button>
              </div>
            </div>
          </div>

          {#if useAdvancedBuilder}
            <DesignTools {customColors} {newThemeName} themeId={editingThemeId} onSaved={() => { editingThemeId = null; }} />
          {:else}
            <div class="space-y-5">
              <div class="flex flex-col md:flex-row gap-4 items-end justify-between">
                <div class="flex flex-col gap-1.5 flex-1 max-w-sm">
                  <label for="theme-name-input" class="text-xs text-brand-text-secondary font-semibold">Theme Name</label>
                  <input
                    id="theme-name-input"
                    type="text"
                    bind:value={newThemeName}
                    placeholder="e.g. Emerald Coast, Cyberpunk..."
                    class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent w-full"
                  />
                </div>
                <button
                  onclick={loadActiveThemeColors}
                  class="bg-brand-main hover:bg-brand-sidebar border border-brand-border hover:border-brand-accent/40 text-brand-text-primary px-4 py-2 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-colors cursor-pointer shrink-0 h-9"
                >
                  <Palette class="w-4 h-4 text-brand-accent-text" /> Import Active Colors
                </button>
              </div>

              <div class="grid grid-cols-2 md:grid-cols-4 gap-6 pt-2">
                <!-- Main Background -->
                <div class="flex items-center gap-3">
                  <input type="color" bind:value={customColors['bg-main']} oninput={handleLivePreview} class="w-9 h-9 rounded border border-brand-border cursor-pointer bg-transparent shrink-0" />
                  <div class="flex flex-col">
                    <span class="text-xs font-semibold text-brand-text-primary">Main View</span>
                    <span class="text-[10px] text-brand-text-secondary/50">bg-main</span>
                  </div>
                </div>

                <!-- Sidebar Background -->
                <div class="flex items-center gap-3">
                  <input type="color" bind:value={customColors['bg-sidebar']} oninput={handleLivePreview} class="w-9 h-9 rounded border border-brand-border cursor-pointer bg-transparent shrink-0" />
                  <div class="flex flex-col">
                    <span class="text-xs font-semibold text-brand-text-primary">Sidebar</span>
                    <span class="text-[10px] text-brand-text-secondary/50">bg-sidebar</span>
                  </div>
                </div>

                <!-- Player Bar Background -->
                <div class="flex items-center gap-3">
                  <input type="color" bind:value={customColors['bg-playerbar']} oninput={handleLivePreview} class="w-9 h-9 rounded border border-brand-border cursor-pointer bg-transparent shrink-0" />
                  <div class="flex flex-col">
                    <span class="text-xs font-semibold text-brand-text-primary">Player Bar</span>
                    <span class="text-[10px] text-brand-text-secondary/50">bg-playerbar</span>
                  </div>
                </div>

                <!-- Accent Color -->
                <div class="flex items-center gap-3">
                  <input type="color" bind:value={customColors['color-accent']} oninput={handleLivePreview} class="w-9 h-9 rounded border border-brand-border cursor-pointer bg-transparent shrink-0" />
                  <div class="flex flex-col">
                    <span class="text-xs font-semibold text-brand-text-primary">Accent</span>
                    <span class="text-[10px] text-brand-text-secondary/50">color-accent</span>
                  </div>
                </div>

                <!-- Accent Hover Color -->
                <div class="flex items-center gap-3">
                  <input type="color" bind:value={customColors['color-accent-hover']} oninput={handleLivePreview} class="w-9 h-9 rounded border border-brand-border cursor-pointer bg-transparent shrink-0" />
                  <div class="flex flex-col">
                    <span class="text-xs font-semibold text-brand-text-primary">Accent Hover</span>
                    <span class="text-[10px] text-brand-text-secondary/50">accent-hover</span>
                  </div>
                </div>

                <!-- Primary Text -->
                <div class="flex items-center gap-3">
                  <input type="color" bind:value={customColors['color-text-primary']} oninput={handleLivePreview} class="w-9 h-9 rounded border border-brand-border cursor-pointer bg-transparent shrink-0" />
                  <div class="flex flex-col">
                    <span class="text-xs font-semibold text-brand-text-primary">Primary Text</span>
                    <span class="text-[10px] text-brand-text-secondary/50">text-primary</span>
                  </div>
                </div>

                <!-- Secondary Text -->
                <div class="flex items-center gap-3">
                  <input type="color" bind:value={customColors['color-text-secondary']} oninput={handleLivePreview} class="w-9 h-9 rounded border border-brand-border cursor-pointer bg-transparent shrink-0" />
                  <div class="flex flex-col">
                    <span class="text-xs font-semibold text-brand-text-primary">Secondary Text</span>
                    <span class="text-[10px] text-brand-text-secondary/50">text-secondary</span>
                  </div>
                </div>

                <!-- Border Color -->
                <div class="flex items-center gap-3">
                  <input type="color" bind:value={customColors['color-border']} oninput={handleLivePreview} class="w-9 h-9 rounded border border-brand-border cursor-pointer bg-transparent shrink-0" />
                  <div class="flex flex-col">
                    <span class="text-xs font-semibold text-brand-text-primary">Borders</span>
                    <span class="text-[10px] text-brand-text-secondary/50">color-border</span>
                  </div>
                </div>
              </div>

              <div class="flex items-center gap-3 pt-3 border-t border-brand-border">
                <button
                  onclick={saveCustomTheme}
                  class="bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast px-4 py-2 rounded-lg text-xs font-semibold transition-all shadow-md shadow-brand-accent/10 cursor-pointer"
                >
                  {editingThemeId ? 'Save Changes' : 'Save Custom Theme'}
                </button>
                <span class="text-[10px] text-brand-text-secondary/50">Colors update the app instantly as you pick them!</span>
              </div>
            </div>
          {/if}
        </div>
      </div>
    {:else if settingsTab === "equalizer"}
      <!-- Equalizer Section -->
      <div class="space-y-6">
        <Equalizer />
      </div>
    {:else if settingsTab === "formats"}
      <!-- File Formats Filter Section -->
      <div class="space-y-6">
        <div>
          <h3 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase mb-1">Filter Library by File Format</h3>
          <p class="text-xs text-brand-text-secondary/60 mb-4">Uncheck formats you want to exclude from your library and playlists.</p>
          
          <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
            {#each ["MP3", "FLAC", "WAV", "AAC", "ALAC", "OGG", "AIFF", "APE"] as format}
              {@const isChecked = !collectionStore.excludedFormats.includes(format)}
              <button
                onclick={() => collectionStore.toggleFormat(format)}
                class="bg-brand-sidebar/40 border rounded-xl p-4 flex items-center justify-between transition-all duration-200 hover:border-brand-accent/40 cursor-pointer w-full text-left {isChecked ? 'border-brand-accent bg-brand-sidebar/60' : 'border-brand-border'}"
              >
                <div class="flex flex-col">
                  <span class="font-semibold text-sm text-brand-text-primary">{format}</span>
                  <span class="text-[10px] text-brand-text-secondary/50 mt-0.5">
                    {isChecked ? 'Enabled' : 'Excluded'}
                  </span>
                </div>
                <div class="w-5 h-5 rounded border flex items-center justify-center transition-colors {isChecked ? 'bg-brand-accent border-brand-accent text-brand-accent-contrast' : 'border-brand-border bg-black/10'}">
                  {#if isChecked}
                    <Check class="w-3 h-3 stroke-[3]" />
                  {/if}
                </div>
              </button>
            {/each}
          </div>
        </div>
      </div>
    {/if}
  </div>
</div>
