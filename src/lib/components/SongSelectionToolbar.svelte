<script lang="ts">
  import { PlayIcon as Play } from "phosphor-svelte";
  import Plus from "./PlusIcon.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { getPlaylistDisplayName } from "../utils/playlist";

  interface Props {
    count: number;
    onPlaySelected: () => void;
    onAddToPlaylist: () => void;
    onClear: () => void;
  }

  let { count, onPlaySelected, onAddToPlaylist, onClear }: Props = $props();
</script>

<div
  data-floating-toolbar="true"
  class="absolute left-1/2 -translate-x-1/2 bottom-dock z-40 acrylic-float border border-brand-border/80 shadow-2xl rounded-full px-5 py-2.5 flex items-center gap-4 text-xs font-semibold"
>
  <span class="text-brand-accent-text font-bold">
    {i18n.t("playlists.selectedCount", { count })}
  </span>
  <div class="h-4 w-px bg-brand-border/60"></div>
  <button
    onclick={onPlaySelected}
    class="flex items-center gap-1.5 hover:text-brand-accent-text transition-colors"
  >
    <Play class="w-3.5 h-3.5 fill-current text-brand-accent-text" />
    <span>{i18n.t("playlists.playSelected")}</span>
  </button>
  <button
    onclick={onAddToPlaylist}
    class="flex items-center gap-1.5 hover:text-brand-accent-text transition-colors"
  >
    <Plus class="w-3.5 h-3.5 text-brand-accent-text" />
    <span>
      {playlistsStore.activeCustomPlaylist
        ? i18n.t("playlists.contextMenuAddToPlaylist", {
            name: getPlaylistDisplayName(playlistsStore.activeCustomPlaylist),
          })
        : i18n.t("playlists.contextMenuAddToPlaylistDefault")}
    </span>
  </button>
  <div class="h-4 w-px bg-brand-border/60"></div>
  <button
    onclick={onClear}
    class="text-brand-text-secondary hover:text-brand-text-primary transition-colors"
  >
    {i18n.t("playlists.clearSelection")}
  </button>
</div>
