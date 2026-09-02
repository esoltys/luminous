<script lang="ts">
  import { themeStore, PREDEFINED_THEMES, LUMINOUS_DARK_COLORS, LUMINOUS_LIGHT_COLORS, type ThemeColors, type Theme, type ColorSchemeMode } from "../stores/theme.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { onDestroy } from "svelte";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import { Palette, Trash2, RotateCcw, Sun, Moon, SlidersHorizontal } from "lucide-svelte";

  const COLOR_SCHEME_MODE_OPTIONS: { mode: ColorSchemeMode; labelKey: string; icon: typeof Sun }[] = [
    { mode: "light", labelKey: "colorSchemeModeLight", icon: Sun },
    { mode: "dark", labelKey: "colorSchemeModeDark", icon: Moon },
    { mode: "system", labelKey: "colorSchemeModeSystem", icon: SlidersHorizontal }
  ];

  let editingThemeId = $state<string | null>(null);

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

  const DYNAMIC_THEMES = PREDEFINED_THEMES.filter((t) => t.id === "dynamic-artwork" || t.id === "system");
  const STATIC_PREDEFINED_THEMES = PREDEFINED_THEMES.filter((t) => t.id !== "dynamic-artwork" && t.id !== "system");

  // The System theme's live colors depend on the OS light/dark preference,
  // not the static (dark) preview baked into its PREDEFINED_THEMES entry —
  // use the current system scheme so its swatch matches what's on screen.
  function getPreviewColors(theme: Theme): ThemeColors {
    if (theme.id === "system") {
      return themeStore.effectiveColorScheme === "dark" ? LUMINOUS_DARK_COLORS : LUMINOUS_LIGHT_COLORS;
    }
    if (theme.id === "dynamic-artwork") {
      return themeStore.resolvedColors;
    }
    return theme.colors;
  }

  function loadActiveThemeColors() {
    if (editingTheme) {
      customColors = { ...editingTheme.colors };
    } else {
      customColors = { ...themeStore.resolvedColors };
    }
  }

  // Pre-fill theme builder with current active theme colors on mount and
  // updates — skipped while editing an existing custom theme, since that
  // case is seeded from the theme being edited instead (below).
  $effect(() => {
    const colors = themeStore.resolvedColors;
    if (!editingThemeId) {
      customColors = { ...colors };
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

  $effect(() => {
    if (customColors) {
      // deep read to trigger reactivity
      const _ = customColors["bg-main"] + customColors["bg-sidebar"] + customColors["bg-playerbar"] + customColors["color-accent"] + customColors["color-accent-hover"] + customColors["color-border"];
      themeStore.applyThemeColorsPreview(customColors);
    }
  });

  // Revert to the actually-active theme when this tab is left (unmounted on
  // tab switch) or the app closes while it's open.
  onDestroy(() => {
    themeStore.applyActiveTheme();
  });

  async function saveCustomTheme() {
    if (newThemeName.trim() === "") {
      toastStore.show(i18n.t('settings.enterThemeNameAlert'));
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

<div class="space-y-6">
  <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-6">
  <div class="pb-1 flex justify-between items-center">
    <div class="flex items-center gap-3">
      <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
        <Palette class="w-5 h-5" />
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.tabThemes')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">{i18n.t('settings.themesSubtitle', {}, 'Customize the visual appearance and colours of Luminous.')}</p>
      </div>
    </div>
  </div>

  <div>
    <h4 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase mb-3">{i18n.t('settings.dynamicThemes', {}, 'Dynamic Themes')}</h4>
    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">
      {#each DYNAMIC_THEMES as theme}
        {@const previewColors = getPreviewColors(theme)}
        {#if theme.id === 'system'}
          <!-- Not a single <button>: the segmented Light/Dark/System control
               below is itself interactive, so this card uses an inner
               button (selects the System theme) plus a separate control
               region, rather than nesting buttons. -->
          <div class="bg-brand-main/50 border-2 rounded-xl p-4 flex flex-col items-start gap-3 text-left transition-colors duration-200 w-full relative {themeStore.activeThemeId === theme.id ? 'border-brand-accent shadow-md shadow-brand-accent/5' : 'border-brand-border/60'}">
            <button
              type="button"
              onclick={() => themeStore.setTheme(theme.id)}
              class="flex flex-col items-start gap-3 text-left w-full group"
            >
              <div class="flex items-center justify-between w-full">
                <span class="font-semibold text-sm text-brand-text-primary">
                  {i18n.t('themes.' + theme.id, {}, theme.name)}
                </span>
              </div>
              <!-- Miniature colors preview matching 1-6 Theme Builder archetype order -->
              <div class="flex gap-0.5 w-full h-8 rounded-lg overflow-hidden border border-brand-border/40 bg-black/10">
                <div class="flex-1" style="background-color: {previewColors['bg-main']}" title={i18n.t('settings.mainViewLabel')}></div>
                <div class="flex-1" style="background-color: {previewColors['bg-sidebar']}" title={i18n.t('settings.sidebarLabel')}></div>
                <div class="flex-1" style="background-color: {previewColors['bg-playerbar']}" title={i18n.t('settings.playerBarLabel')}></div>
                <div class="flex-1" style="background-color: {previewColors['color-accent']}" title={i18n.t('settings.accentLabel')}></div>
                <div class="flex-1" style="background-color: {previewColors['color-accent-hover']}" title={i18n.t('settings.accentHoverLabel')}></div>
                <div class="flex-1" style="background-color: {previewColors['color-border']}" title={i18n.t('settings.bordersLabel')}></div>
              </div>
              <span class="text-xs text-brand-text-secondary leading-relaxed">{i18n.t('settings.systemFootnote', {}, 'Switches between light and dark to match your OS')}</span>
            </button>

            <!-- Explicit Light/Dark/System appearance override (#692) — a
                 sub-setting of the System theme, scoped to this card. -->
            <div class="w-full space-y-1.5">
              <span class="text-[10px] text-brand-text-secondary/80 font-semibold tracking-wider uppercase">{i18n.t('settings.colorSchemeModeLabel', {}, 'Select Theme')}</span>
              <div
                class="flex items-center gap-0.5 p-0.5 rounded-full border border-brand-border bg-brand-main/40 select-none"
                role="tablist"
                aria-label={i18n.t('settings.colorSchemeModeLabel', {}, 'Select Theme')}
              >
                {#each COLOR_SCHEME_MODE_OPTIONS as opt (opt.mode)}
                  <button
                    type="button"
                    role="tab"
                    aria-selected={themeStore.colorSchemeMode === opt.mode}
                    onclick={() => themeStore.setColorSchemeMode(opt.mode)}
                    class="flex-1 flex items-center justify-center gap-1.5 px-2.5 py-1.5 rounded-full text-[11px] font-semibold whitespace-nowrap transition-colors
                      {themeStore.colorSchemeMode === opt.mode
                      ? 'bg-brand-accent text-brand-accent-contrast shadow-sm'
                      : 'text-brand-text-secondary/70 hover:text-brand-text-primary hover:bg-brand-sidebar'}"
                  >
                    <opt.icon class="w-3.5 h-3.5" />
                    {i18n.t('settings.' + opt.labelKey, {}, opt.mode)}
                  </button>
                {/each}
              </div>
            </div>
          </div>
        {:else}
          <button
            onclick={() => themeStore.setTheme(theme.id)}
            class="bg-brand-main/50 border-2 rounded-xl p-4 flex flex-col items-start gap-3 text-left transition-colors duration-200 group hover:border-brand-accent/40 w-full relative {themeStore.activeThemeId === theme.id ? 'border-brand-accent shadow-md shadow-brand-accent/5' : 'border-brand-border/60'}"
          >
            <div class="flex items-center justify-between w-full">
              <span class="font-semibold text-sm text-brand-text-primary flex items-center gap-1.5">
                {theme.isCustom ? theme.name : i18n.t('themes.' + theme.id, {}, theme.name)}
              </span>
            </div>
            <!-- Miniature colors preview matching 1-6 Theme Builder archetype order -->
            <div class="flex gap-0.5 w-full h-8 rounded-lg overflow-hidden border border-brand-border/40 bg-black/10">
              <div class="flex-1" style="background-color: {previewColors['bg-main']}" title={i18n.t('settings.mainViewLabel')}></div>
              <div class="flex-1" style="background-color: {previewColors['bg-sidebar']}" title={i18n.t('settings.sidebarLabel')}></div>
              <div class="flex-1" style="background-color: {previewColors['bg-playerbar']}" title={i18n.t('settings.playerBarLabel')}></div>
              <div class="flex-1" style="background-color: {previewColors['color-accent']}" title={i18n.t('settings.accentLabel')}></div>
              <div class="flex-1" style="background-color: {previewColors['color-accent-hover']}" title={i18n.t('settings.accentHoverLabel')}></div>
              <div class="flex-1" style="background-color: {previewColors['color-border']}" title={i18n.t('settings.bordersLabel')}></div>
            </div>
            <span class="text-xs text-brand-text-secondary leading-relaxed">{i18n.t('settings.luminousFootnote', {}, 'Colors shift to match whatever album art is playing now')}</span>
          </button>
        {/if}
      {/each}
    </div>
  </div>

  <div>
    <h4 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase mb-3">{i18n.t('settings.predefinedThemes', {}, 'Predefined Themes')}</h4>
    <div class="grid grid-cols-1 md:grid-cols-3 lg:grid-cols-5 gap-4">
      {#each STATIC_PREDEFINED_THEMES as theme}
        {@const previewColors = getPreviewColors(theme)}
        <button
          onclick={() => themeStore.setTheme(theme.id)}
          class="bg-brand-main/50 border-2 rounded-xl p-4 flex flex-col items-start gap-3 text-left transition-colors duration-200 group hover:border-brand-accent/40 w-full relative {themeStore.activeThemeId === theme.id ? 'border-brand-accent shadow-md shadow-brand-accent/5' : 'border-brand-border/60'}"
        >
          <div class="flex items-center justify-between w-full">
            <span class="font-semibold text-sm text-brand-text-primary flex items-center gap-1.5">
              {theme.isCustom ? theme.name : i18n.t('themes.' + theme.id, {}, theme.name)}
            </span>
          </div>
          <!-- Miniature colors preview matching 1-6 Theme Builder archetype order -->
          <div class="flex gap-0.5 w-full h-8 rounded-lg overflow-hidden border border-brand-border/40 bg-black/10">
            <div class="flex-1" style="background-color: {previewColors['bg-main']}" title={i18n.t('settings.mainViewLabel')}></div>
            <div class="flex-1" style="background-color: {previewColors['bg-sidebar']}" title={i18n.t('settings.sidebarLabel')}></div>
            <div class="flex-1" style="background-color: {previewColors['bg-playerbar']}" title={i18n.t('settings.playerBarLabel')}></div>
            <div class="flex-1" style="background-color: {previewColors['color-accent']}" title={i18n.t('settings.accentLabel')}></div>
            <div class="flex-1" style="background-color: {previewColors['color-accent-hover']}" title={i18n.t('settings.accentHoverLabel')}></div>
            <div class="flex-1" style="background-color: {previewColors['color-border']}" title={i18n.t('settings.bordersLabel')}></div>
          </div>
        </button>
      {/each}
    </div>
  </div>

  {#if themeStore.customThemes.length > 0}
    <div>
      <h3 class="text-xs text-brand-text-secondary font-bold tracking-wider uppercase mb-3">{i18n.t('settings.customThemes')}</h3>
      <div class="grid grid-cols-1 md:grid-cols-3 lg:grid-cols-5 gap-4">
        {#each themeStore.customThemes as theme}
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            onclick={() => themeStore.setTheme(theme.id)}
            role="button"
            tabindex="0"
            class="bg-brand-main/50 border-2 rounded-xl p-4 flex flex-col gap-3 text-left transition-colors w-full {themeStore.activeThemeId === theme.id ? 'border-brand-accent shadow-md shadow-brand-accent/5' : 'border-brand-border/60 hover:border-brand-border'}"
          >
            <div class="flex items-center justify-between w-full">
              <span class="font-semibold text-sm text-brand-text-primary truncate">{theme.name}</span>
            </div>
            <!-- Miniature colors preview matching 1-6 Theme Builder archetype order -->
            <div class="flex gap-0.5 w-full h-8 rounded-lg overflow-hidden border border-brand-border/40 bg-black/10">
              <div class="flex-1" style="background-color: {theme.colors['bg-main']}" title={i18n.t('settings.mainViewLabel')}></div>
              <div class="flex-1" style="background-color: {theme.colors['bg-sidebar']}" title={i18n.t('settings.sidebarLabel')}></div>
              <div class="flex-1" style="background-color: {theme.colors['bg-playerbar']}" title={i18n.t('settings.playerBarLabel')}></div>
              <div class="flex-1" style="background-color: {theme.colors['color-accent']}" title={i18n.t('settings.accentLabel')}></div>
              <div class="flex-1" style="background-color: {theme.colors['color-accent-hover']}" title={i18n.t('settings.accentHoverLabel')}></div>
              <div class="flex-1" style="background-color: {theme.colors['color-border']}" title={i18n.t('settings.bordersLabel')}></div>
            </div>
            <div class="flex gap-2">
              <button
                onclick={(e) => { e.stopPropagation(); editingThemeId = theme.id; }}
                class="flex-1 px-2 py-1 rounded text-xs font-semibold bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast transition-colors"
                title={i18n.t('settings.editTheme')}
              >
                {i18n.t('settings.editThemeShort')}
              </button>
              <button
                onclick={(e) => { e.stopPropagation(); themeStore.deleteCustomTheme(theme.id); }}
                class="p-1 rounded bg-brand-main hover:bg-red-950/20 text-brand-text-secondary hover:text-red-400 border border-brand-border hover:border-red-900/30 transition-colors"
                title={i18n.t('settings.deleteTheme')}
              >
                <Trash2 class="w-3.5 h-3.5" />
              </button>
            </div>
          </div>
        {/each}
      </div>
    </div>
  {/if}
  </div>

  <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
    <div class="flex items-center justify-between border-b border-brand-border pb-3">
      <div class="flex items-center gap-3">
        <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
          <Palette class="w-5 h-5" />
        </div>
        <h4 class="font-bold text-sm text-brand-text-primary">
          {editingTheme ? i18n.t('settings.editingThemeTitle', { name: editingTheme.name }) : i18n.t('settings.customThemeBuilderTitle')}
        </h4>
      </div>
      {#if editingThemeId}
        <Button onclick={() => { editingThemeId = null; themeStore.applyActiveTheme(); }} variant="secondary" size="sm">
          {i18n.t('settings.cancel')}
        </Button>
      {/if}
    </div>

    <div class="space-y-5">
        <div class="flex flex-col md:flex-row gap-4 items-end justify-between">
          <div class="flex flex-col gap-1.5 flex-1 max-w-sm">
            <label for="theme-name-input" class="text-xs text-brand-text-secondary font-semibold">{i18n.t('settings.themeNameLabel')}</label>
            <Input
              id="theme-name-input"
              type="text"
              bind:value={newThemeName}
              placeholder={i18n.t('settings.themeNamePlaceholder')}
              size="md"
              class="w-full"
            />
          </div>
          <Button onclick={loadActiveThemeColors} variant="secondary" size="sm" class="shrink-0 h-9">
            <RotateCcw class="w-4 h-4 text-brand-accent-text" /> {i18n.t('settings.importColors')}
          </Button>
        </div>

        <div class="grid grid-cols-2 md:grid-cols-3 gap-6 pt-2">
          <!-- Dark Muted (Main Background) -->
          <div class="flex items-center gap-3">
            <div class="flex items-center rounded border border-brand-border bg-brand-main overflow-hidden shrink-0">
              <input type="color" bind:value={customColors['bg-main']} class="w-9 h-9 bg-transparent border-none shrink-0" />
              <input type="text" bind:value={customColors['bg-main']} maxlength="7" class="w-16 h-9 px-2 text-[11px] bg-transparent text-brand-text-primary outline-none font-mono uppercase" />
            </div>
            <div class="flex flex-col min-w-0">
              <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.mainViewLabel')}</span>
              <span class="text-[10px] text-brand-text-secondary font-medium">{i18n.t('settings.mainViewDescription')}</span>
            </div>
          </div>

          <!-- Dark Vibrant (Sidebar Background) -->
          <div class="flex items-center gap-3">
            <div class="flex items-center rounded border border-brand-border bg-brand-main overflow-hidden shrink-0">
              <input type="color" bind:value={customColors['bg-sidebar']} class="w-9 h-9 bg-transparent border-none shrink-0" />
              <input type="text" bind:value={customColors['bg-sidebar']} maxlength="7" class="w-16 h-9 px-2 text-[11px] bg-transparent text-brand-text-primary outline-none font-mono uppercase" />
            </div>
            <div class="flex flex-col min-w-0">
              <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.sidebarLabel')}</span>
              <span class="text-[10px] text-brand-text-secondary font-medium">{i18n.t('settings.sidebarDescription')}</span>
            </div>
          </div>

          <!-- Light Muted (Player Bar Background) -->
          <div class="flex items-center gap-3">
            <div class="flex items-center rounded border border-brand-border bg-brand-main overflow-hidden shrink-0">
              <input type="color" bind:value={customColors['bg-playerbar']} class="w-9 h-9 bg-transparent border-none shrink-0" />
              <input type="text" bind:value={customColors['bg-playerbar']} maxlength="7" class="w-16 h-9 px-2 text-[11px] bg-transparent text-brand-text-primary outline-none font-mono uppercase" />
            </div>
            <div class="flex flex-col min-w-0">
              <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.playerBarLabel')}</span>
              <span class="text-[10px] text-brand-text-secondary font-medium">{i18n.t('settings.playerBarDescription')}</span>
            </div>
          </div>

          <!-- Vibrant (Accent Color) -->
          <div class="flex items-center gap-3">
            <div class="flex items-center rounded border border-brand-border bg-brand-main overflow-hidden shrink-0">
              <input type="color" bind:value={customColors['color-accent']} class="w-9 h-9 bg-transparent border-none shrink-0" />
              <input type="text" bind:value={customColors['color-accent']} maxlength="7" class="w-16 h-9 px-2 text-[11px] bg-transparent text-brand-text-primary outline-none font-mono uppercase" />
            </div>
            <div class="flex flex-col min-w-0">
              <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.accentLabel')}</span>
              <span class="text-[10px] text-brand-text-secondary font-medium">{i18n.t('settings.accentDescription')}</span>
            </div>
          </div>

          <!-- Light Vibrant (Accent Hover Color) -->
          <div class="flex items-center gap-3">
            <div class="flex items-center rounded border border-brand-border bg-brand-main overflow-hidden shrink-0">
              <input type="color" bind:value={customColors['color-accent-hover']} class="w-9 h-9 bg-transparent border-none shrink-0" />
              <input type="text" bind:value={customColors['color-accent-hover']} maxlength="7" class="w-16 h-9 px-2 text-[11px] bg-transparent text-brand-text-primary outline-none font-mono uppercase" />
            </div>
            <div class="flex flex-col min-w-0">
              <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.accentHoverLabel')}</span>
              <span class="text-[10px] text-brand-text-secondary font-medium">{i18n.t('settings.accentHoverDescription')}</span>
            </div>
          </div>

          <!-- Muted (Border Color) -->
          <div class="flex items-center gap-3">
            <div class="flex items-center rounded border border-brand-border bg-brand-main overflow-hidden shrink-0">
              <input type="color" bind:value={customColors['color-border']} class="w-9 h-9 bg-transparent border-none shrink-0" />
              <input type="text" bind:value={customColors['color-border']} maxlength="7" class="w-16 h-9 px-2 text-[11px] bg-transparent text-brand-text-primary outline-none font-mono uppercase" />
            </div>
            <div class="flex flex-col min-w-0">
              <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('settings.bordersLabel')}</span>
              <span class="text-[10px] text-brand-text-secondary font-medium">{i18n.t('settings.bordersDescription')}</span>
            </div>
          </div>
        </div>

        <div class="flex flex-wrap items-center gap-3 pt-3 border-t border-brand-border">
          <Button onclick={saveCustomTheme} variant="primary" size="sm">
            {editingThemeId ? i18n.t('settings.saveChanges') : i18n.t('settings.saveCustom')}
          </Button>
        </div>
      </div>
  </div>
</div>
