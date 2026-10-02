<script lang="ts">
  import { onMount } from "svelte";
  import { portal } from "../utils/portal";
  import { i18n } from "../stores/i18n.svelte";
  import { musicbrainzStore } from "../stores/musicbrainz.svelte";
  import { scrobblerStore } from "../stores/scrobbler.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import {
    XIcon as X,
    ArrowUpRightIcon as ArrowUpRight,
    SignOutIcon as LogOut,
    CircleNotchIcon as LoaderCircle,
    CheckIcon as Check
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

  $effect(() => {
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
      lbConnectError = typeof err === "string" ? err : "Failed to connect ListenBrainz";
    } finally {
      isConnectingLb = false;
    }
  }

  function handleOpenScrobblerSettings() {
    navigationStore.openSettings("integrations");
    onClose();
  }

  async function handleLogout() {
    await musicbrainzStore.logout();
    onClose();
  }
</script>

<svelte:window onclick={handleWindowClick} onkeydown={handleWindowKeydown} onresize={updatePosition} />

{#if isOpen}
  <div
    use:portal
    bind:this={popoverEl}
    style="left: {coords.left}px; bottom: {coords.bottom}px;"
    class="fixed z-50 w-[340px] bg-brand-sidebar border border-brand-border rounded-2xl shadow-2xl p-4 text-brand-text-primary space-y-4 select-none animate-in fade-in zoom-in-95 duration-150"
  >
    <!-- Header: User Identity -->
    <div class="flex items-start justify-between gap-3 pb-3 border-b border-brand-border/60">
      <div class="flex items-center gap-3 min-w-0">
        <div class="p-2.5 rounded-xl bg-[#ba478f]/15 border border-[#ba478f]/25 shrink-0 flex items-center justify-center">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 25 28" class="w-5 h-5">
            <polygon fill="#ba478f" points="12 0 0 7 0 21 12 28 12 0"/>
            <polygon fill="#eb743b" points="13 0 25 7 25 21 13 28 13 0"/>
          </svg>
        </div>
        <div class="min-w-0">
          <h3 class="font-bold text-sm text-brand-text-primary truncate">
            {musicbrainzStore.username ?? "MusicBrainz User"}
          </h3>
          {#if musicbrainzStore.email}
            <p class="text-xs text-brand-text-secondary truncate mt-0.5">
              {musicbrainzStore.email}
            </p>
          {/if}
        </div>
      </div>

      <button
        onclick={onClose}
        class="text-brand-text-secondary hover:text-brand-text-primary p-0.5 rounded hover:bg-white/5 transition-colors"
        title={i18n.t('common.close', {}, 'Close')}
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <!-- ListenBrainz Scrobbling Section -->
    <div class="space-y-2">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <img src="/listenbrainz-icon.png" alt="ListenBrainz" class="w-4 h-4 object-contain" />
          <span class="text-xs font-semibold text-brand-text-primary">
            {i18n.t('auth.listenbrainzScrobbling', {}, 'ListenBrainz Scrobbling')}
          </span>
        </div>
        {#if scrobblerStore.enabled}
          <span class="inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded-full bg-emerald-500/15 text-emerald-400 font-medium">
            <Check class="w-2.5 h-2.5" />
            {i18n.t('common.active', {}, 'Active')}
          </span>
        {/if}
      </div>

      {#if scrobblerStore.enabled}
        <div class="pt-0.5">
          <button
            type="button"
            onclick={handleOpenScrobblerSettings}
            class="text-xs text-brand-text-primary hover:text-brand-accent-text-hover hover:underline underline underline-offset-2 font-medium inline-flex items-center gap-1 transition-colors cursor-pointer"
          >
            <span>{i18n.t('auth.modifyScrobblingSettings', {}, 'Modify scrobbling settings in Integrations')}</span>
            <ArrowUpRight class="w-3.5 h-3.5" />
          </button>
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
            <p class="text-[11px] text-amber-500 pt-0.5">{lbConnectError}</p>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Quick External Links & Logout -->
    <div class="space-y-1.5 pt-2 border-t border-brand-border/60">
      {#if musicbrainzStore.username}
        <button
          onclick={() => openExternalUrl(`https://musicbrainz.org/user/${encodeURIComponent(musicbrainzStore.username!)}`)}
          class="w-full flex items-center justify-between py-1 text-xs text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
        >
          <span class="hover:underline">{i18n.t('auth.viewProfileOnMb', {}, 'View profile on MusicBrainz')}</span>
          <ArrowUpRight class="w-3.5 h-3.5" />
        </button>
      {/if}

      <button
        onclick={handleLogout}
        class="w-full flex items-center justify-between py-1 text-xs text-red-400 hover:text-red-300 transition-colors cursor-pointer"
      >
        <span class="hover:underline">{i18n.t('auth.logout', {}, 'Log out')}</span>
        <LogOut class="w-3.5 h-3.5" />
      </button>
    </div>
  </div>
{/if}
