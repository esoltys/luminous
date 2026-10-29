<script lang="ts">
  import type { Playlist, PlaylistSizeFit } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { formatStorageSize } from "../utils/formatters";
  import Button from "./Button.svelte";
  import IconActionButton from "./IconActionButton.svelte";
  import PlaylistSizeLimitPopover from "./PlaylistSizeLimitPopover.svelte";
  import { UsbIcon as Usb } from "phosphor-svelte";

  /** The playlist header's size-limit button and its popover. `fit` is the
   * latest answer for the playlist, which names its device when unplugged. */
  let { playlist, fit = null }: { playlist: Playlist; fit?: PlaylistSizeFit | null } = $props();

  let isOpen = $state(false);
  let anchorEl = $state<HTMLElement | null>(null);
</script>

<!-- Icon-only until a limit is set, so the header row doesn't wrap for a control most playlists never use. -->
<span bind:this={anchorEl} class="inline-flex">
  {#if playlist.size_limit_bytes}
    <Button onclick={() => (isOpen = !isOpen)} variant="accent-soft">
      <Usb class="w-4 h-4" />
      <span>{i18n.t("playlists.sizeLimitSetBtn", { limit: formatStorageSize(playlist.size_limit_bytes) })}</span>
    </Button>
  {:else}
    <IconActionButton onclick={() => (isOpen = !isOpen)} title={i18n.t("playlists.sizeLimitBtn")} class="shrink-0">
      {#snippet icon()}<Usb class="w-4 h-4" />{/snippet}
    </IconActionButton>
  {/if}
</span>
<PlaylistSizeLimitPopover
  {isOpen}
  {anchorEl}
  limitBytes={playlist.size_limit_bytes ?? null}
  deviceId={playlist.size_limit_device ?? null}
  deviceLabel={fit?.device?.name ?? null}
  refreshReorders={playlist.dynamic_enabled && !playlist.sort_field}
  onApply={(bytes, deviceId) => playlistsStore.setSizeLimit(playlist.id, bytes, deviceId)}
  onClose={() => (isOpen = false)}
/>
