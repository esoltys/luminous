// Small, hand-picked fixture library used by the Tauri IPC mock when no local
// database is configured (see mock-config.example.json). Intentionally tiny —
// this used to be a ~8,000-line dump of one real library; a handful of
// albums is enough to exercise every view in the UI.
import type { Playlist, Song } from "../src/lib/types/index";

interface TrackSeed {
  title: string;
  lengthSec: number;
  lastplayed?: number;
  playcount?: number;
}

interface AlbumSeed {
  artist: string;
  album: string;
  genre: string;
  year: number;
  art: string;
  artEmbedded?: boolean;
  bitrate: number;
  samplerate: number;
  tracks: TrackSeed[];
}

const ALBUM_SEEDS: AlbumSeed[] = [
  {
    artist: "Eric Soltys",
    album: "Basin",
    genre: "Ambient",
    year: 2025,
    art: "eric_soltys_basin.jpg",
    bitrate: 850,
    samplerate: 48000,
    tracks: [
      { title: "Transmission Ley Lines", lengthSec: 315, playcount: 12 },
      { title: "Reverb from the Reservoir", lengthSec: 426 },
      { title: "Wind Carries the Signal", lengthSec: 358 },
    ],
  },
  {
    artist: "Eric Soltys",
    album: "Contour",
    genre: "Ambient",
    year: 2024,
    art: "eric_soltys_contour.jpg",
    bitrate: 750,
    samplerate: 48000,
    tracks: [
      { title: "Origami Heartbeat", lengthSec: 95 },
      { title: "Metro Sunset", lengthSec: 182, playcount: 8 },
      { title: "Kaleidoscope Dreams", lengthSec: 140 },
    ],
  },
  {
    artist: "Eric Soltys",
    album: "Horizon",
    genre: "Ambient",
    year: 2024,
    art: "eric_soltys_horizon.jpg",
    bitrate: 700,
    samplerate: 48000,
    tracks: [
      { title: "Kaslo Mornings", lengthSec: 179 },
      { title: "Summer at Syringa", lengthSec: 162 },
      { title: "Columbia River Mist", lengthSec: 85 },
    ],
  },
  {
    artist: "Tom Petty",
    album: "Wildflowers",
    genre: "Rock",
    year: 1994,
    art: "tom_petty_wildflowers.jpg",
    artEmbedded: false,
    bitrate: 5200,
    samplerate: 192000,
    tracks: [
      { title: "Wildflowers", lengthSec: 200 },
      { title: "You Don't Know How It Feels", lengthSec: 278 },
      { title: "You Wreck Me", lengthSec: 203, lastplayed: 10003, playcount: 20 },
    ],
  },
  {
    artist: "Tom Petty",
    album: "Full Moon Fever",
    genre: "Rock",
    year: 1989,
    art: "tom_petty_full_moon_fever.jpg",
    artEmbedded: false,
    bitrate: 900,
    samplerate: 44100,
    tracks: [
      { title: "Free Fallin'", lengthSec: 259, lastplayed: 9500, playcount: 15 },
      { title: "I Won't Back Down", lengthSec: 170 },
      { title: "Runnin' Down a Dream", lengthSec: 259 },
    ],
  },
  {
    artist: "Bruno Mars",
    album: "The Romantic",
    genre: "R&B",
    year: 2026,
    art: "bruno_mars_the_romantic.jpg",
    bitrate: 2900,
    samplerate: 96000,
    tracks: [
      { title: "Risk It All", lengthSec: 204, lastplayed: 8000, playcount: 5 },
      { title: "Cha Cha Cha", lengthSec: 237 },
      { title: "I Just Might", lengthSec: 213 },
    ],
  },
  {
    artist: "Cannons",
    album: "Heartbeat Highway",
    genre: "Electronic",
    year: 2023,
    art: "cannons_heartbeat_highway.jpg",
    bitrate: 980,
    samplerate: 44100,
    tracks: [
      { title: "Heartbeat Highway", lengthSec: 221, playcount: 9 },
      { title: "Crush", lengthSec: 181 },
      { title: "Metal Heart", lengthSec: 174 },
    ],
  },
  {
    artist: "Ella Langley",
    album: "Dandelion",
    genre: "Country",
    year: 2026,
    art: "ella_langley_dandelion.jpg",
    bitrate: 900,
    samplerate: 44100,
    tracks: [
      { title: "Dandelion", lengthSec: 241, lastplayed: 7000 },
      { title: "Choosin' Texas", lengthSec: 232 },
      { title: "We Know Us", lengthSec: 186 },
    ],
  },
  {
    artist: "OneRepublic",
    album: "Human",
    genre: "Pop",
    year: 2021,
    art: "onerepublic_human.jpg",
    bitrate: 950,
    samplerate: 44100,
    tracks: [
      { title: "Run", lengthSec: 200, playcount: 3 },
      { title: "Better Days", lengthSec: 190 },
      { title: "Someday", lengthSec: 210 },
    ],
  },
];

let nextId = 1;

function buildSong(seed: AlbumSeed, track: TrackSeed, index: number): Song {
  const lengthNanosec = track.lengthSec * 1_000_000_000;
  const filesize = Math.round((seed.bitrate * 1000 * track.lengthSec) / 8);
  return {
    id: nextId++,
    source: "local_file",
    filetype: "FLAC",
    path: `/Music/${seed.artist}/${seed.album}/${String(index + 1).padStart(2, "0")} ${track.title}.flac`,
    title: track.title,
    artist: seed.artist,
    album: seed.album,
    genre: seed.genre,
    year: seed.year,
    track: index + 1,
    disc: 1,
    compilation: false,
    length_nanosec: lengthNanosec,
    beginning_nanosec: 0,
    end_nanosec: 0,
    bitrate: seed.bitrate,
    samplerate: seed.samplerate,
    channels: 2,
    filesize,
    rating: -1,
    playcount: track.playcount ?? 0,
    skipcount: 0,
    lastplayed: track.lastplayed,
    added: 1783727350,
    art_embedded: seed.artEmbedded ?? true,
    art_automatic: `/fixtures/${seed.art}`,
    art_unset: false,
    unavailable: false,
  };
}

export const FALLBACK_SONGS: Song[] = ALBUM_SEEDS.flatMap((seed) =>
  seed.tracks.map((track, index) => buildSong(seed, track, index))
);

export const FALLBACK_PLAYLISTS: Playlist[] = [
  { id: 1, name: "Chill Midnight", dynamic_enabled: false, created: 1782800000000, updated: 1782800000000, track_count: 3 },
  { id: 2, name: "Heavy Riffs", dynamic_enabled: false, created: 1782810000000, updated: 1782810000000, track_count: 2 },
  { id: 3, name: "Acoustic Morning", dynamic_enabled: false, created: 1782820000000, updated: 1782820000000, track_count: 6 },
  { id: 4, name: "Indie Rock", dynamic_enabled: true, dynamic_spec: "Indie Rock", created: 1782830000000, updated: 1782830000000, track_count: 8 },
  { id: 5, name: "Queue", dynamic_enabled: false, created: 1782840000000, updated: 1782840000000, track_count: 4 },
];

export const FEATURED_YOU_WRECK_ME_LYRICS = `[00:00.00] Tom Petty - You Wreck Me
[00:12.00] Tonight we're gonna run
[00:15.50] Have ourselves some fun
[00:19.00] Hope we don't get caught
[00:22.50] Keep quiet or we might
[00:26.00] Oh yeah, you wreck me, baby
[00:30.00] You break me in two
[00:33.50] But you move me, honey
[00:37.00] Yes you do
[00:41.00] Flyin' high again
[00:44.50] Watch out for the bend
[00:48.00] Don't look down my friend
[00:51.50] We're coming to the end
[00:55.00] Oh yeah, you wreck me, baby
[00:59.00] You break me in two
[01:02.50] But you move me, honey
[01:06.00] Yes you do
[01:10.00] Now and then I find
[01:13.50] You cross my mind
[01:17.00] Good love is hard to find
[01:20.50] You're custom made, you're one of a kind
[01:24.00] Oh yeah, you wreck me, baby
[01:28.00] You break me in two
[01:31.50] But you move me, honey
[01:35.00] Yes you do`;

export function getFallbackLyricsForTrack(title?: string, artist?: string): string {
  const cleanTitle = (title || "Track").trim();
  const cleanArtist = (artist || "Artist").trim();

  if (cleanTitle.toLowerCase() === "you wreck me" || cleanTitle.toLowerCase().includes("wreck me")) {
    return FEATURED_YOU_WRECK_ME_LYRICS;
  }

  return `[00:00.00] ${cleanArtist} - ${cleanTitle}
[00:08.00] Sound of the rhythm in the quiet room
[00:14.00] Watching the light fading into afternoon
[00:22.00] Echoes of frequencies across the wire
[00:29.00] Lighting up the sparks of a steady fire
[00:37.00] Step by step we are moving through the beat
[00:44.00] Every single waveform crisp and clean
[00:52.00] Keeping the balance through the line and sound
[01:01.00] Best melody that can ever be found
[01:10.00] Oh yeah, feeling the rhythm carry through
[01:18.00] Every single moment belonging to you
[01:27.00] Resonating harmonic and strong
[01:35.00] Right where we belong`;
}

export const FALLBACK_LYRICS = FEATURED_YOU_WRECK_ME_LYRICS;

