# About Luminous Music Player

Created by [Eric Soltys](https://esoltys.github.io/), a Canadian software developer and hobbyist creating art and music in the BC Kootenays.

---

## Currently Integrated Tech Stack & Libraries

### Core Framework & Storage
- **Backend Architecture**: [Rust](https://www.rust-lang.org/) & [Tauri v2](https://tauri.app/)
- **Frontend Architecture**: [Svelte 5 (Runes)](https://svelte.dev/) & [TypeScript](https://www.typescriptlang.org/)
- **Database Engine**: [SQLite](https://sqlite.org/) via `rusqlite` & `r2d2`
- **UI Framework & Styling**: [Tailwind CSS v4](https://tailwindcss.com/)
- **Icons**: [Lucide Icons](https://lucide.dev/)
- **Typography**: [Inter](https://rsms.me/inter/) font family by Rasmus Andersson

### Audio Engine & Signal Processing
- **Audio Decoding**: [Symphonia](https://github.com/pdeljanov/Symphonia)
- **Audio Output**: [CPAL (Cross-Platform Audio Layer)](https://github.com/RustAudio/cpal)
- **Audio Resampling**: [rubato](https://github.com/HEnquist/rubato)
- **Loudness Analysis**: `bs1770` (EBU R128 / ITU BS.1770 loudness measurement)
- **Spectrum Analysis & Moodbars**: [rustfft](https://github.com/ejmahler/RustFFT)

### Metadata & Tagging
- **Tag Reading & Writing**: [lofty](https://github.com/Serial-Scanner/lofty-rs) (FLAC, ID3, MP4, Ogg Vorbis, WAV metadata)
- **Audio Fingerprinting**: [AcoustID / Chromaprint (`fpcalc`)](https://acoustid.org/) for acoustic track identification

### External APIs & Web Services (Active)
- **Synced Lyrics**: [LRCLIB](https://lrclib.net/) and [Lyrics.ovh](https://lyricsovh.docs.apiary.io/)
- **Cover Art Fallback**: [iTunes Search API](https://performance-partners.apple.com/)

---

## Planned Web Integrations (v2.0 Milestone)

The following 3rd-party services are planned for implementation in the **v2.0 Web Release**:
- **Scrobbling & Play History**: [ListenBrainz](https://listenbrainz.org/) and [Last.fm](https://www.last.fm/) ([#83](https://github.com/esoltys/luminous/issues/83))
- **Metadata Resolution**: [MusicBrainz](https://musicbrainz.org/) ([#23](https://github.com/esoltys/luminous/issues/23))
- **Reviews & Community Ratings**: [CritiqueBrainz](https://critiquebrainz.org/), [Wikipedia API](https://en.wikipedia.org/api/rest_v1/), and [TheAudioDB](https://www.theaudiodb.com/) ([#23](https://github.com/esoltys/luminous/issues/23))

---

## Influences & Recommended Music Players

Inspired by open-source and indie media players:
- **[Audacious Media Player](https://audacious-media-player.org/)** (Open Source)
- **[Strawberry Music Player](https://www.strawberrymusicplayer.org/)** (Open Source)
- **[Elisa Music Player](https://apps.kde.org/elisa/)** (KDE / Open Source)
- **[MusicBee Music Manager](https://www.getmusicbee.com/)** (Indie)
