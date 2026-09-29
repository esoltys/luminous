# Genre & Artist Tag Hierarchy Sidecar

Pick a **Default Library** in Settings → Folders → Watched Folders. Luminous then keeps your
genre hierarchy and artist tag hierarchy in a `luminous-hierarchy.json` file in that folder.
If you move the library or open it from a second Luminous install, such as a shared network
library, the curation comes along with it.

```
Music/                       <- the default library
├── luminous-hierarchy.json
└── Devin Townsend/
```

## Behaviour

- **Automatic default:** when you have exactly one watched folder (your first, or the one
  left after removing the others), it becomes the default library unless you've picked one
  yourself. Choosing **None** is also a choice, and it's respected.
- **Linking:** if the folder already has a `luminous-hierarchy.json`, Luminous adopts it and
  it replaces the local hierarchy. Otherwise Luminous writes the local hierarchy out. A file
  that can't be read refuses the link, and the picker shows the reason.
- **Writing:** every change is written to the file straight away: grouping, reordering,
  colours, merges and deletes, plus new genres placed automatically. Writes are atomic (a temp
  file, then a rename). When two installs edit the file, the last writer wins.
- **Reading:** Luminous loads the file on startup and whenever it changes on disk, so an edit
  from another install shows up live.
- **Unused genres are kept:** while a default library is linked, a genre or tag with no songs
  stays in the file and is only hidden. Another install whose library has those songs still
  sees it. Merging or deleting still removes the entry.
- **Malformed file:** Luminous never overwrites a file it can't parse. It keeps the current
  hierarchy and shows an error until the file is fixed.
- **Unlinking:** choose **None**, or remove the folder from Watched Folders. The local
  hierarchy stays as it is, and the file is left in place. Removing the folder doesn't count
  as choosing **None**, so if a single folder remains it becomes the default.

## Format

Versioned, pretty-printed JSON. Array order is the display order.

```json
{
  "version": 1,
  "genres": [
    { "name": "Metal", "color": 3, "children": ["Progressive Metal", "Symphonic Metal"] }
  ],
  "artistTags": [
    { "name": "Favourites", "color": 0, "custom": true, "children": ["Seen Live"] }
  ]
}
```

`custom` marks an artist tag group you created, as opposed to one that exists only because
a tag uses that name. A custom group stays visible even when it's empty, so you can still
drop tags onto it.
