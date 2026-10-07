import type { Messages } from "./index";

/** American spelling (color, favorite, canceled) over the Canadian English base. Sparse: only the strings that differ; everything else falls back to `en-CA`. */
export const enUS: Messages = {
  settings: {
    ratingStyleHeart: "Heart (favorite)",
    ratingStyleHint: "Choose Heart to mark songs as favorites, 5-star for half-step ratings, or Both to display hearts and star ratings side-by-side. Album ratings always use 5 stars.",
    folderColor: "Badge Color",
    themesSubtitle: "Customize the visual appearance and colors of Luminous.",
    importColors: "Reset Colors",
    accentLabel: "Accent Color",
    accentHoverLabel: "Accent Hover Color",
    bordersLabel: "Border Color",
    colorPalette: "Color Palette",
    textColors: "Text Colors"
  },
  playlists: {
    autoFavourites: "Favorite Songs",
    populationModeFavourites: "Favorites",
    populationModeTooltipFamiliar: "Favor your most-played songs",
    populationModeEmptyFavourites: "No Favorite tracks found in this category. Heart some songs or switch fill mode."
  },
  artistEvents: {
    cancelled: "Canceled"
  },
  rating: {
    favoriteTooltip: "Add to favorites",
    unfavoriteTooltip: "Remove from favorites"
  },
  listenbrainz: {
    ratingsHint: "Submit favorite tracks as loved tracks"
  }
};
