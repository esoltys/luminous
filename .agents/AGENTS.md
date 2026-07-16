# Luminous Music Player Rules

## Navigation & Stores

When adding or modifying navigation links, buttons, or shortcuts in the UI:
- Use `collectionStore` from `src/lib/stores/collection.svelte.ts` to manage routing, active tabs, and filter views.
- **Navigate to Album Detail View**: Use `collectionStore.viewAlbum(albumName)` to navigate directly to the detailed album screen.
- **Navigate to Artist Detail View**: Use `collectionStore.viewArtist(artistName)` to navigate directly to the detailed artist discography screen.
- **Navigate to General Tabs**: Use `collectionStore.navigateTo(tab, subTab?, query?)` where `tab` is one of `"home" | "collection" | "playlists" | "settings" | "lyrics"`.
