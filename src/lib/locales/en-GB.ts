import type { Messages } from "./index";

/** British spelling (-ise, colour, catalogue) over the Canadian English base. Sparse: only the strings that differ; everything else falls back to `en-CA`. */
export const enGB: Messages = {
  sidebar: {
    organize: "Organise"
  },
  home: {
    emptyState: "Start adding music to see your personalised collections"
  },
  settings: {
    aboutTechBs1770: "EBU R128 / ITU BS.1770 loudness normalisation.",
    aboutTechLrcLib: "Primary synchronised (.lrc) lyrics provider.",
    aboutTechNetEase: "Synchronised lyrics with extensive Asian and international coverage.",
    systemSubtitle: "Control how Luminous starts, minimises and stores its data.",
    minimizeToTrayLabel: "Minimise to tray",
    loudnessAnalysisActive: {
      one: "Loudness Normalisation is analysing {count} song in the background — this can look like scanning even when folder watching is off. Disable it in the Equalizer tab to stop.",
      other: "Loudness Normalisation is analysing {count} songs in the background — this can look like scanning even when folder watching is off. Disable it in the Equalizer tab to stop."
    },
    themesSubtitle: "Customise the visual appearance and colours of Luminous.",
    autoOrganizeLabel: "Auto-Organise New Files & Tag Updates",
    autoOrganizeHint: "Organise files automatically in the background using your active template when new tracks arrive or tags are edited."
  },
  playerBar: {
    shuffleAllDesc: "Play all tracks in a randomised order",
    shuffleInsideAlbumDesc: "Play albums in their normal order, but randomise the tracks within each album",
    shuffleAlbumsDesc: "Play albums in a randomised order, but keep the tracks within each album in order",
    catalogNumberLabel: "Catalogue #"
  },
  loudness: {
    title: "Loudness Normalisation",
    mode: "Normalise by",
    analyzing: {
      one: "Analysing library: {count} song remaining",
      other: "Analysing library: {count} songs remaining"
    },
    analyzed: "All songs analysed",
    analysisPaused: {
      one: "{count} song not yet analysed — enable to analyse in the background",
      other: "{count} songs not yet analysed — enable to analyse in the background"
    }
  },
  organizer: {
    title: "Organise Files",
    subtitle: "Rename and reorganise library files using tag templates",
    summaryReady: {
      one: "{count} unorganised song",
      other: "{count} unorganised songs"
    },
    nothingToOrganize: "Nothing to Organise!",
    autoOrganizeToggle: "Auto-organise",
    autoOrganizeToggleTooltip: "Organise files automatically in the background when new tracks arrive or tags are edited",
    autoOrganizeSuccess: {
      one: "Auto-organised {count} file",
      other: "Auto-organised {count} files"
    },
    toastDuplicatesDetected: {
      one: "{count} duplicate file detected during auto-organise",
      other: "{count} duplicate files detected during auto-organise"
    },
    applyButton: "Organise Music",
    applySuccess: {
      one: "Successfully reorganised {count} file",
      other: "Successfully reorganised {count} files"
    },
    organizeFilesTooltip: "Organise files by tag template",
    organizeEntireLibrary: "Organise",
    noTracksToOrganize: "No songs to organise.",
    noChangingFilesMatch: "No files need to be reorganised.",
    toastErrors: "Files that couldn't be organised: {count}"
  },
  listenbrainz: {
    ratingsLabel: "Synchronise track ratings"
  },
  audioPipeline: {
    volumeNormalization: "Volume Normalisation"
  },
  auth: {
    waitingForBrowser: "Waiting for authorisation in browser…"
  }
};
