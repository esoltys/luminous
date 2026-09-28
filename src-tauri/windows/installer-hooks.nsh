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

  !insertmacro LuminousRegisterDefaultApp
!macroend

!macro LuminousSetFileIcon FILECLASS ICONFILE
  WriteRegStr SHCTX "Software\Classes\${FILECLASS}\DefaultIcon" "" "$\"$INSTDIR\icons\filetypes\${ICONFILE}$\",0"
!macroend

; Registers Luminous with Windows' Default Apps (#1265), so Settings' "Make
; Luminous the default player" can deep-link to Luminous's own page via
; ms-settings:defaultapps?registeredAppUser=Luminous (registeredAppMachine
; for a per-machine install — the backend checks which hive has it). The
; value name must match REGISTERED_APP_NAME in src-tauri/src/default_apps.rs.
;
; Capabilities\FileAssociations maps each extension to the ProgID Tauri's
; APP_ASSOCIATE already created (the association's `name`, as above). A test
; in default_apps.rs checks every tauri.conf.json extension is listed here.
!define LUMINOUS_REGISTERED_APP "Luminous"
!define LUMINOUS_CAPABILITIES_KEY "Software\Luminous\Capabilities"

!macro LuminousRegisterDefaultApp
  WriteRegStr SHCTX "${LUMINOUS_CAPABILITIES_KEY}" "ApplicationName" "${PRODUCTNAME}"
  WriteRegStr SHCTX "${LUMINOUS_CAPABILITIES_KEY}" "ApplicationDescription" "Play the music you already own"
  WriteRegStr SHCTX "${LUMINOUS_CAPABILITIES_KEY}" "ApplicationIcon" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"

  !insertmacro LuminousDeclareFileType ".mp3" "MP3 Audio"
  !insertmacro LuminousDeclareFileType ".flac" "FLAC Audio"
  !insertmacro LuminousDeclareFileType ".ogg" "Ogg Audio"
  !insertmacro LuminousDeclareFileType ".opus" "Ogg Audio"
  !insertmacro LuminousDeclareFileType ".m4a" "M4A Audio"
  !insertmacro LuminousDeclareFileType ".alac" "ALAC Audio"
  !insertmacro LuminousDeclareFileType ".aac" "AAC Audio"
  !insertmacro LuminousDeclareFileType ".wav" "WAV Audio"
  !insertmacro LuminousDeclareFileType ".aiff" "AIFF Audio"
  !insertmacro LuminousDeclareFileType ".aif" "AIFF Audio"
  !insertmacro LuminousDeclareFileType ".wv" "WavPack Audio"
  !insertmacro LuminousDeclareFileType ".mpc" "Musepack Audio"
  !insertmacro LuminousDeclareFileType ".ape" "Monkey's Audio"
  !insertmacro LuminousDeclareFileType ".tta" "True Audio"
  !insertmacro LuminousDeclareFileType ".dsf" "DSD Stream File"
  !insertmacro LuminousDeclareFileType ".dff" "DSDIFF Audio"
  !insertmacro LuminousDeclareFileType ".asf" "Advanced Systems Format"
  !insertmacro LuminousDeclareFileType ".wma" "Windows Media Audio"
  !insertmacro LuminousDeclareFileType ".m4b" "AAC Audiobook"
  !insertmacro LuminousDeclareFileType ".m3u" "M3U Playlist"
  !insertmacro LuminousDeclareFileType ".m3u8" "M3U8 Playlist"
  !insertmacro LuminousDeclareFileType ".pls" "PLS Playlist"
  !insertmacro LuminousDeclareFileType ".xspf" "XSPF Playlist"

  WriteRegStr SHCTX "Software\RegisteredApplications" "${LUMINOUS_REGISTERED_APP}" "${LUMINOUS_CAPABILITIES_KEY}"
!macroend

!macro LuminousDeclareFileType EXT FILECLASS
  WriteRegStr SHCTX "${LUMINOUS_CAPABILITIES_KEY}\FileAssociations" "${EXT}" "${FILECLASS}"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue SHCTX "Software\RegisteredApplications" "${LUMINOUS_REGISTERED_APP}"
  DeleteRegKey SHCTX "${LUMINOUS_CAPABILITIES_KEY}"
  DeleteRegKey /ifempty SHCTX "Software\Luminous"
!macroend
