<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import { picardStore } from "../stores/picard.svelte";
  import { scrobblerStore, DEFAULT_DISCORD_CLIENT_ID } from "../stores/scrobbler.svelte";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import { isWindows } from "../platform";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import Toggle from "./Toggle.svelte";
  import {
    CheckIcon as Check,
    EyeIcon as Eye,
    EyeSlashIcon as EyeOff,
    WarningIcon as AlertTriangle,
    ArrowsClockwiseIcon as RefreshCw,
    FolderOpenIcon as FolderOpen,
    CircleNotchIcon as LoaderCircle,
    ArrowUpRightIcon as ArrowUpRight,
    HeartIcon as Heart,
    BookOpenIcon as Globe,
    DiscordLogoIcon as DiscordLogo
  } from "phosphor-svelte";

  let showListenBrainzToken = $state(false);
  let picardCustomPath = $state("");
  let isRecheckingPicard = $state(false);
  let contextEnrichmentEnabled = $state(true);

  async function handleContextEnrichmentToggle(v: boolean) {
    contextEnrichmentEnabled = v;
    await invoke("set_app_setting", { key: "context_enrichment_enabled", value: v ? "true" : "false" });
  }

  async function handlePicardCustomPathChange() {
    await invoke("set_app_setting", { key: "picard_path", value: picardCustomPath.trim() });
    await picardStore.refresh();
  }

  async function handleBrowsePicardPath() {
    const selected = await open({
      multiple: false,
      title: i18n.t("picard.browseBtn"),
      filters: isWindows ? [{ name: "Picard executable", extensions: ["exe"] }] : undefined,
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
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      picardCustomPath = settings?.picard_path ?? "";
      contextEnrichmentEnabled = settings?.context_enrichment_enabled !== "false";
    } catch (e) {
      console.error("Failed to load Picard custom path on mount:", e);
    }
  });
</script>

<!-- Online Data Sources (Context & Bio Enrichment) Integration Card -->
<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4">
  <div class="pb-3 flex justify-between items-center">
    <div class="flex items-center gap-3">
      <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
        <Globe class="w-5 h-5" />
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.contextEnrichmentIntegrationTitle')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">{i18n.t('settings.contextEnrichmentDesc')}</p>
      </div>
    </div>
  </div>

  <div class="flex items-center justify-between gap-4 py-1">
    <div class="flex flex-col gap-0.5 min-w-0">
      <span class="text-sm font-medium text-brand-text-primary">{i18n.t('settings.contextEnrichmentLabel')}</span>
      <p class="text-xs text-brand-text-secondary">{i18n.t('settings.contextEnrichmentHint')}</p>
    </div>
    <Toggle
      checked={contextEnrichmentEnabled}
      onchange={(v) => handleContextEnrichmentToggle(v)}
      label={i18n.t('settings.contextEnrichmentLabel')}
    />
  </div>
</div>

<!-- ListenBrainz Scrobbler Integration Card -->
<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
  <div class="pb-3 flex justify-between items-start gap-4">
    <div class="flex items-center gap-3 min-w-0">
      <img src="/listenbrainz-icon.png" alt="ListenBrainz" class="w-9 h-9 shrink-0 object-contain" />
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('listenbrainz.integrationTitle')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">
          <button onclick={() => openExternalUrl("https://listenbrainz.org")} class="text-brand-accent hover:underline">ListenBrainz</button>
          {i18n.t('listenbrainz.integrationDesc')}
        </p>
      </div>
    </div>
    {#if scrobblerStore.enabled && scrobblerStore.username}
      <span class="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full bg-brand-accent/15 text-brand-text-primary border border-brand-accent/25 font-medium shrink-0">
        <Check class="w-3 h-3" />
        {scrobblerStore.username}
      </span>
    {:else if scrobblerStore.enabled && scrobblerStore.paused}
      <span class="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full bg-amber-500/15 text-amber-400 font-medium shrink-0">
        {i18n.t('listenbrainz.statusPaused')}
      </span>
    {/if}
  </div>

  <div class="space-y-2 pt-2 border-t border-brand-border/60">
    <div class="flex items-center justify-between">
      <label for="listenbrainz-token-input" class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider">
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
          value={scrobblerStore.token}
          oninput={(e) => scrobblerStore.setToken((e.target as HTMLInputElement).value)}
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
      <div class="flex items-start gap-2 text-xs text-amber-500 pt-1">
        <AlertTriangle class="w-3.5 h-3.5 shrink-0 translate-y-[calc((1lh-0.875rem)/2)]" />
        <span>{scrobblerStore.validationError}</span>
      </div>
    {/if}
  </div>

  {#if scrobblerStore.username}
    <div class="flex items-center justify-between gap-4 py-1 pt-2 border-t border-brand-border/60">
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

        {#if scrobblerStore.ratingsEnabled}
          <div class="ml-2 pl-3 border-l-2 border-brand-accent/30 flex flex-wrap items-center justify-between gap-3 py-1">
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('listenbrainz.syncFavouritesLabel')}</span>
              <p class="text-[11px] text-brand-text-secondary">{i18n.t('listenbrainz.syncFavouritesHint')}</p>
              {#if scrobblerStore.syncFavouritesResult}
                <p class="text-[11px] text-brand-text-primary font-medium">
                  {i18n.t('listenbrainz.syncFavouritesSuccess', {
                    synced: scrobblerStore.syncFavouritesResult.synced,
                    total: scrobblerStore.syncFavouritesResult.total_favourites,
                    skipped: scrobblerStore.syncFavouritesResult.skipped_no_mbid
                  })}
                </p>
              {:else if scrobblerStore.syncFavouritesError}
                <p class="text-[11px] text-brand-text-primary font-medium">{scrobblerStore.syncFavouritesError}</p>
              {/if}
            </div>
            <Button
              variant="secondary"
              size="sm"
              onclick={() => scrobblerStore.syncFavourites()}
              disabled={scrobblerStore.isSyncingFavourites}
              class="gap-1.5 shrink-0"
            >
              {#if scrobblerStore.isSyncingFavourites}
                <LoaderCircle class="w-3.5 h-3.5 animate-spin" />
                <span>{i18n.t('listenbrainz.syncingFavouritesBtn')}</span>
              {:else}
                <Heart weight="fill" class="w-3.5 h-3.5 text-rose-400" />
                <span>{i18n.t('listenbrainz.syncFavouritesBtn')}</span>
              {/if}
            </Button>
          </div>
        {/if}

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
              <span class="text-xs text-brand-text-primary font-medium">({scrobblerStore.flushSuccessMessage})</span>
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
  {/if}
</div>

<!-- Discord Rich Presence Card -->
<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4">
  <div class="pb-3 flex justify-between items-center border-b border-brand-border/60">
    <div class="flex items-center gap-3">
      <div class="w-9 h-9 rounded-lg bg-[#5865F2]/15 flex items-center justify-center text-[#5865F2] shrink-0">
        <DiscordLogo class="w-5 h-5" weight="fill" />
      </div>
      <div class="space-y-1 min-w-0">
        <div class="flex items-center gap-2 flex-wrap">
          <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('discord.integrationTitle')}</h3>
          {#if scrobblerStore.discordEnabled}
            {#if scrobblerStore.discordStatus === 'connected'}
              <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-medium bg-emerald-500/15 text-emerald-600 dark:text-emerald-400">
                <span class="w-1.5 h-1.5 rounded-full bg-emerald-500"></span>
                {i18n.t('discord.statusConnected')}
              </span>
            {:else if scrobblerStore.discordStatus === 'not_running'}
              <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-medium bg-amber-500/15 text-amber-600 dark:text-amber-400">
                <AlertTriangle class="w-3 h-3" />
                {i18n.t('discord.statusNotRunning')}
              </span>
            {:else}
              <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[11px] font-medium bg-brand-surface text-brand-text-secondary">
                {i18n.t('discord.statusDisconnected')}
              </span>
            {/if}
            <button
              onclick={() => scrobblerStore.checkDiscordStatus()}
              disabled={scrobblerStore.isCheckingDiscord}
              class="text-brand-text-secondary hover:text-brand-accent-text transition-colors disabled:opacity-50"
              title={i18n.t('discord.recheckTooltip')}
            >
              <RefreshCw class="w-3.5 h-3.5 {scrobblerStore.isCheckingDiscord ? 'animate-spin' : ''}" />
            </button>
          {/if}
        </div>
        <p class="text-xs text-brand-text-secondary leading-relaxed">
          <span class="text-brand-text-primary font-medium">Discord Rich Presence</span>
          {" "}{i18n.t('discord.integrationDesc')}
        </p>
      </div>
    </div>
  </div>

  <div class="flex items-center justify-between gap-4 py-1">
    <div class="flex flex-col gap-0.5 min-w-0">
      <span class="text-sm font-medium text-brand-text-primary">{i18n.t('discord.enableLabel')}</span>
      <p class="text-xs text-brand-text-secondary">{i18n.t('discord.enableHint')}</p>
    </div>
    <Toggle
      checked={scrobblerStore.discordEnabled}
      onchange={(v) => scrobblerStore.setDiscordEnabled(v)}
      label={i18n.t('discord.enableLabel')}
    />
  </div>

  {#if scrobblerStore.discordEnabled}
    <div class="space-y-3 pt-3 border-t border-brand-border/60">
      <div class="flex items-center justify-between gap-4 py-1">
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-sm font-medium text-brand-text-primary">{i18n.t('discord.showAlbumLabel')}</span>
          <p class="text-xs text-brand-text-secondary">{i18n.t('discord.showAlbumHint')}</p>
        </div>
        <Toggle
          checked={scrobblerStore.discordShowAlbum}
          onchange={(v) => scrobblerStore.setDiscordShowAlbum(v)}
          label={i18n.t('discord.showAlbumLabel')}
        />
      </div>

      <div class="flex items-center justify-between gap-4 py-1">
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-sm font-medium text-brand-text-primary">{i18n.t('discord.showTimeLabel')}</span>
          <p class="text-xs text-brand-text-secondary">{i18n.t('discord.showTimeHint')}</p>
        </div>
        <Toggle
          checked={scrobblerStore.discordShowTime}
          onchange={(v) => scrobblerStore.setDiscordShowTime(v)}
          label={i18n.t('discord.showTimeLabel')}
        />
      </div>

      <div class="flex items-center justify-between gap-4 py-1 pt-2 border-t border-brand-border/60">
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-xs font-medium text-brand-text-primary">{i18n.t('discord.applicationIdLabel')}</span>
          <p class="text-xs text-brand-text-secondary">{i18n.t('discord.applicationIdHint')}</p>
        </div>
        <span class="text-xs bg-brand-surface px-2.5 py-1 rounded-md border border-brand-border text-brand-text-secondary select-all">
          {DEFAULT_DISCORD_CLIENT_ID}
        </span>
      </div>
    </div>
  {/if}
</div>

<!-- MusicBrainz Picard Card -->
<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4">
  <div class="pb-3 flex justify-between items-center">
    <div class="flex items-center gap-3">
      <img src="/picard-icon.png" alt="Picard" class="w-9 h-9 shrink-0 object-contain" />
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('picard.integrationTitle')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">
          <button onclick={() => openExternalUrl("https://picard.musicbrainz.org")} class="text-brand-accent hover:underline">MusicBrainz Picard</button>
          {i18n.t('picard.integrationDesc')}
        </p>
      </div>
    </div>
  </div>

  <div class="flex items-center justify-between gap-4 py-1">
    <div class="flex flex-col gap-0.5 min-w-0">
      <span class="text-sm font-medium text-brand-text-primary">{i18n.t('picard.missingPlaylistLabel')}</span>
      <p class="text-xs text-brand-text-secondary">{i18n.t('picard.missingPlaylistHint')}</p>
    </div>
    <Toggle
      checked={picardStore.missingPlaylistEnabled}
      onchange={(v) => picardStore.setMissingPlaylistEnabled(v)}
      label={i18n.t('picard.missingPlaylistLabel')}
    />
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
    <label for="picard-custom-path-input" class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider">
      {i18n.t('picard.customPathLabel')}
    </label>
    <div class="flex items-center gap-2">
      <div class="max-w-md flex-1">
        <Input
          id="picard-custom-path-input"
          type="text"
          bind:value={picardCustomPath}
          onchange={handlePicardCustomPathChange}
          placeholder={i18n.t(isWindows ? 'picard.customPathPlaceholder' : 'picard.customPathPlaceholderLinux')}
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
