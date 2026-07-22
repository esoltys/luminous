# Walkthrough — Fix Playbar Backdrop Blur Loss in Production Builds (#102)

Branch: `fix-playbar-blur`  
Issue: [#102: Bug: Playbar loses backdrop blur effect in production builds](https://github.com/esoltys/luminous/issues/102)

## Problem Summary

In production builds (`bun run build` / `bun run tauri build`), the floating `PlayerBar` lost its frosted glass `backdrop-filter: blur(20px)` effect. While the translucent background tint rendered, the content scrolling underneath the playbar remained sharp instead of blurred.

## Root Cause Analysis

1. **Tree-Shaking of Utility & Scoped Styles**:
   - In Svelte 5 + Tailwind CSS v4, `.glass-surface` was applied dynamically via `{themeStore.isGlassTheme ? 'glass-surface' : ''}`.
   - Svelte 5's template analyzer saw no static `class="glass-surface"` string literal in HTML and marked `footer.glass-surface` in component `<style>` blocks as an unused selector (`css_unused_selector`), purging it.
   - Tailwind CSS v4's candidate extractor also tree-shaked `@utility glass-surface` from the production CSS bundle because it didn't recognize dynamic JS ternary strings.
   - Result: `.glass-surface` and `backdrop-filter` were missing completely from `build/_app/immutable/assets/0.*.css` in release builds.

2. **Compositing Layer Isolation**:
   - In `+layout.svelte`, `PlayerBar` was wrapped inside `<div class="absolute inset-x-4 bottom-4 z-40" transition:fly={{ y: 40, duration: 300, easing: cubicOut }}>`.
   - Svelte's `transition:fly` applied persistent inline `transform` compositing properties to the parent `div`. In Chromium WebView2, parent compositing layers isolate `backdrop-filter` from sampling elements outside their layer subtree (such as the main scrolling content area).

## Changes Made

### 1. Style Preservation (`src/app.css` & Component Style Blocks)
- **`src/app.css`**: Defined `:global(.glass-surface)` using explicit `:global()` wrappers and `!important` flags for `backdrop-filter` and `background-color: var(--glass-bg-playerbar) !important`. This prevents both Svelte 5 and Tailwind CSS v4 from purging glassmorphism rules during production bundling.
- **`src/lib/components/PlayerBar.svelte`**: Added `:global(footer.glass-surface)` rules directly to the component `<style>` block to ensure `backdrop-filter: blur(20px) saturate(180%) !important;` and `background-color: var(--glass-bg-playerbar) !important;` are bundled directly into the component asset output.
- **`src/lib/components/Sidebar.svelte`**, **`TopNavigation.svelte`**, **`RightPanel.svelte`**: Added `:global()` glass-surface rules for `aside` and `header` elements to preserve glassmorphism across all UI panels in production.

### 2. Parent Compositing Context Cleanup (`src/routes/+layout.svelte` & `PlayerBar.svelte`)
- **`src/routes/+layout.svelte`**: Removed `transition:fly` from the outer PlayDock `<div>` wrapper so it does not establish a GPU compositing isolation root over the playbar.
- **`src/lib/components/PlayerBar.svelte`**: Applied `transition:fly` directly to `<footer class="glass-surface">`. Once the mounting fly transition completes, Svelte clears the `transform` style from the DOM element, restoring full `backdrop-filter` sampling across underlying content layers.
- Removed `isolation: isolate` from `footer.glass-surface` in `PlayerBar.svelte`.

## Visual Verification

Production build verified running natively via `luminous.exe` (`C:\Users\ericj\source\luminous\.worktrees\fix-playbar-blur\src-tauri\target\release\luminous.exe`):

![Playbar Blur Fixed](file:///C:/Users/ericj/.gemini/antigravity/brain/925e94d8-8ff9-4d2b-9553-15a3d103098f/.user_uploaded/media__1784740994774.png)

## Verification & Test Results

- **Type Check**: `bun run check` passed with 0 errors, 0 warnings.
- **Unit Tests**: `bun run test:run` passed 20/20 test files (226/226 tests passed).
- **CSS Bundle Verification**: Confirmed via grep that `.glass-surface` and `backdrop-filter: blur(20px) saturate(180%)` are present in `build/_app/immutable/assets/0.*.css`.
- **Production Build**: Built and verified release executable `luminous.exe` (`bun run tauri build`).

## Next Steps

1. Commit changes to `fix-playbar-blur` branch.
2. Merge `fix-playbar-blur` into `main`.
3. Comment on and close GitHub Issue [#102](https://github.com/esoltys/luminous/issues/102).
4. Remove temporary worktree directory `.worktrees/fix-playbar-blur`.
