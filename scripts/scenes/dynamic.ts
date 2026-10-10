import { defineScene, type Featured } from "./types";

// The Dynamic Artwork slideshow: one album per palette family, captured as the app
// paints it (a borderline cover like The Warning can flip between light and dark).
// English only, one image each, in the dynamic/ folder.
const dynamicScene = (name: string, featured: Required<Featured>, position: number) =>
  defineScene({
    name: `theme-dynamic-${name}`,
    file: `theme-dynamic-${name}.png`,
    locales: ["en-CA"],
    schemes: ["dark"],
    outputSubdir: "dynamic",
    view: { tab: "collection", subTab: "albums" },
    theme: "dynamic-artwork",
    layout: { sidebarWidth: 220 },
    featured,
    position,
    run: async ({ api, featured: f }) => api.navigate.album(f.album!),
  });

export const dynamicScenes = [
  dynamicScene("myles-smith", { song: "My Mess", artist: "Myles Smith", album: "My Mess, My Heart, My Life." }, 73),
  dynamicScene("cannons", { song: "All I Need", artist: "Cannons", album: "Everything Glows" }, 137),
  dynamicScene("the-warning", { song: "Ritual", artist: "The Warning", album: "Everything’s Falling" }, 65),
  dynamicScene("lorna-shore", { song: "Prison of Flesh", artist: "Lorna Shore", album: "I Feel the Everblack Festering Within Me" }, 97),
  dynamicScene("shania-twain", { song: "Stranger Things", artist: "Shania Twain", album: "Little Miss Twain" }, 55),
  dynamicScene("phoebe-bridgers", { song: "The Outside", artist: "Phoebe Bridgers", album: "Lost Weekend" }, 86),
  dynamicScene("wet-leg", { song: "CPR", artist: "Wet Leg", album: "moisturizer" }, 49),
];
