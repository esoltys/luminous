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

Each file holds the Markdown bio/description text verbatim — nothing else. Tags and links
stay in the database only, keeping the `.md` file a clean document you can freely edit in
external Markdown tools.

## Example `artist.md`

```md
Devin Townsend is a Canadian musician, songwriter, and record producer known for his
work in progressive and extreme metal, as well as ambient and ballad-oriented material.

## Career
He founded Strapping Young Lad and has recorded extensively under the Devin Townsend Project and as a solo artist.
```

## Example `album.md`

```md
Empath is Devin Townsend's tenth solo studio album, released in 2019. It draws
together nearly every style he's worked in — ambient, extreme metal, orchestral,
and pop — into a single, deliberately unclassifiable record.
```

## Format notes

- The file content is the bio/description verbatim. Any Markdown formatting and headings
  (`#`, `##`, etc.) you write are preserved and read back in full.
- Clearing a bio down to nothing deletes the `.md` file rather than leaving an empty one behind.
- **Editing in Luminous & External Editors**: click **Edit Markdown** next to the bio or
  description field in the profile editor to open a roomy editing modal with live preview and
  an **Open in external editor** shortcut that launches your default OS Markdown editor.
  When the file is modified externally, returning to Luminous automatically detects the change,
  prompting you to reload from disk if you have unsaved in-app edits.
- **Backward compatibility**: on read, Luminous automatically strips trailing generated
  `## Tags` and `## Links` blocks produced by older versions of Luminous so existing sidecars
  don't import that boilerplate into the bio; the next save rewrites the file in the new shape.
