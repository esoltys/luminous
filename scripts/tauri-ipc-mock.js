// scripts/tauri-ipc-mock.js
(function () {
  console.log("[Tauri Mock] Initializing Tauri IPC Mock layer...");

  // Default Mock Settings
  window.mockSettings = window.mockSettings || {
    active_theme_id: "nordic-blue",
    custom_themes: "[]",
    active_tab: "collection",
    active_sub_tab: "songs",
    excluded_formats: "[]"
  };

  const mockSongs = [
    {
      id: 1,
      path: "/music/Daft Punk - Get Lucky.mp3",
      filetype: "MP3",
      title: "Get Lucky",
      artist: "Daft Punk",
      album: "Random Access Memories",
      genre: "Electronic",
      year: 2013,
      track: 6,
      disc: 1,
      length_nanosec: 369000000000, // 6:09
      beginning_nanosec: 0,
      end_nanosec: 369000000000,
      bitrate: 320,
      samplerate: 44100,
      channels: 2,
      filesize: 14760000,
      rating: 5,
      playcount: 42,
      skipcount: 0,
      lastplayed: 1720900000,
      added: 1700000000,
      art_embedded: true,
      art_automatic: "/fixtures/random_access_memories.png",
      art_unset: false,
      compilation: false
    },
    {
      id: 2,
      path: "/music/The Weeknd - After Hours.mp3",
      filetype: "FLAC",
      title: "After Hours",
      artist: "The Weeknd",
      album: "After Hours",
      genre: "R&B/Pop",
      year: 2020,
      track: 13,
      disc: 1,
      length_nanosec: 361000000000, // 6:01
      beginning_nanosec: 0,
      end_nanosec: 361000000000,
      bitrate: 980,
      samplerate: 48000,
      channels: 2,
      filesize: 44200000,
      rating: 5,
      playcount: 85,
      skipcount: 1,
      lastplayed: 1720850000,
      added: 1705000000,
      art_embedded: true,
      art_automatic: "/fixtures/after_hours.png",
      art_unset: false,
      compilation: false
    },
    {
      id: 3,
      path: "/music/Def Leppard - Pour Some Sugar on Me.mp3",
      filetype: "MP3",
      title: "Pour Some Sugar on Me",
      artist: "Def Leppard",
      album: "Hysteria",
      genre: "Hard Rock",
      year: 1987,
      track: 7,
      disc: 1,
      length_nanosec: 270000000000, // 4:30
      beginning_nanosec: 0,
      end_nanosec: 270000000000,
      bitrate: 320,
      samplerate: 44100,
      channels: 2,
      filesize: 10800000,
      rating: 5,
      playcount: 88,
      skipcount: 1,
      lastplayed: 1720920000,
      added: 1710000000,
      art_embedded: true,
      art_automatic: "/fixtures/hysteria.jpg",
      art_unset: false,
      compilation: false
    },
    {
      id: 4,
      path: "/music/Led Zeppelin - Stairway to Heaven.mp3",
      filetype: "MP3",
      title: "Stairway to Heaven",
      artist: "Led Zeppelin",
      album: "Led Zeppelin IV",
      genre: "Classic Rock",
      year: 1971,
      track: 4,
      disc: 1,
      length_nanosec: 482000000000, // 8:02
      beginning_nanosec: 0,
      end_nanosec: 482000000000,
      bitrate: 320,
      samplerate: 44100,
      channels: 2,
      filesize: 19280000,
      rating: 5,
      playcount: 310,
      skipcount: 2,
      lastplayed: 1720880000,
      added: 1695000000,
      art_embedded: true,
      art_automatic: "/fixtures/led_zeppelin_iv.jpg",
      art_unset: false,
      compilation: false
    },
    {
      id: 5,
      path: "/music/Massive Attack - Teardrop.mp3",
      filetype: "MP3",
      title: "Teardrop",
      artist: "Massive Attack",
      album: "Mezzanine",
      genre: "Trip Hop",
      year: 1998,
      track: 3,
      disc: 1,
      length_nanosec: 330000000000, // 5:30
      beginning_nanosec: 0,
      end_nanosec: 330000000000,
      bitrate: 320,
      samplerate: 44100,
      channels: 2,
      filesize: 13200000,
      rating: 4,
      playcount: 67,
      skipcount: 0,
      lastplayed: 1720870000,
      added: 1708000000,
      art_embedded: true,
      art_automatic: "/fixtures/mezzanine.png",
      art_unset: false,
      compilation: false
    },
    {
      id: 6,
      path: "/music/Queen - Bohemian Rhapsody.mp3",
      filetype: "FLAC",
      title: "Bohemian Rhapsody",
      artist: "Queen",
      album: "A Night at the Opera",
      genre: "Rock",
      year: 1975,
      track: 11,
      disc: 1,
      length_nanosec: 355000000000, // 5:55
      beginning_nanosec: 0,
      end_nanosec: 355000000000,
      bitrate: 940,
      samplerate: 44100,
      channels: 2,
      filesize: 41800000,
      rating: 5,
      playcount: 450,
      skipcount: 5,
      lastplayed: 1720930000,
      added: 1690000000,
      art_embedded: true,
      art_automatic: "/fixtures/night_at_the_opera.png",
      art_unset: false,
      compilation: false
    }
  ];

  const mockAlbums = [
    {
      album: "Random Access Memories",
      artist: "Daft Punk",
      year: 2013,
      track_count: 13,
      art_embedded: true,
      art_automatic: "/fixtures/random_access_memories.png",
      art_manual: null
    },
    {
      album: "After Hours",
      artist: "The Weeknd",
      year: 2020,
      track_count: 14,
      art_embedded: true,
      art_automatic: "/fixtures/after_hours.png",
      art_manual: null
    },
    {
      album: "Hysteria",
      artist: "Def Leppard",
      year: 1987,
      track_count: 12,
      art_embedded: true,
      art_automatic: "/fixtures/hysteria.jpg",
      art_manual: null
    },
    {
      album: "Led Zeppelin IV",
      artist: "Led Zeppelin",
      year: 1971,
      track_count: 8,
      art_embedded: true,
      art_automatic: "/fixtures/led_zeppelin_iv.jpg",
      art_manual: null
    },
    {
      album: "Mezzanine",
      artist: "Massive Attack",
      year: 1998,
      track_count: 11,
      art_embedded: true,
      art_automatic: "/fixtures/mezzanine.png",
      art_manual: null
    },
    {
      album: "A Night at the Opera",
      artist: "Queen",
      year: 1975,
      track_count: 12,
      art_embedded: true,
      art_automatic: "/fixtures/night_at_the_opera.png",
      art_manual: null
    }
  ];

  const mockArtists = [
    { name: "Daft Punk", album_count: 1, song_count: 1 },
    { name: "The Weeknd", album_count: 1, song_count: 2 },
    { name: "Pink Floyd", album_count: 1, song_count: 1 },
    { name: "Led Zeppelin", album_count: 1, song_count: 1 },
    { name: "Massive Attack", album_count: 1, song_count: 1 },
    { name: "Queen", album_count: 1, song_count: 1 }
  ];

  const mockPlaylists = [
    { id: 1, name: "Chill Midnight", dynamic_enabled: false, created: 1782800000000, track_count: 3 },
    { id: 2, name: "Heavy Riffs", dynamic_enabled: false, created: 1782810000000, track_count: 2 },
    { id: 3, name: "Acoustic Morning", dynamic_enabled: false, created: 1782820000000, track_count: 6 }
  ];

  const mockLyrics = `[00:00.00] Daft Punk - Get Lucky
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

  const callbacks = {};
  let nextCallbackId = 1;
  const eventListeners = {};

  // Define window.__TAURI_INTERNALS__ and implement invoke & ipc
  window.__TAURI_INTERNALS__ = {
    transformCallback: function (callback, once = false) {
      const id = nextCallbackId++;
      callbacks[id] = (data) => {
        if (once) {
          delete callbacks[id];
        }
        callback(data);
      };
      return id;
    },
    unregisterCallback: function (id) {
      delete callbacks[id];
    },
    invoke: async function (cmd, args = {}) {
      console.log(`[Tauri Mock Invoke] cmd: ${cmd}`, args);

      switch (cmd) {
        case "get_all_app_settings":
          return window.mockSettings;

        case "get_playback_state":
          return {
            state: "playing",
            current_song: mockSongs[0], // Get Lucky
            playlist_id: 1,
            playlist_item_uuid: "item-uuid-1",
            position_nanosec: 122000000000, // 2:02
            volume: 0.75,
            shuffle_mode: "off",
            repeat_mode: "all",
            stop_after_current: false
          };

        case "get_directories":
          return [
            { id: 1, path: "C:\\Users\\ericj\\Music\\Retro Hits", subdirs: true },
            { id: 2, path: "C:\\Users\\ericj\\Music\\Studio Masters", subdirs: true }
          ];

        case "get_library_stats":
          return {
            total_songs: mockSongs.length,
            total_artists: mockArtists.length,
            total_albums: mockAlbums.length,
            total_duration_nanosec: mockSongs.reduce((acc, s) => acc + (s.length_nanosec || 0), 0),
            total_filesize_bytes: mockSongs.reduce((acc, s) => acc + (s.filesize || 0), 0)
          };

        case "get_songs":
          return mockSongs;

        case "get_recently_played":
          return mockSongs
            .filter(s => s.lastplayed)
            .sort((a, b) => (b.lastplayed || 0) - (a.lastplayed || 0))
            .slice(0, args.limit || 10);

        case "get_most_frequently_played":
          return mockSongs
            .sort((a, b) => (b.playcount || 0) - (a.playcount || 0))
            .slice(0, args.limit || 10);

        case "get_recently_added":
          return mockSongs
            .filter(s => s.added)
            .sort((a, b) => (b.added || 0) - (a.added || 0))
            .slice(0, args.limit || 10);

        case "get_albums":
          return mockAlbums;

        case "get_artists":
          return mockArtists;

        case "get_songs_by_album":
          return mockSongs.filter(s => s.album === args.album);

        case "get_playlists":
          return mockPlaylists;

        case "get_playlist_tracks":
          return mockSongs.slice(0, 3).map((song, i) => ({
            id: i + 1,
            playlist_id: args.playlistId,
            position: i,
            uuid: `uuid-${i}`,
            item_type: "song",
            song: song
          }));

        case "get_waveform_data":
          // Create 150 visual bars
          const peaks = [];
          for (let i = 0; i < 150; i++) {
            const angle = (i / 150) * Math.PI * 6;
            const wave = Math.sin(angle) * Math.cos(angle * 2.3) * 0.4 + 0.5;
            const noise = Math.random() * 0.15;
            peaks.push(Math.round(Math.min(1, Math.max(0.1, wave + noise)) * 255));
          }
          return peaks;

        case "get_lyrics":
          return mockLyrics;

        case "plugin:event|listen":
          const { event, handler } = args;
          if (!eventListeners[event]) {
            eventListeners[event] = [];
          }
          eventListeners[event].push(handler);
          return Promise.resolve();

        case "get_equalizer_state":
          return {
            enabled: true,
            preamp: 3.0,
            gains: [10.0, 8.0, 5.0, -3.0, -6.0, -4.0, 3.0, 6.0, 8.0, 10.0]
          };

        case "load_equalizer_preset":
          const presetName = args.presetName;
          const rockGains = [4.0, 3.0, 2.0, -1.0, -2.0, -1.0, 1.0, 2.0, 3.0, 4.0];
          const popGains = [-2.0, -1.0, 0.0, 2.0, 4.0, 4.0, 2.0, 0.0, -1.0, -2.0];
          const classicalGains = [5.0, 3.0, 2.0, 2.0, -1.0, -1.0, 0.0, 2.0, 3.0, 4.0];
          const jazzGains = [3.0, 2.0, 1.0, 2.0, -1.0, -1.0, 0.0, 1.0, 2.0, 3.0];
          const bassBoostGains = [6.0, 5.0, 4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
          const vocalBoostGains = [-2.0, -2.0, -1.0, 1.0, 3.0, 4.0, 3.0, 1.0, -1.0, -2.0];
          const flatGains = Array(10).fill(0.0);
          
          let selectedGains = flatGains;
          if (presetName === "Rock") selectedGains = rockGains;
          else if (presetName === "Pop") selectedGains = popGains;
          else if (presetName === "Classical") selectedGains = classicalGains;
          else if (presetName === "Jazz") selectedGains = jazzGains;
          else if (presetName === "Bass Boost") selectedGains = bassBoostGains;
          else if (presetName === "Vocal Boost") selectedGains = vocalBoostGains;
          
          return {
            enabled: true,
            preamp: 3.0,
            gains: selectedGains
          };

        case "set_app_setting":
          if (args.key) {
            window.mockSettings[args.key] = args.value;
          }
          return null;

        case "set_equalizer_enabled":
        case "set_equalizer_preamp":
        case "set_equalizer_band":
        case "set_spectrum_enabled":
        case "play_song":
        case "play_songs":
        case "play_playlist_item":
        case "pause":
        case "resume":
        case "stop":
        case "next_track":
        case "previous_track":
        case "seek_to":
        case "set_volume":
        case "set_shuffle_mode":
        case "set_repeat_mode":
          return null;

        case "plugin:event|listen": {
          const { event, handler } = args;
          if (!eventListeners[event]) {
            eventListeners[event] = [];
          }
          eventListeners[event].push(handler);
          return handler;
        }

        case "plugin:event|unlisten": {
          const { event, eventId } = args;
          if (eventListeners[event]) {
            eventListeners[event] = eventListeners[event].filter(h => h !== eventId);
          }
          return null;
        }

        default:
          console.warn(`[Tauri Mock] Unhandled command: ${cmd}`, args);
          return null;
      }
    },

    ipc: function (message) {
      // Sometimes Tauri v2 calls internal IPC directly
      console.log("[Tauri Mock IPC] message:", message);
      if (message && message.cmd === "plugin:event|listen") {
        const { event, handler } = message;
        if (!eventListeners[event]) {
          eventListeners[event] = [];
        }
        eventListeners[event].push(handler);
        // callback success
        if (message.callback && window[`_${message.callback}`]) {
          window[`_${message.callback}`]();
        }
        return;
      }

      // fallback to invoke
      if (message && message.cmd) {
        this.invoke(message.cmd, message.params || {})
          .then(res => {
            if (message.callback && window[`_${message.callback}`]) {
              window[`_${message.callback}`](res);
            }
          })
          .catch(err => {
            if (message.error && window[`_${message.error}`]) {
              window[`_${message.error}`](err);
            }
          });
      }
    }
  };

  // Simulate spectral FFT visualizer events periodically
  setInterval(() => {
    if (eventListeners["spectrum-data"] && eventListeners["spectrum-data"].length > 0) {
      // Generate 32 values between 0.0 and 1.0
      const mockFFT = Array.from({ length: 32 }, (_, i) => {
        // High energy in bass (first 6 bars), mid energy in midranges, low energy in highs
        const energy = i < 6 ? 0.7 : i < 18 ? 0.45 : 0.2;
        const bounce = Math.sin(Date.now() / 150 + i) * 0.15; // rhythmic bounce
        const jitter = Math.random() * 0.15; // fast changes
        return Math.min(1.0, Math.max(0.02, energy + bounce + jitter));
      });

      for (const handlerId of eventListeners["spectrum-data"]) {
        const cb = callbacks[handlerId];
        if (typeof cb === "function") {
          cb({ event: "spectrum-data", payload: mockFFT });
        }
      }
    }
  }, 80); // ~12 FPS is great for screenshots without loading CPU
})();
