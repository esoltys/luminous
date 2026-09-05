<script lang="ts">
  import type { Component } from "svelte";
  import type { MusicDirectory } from "../types";
  import {
    FolderIcon,
    HardDriveIcon,
    CloudIcon,
    DesktopIcon,
    UsbIcon,
    HouseIcon,
    DiscIcon,
    MusicNotesIcon,
    ArchiveIcon,
    BroadcastIcon,
    WarningIcon,
  } from "phosphor-svelte";

  interface Props {
    directory: MusicDirectory;
    size?: "xs" | "sm" | "md";
    showName?: boolean;
    class?: string;
  }

  let {
    directory,
    size = "sm",
    showName = true,
    class: className = "",
  }: Props = $props();

  const ICONS: Record<string, Component<any>> = {
    folder: FolderIcon,
    "hard-drive": HardDriveIcon,
    cloud: CloudIcon,
    desktop: DesktopIcon,
    usb: UsbIcon,
    house: HouseIcon,
    disc: DiscIcon,
    music: MusicNotesIcon,
    archive: ArchiveIcon,
    broadcast: BroadcastIcon,
  };

  let IconComponent = $derived(
    directory.icon && ICONS[directory.icon] ? ICONS[directory.icon] : FolderIcon
  );

  let displayName = $derived.by(() => {
    if (directory.nickname && directory.nickname.trim() !== "") {
      return directory.nickname.trim();
    }
    const p = directory.path.replace(/\\/g, "/").replace(/\/+$/, "");
    const parts = p.split("/");
    return parts[parts.length - 1] || directory.path;
  });

  let isUnavailable = $derived(directory.is_available === false);
  let customColor = $derived(directory.color?.trim() || null);

  let badgeStyle = $derived(
    customColor
      ? `background-color: color-mix(in srgb, ${customColor} 18%, transparent); border-color: color-mix(in srgb, ${customColor} 45%, transparent); color: ${customColor};`
      : undefined
  );
</script>

<span
  class="inline-flex items-center font-medium border backdrop-blur-md transition-colors select-none max-w-full truncate
    {size === 'xs' ? 'text-[10px] px-1.5 py-0.5 gap-1 rounded-md' : ''}
    {size === 'sm' ? 'text-xs px-2 py-0.5 gap-1.5 rounded-lg' : ''}
    {size === 'md' ? 'text-xs px-2.5 py-1 gap-2 rounded-xl' : ''}
    {!customColor ? 'bg-brand-sidebar/80 border-brand-border/70 text-brand-text-secondary' : ''}
    {isUnavailable ? 'opacity-70 border-dashed border-red-400/60' : ''}
    {className}"
  style={badgeStyle}
  title={isUnavailable ? `${displayName} (Disconnected) — ${directory.path}` : `${displayName} — ${directory.path}`}
>
  {#if isUnavailable}
    <WarningIcon class="{size === 'xs' ? 'w-2.5 h-2.5' : 'w-3.5 h-3.5'} text-red-400 shrink-0" />
  {:else}
    <IconComponent class="{size === 'xs' ? 'w-2.5 h-2.5' : size === 'sm' ? 'w-3 h-3' : 'w-3.5 h-3.5'} shrink-0" />
  {/if}
  {#if showName}
    <span class="truncate">{displayName}</span>
  {/if}
</span>
