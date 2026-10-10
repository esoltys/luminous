<script lang="ts">
  import { getPlaylistCardTheme, type PlaylistCardThemeKind } from "../utils/playlistCardTheme";

  interface Props {
    kind: PlaylistCardThemeKind;
    /** Size classes, e.g. "w-10 h-10". */
    sizeClass: string;
  }

  let { kind, sizeClass }: Props = $props();

  const theme = $derived(getPlaylistCardTheme(kind));
  const Icon = $derived(theme.icon);
</script>

{#if kind === "missing_musicbrainz"}
  <img src="/picard-icon.png" alt="Picard" class="{sizeClass} object-contain" />
{:else}
  <Icon class="{sizeClass} {theme.iconColorClass} {kind === 'favourites' ? 'fill-current' : ''}" />
{/if}
