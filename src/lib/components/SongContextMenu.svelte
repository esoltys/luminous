<script lang="ts">
  import { isRemoteSource } from "../utils/remoteSource";
  import {
    PlayIcon as Play,
    PlusIcon as Plus,
    ListPlusIcon as ListPlus,
    MicrophoneStageIcon as Mic2,
    DiscIcon as DiscAlbum,
    PencilSimpleIcon as Edit3,
    FolderIcon as Folder,
    StackIcon as Layers,
    PushPinIcon as Pin,
    PushPinSlashIcon as PinOff,
    ArrowSquareOutIcon as OpenInPicard,
    EyeSlashIcon as EyeSlash,
    EyeIcon as Eye,
    ChartBarIcon as BarChart2,
    ShareNetworkIcon as Share,
    RadioIcon as Radio,
    PlaylistIcon as PlaylistIcon,
    XIcon as X,
    HeartIcon,
    HeartBreakIcon,
    ShieldCheckIcon as ShieldCheck,
    ProhibitIcon as Prohibit,
  } from "phosphor-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { Snippet } from "svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { picardStore } from "../stores/picard.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import { statsExclusionsStore } from "../stores/statsExclusions.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { provenanceStore } from "../stores/provenance.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { startSongRadio } from "../utils/playlist";
  import {
    isLosslessFileType,
    isOnlineLyricsSource,
    type Song,
    type Playlist,
    type LyricsPayload,
  } from "../types";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuSubmenu from "./ContextMenuSubmenu.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";
  import ShareModal from "./ShareModal.svelte";
  import ViewInFilesItem from "./ViewInFilesItem.svelte";
  import AddToDeviceMenuItem from "./AddToDeviceMenuItem.svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";

  let {
    x,
    y,
    song,
    selectedCount = 1,
    selectedSongIds,
    selectedSongs,
    onPlay,
    onAddToQueue,
    onAddToPlaylist,
    onGoToArtist,
    onGoToAlbum,
    onEditTags,
    onEditBatchTags,
    onOpenInPicard,
    onOrganizeFiles,
    onClose,
    notice,
    showViewInFiles = true,
    trailing,
  }: {
    x: number;
    y: number;
    song: Song;
    selectedCount?: number;
    selectedSongIds?: number[];
    selectedSongs?: Song[];
    onPlay: () => void;
    onAddToQueue?: () => void;
    onAddToPlaylist?: ((playlistId: number) => void) | (() => void);
    onGoToArtist?: () => void;
    onGoToAlbum?: () => void;
    onEditTags?: () => void;
    onEditBatchTags?: () => void;
    onOpenInPicard?: () => void;
    onOrganizeFiles?: () => void;
    onClose: () => void;
    /** A sentence added to the toast after adding to the queue or a playlist,
     * e.g. what the caller left out of the selection. */
    notice?: string;
    /** False where the song is already shown in the Files view. */
    showViewInFiles?: boolean;
    /** Items the caller adds at the end of the menu, after a divider. */
    trailing?: Snippet;
  } = $props();

  /** A success toast, with the caller's `notice` after it as a second sentence
   * (the success messages carry no full stop of their own). */
  function showAdded(message: string) {
    toastStore.show(notice ? `${message}${/[.!?]$/.test(message) ? "" : "."} ${notice}` : message);
  }

  /** What a toast calls the songs acted on: the song's title, or how many. */
  function songsName(count: number): string {
    return count > 1
      ? i18n.plural("playlists.songsCount", count)
      : song.title || i18n.t("collection.unknownSong");
  }

  async function handleDefaultAddToQueue() {
    const ids = selectedSongIds && selectedSongIds.length > 0 ? selectedSongIds : [song.id];
    await playlistsStore.addSongsToQueue(ids);
    const name = songsName(ids.length);
    showAdded(i18n.t("playlists.addedToQueueSuccess", { name }, `Added ${name} to Queue`));
  }

  let showNewPlaylistModal = $state(false);
  let newPlaylistName = $state("");

  const activeCustom = $derived(playlistsStore.activeCustomPlaylist);
  const otherCustomPlaylists = $derived(
    activeCustom
      ? playlistsStore.customPlaylists.filter((p) => p.id !== activeCustom.id)
      : playlistsStore.customPlaylists,
  );

  async function handleAddToCustomPlaylist(targetPlaylist: Playlist) {
    const ids = selectedSongIds && selectedSongIds.length > 0 ? selectedSongIds : [song.id];
    await playlistsStore.addSongsToPlaylist(targetPlaylist.id, ids);
    showAdded(
      i18n.t(
        "playlists.addedToPlaylistSuccess",
        { name: targetPlaylist.name },
        `Added to ${targetPlaylist.name}`,
      ),
    );
    if (onAddToPlaylist && onAddToPlaylist.length > 0) {
      (onAddToPlaylist as (id: number) => void)(targetPlaylist.id);
    }
    onClose();
  }

  function handleOpenNewPlaylistModal() {
    newPlaylistName = "";
    menuVisible = false;
    showNewPlaylistModal = true;
  }

  async function handleConfirmCreatePlaylist() {
    const trimmed = newPlaylistName.trim();
    const finalName = trimmed || i18n.t("playlists.untitledPlaylistName", {}, "Untitled Playlist");
    try {
      const created = await playlistsStore.createPlaylist(finalName);
      if (created) {
        const ids = selectedSongIds && selectedSongIds.length > 0 ? selectedSongIds : [song.id];
        await playlistsStore.addSongsToPlaylist(created.id, ids);
        showAdded(
          i18n.t(
            "playlists.addedToPlaylistSuccess",
            { name: created.name },
            `Added to ${created.name}`,
          ),
        );
      }
    } catch (err) {
      console.error("Failed to create playlist from song menu:", err);
    }
    showNewPlaylistModal = false;
    onClose();
  }

  const hasLosslessSelected = $derived.by(() => {
    if (selectedSongs && selectedSongs.length > 0) {
      return selectedSongs.some(
        (s) =>
          isLosslessFileType(s.filetype) &&
          (s.source === "local_file" || s.source === "collection"),
      );
    }
    return (
      isLosslessFileType(song.filetype) &&
      (song.source === "local_file" || song.source === "collection")
    );
  });

  async function handleVerifyLossless() {
    let idsToVerify: number[] = [];
    if (selectedSongs && selectedSongs.length > 0) {
      idsToVerify = selectedSongs
        .filter(
          (s) =>
            isLosslessFileType(s.filetype) &&
            (s.source === "local_file" || s.source === "collection"),
        )
        .map((s) => s.id);
    } else if (
      isLosslessFileType(song.filetype) &&
      (song.source === "local_file" || song.source === "collection")
    ) {
      idsToVerify = [song.id];
    }
    if (idsToVerify.length === 0) return;

    try {
      await provenanceStore.verifySongs(idsToVerify, song.album || song.title);
    } catch (err) {
      console.error("Batch provenance verification failed:", err);
    }
  }

  function autofocus(node: HTMLInputElement) {
    setTimeout(() => node.focus(), 50);
  }

  async function handleToggleNotIncluded() {
    const ids = selectedSongIds && selectedSongIds.length > 0 ? selectedSongIds : [song.id];
    const notIncluded = !song.not_included;
    await invoke("set_songs_not_included", { songIds: ids, notIncluded });
    const name = songsName(ids.length);
    const message = notIncluded
      ? i18n.t("playlists.markedNotIncluded", { name })
      : i18n.t("playlists.unmarkedNotIncluded", { name });
    toastStore.show(message);
  }

  const picardSelection = $derived(
    selectedSongs && selectedSongs.length > 0 ? selectedSongs : [song],
  );
  const allSelectedRemote = $derived(picardSelection.every(isRemoteSource));

  let showShareModal = $state(false);
  // See AlbumContextMenu.svelte for why the menu must be hidden (not left
  // mounted) the instant Share is clicked: ContextMenu's outside-click
  // listener would otherwise treat any click inside the portalled
  // ShareModal as "outside this menu" and tear the whole thing down.
  let menuVisible = $state(true);

  async function handleToggleStatsExcluded() {
    const reason = statsExclusionsStore.getSongExclusionReason(song);
    if (reason === "album" && song.album) {
      await statsExclusionsStore.toggleWithToast("album", song.album);
    } else if (reason === "artist") {
      const artist = song.album_artist || song.artist;
      if (artist) {
        await statsExclusionsStore.toggleWithToast("artist", artist);
      }
    } else {
      await statsExclusionsStore.toggleWithToast(
        "song",
        String(song.id),
        song.title || i18n.t("collection.unknownSong"),
      );
    }
  }

  async function handleToggleFavourite() {
    const isFav = song.loved === 1 || (song.loved === undefined && song.rating === 5);
    const newLoved = isFav ? 0 : 1;
    try {
      song.loved = await invoke<number>("set_song_loved", { songId: song.id, loved: newLoved });
    } catch (e) {
      console.error("Failed to update favourite:", e);
    }
  }

  async function handleToggleDislike() {
    const newLoved = song.loved === -1 ? 0 : -1;
    try {
      song.loved = await invoke<number>("set_song_loved", { songId: song.id, loved: newLoved });
    } catch (e) {
      console.error("Failed to update dislike:", e);
    }
  }
</script>

{#if menuVisible}
  <ContextMenu {x} {y} {onClose}>
    <div
      class="px-3 py-1 text-[11px] font-bold text-brand-text-primary border-b border-brand-border/40 mb-1 truncate"
    >
      {#if selectedCount > 1}
        {i18n.t("playlists.selectedCount", { count: selectedCount })}
      {:else}
        {song.title || i18n.t("collection.unknownSong")}
      {/if}
    </div>

    <ContextMenuItem
      icon={Play}
      accent
      label={selectedCount > 1
        ? i18n.t("playlists.contextMenuPlay")
        : i18n.t("playlists.contextMenuPlaySong")}
      onclick={() => {
        onPlay();
        onClose();
      }}
    />

    <ContextMenuItem
      icon={Layers}
      label={i18n.t("playlists.contextMenuAddQueue", {}, "Add to Queue")}
      onclick={async () => {
        if (onAddToQueue) {
          onAddToQueue();
        } else {
          await handleDefaultAddToQueue();
        }
        onClose();
      }}
    />

    {#if selectedCount === 1}
      <ContextMenuItem
        icon={Radio}
        label={i18n.t("playlists.contextMenuStartSongRadio", {}, "Start Song Radio")}
        onclick={async () => {
          await startSongRadio(song);
          onClose();
        }}
      />
    {/if}

    <ContextMenuSubmenu
      icon={ListPlus}
      label={i18n.t("playlists.contextMenuAddToPlaylistGeneric", {}, "Add to Playlist")}
    >
      {#snippet children()}
        {#if playlistsStore.customPlaylists.length === 0}
          <div class="px-3 py-1.5 text-brand-text-secondary/60 italic select-none">
            {i18n.t("playlists.contextMenuNoCustomPlaylists", {}, "No custom playlists")}
          </div>
        {:else}
          {#if activeCustom}
            <button
              type="button"
              onclick={() => handleAddToCustomPlaylist(activeCustom)}
              class="w-full text-left px-3 py-1.5 flex items-center gap-2 transition-colors hover:bg-brand-main hover:text-brand-text-primary group"
              role="menuitem"
            >
              <Radio class="w-3.5 h-3.5 shrink-0 text-brand-accent-text" />
              <span
                class="truncate flex-1 font-medium text-brand-text-primary"
                title={activeCustom.name}>{activeCustom.name}</span
              >
              <span
                class="label-small text-brand-accent-text px-1 py-0.5 rounded bg-brand-accent/15 shrink-0"
              >
                {i18n.t("playlists.activeBadgeLabel", {}, "Active")}
              </span>
            </button>
            {#if otherCustomPlaylists.length > 0}
              <ContextMenuDivider />
            {/if}
          {/if}

          {#each activeCustom ? otherCustomPlaylists : playlistsStore.customPlaylists as pl (pl.id)}
            <button
              type="button"
              onclick={() => handleAddToCustomPlaylist(pl)}
              class="w-full text-left px-3 py-1.5 flex items-center gap-2 transition-colors hover:bg-brand-main hover:text-brand-text-primary"
              role="menuitem"
            >
              <PlaylistIcon class="w-3.5 h-3.5 shrink-0 text-brand-text-secondary" />
              <span class="truncate flex-1" title={pl.name}>{pl.name}</span>
            </button>
          {/each}
        {/if}

        <ContextMenuDivider />

        <button
          type="button"
          onclick={handleOpenNewPlaylistModal}
          class="w-full text-left px-3 py-1.5 flex items-center gap-2 transition-colors hover:bg-brand-accent/15 hover:text-brand-accent-text text-brand-accent-text font-medium"
          role="menuitem"
        >
          <Plus class="w-3.5 h-3.5 shrink-0" />
          <span>{i18n.t("playlists.contextMenuNewPlaylist", {}, "New Playlist...")}</span>
        </button>
      {/snippet}
    </ContextMenuSubmenu>

    {#if !allSelectedRemote}
      <AddToDeviceMenuItem
        songIds={selectedSongIds && selectedSongIds.length > 0 ? selectedSongIds : [song.id]}
        {onClose}
      />
    {/if}

    <ContextMenuItem
      icon={song.not_included ? Eye : EyeSlash}
      label={song.not_included
        ? i18n.t("playlists.contextMenuIncludeInPlaylists")
        : i18n.t("playlists.contextMenuMarkNotIncluded")}
      onclick={() => {
        handleToggleNotIncluded();
        onClose();
      }}
    />

    {#if selectedCount === 1}
      <ContextMenuItem
        icon={HeartIcon}
        label={song.loved === 1 || (song.loved === undefined && song.rating === 5)
          ? i18n.t("rating.unfavoriteTooltip")
          : i18n.t("rating.favoriteTooltip")}
        onclick={() => {
          handleToggleFavourite();
          onClose();
        }}
      />

      <ContextMenuItem
        icon={HeartBreakIcon}
        label={song.loved === -1 ? i18n.t("rating.clearHateTooltip") : i18n.t("rating.hateAction")}
        onclick={() => {
          handleToggleDislike();
          onClose();
        }}
      />

      <ContextMenuItem
        icon={BarChart2}
        label={statsExclusionsStore.isSongExcluded(song)
          ? i18n.t("stats.includeInStats")
          : i18n.t("stats.excludeFromStats")}
        onclick={() => {
          handleToggleStatsExcluded();
          onClose();
        }}
      />

      <ContextMenuDivider />

      {#if onGoToArtist && song.artist}
        <ContextMenuItem
          icon={Mic2}
          label={i18n.t("playlists.contextMenuGoArtist")}
          onclick={() => {
            onGoToArtist?.();
            onClose();
          }}
        />
      {/if}

      {#if onGoToAlbum && song.album}
        <ContextMenuItem
          icon={DiscAlbum}
          label={i18n.t("playlists.contextMenuGoAlbum")}
          onclick={() => {
            onGoToAlbum?.();
            onClose();
          }}
        />
      {/if}

      {#if showViewInFiles}
        <ViewInFilesItem songId={song.id} {onClose} />
      {/if}

      {#if song.artist}
        <ContextMenuItem
          icon={Share}
          label={i18n.t("shareModal.menuItem")}
          onclick={() => {
            menuVisible = false;
            showShareModal = true;
          }}
        />
      {/if}

      <ContextMenuItem
        icon={pinnedStore.isPinned("song", String(song.id)) ? PinOff : Pin}
        label={pinnedStore.isPinned("song", String(song.id))
          ? i18n.t("playlists.contextMenuUnpinHome")
          : i18n.t("playlists.contextMenuPinHome")}
        onclick={() => {
          pinnedStore.toggle("song", String(song.id));
          onClose();
        }}
      />
    {/if}

    <ContextMenuDivider />

    {#if hasLosslessSelected}
      <ContextMenuItem
        icon={ShieldCheck}
        label={selectedCount > 1
          ? i18n.t(
              "collection.verifyAuthenticityCount",
              { count: selectedCount },
              `Verify Audio Authenticity (${selectedCount})`,
            )
          : i18n.t("collection.verifyAuthenticity", {}, "Verify Audio Authenticity")}
        onclick={() => {
          handleVerifyLossless();
          onClose();
        }}
      />
    {/if}

    {#if onOrganizeFiles}
      <ContextMenuItem
        icon={Folder}
        label={i18n.t("organizer.title")}
        onclick={() => {
          onOrganizeFiles?.();
          onClose();
        }}
      />
    {/if}

    {#if (selectedCount === 1 && onEditTags) || (selectedCount > 1 && onEditBatchTags)}
      <ContextMenuItem
        icon={Edit3}
        label={i18n.t("collection.editTagsTooltip")}
        onclick={() => {
          if (selectedCount === 1) onEditTags?.();
          else onEditBatchTags?.();
          onClose();
        }}
      />
    {/if}

    {#if selectedCount === 1 && isOnlineLyricsSource(song.lyrics_source)}
      <ContextMenuItem
        icon={Prohibit}
        label={i18n.t("lyrics.rejectMatch", {}, "Reject Online Lyrics")}
        onclick={async () => {
          const songId = song.id;
          onClose();
          try {
            const nextLyrics = await invoke<LyricsPayload | null>("reject_song_lyrics", { songId });
            if (playerStore.currentSong && playerStore.currentSong.id === songId) {
              playerStore.currentSong.lyrics = nextLyrics?.text;
              playerStore.currentSong.lyrics_source = nextLyrics?.source;
            }
            toastStore.show(
              i18n.t("lyrics.rejectMatchSuccess", {}, "Lyrics match rejected and blocklisted"),
              "success",
            );
          } catch (e: any) {
            toastStore.show(e.toString(), "error");
          }
        }}
      />
    {/if}

    {#if onOpenInPicard}
      <ContextMenuItem
        icon={OpenInPicard}
        label={i18n.t("picard.openInPicard")}
        onclick={() => {
          onOpenInPicard?.();
          onClose();
        }}
        disabled={!picardStore.available || allSelectedRemote}
        title={!picardStore.available
          ? i18n.t("picard.notFoundTooltip")
          : allSelectedRemote
            ? i18n.t("picard.remoteNotSupportedTooltip")
            : undefined}
      />
    {/if}

    {#if trailing}
      <ContextMenuDivider />
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div>{@render trailing()}</div>
    {/if}
  </ContextMenu>
{/if}

{#if showShareModal && song.artist}
  <ShareModal
    entity={{ kind: "artist", artistName: song.artist }}
    onClose={() => {
      showShareModal = false;
      onClose();
    }}
  />
{/if}

{#if showNewPlaylistModal}
  <Modal
    onClose={() => {
      showNewPlaylistModal = false;
      onClose();
    }}
    maxWidth="max-w-sm"
  >
    <div
      class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main"
    >
      <div class="flex items-center gap-2">
        <ListPlus class="w-4 h-4 text-brand-accent-text" />
        <h3 class="text-sm font-bold">
          {i18n.t("playlists.newPlaylistModalTitle", {}, "New Playlist")}
        </h3>
      </div>
      <button
        type="button"
        onclick={() => {
          showNewPlaylistModal = false;
          onClose();
        }}
        class="text-brand-text-secondary hover:text-brand-text-primary transition-colors"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <div class="p-6 flex flex-col gap-4">
      <label class="text-xs text-brand-text-secondary font-medium" for="new-playlist-name-input">
        {i18n.t("playlists.newPlaylistPrompt", {}, "Enter a name for the new playlist:")}
      </label>
      <input
        id="new-playlist-name-input"
        type="text"
        use:autofocus
        bind:value={newPlaylistName}
        placeholder={i18n.t("playlists.untitledPlaylistName", {}, "Untitled Playlist")}
        onkeydown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            handleConfirmCreatePlaylist();
          } else if (e.key === "Escape") {
            e.preventDefault();
            showNewPlaylistModal = false;
            onClose();
          }
        }}
        class="w-full bg-brand-sidebar border border-brand-border rounded-lg px-3 py-2 text-sm text-brand-text-primary focus:outline-none focus:border-brand-accent transition-colors"
      />
      <div class="flex justify-end gap-2 mt-2">
        <Button
          variant="secondary"
          onclick={() => {
            showNewPlaylistModal = false;
            onClose();
          }}
        >
          {i18n.t("common.cancel", {}, "Cancel")}
        </Button>
        <Button variant="primary" onclick={handleConfirmCreatePlaylist}>
          {i18n.t("common.create")}
        </Button>
      </div>
    </div>
  </Modal>
{/if}
