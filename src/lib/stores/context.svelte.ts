import { invoke } from "@tauri-apps/api/core";
import type { ArtistEvent, ArtistProfile, AlbumProfile, Song, SongContextEnrichment } from "../types";
import { collectionStore } from "./collection.svelte";
import { i18n } from "./i18n.svelte";
import { prefs } from "./prefs.svelte";
import {
  deriveFanartTvUrlFromMbid,
  deriveMusicbrainzArtistUrl,
  deriveMusicbrainzEventsUrl,
  formatDisplayLabel,
  isBlacklistedLink,
  normalizeWebsitePlatform,
  resolveArtistMbid,
  resolveSocialUrl,
} from "../utils/artistSocials";

/** What a set of info sections describes. `song` is a representative song of an album/artist. */
export interface ContextSubject {
  kind: "song" | "album" | "artist";
  /** Changes if and only if the subject changes. */
  key: string;
  /** The artist name, album name or song's artist, used for events and profile lookups. */
  name: string;
  song: Song;
}

interface LinkItem {
  key: string;
  platform: string;
  url: string;
  label: string;
  /** The subject's own official site, rendered more prominently than a cross-reference. */
  isOfficial: boolean;
}

interface ListenBrainzRow {
  label: string;
  id: string;
  url: string;
  name?: string;
}

/** An info section the store has decided is worth showing; the order of the list is the display order. */
export type InfoSection =
  | { id: "facts"; context: SongContextEnrichment }
  | { id: "bio"; text: string; source: "profile" | "wikipedia"; wikipediaUrl?: string }
  | {
      id: "events";
      events: ArtistEvent[];
      loading: boolean;
      artistName: string;
      songkickUrl: string | null;
      setlistfmUrl: string | null;
      bandsintownUrl: string | null;
      musicbrainzUrl: string | null;
    }
  | { id: "links"; items: LinkItem[] }
  | {
      id: "external";
      listenbrainz: { url: string; rows: ListenBrainzRow[] } | null;
      critiquebrainz: { rating: number; count?: number; releaseGroupMbid?: string } | null;
    };

export interface CommunityRatingInfo {
  rating: number;
  count: number | null;
  source: "critiquebrainz" | "musicbrainz";
}

export interface ContextViewOptions {
  /** Whether a song subject should also show (and so fetch) its artist's events. Artists always do. */
  songEvents?: () => boolean;
}

const hasMbid = (id: string | null | undefined) => !!id && id.trim().length > 0;

/**
 * The song that stands in for an album or artist when looking up its context: the backend
 * resolves MusicBrainz IDs from a song row, so prefer one that carries them.
 */
function representativeSong(kind: "album" | "artist", songs: Song[]): Song | undefined {
  const preferred =
    kind === "artist"
      ? songs.find((s) => hasMbid(s.musicbrainz_artist_id) || hasMbid(s.musicbrainz_album_artist_id))
      : songs.find((s) => hasMbid(s.musicbrainz_release_group_id));
  return preferred ?? songs[0];
}

/** The subject for a viewed album or artist, or null if it has no songs to look it up by. */
export function entitySubject(kind: "album" | "artist", name: string, songs: Song[]): ContextSubject | null {
  const song = representativeSong(kind, songs);
  return song ? { kind, key: `${kind}:${name}`, name, song } : null;
}

interface Entry {
  context?: SongContextEnrichment;
  events?: ArtistEvent[];
  loadingContext: boolean;
  loadingEvents: boolean;
  error: string;
}

function linkItems(
  website: string | null | undefined,
  links: { platform: string; handle_or_url: string }[] | undefined,
  extra: Omit<LinkItem, "isOfficial">[]
): LinkItem[] {
  const items: LinkItem[] = [];
  if (website?.trim() && !isBlacklistedLink(website, "website")) {
    const url = resolveSocialUrl("website", website);
    items.push({
      key: "website",
      platform: normalizeWebsitePlatform("website", url),
      url,
      label: formatDisplayLabel("website", website),
      isOfficial: true,
    });
  }
  for (const link of links ?? []) {
    if (isBlacklistedLink(link.handle_or_url, link.platform)) continue;
    const url = resolveSocialUrl(link.platform, link.handle_or_url);
    items.push({
      key: `${link.platform}:${link.handle_or_url}`,
      platform: normalizeWebsitePlatform(link.platform, url),
      url,
      label: formatDisplayLabel(link.platform, link.handle_or_url),
      // A secondary "official homepage" is stored under platform "website" too.
      isOfficial: link.platform === "website",
    });
  }
  for (const e of extra) items.push({ ...e, isOfficial: false });
  // Official homepage(s) lead as a group, ahead of the alphabetical sort.
  return items.sort((a, b) => {
    if (a.isOfficial !== b.isOfficial) return a.isOfficial ? -1 : 1;
    if (a.key === "website") return -1;
    if (b.key === "website") return 1;
    return a.label.localeCompare(b.label);
  });
}

function listenbrainz(subject: ContextSubject): { url: string; rows: ListenBrainzRow[] } | null {
  const song = subject.song;
  const albumMbid = song.musicbrainz_release_group_id || song.musicbrainz_album_id;
  const albumPath = song.musicbrainz_release_group_id ? "album" : "release";
  const recordingMbid = song.musicbrainz_recording_id || song.musicbrainz_track_id;
  const recordingPath = song.musicbrainz_recording_id ? "recording" : "track";
  const t = (key: string, fallback: string) => i18n.t(`playerBar.${key}`, {}, fallback);
  const rows: ListenBrainzRow[] = [
    {
      label: t("listenbrainzArtistLabel", "Artist"),
      id: song.musicbrainz_artist_id ?? "",
      url: song.musicbrainz_artist_id ? `https://listenbrainz.org/artist/${song.musicbrainz_artist_id}/` : "",
      name: song.artist ?? undefined,
    },
    ...(song.musicbrainz_album_artist_id && song.musicbrainz_album_artist_id !== song.musicbrainz_artist_id
      ? [
          {
            label: t("listenbrainzAlbumArtistLabel", "Album Artist"),
            id: song.musicbrainz_album_artist_id,
            url: `https://listenbrainz.org/artist/${song.musicbrainz_album_artist_id}/`,
            name: song.album_artist ?? undefined,
          },
        ]
      : []),
    {
      label: t("listenbrainzAlbumLabel", "Album"),
      id: albumMbid ?? "",
      url: albumMbid ? `https://listenbrainz.org/${albumPath}/${albumMbid}/` : "",
      name: song.album ?? undefined,
    },
    // A recording belongs to a song, not to an album or artist.
    ...(subject.kind === "song"
      ? [
          {
            label: t("listenbrainzRecordingLabel", "Track"),
            id: recordingMbid ?? "",
            url: recordingMbid ? `https://listenbrainz.org/${recordingPath}/${recordingMbid}/` : "",
            name: song.title,
          },
        ]
      : []),
  ].filter((r) => r.id);
  if (rows.length === 0) return null;
  const url = albumMbid
    ? `https://listenbrainz.org/${albumPath}/${albumMbid}/`
    : song.musicbrainz_artist_id
      ? `https://listenbrainz.org/artist/${song.musicbrainz_artist_id}/`
      : "https://listenbrainz.org";
  return { url, rows };
}

/**
 * A live, ordered list of the info sections worth showing for one subject. Create with
 * `contextStore.for()` during component init: it owns an effect that keeps the cache filled.
 */
export class ContextView {
  private readonly entry = $derived.by(() => {
    const subject = this.getSubject();
    return subject ? contextStore.entry(subject) : undefined;
  });

  constructor(
    private readonly getSubject: () => ContextSubject | null,
    private readonly options: ContextViewOptions
  ) {
    // Selection changes far more often than tracks do (arrowing through rows), so debounce the fetch.
    $effect(() => {
      const subject = this.getSubject();
      const online = prefs.onlineEnabled;
      void i18n.currentLocale;
      if (!subject || !online) return;
      const events = this.wantsEvents(subject);
      const timer = setTimeout(() => void contextStore.ensure(subject, { events }), 150);
      return () => clearTimeout(timer);
    });
  }

  private wantsEvents(subject: ContextSubject): boolean {
    return subject.kind === "artist" || (subject.kind === "song" && !!this.options.songEvents?.());
  }

  /** The raw fetched enrichment, for consumers that show fields the sections don't (e.g. MusicBrainz tags). */
  get context(): SongContextEnrichment | null {
    return this.entry?.context ?? null;
  }

  get loading(): boolean {
    return !!this.entry?.loadingContext;
  }

  get error(): string {
    return this.entry?.error ?? "";
  }

  /** True when network-backed sections are dropped because online features are off. */
  get offline(): boolean {
    return !prefs.onlineEnabled;
  }

  /** The release group's CritiqueBrainz rating, else its MusicBrainz rating. */
  get communityRating(): CommunityRatingInfo | null {
    const ctx = this.context;
    if (!ctx) return null;
    if (ctx.critiquebrainz_rating != null) {
      return { rating: ctx.critiquebrainz_rating, count: ctx.critiquebrainz_review_count || null, source: "critiquebrainz" };
    }
    if (ctx.mb_rating != null) {
      return { rating: ctx.mb_rating, count: ctx.mb_rating_votes || null, source: "musicbrainz" };
    }
    return null;
  }

  get sections(): InfoSection[] {
    const subject = this.getSubject();
    if (!subject) return [];
    const ctx = this.context;
    const sections: InfoSection[] = [];
    const artistProfile: ArtistProfile | undefined =
      subject.kind === "album" ? undefined : collectionStore.getArtistProfile(subject.kind === "artist" ? subject.name : subject.song.artist);
    const albumProfile: AlbumProfile | undefined =
      subject.kind === "album" ? collectionStore.getAlbumProfile(subject.name) : undefined;

    if (subject.kind !== "album" && ctx && (ctx.artist_begin_date || ctx.artist_end_date || ctx.artist_begin_area_name || ctx.artist_area_name)) {
      sections.push({ id: "facts", context: ctx });
    }

    // The user's own bio (personal curation) beats the fetched Wikipedia extract, which describes the artist.
    if (subject.kind === "album") {
      const description = albumProfile?.description?.trim();
      if (description) sections.push({ id: "bio", text: description, source: "profile" });
    } else {
      const own = subject.kind === "artist" ? artistProfile?.bio?.trim() : "";
      if (own) {
        sections.push({ id: "bio", text: own, source: "profile" });
      } else if (ctx?.wikipedia_extract) {
        sections.push({ id: "bio", text: ctx.wikipedia_extract, source: "wikipedia", wikipediaUrl: ctx.wikipedia_page_url });
      }
    }

    const artistMbid =
      resolveArtistMbid(artistProfile?.musicbrainz_artist_id, artistProfile?.social_links) ||
      subject.song.musicbrainz_artist_id?.trim() ||
      null;
    if (prefs.onlineEnabled && this.wantsEvents(subject)) {
      const linkFor = (platform: string) => artistProfile?.social_links?.find((l) => l.platform === platform)?.handle_or_url ?? null;
      const songkickUrl = linkFor("songkick");
      const setlistfmUrl = linkFor("setlistfm");
      const bandsintownUrl = linkFor("bandsintown");
      const events = this.entry?.events ?? [];
      if (events.length > 0 || songkickUrl || setlistfmUrl || bandsintownUrl || artistMbid) {
        sections.push({
          id: "events",
          events,
          loading: !!this.entry?.loadingEvents,
          artistName: subject.kind === "artist" ? subject.name : (subject.song.artist ?? ""),
          songkickUrl,
          setlistfmUrl,
          bandsintownUrl,
          musicbrainzUrl: deriveMusicbrainzEventsUrl(artistMbid),
        });
      }
    }

    if (subject.kind === "artist") {
      const musicbrainz = deriveMusicbrainzArtistUrl(artistMbid);
      const fanart = deriveFanartTvUrlFromMbid(artistMbid);
      // MusicBrainz stays here: an artist has no Technical tab, which is where its block lives for a song.
      const items = linkItems(artistProfile?.website, artistProfile?.social_links, [
        ...(musicbrainz ? [{ key: "musicbrainz-derived", platform: "musicbrainz", url: musicbrainz, label: "MusicBrainz" }] : []),
        ...(fanart ? [{ key: "fanart-tv", platform: "fanart_tv", url: fanart, label: "Fanart.tv" }] : []),
      ]);
      if (items.length > 0) sections.push({ id: "links", items });
    } else if (subject.kind === "album") {
      const items = linkItems(albumProfile?.website, albumProfile?.links, []);
      if (items.length > 0) sections.push({ id: "links", items });
    }

    const lb = listenbrainz(subject);
    const cb =
      subject.kind !== "artist" && ctx?.critiquebrainz_rating != null
        ? {
            rating: ctx.critiquebrainz_rating,
            count: ctx.critiquebrainz_review_count,
            releaseGroupMbid: subject.song.musicbrainz_release_group_id ?? undefined,
          }
        : null;
    if (lb || cb) sections.push({ id: "external", listenbrainz: lb, critiquebrainz: cb });

    return sections;
  }

  /** Re-fetches the subject's context (and events) from the network, bypassing the backend cache. */
  async refresh(): Promise<void> {
    const subject = this.getSubject();
    if (subject) await contextStore.refresh(subject, this.wantsEvents(subject));
  }
}

/**
 * The single place artist/album/song context is fetched and shaped: the info sidebar and the
 * detail views' narrow-window fallback both read `for(subject)`, so each piece shows once and a
 * subject is requested once. Callers never see the fixed section order or the bio precedence.
 */
class ContextStore {
  /** Fetched data by subject, locale and representative song, so a stale reply never shows for another subject. */
  private entries = $state<Record<string, Entry>>({});
  private inflight = new Map<string, Promise<void>>();

  private cacheKey(subject: ContextSubject): string {
    return `${i18n.currentLocale}|${subject.key}|${subject.song.id}`;
  }

  entry(subject: ContextSubject): Entry | undefined {
    return this.entries[this.cacheKey(subject)];
  }

  /** Subscribes the caller to the sections of the subject returned by `getSubject`. Call during component init. */
  for(getSubject: () => ContextSubject | null, options: ContextViewOptions = {}): ContextView {
    return new ContextView(getSubject, options);
  }

  /** Fills the cache for `subject`; concurrent calls for the same data share one request. */
  ensure(subject: ContextSubject, opts: { events: boolean; force?: boolean }): Promise<void> {
    const key = this.cacheKey(subject);
    const force = !!opts.force;
    const current = this.entries[key];
    const needContext = force || !current?.context;
    const needEvents = opts.events && (force || !current?.events);
    if (!needContext && !needEvents) return Promise.resolve();

    const flightKey = `${key}|${needContext}|${needEvents}|${force}`;
    const existing = this.inflight.get(flightKey);
    if (existing) return existing;

    const patch = (p: Partial<Entry>) => {
      this.entries[key] = { ...(this.entries[key] ?? { loadingContext: false, loadingEvents: false, error: "" }), ...p };
    };
    const songId = subject.song.id;
    const run = (async () => {
      patch({ loadingContext: needContext, loadingEvents: needEvents, ...(needContext ? { error: "" } : {}) });
      const context = needContext
        ? invoke<SongContextEnrichment>("get_song_context", { songId, forceRefresh: force, locale: i18n.currentLocale }).then(
            (data) => patch({ context: data, loadingContext: false }),
            (e) => patch({ error: e instanceof Error ? e.message : String(e), loadingContext: false })
          )
        : Promise.resolve();
      const events = needEvents
        ? invoke<ArtistEvent[]>("get_artist_events", {
            artist: subject.kind === "artist" ? subject.name : subject.song.artist,
            songId,
            ...(force ? { forceRefresh: true } : {}),
          }).then(
            (data) => patch({ events: data || [], loadingEvents: false }),
            () => patch({ events: [], loadingEvents: false })
          )
        : Promise.resolve();
      await Promise.all([context, events]);
    })().finally(() => this.inflight.delete(flightKey));
    this.inflight.set(flightKey, run);
    return run;
  }

  /**
   * Re-fetches `subject` (and its artist's events) from the network, bypassing the backend cache.
   * For callers that curate data and only need it fresh, without reading sections themselves.
   * A missing subject (an album/artist with no songs) has nothing to refresh.
   */
  async refresh(subject: ContextSubject | null, events = subject?.kind === "artist"): Promise<void> {
    if (subject && prefs.onlineEnabled) await this.ensure(subject, { events, force: true });
  }

  /** Drops everything fetched (test helper). */
  clearAll(): void {
    this.entries = {};
    this.inflight.clear();
  }
}

export const contextStore = new ContextStore();
