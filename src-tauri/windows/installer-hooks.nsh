; Overrides the DefaultIcon registry value Tauri's APP_ASSOCIATE macro sets
; for each file association (which always points at the app's own .exe icon)
; with Luminous's per-format file-type icons. Runs after file associations
; are created (see NSIS_HOOK_POSTINSTALL timing in tauri-bundler's
; installer.nsi) and the icon files under $INSTDIR\icons\filetypes have
; already been copied as bundle resources.
;
; The registry class name for each association is its `name` field from
; tauri.conf.json's bundle.fileAssociations (Tauri falls back to the raw
; extension when `name` is unset) — keep these two in sync.
!macro NSIS_HOOK_POSTINSTALL
  !insertmacro LuminousSetFileIcon "MP3 Audio" "luminous-file-mp3.ico"
  !insertmacro LuminousSetFileIcon "FLAC Audio" "luminous-file-flac.ico"
  !insertmacro LuminousSetFileIcon "Ogg Audio" "luminous-file-ogg.ico"
  !insertmacro LuminousSetFileIcon "M4A Audio" "luminous-file-m4a.ico"
  !insertmacro LuminousSetFileIcon "ALAC Audio" "luminous-file-alac.ico"
  !insertmacro LuminousSetFileIcon "AAC Audio" "luminous-file-aac.ico"
  !insertmacro LuminousSetFileIcon "WAV Audio" "luminous-file-wav.ico"
  !insertmacro LuminousSetFileIcon "M3U Playlist" "luminous-file-m3u.ico"
  !insertmacro LuminousSetFileIcon "M3U8 Playlist" "luminous-file-m3u8.ico"

  ; Formats without a dedicated badge yet share the generic file icon.
  !insertmacro LuminousSetFileIcon "AIFF Audio" "luminous-file-generic.ico"
  !insertmacro LuminousSetFileIcon "WavPack Audio" "luminous-file-generic.ico"
  !insertmacro LuminousSetFileIcon "Musepack Audio" "luminous-file-generic.ico"
  !insertmacro LuminousSetFileIcon "Monkey's Audio" "luminous-file-generic.ico"
  !insertmacro LuminousSetFileIcon "True Audio" "luminous-file-generic.ico"
  !insertmacro LuminousSetFileIcon "DSD Stream File" "luminous-file-generic.ico"
  !insertmacro LuminousSetFileIcon "DSDIFF Audio" "luminous-file-generic.ico"
  !insertmacro LuminousSetFileIcon "Advanced Systems Format" "luminous-file-generic.ico"
  !insertmacro LuminousSetFileIcon "Windows Media Audio" "luminous-file-generic.ico"
  !insertmacro LuminousSetFileIcon "AAC Audiobook" "luminous-file-generic.ico"
  !insertmacro LuminousSetFileIcon "PLS Playlist" "luminous-file-generic.ico"
  !insertmacro LuminousSetFileIcon "XSPF Playlist" "luminous-file-generic.ico"
!macroend

!macro LuminousSetFileIcon FILECLASS ICONFILE
  WriteRegStr SHCTX "Software\Classes\${FILECLASS}\DefaultIcon" "" "$\"$INSTDIR\icons\filetypes\${ICONFILE}$\",0"
!macroend
