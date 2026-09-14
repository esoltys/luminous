# Artist/Album Bio Sidecar Files

Artist bios and album descriptions are mirrored to `artist.md`/`album.md` files that live
next to your music — the same folder convention `artist.jpg`/`cover.jpg` already use:

- `album.md` sits in the album folder, next to `cover.jpg`.
- `artist.md` sits one folder up, next to `artist.jpg`.

```
Music/
└── Devin Townsend/          <- artist.md goes here
    ├── artist.md
    ├── artist.jpg
    └── Empath/               <- album.md goes here
        ├── album.md
        ├── cover.jpg
        └── 01 Castaway.flac
```

The database still caches the bio/description for fast reads, but the `.md` file is
what makes it portable: copy, sync, or back up the library folder and another Luminous
instance picks it up automatically. If a `.md` file already exists but the database has
no bio cached yet, Luminous adopts it the next time you view that artist/album.

Each file mirrors the whole profile, not just the prose — the bio/description paragraph,
followed by a `## Links` list for the website and social/external links. `artist.md` also
gets a `## Tags` list (if any tags are set); `album.md` doesn't — an album has no curated
tag list of its own, only the embedded genre tag already stored in each track's own file.

## Example `artist.md`

```md
Devin Townsend is a Canadian musician, songwriter, and record producer known for his
work in progressive and extreme metal, as well as ambient and ballad-oriented material.

## Tags
- canadian
- progressive metal
- ambient

## Links
- [Website](https://hevydevy.com)
- Instagram: @devintownsend
- [YouTube](https://youtube.com/@devintownsend)
```

## Example `album.md`

```md
Empath is Devin Townsend's tenth solo studio album, released in 2019. It draws
together nearly every style he's worked in — ambient, extreme metal, orchestral,
and pop — into a single, deliberately unclassifiable record.

## Links
- [Website](https://hevydevy.com/empath)
- [Bandcamp](https://devintownsend.bandcamp.com/album/empath)
```

## Format notes

- A link is rendered as `[Label](url)` when the value looks like a URL, or a plain
  `Label: value` bullet otherwise (e.g. a bare social handle).
- Clearing a profile down to nothing (no bio, no tags, no links) deletes the `.md`
  file rather than leaving a stale one behind.
- Hand-editing a `.md` file works too — only the text *before* the first `## `
  heading is read back as the bio/description; anything under `## Tags` (`artist.md`
  only) or `## Links` is Luminous's own generated section and isn't parsed back into
  structured data.
