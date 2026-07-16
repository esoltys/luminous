// Small, hand-picked fixture library used by the Tauri IPC mock when no local
// database is configured (see mock-config.example.json). Intentionally tiny —
// this used to be a ~8,000-line dump of one real library; a handful of
// albums is enough to exercise every view in the UI.
import type { Playlist, Song } from "../src/lib/types/index";

interface TrackSeed {
  title: string;
  lengthSec: number;
  lastplayed?: number;
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
      { title: "Transmission Ley Lines", lengthSec: 315 },
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
      { title: "Metro Sunset", lengthSec: 182 },
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
      { title: "You Wreck Me", lengthSec: 203, lastplayed: 10003 },
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
      { title: "Free Fallin'", lengthSec: 259 },
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
      { title: "Risk It All", lengthSec: 204 },
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
      { title: "Heartbeat Highway", lengthSec: 221 },
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
      { title: "Dandelion", lengthSec: 241 },
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
      { title: "Run", lengthSec: 200 },
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
    playcount: 0,
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
  { id: 1, name: "Chill Midnight", dynamic_enabled: false, created: 1782800000000, track_count: 3 },
  { id: 2, name: "Heavy Riffs", dynamic_enabled: false, created: 1782810000000, track_count: 2 },
  { id: 3, name: "Acoustic Morning", dynamic_enabled: false, created: 1782820000000, track_count: 6 },
];

export const FALLBACK_LYRICS = `[00:00.00] Daft Punk - Get Lucky
[00:08.00] Like the legend of the phoenix
[00:12.00] All ends with beginnings
[00:16.00] What keeps the planet spinning
[00:20.00] The force from the beginning
[00:24.00] We've come too far to give up who we are
[00:31.00] So let's raise the bar and our cups to the stars
[00:39.00] She's up all night 'til the sun
[00:41.00] I'm up all night to get some
[00:43.00] She's up all night for good fun
[00:45.00] I'm up all night to get lucky
[00:48.00] We're up all night 'til the sun
[00:50.00] We're up all night to get some
[00:52.00] We're up all night for good fun
[00:54.00] We're up all night to get lucky
[00:57.00] We're up all night to get lucky`;
