<script lang="ts">
  import { cubicOut } from "svelte/easing";
  import { fade, scale } from "../utils/motion";
  import { onMount } from "svelte";
  import { portal } from "../utils/portal";
  import { i18n } from "../stores/i18n.svelte";
  import { musicbrainzStore } from "../stores/musicbrainz.svelte";
  import { scrobblerStore } from "../stores/scrobbler.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import Toggle from "./Toggle.svelte";
  import {
    XIcon as X,
    ArrowUpRightIcon as ArrowUpRight,
    ArrowRightIcon as ArrowRight,
    CircleNotchIcon as LoaderCircle,
    CheckIcon as Check,
    PauseIcon as Pause,
    WarningIcon as AlertTriangle
  } from "phosphor-svelte";

  interface Props {
    isOpen: boolean;
    anchorEl: HTMLElement | null;
    onClose: () => void;
  }

  let { isOpen, anchorEl, onClose }: Props = $props();

  let popoverEl = $state<HTMLDivElement | null>(null);
  let lbTokenInput = $state("");
  let isConnectingLb = $state(false);
  let lbConnectError = $state<string | null>(null);

  let lbUsername = $derived(scrobblerStore.username || musicbrainzStore.username);
  let mbUsername = $derived(musicbrainzStore.username || scrobblerStore.username);

  let coords = $state<{
    left: number;
    bottom: number;
  }>({
    left: 260,
    bottom: 80,
  });

  function updatePosition() {
    if (!anchorEl || typeof window === "undefined") return;
    const rect = anchorEl.getBoundingClientRect();
    const popoverWidth = 340;

    // Place popover to the right of the anchor button
    let left = rect.right + 12;
    // If not enough room to the right, clamp inside window
    if (left + popoverWidth > window.innerWidth - 16) {
      left = Math.max(16, rect.left);
    }

    // Vertically align bottom near anchor's bottom
    const bottom = Math.max(16, window.innerHeight - rect.bottom);
    coords = { left, bottom };
  }

  // Pre-effect: position before the popover's first paint, not one frame after it.
  $effect.pre(() => {
    if (isOpen) {
      updatePosition();
    }
  });

  onMount(() => {
    if (isOpen) {
      updatePosition();
    }
  });

  function handleWindowClick(e: MouseEvent) {
    if (!isOpen) return;
    const target = e.target as Node | null;
    if (
      popoverEl &&
      !popoverEl.contains(target) &&
      anchorEl &&
      !anchorEl.contains(target)
    ) {
      onClose();
    }
  }

  function handleWindowKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && isOpen) {
      onClose();
    }
  }

  async function handleConnectListenBrainz() {
    const token = lbTokenInput.trim();
    if (!token || isConnectingLb) return;
    isConnectingLb = true;
    lbConnectError = null;
    try {
      scrobblerStore.setToken(token);
      await scrobblerStore.validateToken();
      if (scrobblerStore.validationError) {
        lbConnectError = scrobblerStore.validationError;
      } else {
        scrobblerStore.setEnabled(true);
        lbTokenInput = "";
      }
    } catch (err) {
      lbConnectError = typeof err === "string" ? err : i18n.t("listenbrainz.connectFailed");
    } finally {
      isConnectingLb = false;
    }
  }

  function handleOpenScrobblerSettings() {
    navigationStore.openSettings("integrations");
    onClose();
  }
</script>

<svelte:window onclick={handleWindowClick} onkeydown={handleWindowKeydown} onresize={updatePosition} />

{#if isOpen}
  <div
    use:portal
    bind:this={popoverEl}
    style="left: {coords.left}px; bottom: {coords.bottom}px;"
    class="fixed z-50 w-[340px] bg-brand-sidebar border border-brand-border rounded-2xl shadow-2xl p-4 text-brand-text-primary space-y-3 select-none"
    in:scale={{ start: 0.95, duration: 150, easing: cubicOut }}
    out:fade={{ duration: 120, easing: cubicOut }}
  >
    <!-- Scrobbling Section Header with Close Button -->
    <div class="flex items-center justify-between gap-2">
      <div class="flex items-center gap-1.5 flex-wrap">
        <span class="text-xs font-semibold text-brand-text-primary">
          {i18n.t('auth.scrobbling', {}, 'Scrobbling')}
        </span>
        {#if scrobblerStore.enabled}
          <span class="text-[10px] px-1.5 py-0.5 rounded bg-brand-surface border border-brand-border text-brand-text-secondary font-medium">
            ListenBrainz
          </span>
        {/if}
        {#if scrobblerStore.lastfmEnabled}
          <span class="text-[10px] px-1.5 py-0.5 rounded bg-brand-surface border border-brand-border text-brand-text-secondary font-medium">
            Last.fm
          </span>
        {/if}
        {#if scrobblerStore.status === 'paused'}
          <span class="inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded-full bg-brand-text-primary/10 text-brand-text-primary font-medium">
            <Pause class="w-2.5 h-2.5" />
            {i18n.t('common.paused', {}, 'Paused')}
          </span>
        {:else if scrobblerStore.status === 'active'}
          <span class="inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded-full bg-brand-text-primary/10 text-brand-text-primary font-medium">
            <Check class="w-2.5 h-2.5" />
            {i18n.t('common.active', {}, 'Active')}
          </span>
        {:else}
          <span class="inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded-full bg-zinc-500/15 text-zinc-400 font-medium">
            {i18n.t('common.inactive', {}, 'Inactive')}
          </span>
        {/if}
      </div>

      <button
        type="button"
        onclick={onClose}
        class="text-brand-text-secondary hover:text-brand-text-primary p-0.5 rounded hover:bg-white/5 transition-colors cursor-pointer shrink-0"
        title={i18n.t('common.close', {}, 'Close')}
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <!-- Scrobbling Section -->
    <div class="space-y-2.5">

      {#if scrobblerStore.enabled || scrobblerStore.lastfmEnabled}
        <!-- Toggles moved from Settings view -->
        <div class="space-y-1.5 py-0.5">
          <div class="flex items-center justify-between gap-3">
            <span class="text-xs text-brand-text-secondary leading-snug">{i18n.t('listenbrainz.pauseLabel')}</span>
            <Toggle
              checked={scrobblerStore.paused}
              onchange={(v) => scrobblerStore.setPaused(v)}
              label={i18n.t('listenbrainz.pauseLabel')}
              showOnOffLabel={false}
            />
          </div>

          <div class="flex items-center justify-between gap-3">
            <span class="text-xs text-brand-text-secondary leading-snug">{i18n.t('listenbrainz.nowPlayingLabel')}</span>
            <Toggle
              checked={scrobblerStore.nowPlayingEnabled}
              onchange={(v) => scrobblerStore.setNowPlayingEnabled(v)}
              label={i18n.t('listenbrainz.nowPlayingLabel')}
              showOnOffLabel={false}
            />
          </div>
        </div>
      {:else}
        <p class="text-[11px] text-brand-text-secondary leading-relaxed">
          {i18n.t('auth.listenbrainzPromptDesc', {}, 'Scrobble your listening history automatically to ListenBrainz with your user token.')}
        </p>

        <div class="space-y-1.5 pt-1">
          <div class="flex items-center gap-2">
            <Input
              type="password"
              placeholder={i18n.t('listenbrainz.userTokenPlaceholder', {}, 'ListenBrainz User Token')}
              bind:value={lbTokenInput}
              class="flex-1 text-xs"
            />
            <Button
              variant="secondary"
              size="sm"
              disabled={!lbTokenInput.trim() || isConnectingLb}
              onclick={handleConnectListenBrainz}
            >
              {#if isConnectingLb}
                <LoaderCircle class="w-3.5 h-3.5 animate-spin" />
              {:else}
                <Check class="w-3.5 h-3.5" />
              {/if}
              {i18n.t('common.connect', {}, 'Connect')}
            </Button>
          </div>

          <button
            type="button"
            onclick={() => openExternalUrl("https://listenbrainz.org/profile/")}
            class="text-[11px] text-brand-text-primary hover:text-brand-accent-text-hover hover:underline underline underline-offset-2 font-medium inline-flex items-center gap-1 transition-colors cursor-pointer"
          >
            {i18n.t('listenbrainz.getTokenLink', {}, 'Get token on listenbrainz.org')}
            <ArrowUpRight class="w-3 h-3" />
          </button>

          {#if lbConnectError}
            <p class="flex items-start gap-1.5 text-[11px] text-brand-text-primary pt-0.5"><AlertTriangle class="w-3 h-3 shrink-0 mt-px" />{lbConnectError}</p>
          {/if}
        </div>
      {/if}

      <!-- Profile Links -->
      <div class="space-y-1 pt-2 border-t border-brand-border/60">
        {#if scrobblerStore.lastfmUsername}
          <button
            type="button"
            onclick={() => openExternalUrl(`https://www.last.fm/user/${encodeURIComponent(scrobblerStore.lastfmUsername!)}`)}
            class="w-full flex items-center justify-between py-1 text-xs text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
          >
            <span class="hover:underline">{i18n.t('auth.lastfmProfile', {}, 'Last.fm listener profile')}</span>
            <ArrowUpRight class="w-3.5 h-3.5" />
          </button>
        {/if}

        {#if lbUsername}
          <button
            type="button"
            onclick={() => openExternalUrl(`https://listenbrainz.org/user/${encodeURIComponent(lbUsername!)}/`)}
            class="w-full flex items-center justify-between py-1 text-xs text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
          >
            <span class="hover:underline">{i18n.t('auth.listenbrainzProfile', {}, 'ListenBrainz listener profile')}</span>
            <ArrowUpRight class="w-3.5 h-3.5" />
          </button>
        {/if}

        {#if mbUsername}
          <button
            type="button"
            onclick={() => openExternalUrl(`https://musicbrainz.org/user/${encodeURIComponent(mbUsername!)}`)}
            class="w-full flex items-center justify-between py-1 text-xs text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
          >
            <span class="hover:underline">{i18n.t('auth.musicbrainzProfile', {}, 'MusicBrainz editor profile')}</span>
            <ArrowUpRight class="w-3.5 h-3.5" />
          </button>
        {/if}
      </div>
    </div>

    <!-- Integration Settings -->
    <div class="pt-2 border-t border-brand-border/60">
      <button
        type="button"
        onclick={handleOpenScrobblerSettings}
        class="w-full flex items-center justify-between py-1 text-xs text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
      >
        <span class="hover:underline">{i18n.t('auth.integrationSettings', {}, 'Integration settings')}</span>
        <ArrowRight class="w-3.5 h-3.5" />
      </button>
    </div>
  </div>
{/if}
