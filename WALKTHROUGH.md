# Feature #58: Home Discovery & Personalized Curation Landing Hub

## Summary

This implementation adds a new **Home** tab to the Luminous music app that serves as a personalized curation landing hub. When users launch the app or click the Home button, they're greeted with:

1. **Time-based greeting** (Good Morning, Good Afternoon, Good Evening, Good Night)
2. **Three personalized curation rows**:
   - Recently Played tracks
   - Your Favorites (most frequently played)
   - Recently Added tracks

Each row displays album/playlist covers with metadata and interactive controls for playback and adding to playlists.

## Implementation Details

### Backend Changes (Rust)

**New Database Queries** (`src-tauri/src/collection.rs`):
- `get_recently_played(limit)` - Returns songs sorted by `lastplayed` DESC
- `get_most_frequently_played(limit)` - Returns songs sorted by `playcount` DESC
- `get_recently_added(limit)` - Returns songs sorted by `added` DESC

**New Tauri Commands** (`src-tauri/src/commands/collection.rs`):
- `get_recently_played` - IPC command with optional limit (default 10)
- `get_most_frequently_played` - IPC command with optional limit (default 10)
- `get_recently_added` - IPC command with optional limit (default 10)

**Command Registration** (`src-tauri/src/lib.rs`):
- Added three new commands to the `generate_handler!` macro

### Frontend Changes (Svelte)

**New Components**:
1. **HomeView.svelte** - Main home discovery hub
   - Displays time-based greeting
   - Fetches curated data on mount
   - Renders three carousel sections
   - Shows empty state if no curated data available

2. **CurationCarousel.svelte** - Reusable carousel component
   - Displays songs in a horizontal scrollable list
   - Left/right navigation arrows with smooth scroll
   - Shows/hides arrows based on scroll position
   - Hides native scrollbar for cleaner UI

3. **CarouselCard.svelte** - Individual song card
   - Album cover art with hover overlay
   - Play button overlay
   - Song metadata (title, artist, duration)
   - Add to playlist dropdown menu

**Store Updates** (`src/lib/stores/collection.svelte.ts`):
- Updated `activeTab` type to include `"home"`
- Updated `navigateTo` method signature to accept `"home"`

**Navigation Updates** (`src/lib/components/Sidebar.svelte`):
- Added Home icon import from lucide-svelte
- Added Home button as first navigation option
- Home button follows same styling/interaction pattern as other nav buttons

**Page Integration** (`src/routes/+page.svelte`):
- Imported HomeView component
- Added conditional rendering when `activeTab === "home"`

## Testing & Verification

### Automatic Testing:
1. ✅ Rust compilation succeeds with `cargo check`
2. ✅ Frontend TypeScript compilation succeeds
3. ✅ Tauri dev server builds and runs
4. ✅ App launches without runtime errors

### Manual Testing Steps:
1. Launch the app
2. Click the **Home** button in the sidebar
3. Verify time-based greeting appears
4. Verify three curation rows display (if user has play history)
5. Hover over carousel cards to reveal play button
6. Click play button to start playback
7. Hover over carousel cards to reveal "Add" button
8. Click "Add" to open playlist dropdown
9. Select a playlist to add the song

### Edge Cases Handled:
- Empty library (shows empty state message)
- No play history (Recently Played carousel hidden)
- No frequently played tracks (Favorites carousel hidden)
- No recently added tracks (Recently Added carousel hidden)
- Carousel scroll boundaries (arrows show/hide appropriately)

## File Changes Summary

- ✅ `src-tauri/src/collection.rs` - Added 3 query methods
- ✅ `src-tauri/src/commands/collection.rs` - Added 3 command handlers
- ✅ `src-tauri/src/lib.rs` - Registered 3 new commands
- ✅ `src/lib/components/HomeView.svelte` - New component (main hub)
- ✅ `src/lib/components/CurationCarousel.svelte` - New component (carousel)
- ✅ `src/lib/components/CarouselCard.svelte` - New component (card)
- ✅ `src/lib/stores/collection.svelte.ts` - Updated activeTab type
- ✅ `src/lib/components/Sidebar.svelte` - Added Home navigation button
- ✅ `src/routes/+page.svelte` - Added HomeView rendering

## Next Steps for User Approval

1. Review this walkthrough
2. Start the dev server: `npm run tauri dev`
3. Click the Home button to see the feature in action
4. Test the interactive elements (play, add to playlist, carousel navigation)
5. Verify the time-based greeting updates correctly
6. Check that other tabs still work correctly

Once approved, the changes will be:
1. Committed with conventional commit message
2. Merged back to main branch
3. Issue #58 will be closed
