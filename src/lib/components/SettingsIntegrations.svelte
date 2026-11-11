<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import { picardStore } from "../stores/picard.svelte";
  import { musicbrainzStore } from "../stores/musicbrainz.svelte";
  import { scrobblerStore } from "../stores/scrobbler.svelte";
  import MusicBrainzLoginModal from "./MusicBrainzLoginModal.svelte";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import { isWindows } from "../platform";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import Toggle from "./Toggle.svelte";
  import InlineMessage from "./InlineMessage.svelte";
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
    WifiHighIcon as WifiHigh,
    WifiSlashIcon as WifiSlash,
    DiscordLogoIcon as DiscordLogo
  } from "phosphor-svelte";

  let showListenBrainzToken = $state(false);
  let showFanartKey = $state(false);
  let hasFanartEnvKey = $state(false);
  let fanartKeyStatus = $state<{ has_env_key: boolean; has_build_key: boolean; has_custom_key: boolean }>({
    has_env_key: false,
    has_build_key: false,
    has_custom_key: false,
  });
  let fanartKeyInput = $state("");
  let isValidatingFanartKey = $state(false);
  let fanartValidationError = $state<string | null>(null);
  let picardCustomPath = $state("");
  let isRecheckingPicard = $state(false);
  let showMusicBrainzLoginModal = $state(false);

  async function refreshFanartKeyStatus() {
    try {
      fanartKeyStatus = await invoke("get_fanart_key_status");
      hasFanartEnvKey = fanartKeyStatus.has_env_key;
    } catch {
      try {
        hasFanartEnvKey = await invoke("has_fanart_env_key");
      } catch (e) {
        console.error("Failed to check fanart.tv env key:", e);
      }
    }
  }

  async function handleValidateFanartKey() {
    const key = fanartKeyInput.trim();
    if (!key || isValidatingFanartKey) return;
    isValidatingFanartKey = true;
    fanartValidationError = null;
    try {
      await invoke("validate_fanart_api_key", { apiKey: key });
      prefs.setFanartApiKey(key);
      await refreshFanartKeyStatus();
    } catch (err) {
      fanartValidationError = typeof err === "string" ? err : i18n.t("settings.fanartValidateFailed");
    } finally {
      isValidatingFanartKey = false;
    }
  }

  async function handleClearFanartKey() {
    fanartKeyInput = "";
    fanartValidationError = null;
    prefs.setFanartApiKey("");
    await refreshFanartKeyStatus();
  }

  async function handlePicardCustomPathChange() {
    await invoke("set_app_setting", { key: "picard_path", value: picardCustomPath.trim() });
    await picardStore.refresh();
  }

  async function handleBrowsePicardPath() {
    const selected = await open({
      multiple: false,
      title: i18n.t("picard.browseBtn"),
      filters: isWindows ? [{ name: i18n.t("settings.picardExecutableFilter"), extensions: ["exe"] }] : undefined,
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
    } catch (e) {
      console.error("Failed to load Picard custom path on mount:", e);
    }
    await refreshFanartKeyStatus();
    fanartKeyInput = prefs.fanartApiKey;
  });
</script>

<!-- Online / Offline master toggle (#1398) -->
<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6">
  <div class="flex justify-between items-start gap-4">
    <div class="flex items-center gap-3 min-w-0">
      <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
        {#if prefs.onlineEnabled}
          <WifiHigh class="w-5 h-5" />
        {:else}
          <WifiSlash class="w-5 h-5" />
        {/if}
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.contextEnrichmentIntegrationTitle')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">{i18n.t('settings.contextEnrichmentDesc')}</p>
      </div>
    </div>
    <Toggle
      checked={prefs.onlineEnabled}
      onchange={(v) => prefs.setOnlineEnabled(v)}
      label={i18n.t('settings.contextEnrichmentIntegrationTitle')}
      onText={i18n.t('settings.onlineLabel')}
      offText={i18n.t('settings.offlineLabel')}
    />
  </div>
</div>

{#if prefs.onlineEnabled}
<!-- MusicBrainz Account Card -->
<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4">
  <div class="flex justify-between items-center gap-4">
    <div class="flex items-center gap-3 min-w-0">
      <div class="w-9 h-9 rounded-lg bg-[#ba478f]/15 border border-[#ba478f]/25 shrink-0 flex items-center justify-center">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 25 28" class="w-5 h-5">
          <polygon fill="#ba478f" points="12 0 0 7 0 21 12 28 12 0"/>
          <polygon fill="#eb743b" points="13 0 25 7 25 21 13 28 13 0"/>
        </svg>
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">MusicBrainz</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">
          <button onclick={() => openExternalUrl("https://musicbrainz.org")} class="text-brand-accent hover:underline">MusicBrainz</button>
          {i18n.t('musicbrainz.integrationDesc')}
        </p>
      </div>
    </div>
    <div class="flex items-center gap-3 shrink-0">
      {#if musicbrainzStore.isLoggedIn && musicbrainzStore.username}
        <span class="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full bg-brand-accent/15 text-brand-text-primary border border-brand-accent/25 font-medium shrink-0">
          <Check class="w-3 h-3" />
          {musicbrainzStore.username}
        </span>
        <Button
          variant="secondary"
          size="sm"
          onclick={() => musicbrainzStore.logout()}
          class="text-xs text-brand-text-secondary hover:text-brand-text-primary shrink-0"
          aria-label="Disconnect MusicBrainz"
        >
          {i18n.t('common.disconnect', {}, 'Disconnect')}
        </Button>
      {:else}
        <Button
          variant="secondary"
          size="sm"
          onclick={() => (showMusicBrainzLoginModal = true)}
          class="shrink-0"
          aria-label={i18n.t('auth.loginWithMb', {}, 'Log In with MusicBrainz')}
        >
          {i18n.t('common.connect', {}, 'Connect')}
        </Button>
      {/if}
    </div>
  </div>
</div>

{#if showMusicBrainzLoginModal}
  <MusicBrainzLoginModal onClose={() => (showMusicBrainzLoginModal = false)} />
{/if}
{/if}

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

    <div class="flex items-center gap-2 text-xs font-medium pt-1">
      {#if picardStore.available}
        <Check class="w-3.5 h-3.5 text-brand-accent-text shrink-0" />
        <span class="text-brand-accent-text min-w-0 wrap-anywhere">
          {i18n.t('picard.foundAt', { path: picardStore.path ?? '' })}
        </span>
      {:else}
        <AlertTriangle class="w-3.5 h-3.5 text-brand-text-primary shrink-0" />
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
  </div>
</div>

{#if prefs.onlineEnabled}
<!-- ListenBrainz Scrobbler Integration Card -->
<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
  <div class="flex justify-between items-center gap-4">
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
    <div class="flex items-center gap-3 shrink-0">
      {#if scrobblerStore.username}
        <span class="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full bg-brand-accent/15 text-brand-text-primary border border-brand-accent/25 font-medium shrink-0">
          <Check class="w-3 h-3" />
          {scrobblerStore.username}
        </span>
        <Button
          variant="secondary"
          size="sm"
          onclick={() => scrobblerStore.disconnectListenBrainz()}
          class="text-xs text-brand-text-secondary hover:text-brand-text-primary shrink-0"
        >
          {i18n.t('common.disconnect', {}, 'Disconnect')}
        </Button>
      {/if}
    </div>
  </div>

  {#if !scrobblerStore.username}
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
            title={showListenBrainzToken ? i18n.t("settings.hideToken") : i18n.t("settings.showToken")}
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
          {i18n.t('common.connect', {}, 'Connect')}
        </Button>
      </div>

      {#if scrobblerStore.validationError}
        <InlineMessage class="pt-1">{scrobblerStore.validationError}</InlineMessage>
      {/if}
    </div>
  {:else}
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
      <!-- Offline cache surface directly under Enable scrobbling -->
      <div class="ml-2 pl-3 border-l-2 border-brand-accent/30 flex flex-wrap items-center justify-between gap-3 py-1">
        <div class="flex flex-col gap-0.5 min-w-0">
          <div class="flex items-center gap-2">
            <span class="text-xs font-semibold text-brand-text-primary">
              {scrobblerStore.pendingCount === 0
                ? i18n.t('listenbrainz.cacheEmpty')
                : i18n.plural("listenbrainz.cachePending", scrobblerStore.pendingCount)}
            </span>
            {#if scrobblerStore.flushSuccessMessage}
              <span class="text-xs text-brand-text-primary font-medium">({scrobblerStore.flushSuccessMessage})</span>
            {/if}
          </div>
          {#if scrobblerStore.lastError}
            <InlineMessage size="compact">{scrobblerStore.lastError}</InlineMessage>
          {:else}
            <p class="text-[11px] text-brand-text-secondary">
              {i18n.t('listenbrainz.cacheDesc')}
            </p>
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

      <div class="space-y-3 pt-3 border-t border-brand-border/60">
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
          <div class="ml-2 pl-3 border-l-2 border-brand-accent/30 flex flex-col gap-3 py-1">
            <div class="flex flex-col gap-1">
              <label for="critiquebrainz-user-input" class="text-xs font-semibold text-brand-text-primary">{i18n.t('listenbrainz.critiquebrainzUserLabel')}</label>
              <p class="text-[11px] text-brand-text-secondary">{i18n.t('listenbrainz.critiquebrainzUserHint')}</p>
              <Input
                id="critiquebrainz-user-input"
                value={scrobblerStore.critiquebrainzUserId}
                oninput={(e) => scrobblerStore.setCritiquebrainzUserId((e.target as HTMLInputElement).value)}
                placeholder={i18n.t('listenbrainz.critiquebrainzUserPlaceholder')}
                class="w-full max-w-md"
              />
            </div>
            <div class="flex flex-wrap items-center justify-between gap-3">
              <div class="flex flex-col gap-0.5 min-w-0">
                <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('listenbrainz.syncRatingsLabel')}</span>
                <p class="text-[11px] text-brand-text-secondary">{i18n.t('listenbrainz.syncRatingsHint')}</p>
                {#if scrobblerStore.syncRatingsResult}
                  {@const r = scrobblerStore.syncRatingsResult}
                  <p class="text-[11px] text-brand-text-primary font-medium">
                    {i18n.t('listenbrainz.syncRatingsSuccess', {
                      loved: r.pulled_loved,
                      hated: r.pulled_hated,
                      songRatings: r.pulled_song_ratings,
                      albumRatings: r.pulled_album_ratings,
                      pushed: r.pushed
                    })}
                    {#if r.failed > 0}{i18n.t('listenbrainz.syncRatingsFailed', { failed: r.failed })}{/if}
                  </p>
                  {#if !r.critiquebrainz_checked}
                    <p class="text-[11px] text-brand-text-secondary">{i18n.t('listenbrainz.syncRatingsNoCritiquebrainz')}</p>
                  {/if}
                {:else if scrobblerStore.syncRatingsError}
                  <p class="text-[11px] text-brand-text-primary font-medium">{scrobblerStore.syncRatingsError}</p>
                {/if}
              </div>
              <Button
                variant="secondary"
                size="sm"
                onclick={() => scrobblerStore.syncRatings()}
                disabled={scrobblerStore.isSyncingRatings}
                class="gap-1.5 shrink-0"
              >
                {#if scrobblerStore.isSyncingRatings}
                  <LoaderCircle class="w-3.5 h-3.5 animate-spin" />
                  <span>{i18n.t('listenbrainz.syncingRatingsBtn')}</span>
                {:else}
                  <Heart weight="fill" class="w-3.5 h-3.5 text-brand-accent-text" />
                  <span>{i18n.t('listenbrainz.syncRatingsBtn')}</span>
                {/if}
              </Button>
            </div>
          </div>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<!-- Last.fm Scrobbler Integration Card -->
<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
  <div class="flex justify-between items-center gap-4">
    <div class="flex items-center gap-3 min-w-0">
      <img src="/lastfm-icon.svg" alt="Last.fm" class="w-9 h-9 shrink-0 object-contain rounded-lg" />
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('lastfm.integrationTitle')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">
          <button onclick={() => openExternalUrl("https://www.last.fm")} class="text-brand-accent hover:underline">Last.fm</button>
          {i18n.t('lastfm.integrationDesc')}
        </p>
      </div>
    </div>
    <div class="flex items-center gap-3 shrink-0">
      {#if scrobblerStore.lastfmUsername}
        <span class="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full bg-brand-accent/15 text-brand-text-primary border border-brand-accent/25 font-medium shrink-0">
          <Check class="w-3 h-3" />
          {scrobblerStore.lastfmUsername}
        </span>
        <Button
          variant="secondary"
          size="sm"
          onclick={() => scrobblerStore.disconnectLastfm()}
          class="text-xs text-brand-text-secondary hover:text-brand-text-primary shrink-0"
          aria-label={i18n.t('lastfm.disconnectBtn', {}, 'Disconnect Last.fm')}
        >
          {i18n.t('common.disconnect', {}, 'Disconnect')}
        </Button>
      {:else if !scrobblerStore.lastfmPendingToken}
        <Button
          onclick={() => scrobblerStore.startLastfmAuth()}
          disabled={scrobblerStore.isStartingLastfm}
          variant="secondary"
          size="sm"
          class="gap-1.5 shrink-0"
          aria-label={i18n.t('lastfm.connectBtn', {}, 'Connect Last.fm')}
        >
          {#if scrobblerStore.isStartingLastfm}
            <LoaderCircle class="w-4 h-4 animate-spin" />
          {:else}
            <ArrowUpRight class="w-4 h-4" />
          {/if}
          {i18n.t('common.connect', {}, 'Connect')}
        </Button>
      {/if}
    </div>
  </div>

  {#if !scrobblerStore.lastfmUsername}
    {#if scrobblerStore.lastfmPendingToken}
      <div class="p-4 rounded-lg bg-brand-surface/60 border border-brand-border/80 space-y-3 pt-2 border-t border-brand-border/60">
        <div class="flex flex-col gap-1">
          <span class="text-sm font-medium text-brand-text-primary">{i18n.t('lastfm.authPendingTitle')}</span>
          <p class="text-xs text-brand-text-secondary">{i18n.t('lastfm.authPendingDesc')}</p>
        </div>
        <div class="flex items-center gap-2">
          <Button
            onclick={() => scrobblerStore.finishLastfmAuth()}
            disabled={scrobblerStore.isFinishingLastfm}
            variant="primary"
            size="sm"
            class="gap-1.5"
          >
            {#if scrobblerStore.isFinishingLastfm}
              <LoaderCircle class="w-4 h-4 animate-spin" />
            {:else}
              <Check class="w-4 h-4" />
            {/if}
            {i18n.t('lastfm.finishAuthBtn')}
          </Button>
          <Button
            onclick={() => scrobblerStore.cancelLastfmAuth()}
            disabled={scrobblerStore.isFinishingLastfm}
            variant="secondary"
            size="sm"
          >
            {i18n.t('lastfm.cancelAuthBtn')}
          </Button>
        </div>
      </div>
    {/if}

    {#if scrobblerStore.lastfmAuthError}
      <InlineMessage class="pt-1">{scrobblerStore.lastfmAuthError}</InlineMessage>
    {/if}
  {:else}
    <div class="flex items-center justify-between gap-4 py-1 pt-2 border-t border-brand-border/60">
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-sm font-medium text-brand-text-primary">{i18n.t('lastfm.enableLabel')}</span>
        <p class="text-xs text-brand-text-secondary">{i18n.t('lastfm.enableHint')}</p>
      </div>
      <Toggle
        checked={scrobblerStore.lastfmEnabled}
        onchange={(v) => scrobblerStore.setLastfmEnabled(v)}
        label={i18n.t('lastfm.enableLabel')}
      />
    </div>

    {#if scrobblerStore.lastfmEnabled}
      <!-- Offline cache surface directly under Enable scrobbling -->
      <div class="ml-2 pl-3 border-l-2 border-brand-accent/30 flex flex-wrap items-center justify-between gap-3 py-1">
        <div class="flex flex-col gap-0.5 min-w-0">
          <div class="flex items-center gap-2">
            <span class="text-xs font-semibold text-brand-text-primary">
              {scrobblerStore.lastfmPendingCount === 0
                ? i18n.t('lastfm.cacheEmpty')
                : i18n.plural("lastfm.cachePending", scrobblerStore.lastfmPendingCount)}
            </span>
            {#if scrobblerStore.flushSuccessMessage}
              <span class="text-xs text-brand-text-primary font-medium">({scrobblerStore.flushSuccessMessage})</span>
            {/if}
          </div>
          {#if scrobblerStore.lastError}
            <InlineMessage size="compact">{scrobblerStore.lastError}</InlineMessage>
          {:else}
            <p class="text-[11px] text-brand-text-secondary">
              {i18n.t('lastfm.cacheDesc')}
            </p>
          {/if}
        </div>

        <Button
          onclick={() => scrobblerStore.flushCache()}
          disabled={scrobblerStore.isFlushing || scrobblerStore.lastfmPendingCount === 0}
          variant="secondary"
          size="sm"
        >
          <RefreshCw class="w-3.5 h-3.5 {scrobblerStore.isFlushing ? 'animate-spin' : ''}" />
          {i18n.t('lastfm.syncNowBtn')}
        </Button>
      </div>

      <div class="space-y-3 pt-3 border-t border-brand-border/60">
        <div class="flex items-center justify-between gap-4 py-1">
          <div class="flex flex-col gap-0.5 min-w-0">
            <span class="text-sm font-medium text-brand-text-primary">{i18n.t('lastfm.lovedTracksLabel')}</span>
            <p class="text-xs text-brand-text-secondary">{i18n.t('lastfm.lovedTracksHint')}</p>
          </div>
          <Toggle
            checked={scrobblerStore.ratingsEnabled}
            onchange={(v) => scrobblerStore.setRatingsEnabled(v)}
            label={i18n.t('lastfm.lovedTracksLabel')}
          />
        </div>

        {#if scrobblerStore.ratingsEnabled}
          <div class="ml-2 pl-3 border-l-2 border-brand-accent/30 flex flex-wrap items-center justify-between gap-3 py-1">
            <div class="flex flex-col gap-0.5 min-w-0">
              <span class="text-xs font-semibold text-brand-text-primary">{i18n.t('lastfm.syncLovedBtn')}</span>
              <p class="text-[11px] text-brand-text-secondary">{i18n.t('lastfm.lovedTracksHint')}</p>
              {#if scrobblerStore.syncRatingsResult}
                {@const r = scrobblerStore.syncRatingsResult}
                <p class="text-[11px] text-brand-text-primary font-medium">
                  {i18n.t('lastfm.syncLovedSuccess', {
                    loved: r.pulled_loved,
                    pushed: r.pushed
                  })}
                </p>
              {:else if scrobblerStore.syncRatingsError}
                <p class="text-[11px] text-brand-text-primary font-medium">{scrobblerStore.syncRatingsError}</p>
              {/if}
            </div>
            <Button
              variant="secondary"
              size="sm"
              onclick={() => scrobblerStore.syncRatings()}
              disabled={scrobblerStore.isSyncingRatings}
              class="gap-1.5 shrink-0"
            >
              {#if scrobblerStore.isSyncingRatings}
                <LoaderCircle class="w-3.5 h-3.5 animate-spin" />
                <span>{i18n.t('lastfm.syncingLovedBtn')}</span>
              {:else}
                <Heart weight="fill" class="w-3.5 h-3.5 text-brand-accent-text" />
                <span>{i18n.t('lastfm.syncLovedBtn')}</span>
              {/if}
            </Button>
          </div>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<!-- Discord Rich Presence Card -->
<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
  <div class="flex justify-between items-center gap-4">
    <div class="flex items-center gap-3 min-w-0">
      <div class="w-9 h-9 rounded-lg bg-[#5865F2]/15 flex items-center justify-center text-[#5865F2] shrink-0">
        <DiscordLogo class="w-5 h-5" weight="fill" />
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('discord.integrationTitle')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">
          <span class="text-brand-text-primary font-medium">Discord Rich Presence</span>
          {" "}{i18n.t('discord.integrationDesc')}
        </p>
      </div>
    </div>
    <div class="flex items-center gap-3 shrink-0">
      {#if scrobblerStore.discordEnabled}
        {#if scrobblerStore.discordStatus === 'connected'}
          <span class="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full bg-brand-accent/15 text-brand-text-primary border border-brand-accent/25 font-medium shrink-0">
            <Check class="w-3 h-3" />
            {i18n.t('discord.statusConnected')}
          </span>
        {:else if scrobblerStore.discordStatus === 'not_running'}
          <span class="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full bg-brand-text-primary/10 text-brand-text-primary border border-brand-border font-medium shrink-0">
            <AlertTriangle class="w-3 h-3" />
            {i18n.t('discord.statusNotRunning')}
          </span>
        {:else}
          <span class="inline-flex items-center gap-1 text-[11px] px-2 py-0.5 rounded-full bg-brand-border/40 text-brand-text-secondary border border-brand-border font-medium shrink-0">
            {i18n.t('discord.statusDisconnected')}
          </span>
        {/if}
        <Button
          variant="secondary"
          size="sm"
          onclick={() => scrobblerStore.checkDiscordStatus()}
          disabled={scrobblerStore.isCheckingDiscord}
          class="text-xs text-brand-text-secondary hover:text-brand-text-primary shrink-0"
          title={i18n.t('discord.recheckTooltip')}
          aria-label={i18n.t('discord.recheckTooltip', {}, 'Check Discord connection status')}
        >
          <RefreshCw class="w-3.5 h-3.5 {scrobblerStore.isCheckingDiscord ? 'animate-spin' : ''}" />
        </Button>
      {/if}
    </div>
  </div>

  <div class="flex items-center justify-between gap-4 py-1 pt-2 border-t border-brand-border/60">
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
    <div class="ml-2 pl-3 border-l-2 border-brand-accent/30 flex flex-col gap-3 py-1">
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
    </div>
  {/if}
</div>

<!-- fanart.tv Integration Card -->
<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4">
  <div class="pb-3 flex justify-between items-center">
    <div class="flex items-center gap-3">
      <img src="/fanart-icon.svg" alt="fanart.tv" class="w-9 h-9 shrink-0 object-contain" />
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.fanartIntegration')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">
          {i18n.t('settings.fanartDesc1')}<button onclick={() => openExternalUrl("https://fanart.tv")} class="text-brand-accent hover:underline">fanart.tv</button>{i18n.t('settings.fanartDesc2')}
        </p>
      </div>
    </div>
  </div>

  <div class="space-y-2 pt-2 border-t border-brand-border/60">
    <div class="flex items-center justify-between">
      <label for="fanart-key-input" class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider">
        {i18n.t('settings.fanartApiKeyLabel')}
      </label>
      <button
        type="button"
        onclick={() => openExternalUrl("https://fanart.tv/get-an-api-key/")}
        class="text-xs text-brand-accent hover:underline inline-flex items-center gap-1"
      >
        {i18n.t('settings.fanartGetKeyLink')}
        <ArrowUpRight class="w-3 h-3" />
      </button>
    </div>

    {#if prefs.fanartApiKey}
      <div class="text-xs font-medium text-brand-accent-text flex items-center gap-2">
        <Check class="w-3.5 h-3.5" />
        <span>{i18n.t('settings.fanartCustomKeyActive')}</span>
      </div>
    {:else if fanartKeyStatus.has_env_key}
      <div class="text-xs font-medium text-brand-accent-text flex items-center gap-2">
        <Check class="w-3.5 h-3.5" />
        <span>{i18n.t('settings.fanartEnvKeyFound', { env: 'FANART_API_KEY' })}</span>
      </div>
    {:else if fanartKeyStatus.has_build_key}
      <div class="text-xs font-medium text-brand-accent-text flex items-center gap-2">
        <Check class="w-3.5 h-3.5" />
        <span>{i18n.t('settings.fanartDefaultKeyActive')}</span>
      </div>
    {/if}

    <div class="flex items-center gap-2">
      <div class="relative flex-1 max-w-md">
        <Input
          id="fanart-key-input"
          type={showFanartKey ? "text" : "password"}
          bind:value={fanartKeyInput}
          oninput={() => { fanartValidationError = null; }}
          placeholder={i18n.t('settings.fanartPlaceholder')}
          class="w-full pr-10"
        />
        <button
          type="button"
          onclick={() => showFanartKey = !showFanartKey}
          class="absolute right-3 top-1/2 -translate-y-1/2 text-brand-text-secondary hover:text-brand-text-primary transition-colors"
          title={showFanartKey ? i18n.t("settings.hideKey") : i18n.t("settings.showKey")}
        >
          {#if showFanartKey}
            <EyeOff class="w-4 h-4" />
          {:else}
            <Eye class="w-4 h-4" />
          {/if}
        </button>
      </div>

      <Button
        onclick={handleValidateFanartKey}
        disabled={isValidatingFanartKey || !fanartKeyInput?.trim()}
        variant="secondary"
        size="sm"
      >
        {#if isValidatingFanartKey}
          <LoaderCircle class="w-4 h-4 animate-spin" />
        {:else}
          <Check class="w-4 h-4" />
        {/if}
        {i18n.t('settings.fanartValidateBtn')}
      </Button>

      {#if prefs.fanartApiKey}
        <Button
          onclick={handleClearFanartKey}
          variant="secondary"
          size="sm"
        >
          {i18n.t('settings.fanartClearKeyBtn')}
        </Button>
      {/if}
    </div>

    {#if fanartValidationError}
      <InlineMessage class="pt-1">{fanartValidationError}</InlineMessage>
    {/if}
  </div>

  <div class="space-y-2 pt-2 border-t border-brand-border/60">
    <div class="space-y-0.5">
      <span class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider">
        {i18n.t('settings.fanartImagesLabel')}
      </span>
      <p class="text-xs text-brand-text-secondary">{i18n.t('settings.fanartImagesHint')}</p>
    </div>
    {#each [
      { label: 'settings.fanartFetchPhoto', hint: 'settings.fanartFetchPhotoHint', checked: prefs.fanartFetchPhoto, set: (v: boolean) => prefs.setFanartFetchPhoto(v) },
      { label: 'settings.fanartFetchLogo', hint: 'settings.fanartFetchLogoHint', checked: prefs.fanartFetchLogo, set: (v: boolean) => prefs.setFanartFetchLogo(v) },
      { label: 'settings.fanartFetchBackground', hint: 'settings.fanartFetchBackgroundHint', checked: prefs.fanartFetchBackground, set: (v: boolean) => prefs.setFanartFetchBackground(v) },
      { label: 'settings.fanartFetchAlbumCover', hint: 'settings.fanartFetchAlbumCoverHint', checked: prefs.fanartFetchAlbumCover, set: (v: boolean) => prefs.setFanartFetchAlbumCover(v) },
      { label: 'settings.fanartFetchDiscArt', hint: 'settings.fanartFetchDiscArtHint', checked: prefs.fanartFetchDiscArt, set: (v: boolean) => prefs.setFanartFetchDiscArt(v) },
    ] as row (row.label)}
      <div class="flex items-center justify-between gap-4 py-1">
        <div class="flex flex-col gap-0.5 min-w-0">
          <span class="text-sm font-medium text-brand-text-primary">{i18n.t(row.label)}</span>
          <p class="text-xs text-brand-text-secondary">{i18n.t(row.hint)}</p>
        </div>
        <Toggle checked={row.checked} onchange={row.set} label={i18n.t(row.label)} />
      </div>
    {/each}
  </div>
</div>
{/if}
