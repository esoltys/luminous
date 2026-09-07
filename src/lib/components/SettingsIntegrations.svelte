<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import { picardStore } from "../stores/picard.svelte";
  import { scrobblerStore } from "../stores/scrobbler.svelte";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import Toggle from "./Toggle.svelte";
  import {
    SparkleIcon as Sparkles,
    CheckIcon as Check,
    EyeIcon as Eye,
    EyeSlashIcon as EyeOff,
    AppWindowIcon as PicardIcon,
    WarningIcon as AlertTriangle,
    ArrowsClockwiseIcon as RefreshCw,
    FolderOpenIcon as FolderOpen,
    BroadcastIcon as Broadcast,
    CircleNotchIcon as LoaderCircle,
    ArrowUpRightIcon as ArrowUpRight
  } from "phosphor-svelte";

  let showAcoustidKey = $state(false);
  let showListenBrainzToken = $state(false);
  let hasEnvKey = $state(false);
  let picardCustomPath = $state("");
  let isRecheckingPicard = $state(false);

  async function handlePicardCustomPathChange() {
    await invoke("set_app_setting", { key: "picard_path", value: picardCustomPath.trim() });
    await picardStore.refresh();
  }

  async function handleBrowsePicardPath() {
    const selected = await open({
      multiple: false,
      title: i18n.t("picard.browseBtn"),
      filters: [{ name: "Picard executable", extensions: ["exe"] }],
    });
    if (selected && typeof selected === "string") {
      picardCustomPath = selected;
      await handlePicardCustomPathChange();
    }
  }

  async function handleRecheckPicard() {
    isRecheckingPicard = true;
    try {
      await picardStore.refresh();
    } finally {
      isRecheckingPicard = false;
    }
  }

  onMount(async () => {
    scrobblerStore.init();
    try {
      hasEnvKey = await invoke("has_acoustid_env_key");
    } catch (e) {
      console.error("Failed to check AcoustID env key on mount:", e);
    }
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      picardCustomPath = settings?.picard_path ?? "";
    } catch (e) {
      console.error("Failed to load Picard custom path on mount:", e);
    }
  });
</script>

<!-- ListenBrainz Scrobbler Integration Card -->
<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
  <div class="pb-3 flex justify-between items-start">
    <div class="flex items-center gap-3">
      <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
        <Broadcast class="w-5 h-5" />
      </div>
      <div class="space-y-1 min-w-0">
        <div class="flex items-center gap-2">
          <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('listenbrainz.integrationTitle')}</h3>
          {#if scrobblerStore.enabled && scrobblerStore.username}
            <span class="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full bg-emerald-500/15 text-emerald-400 font-medium">
              <Check class="w-3 h-3" />
              {scrobblerStore.username}
            </span>
          {:else if scrobblerStore.enabled && scrobblerStore.paused}
            <span class="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full bg-amber-500/15 text-amber-400 font-medium">
              {i18n.t('listenbrainz.statusPaused')}
            </span>
          {/if}
        </div>
        <p class="text-xs text-brand-text-secondary leading-relaxed">
          {i18n.t('listenbrainz.integrationDesc1')}
          <button onclick={() => openExternalUrl("https://listenbrainz.org")} class="text-brand-accent hover:underline">ListenBrainz</button>
          {i18n.t('listenbrainz.integrationDesc2')}
        </p>
      </div>
    </div>
  </div>

  <div class="flex items-center justify-between gap-4 py-1">
    <div class="flex flex-col gap-0.5 min-w-0">
      <span class="text-sm font-medium text-brand-text-primary">{i18n.t('listenbrainz.enableLabel')}</span>
      <p class="text-xs text-brand-text-secondary">{i18n.t('listenbrainz.enableHint')}</p>
    </div>
    <Toggle
      checked={scrobblerStore.enabled}
      onchange={(v) => scrobblerStore.setEnabled(v)}
      label={i18n.t('listenbrainz.enableLabel')}
    />
  </div>

  <div class="space-y-2 pt-2 border-t border-brand-border/60">
    <div class="flex items-center justify-between">
      <label for="listenbrainz-token-input" class="text-xs font-semibold text-brand-text-secondary uppercase tracking-wider">
        {i18n.t('listenbrainz.userTokenLabel')}
      </label>
      <button
        type="button"
        onclick={() => openExternalUrl("https://listenbrainz.org/profile/")}
        class="text-xs text-brand-accent hover:underline inline-flex items-center gap-1"
      >
        {i18n.t('listenbrainz.getTokenLink')}
        <ArrowUpRight class="w-3 h-3" />
      </button>
    </div>

    <div class="flex items-center gap-2">
      <div class="relative flex-1 max-w-md">
        <Input
          id="listenbrainz-token-input"
          type={showListenBrainzToken ? "text" : "password"}
          bind:value={scrobblerStore.token}
          onchange={() => scrobblerStore.saveSettings()}
          placeholder={i18n.t('listenbrainz.userTokenPlaceholder')}
          class="w-full pr-10"
        />
        <button
          type="button"
          onclick={() => showListenBrainzToken = !showListenBrainzToken}
          class="absolute right-3 top-1/2 -translate-y-1/2 text-brand-text-secondary hover:text-brand-text-primary transition-colors"
          title={showListenBrainzToken ? "Hide token" : "Show token"}
        >
          {#if showListenBrainzToken}
            <EyeOff class="w-4 h-4" />
          {:else}
            <Eye class="w-4 h-4" />
          {/if}
        </button>
      </div>

      <Button
        onclick={() => scrobblerStore.validateToken()}
        disabled={scrobblerStore.isValidating || !scrobblerStore.token?.trim()}
        variant="secondary"
        size="sm"
      >
        {#if scrobblerStore.isValidating}
          <LoaderCircle class="w-4 h-4 animate-spin" />
        {:else}
          <Check class="w-4 h-4" />
        {/if}
        {i18n.t('listenbrainz.validateBtn')}
      </Button>
    </div>

    {#if scrobblerStore.validationError}
      <div class="flex items-center gap-2 text-xs text-amber-500 pt-1">
        <AlertTriangle class="w-3.5 h-3.5 shrink-0" />
        <span>{scrobblerStore.validationError}</span>
      </div>
    {/if}
  </div>

  {#if scrobblerStore.enabled}
    <div class="space-y-3 pt-3 border-t border-brand-border/60">
      <div class="flex items-center justify-between gap-4 py-1">
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-sm font-medium text-brand-text-primary">{i18n.t('listenbrainz.nowPlayingLabel')}</span>
          <p class="text-xs text-brand-text-secondary">{i18n.t('listenbrainz.nowPlayingHint')}</p>
        </div>
        <Toggle
          checked={scrobblerStore.nowPlayingEnabled}
          onchange={(v) => scrobblerStore.setNowPlayingEnabled(v)}
          label={i18n.t('listenbrainz.nowPlayingLabel')}
        />
      </div>

      <div class="flex items-center justify-between gap-4 py-1">
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-sm font-medium text-brand-text-primary">{i18n.t('listenbrainz.ratingsLabel')}</span>
          <p class="text-xs text-brand-text-secondary">{i18n.t('listenbrainz.ratingsHint')}</p>
        </div>
        <Toggle
          checked={scrobblerStore.ratingsEnabled}
          onchange={(v) => scrobblerStore.setRatingsEnabled(v)}
          label={i18n.t('listenbrainz.ratingsLabel')}
        />
      </div>

      <div class="flex items-center justify-between gap-4 py-1">
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-sm font-medium text-brand-text-primary">{i18n.t('listenbrainz.pauseLabel')}</span>
          <p class="text-xs text-brand-text-secondary">{i18n.t('listenbrainz.pauseHint')}</p>
        </div>
        <Toggle
          checked={scrobblerStore.paused}
          onchange={(v) => scrobblerStore.setPaused(v)}
          label={i18n.t('listenbrainz.pauseLabel')}
        />
      </div>
    </div>

    <!-- Offline cache surface -->
    <div class="pt-3 border-t border-brand-border/60 flex flex-wrap items-center justify-between gap-3">
      <div class="flex flex-col gap-0.5 min-w-0">
        <div class="flex items-center gap-2">
          <span class="text-xs font-semibold text-brand-text-primary">
            {scrobblerStore.pendingCount === 0
              ? i18n.t('listenbrainz.cacheEmpty')
              : i18n.t('listenbrainz.cachePending', { count: scrobblerStore.pendingCount })}
          </span>
          {#if scrobblerStore.flushSuccessMessage}
            <span class="text-xs text-emerald-400 font-medium">({scrobblerStore.flushSuccessMessage})</span>
          {/if}
        </div>
        {#if scrobblerStore.lastError}
          <span class="text-[11px] text-amber-500 truncate" title={scrobblerStore.lastError}>
            {scrobblerStore.lastError}
          </span>
        {:else}
          <span class="text-[11px] text-brand-text-secondary">
            {i18n.t('listenbrainz.cacheDesc')}
          </span>
        {/if}
      </div>

      <Button
        onclick={() => scrobblerStore.flushCache()}
        disabled={scrobblerStore.isFlushing || scrobblerStore.pendingCount === 0 || !scrobblerStore.token.trim()}
        variant="secondary"
        size="sm"
      >
        <RefreshCw class="w-3.5 h-3.5 {scrobblerStore.isFlushing ? 'animate-spin' : ''}" />
        {i18n.t('listenbrainz.syncNowBtn')}
      </Button>
    </div>
  {/if}
</div>

<!-- MusicBrainz Picard Integration Card -->
<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4">
  <div class="pb-3 flex justify-between items-center">
    <div class="flex items-center gap-3">
      <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
        <PicardIcon class="w-5 h-5" />
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('picard.integrationTitle')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">
          {i18n.t('picard.integrationDesc1')} <button onclick={() => openExternalUrl("https://picard.musicbrainz.org")} class="text-brand-accent hover:underline">MusicBrainz Picard</button> {i18n.t('picard.integrationDesc2')}
        </p>
      </div>
    </div>
  </div>

  <div class="flex items-center gap-2 text-xs font-medium">
    {#if picardStore.available}
      <Check class="w-3.5 h-3.5 text-brand-accent-text shrink-0" />
      <span class="text-brand-accent-text truncate" title={picardStore.path ?? undefined}>
        {i18n.t('picard.foundAt', { path: picardStore.path ?? '' })}
      </span>
    {:else}
      <AlertTriangle class="w-3.5 h-3.5 text-amber-500 shrink-0" />
      <span class="text-brand-text-secondary">{i18n.t('picard.notFound')}</span>
    {/if}
    <button
      onclick={handleRecheckPicard}
      disabled={isRecheckingPicard}
      class="ml-1 text-brand-text-secondary hover:text-brand-accent-text transition-colors disabled:opacity-50"
      title={i18n.t('picard.recheckTooltip')}
    >
      <RefreshCw class="w-3.5 h-3.5 {isRecheckingPicard ? 'animate-spin' : ''}" />
    </button>
  </div>

  <div class="flex flex-col gap-1.5">
    <label for="picard-custom-path-input" class="text-xs font-semibold text-brand-text-secondary uppercase tracking-wider">
      {i18n.t('picard.customPathLabel')}
    </label>
    <div class="flex items-center gap-2">
      <div class="max-w-md flex-1">
        <Input
          id="picard-custom-path-input"
          type="text"
          bind:value={picardCustomPath}
          onchange={handlePicardCustomPathChange}
          placeholder={i18n.t('picard.customPathPlaceholder')}
          class="w-full"
        />
      </div>
      <Button onclick={handleBrowsePicardPath} variant="secondary" size="sm">
        <FolderOpen class="w-4 h-4" />
        {i18n.t('picard.browseBtn')}
      </Button>
    </div>
  </div>
</div>

<!-- AcoustID Integration Card -->
<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4">
  <div class="pb-3 flex justify-between items-center">
    <div class="flex items-center gap-3">
      <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
        <Sparkles class="w-5 h-5" />
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.acoustidIntegration')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">
          {i18n.t('settings.acoustidDesc1')}<button onclick={() => openExternalUrl("https://acoustid.org")} class="text-brand-accent hover:underline">AcoustID</button>{i18n.t('settings.acoustidDesc2')}
          <br />
          {i18n.t('settings.acoustidDesc3')}<button onclick={() => { window.location.hash = '#help'; }} class="text-brand-accent hover:underline">{i18n.t('settings.acoustidUserGuide')}</button>{i18n.t('settings.acoustidDesc4')}
        </p>
      </div>
    </div>
  </div>

  <div>
    {#if hasEnvKey}
      <div class="mb-4 text-xs font-medium text-brand-accent-text flex items-center gap-2">
        <Check class="w-3.5 h-3.5" />
        <span>{i18n.t('settings.acoustidEnvKeyFound', { env: 'ACOUSTID_API_KEY' })}</span>
      </div>
    {/if}

    <div class="flex items-center gap-3 max-w-md">
      <div class="relative flex-1">
        <Input
          type={showAcoustidKey ? "text" : "password"}
          bind:value={prefs.acoustidApiKey}
          onchange={() => prefs.setAcoustidApiKey(prefs.acoustidApiKey)}
          placeholder={i18n.t('settings.acoustidPlaceholder')}
          class="w-full pr-10"
        />
        <button
          type="button"
          onclick={() => showAcoustidKey = !showAcoustidKey}
          class="absolute right-3 top-1/2 -translate-y-1/2 text-brand-text-secondary hover:text-brand-text-primary transition-colors"
          title={showAcoustidKey ? "Hide key" : "Show key"}
        >
          {#if showAcoustidKey}
            <EyeOff class="w-4 h-4" />
          {:else}
            <Eye class="w-4 h-4" />
          {/if}
        </button>
      </div>
    </div>
  </div>
</div>
