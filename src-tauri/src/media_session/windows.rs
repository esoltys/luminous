//! Windows System Media Transport Controls (SMTC) backend — #576.
//!
//! Registers our main window with SMTC via
//! `ISystemMediaTransportControlsInterop::GetForWindow`, the Win32 interop
//! mechanism for unpackaged apps (the same one `souvlaki` used internally).
//! `SystemMediaTransportControls` is thread-affine — it must be created (and
//! its events handled) on a thread with an initialized COM apartment, so
//! this whole backend lives on the dedicated media-session thread spawned
//! in `super::spawn`.

use super::{handle_event, MediaCommand, PlatformMediaSession};
use crate::models::PlayState;
use std::time::Duration;
use tauri::AppHandle;
use windows::core::HSTRING;
use windows::Foundation::TypedEventHandler;
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
    SystemMediaTransportControlsButton,
};
use windows::Storage::Streams::RandomAccessStreamReference;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;

pub struct WindowsMediaSession {
    controls: SystemMediaTransportControls,
}

impl PlatformMediaSession for WindowsMediaSession {
    fn init(app_handle: AppHandle, hwnd: Option<*mut std::ffi::c_void>) -> Option<Self> {
        let Some(hwnd) = hwnd else {
            log::warn!("No window handle available; skipping SMTC initialization");
            return None;
        };

        // SAFETY: this thread is the dedicated, single-purpose media-session
        // thread; it initializes COM once here and never again, and never
        // calls CoUninitialize until the thread is about to exit.
        if let Err(e) = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok() {
            log::warn!("Failed to initialize COM apartment for SMTC: {e:?}");
            return None;
        }

        let controls = match get_smtc_for_window(HWND(hwnd)) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Failed to initialize SMTC: {e:?}");
                unsafe { CoUninitialize() };
                return None;
            }
        };

        if let Err(e) = controls
            .DisplayUpdater()
            .and_then(|u| u.SetType(MediaPlaybackType::Music))
        {
            log::warn!("Failed to set SMTC display type: {e:?}");
        }

        let _ = controls.SetIsPlayEnabled(true);
        let _ = controls.SetIsPauseEnabled(true);
        let _ = controls.SetIsNextEnabled(true);
        let _ = controls.SetIsPreviousEnabled(true);
        let _ = controls.SetIsStopEnabled(true);

        let event_app = app_handle.clone();
        let button_handler = TypedEventHandler::new(
            move |_sender,
                  args: windows::core::Ref<
                '_,
                windows::Media::SystemMediaTransportControlsButtonPressedEventArgs,
            >| {
                if let Some(args) = args.as_ref() {
                    if let Ok(button) = args.Button() {
                        let command = match button {
                            SystemMediaTransportControlsButton::Play => Some(MediaCommand::Play),
                            SystemMediaTransportControlsButton::Pause => Some(MediaCommand::Pause),
                            SystemMediaTransportControlsButton::Next => Some(MediaCommand::Next),
                            SystemMediaTransportControlsButton::Previous => {
                                Some(MediaCommand::Previous)
                            }
                            SystemMediaTransportControlsButton::Stop => Some(MediaCommand::Stop),
                            _ => None,
                        };
                        if let Some(command) = command {
                            handle_event(event_app.clone(), command);
                        }
                    }
                }
                Ok(())
            },
        );
        if let Err(e) = controls.ButtonPressed(&button_handler) {
            log::warn!("Failed to attach SMTC button handler: {e:?}");
        }

        let position_app = app_handle.clone();
        let position_handler = TypedEventHandler::new(
            move |_sender,
                  args: windows::core::Ref<
                '_,
                windows::Media::PlaybackPositionChangeRequestedEventArgs,
            >| {
                if let Some(args) = args.as_ref() {
                    if let Ok(requested) = args.RequestedPlaybackPosition() {
                        let nanos = (requested.Duration.max(0) as u64) * 100;
                        handle_event(
                            position_app.clone(),
                            MediaCommand::SetPosition(Duration::from_nanos(nanos)),
                        );
                    }
                }
                Ok(())
            },
        );
        if let Err(e) = controls.PlaybackPositionChangeRequested(&position_handler) {
            log::warn!("Failed to attach SMTC position handler: {e:?}");
        }

        let _ = controls.SetIsEnabled(true);

        Some(Self { controls })
    }

    fn set_metadata(
        &mut self,
        title: Option<&str>,
        artist: Option<&str>,
        album: Option<&str>,
        _duration: Option<Duration>,
        cover_url: Option<&str>,
    ) {
        let Ok(updater) = self.controls.DisplayUpdater() else {
            return;
        };
        let Ok(music_properties) = updater.MusicProperties() else {
            return;
        };
        let _ = music_properties.SetTitle(&HSTRING::from(title.unwrap_or_default()));
        let _ = music_properties.SetArtist(&HSTRING::from(artist.unwrap_or_default()));
        let _ = music_properties.SetAlbumTitle(&HSTRING::from(album.unwrap_or_default()));

        match cover_url
            .and_then(|url| windows::Foundation::Uri::CreateUri(&HSTRING::from(url)).ok())
        {
            Some(uri) => {
                if let Ok(thumbnail) = RandomAccessStreamReference::CreateFromUri(&uri) {
                    updater.SetThumbnail(&thumbnail).ok();
                }
            }
            None => {
                let _ = updater.SetThumbnail(None);
            }
        }

        let _ = updater.Update();
    }

    fn set_playback(&mut self, status: PlayState, _position: Duration) {
        let smtc_status = match status {
            PlayState::Playing => MediaPlaybackStatus::Playing,
            PlayState::Paused => MediaPlaybackStatus::Paused,
            PlayState::Stopped => MediaPlaybackStatus::Stopped,
        };
        let _ = self.controls.SetPlaybackStatus(smtc_status);
    }
}

impl Drop for WindowsMediaSession {
    fn drop(&mut self) {
        let _ = self.controls.SetIsEnabled(false);
        // SAFETY: paired with the CoInitializeEx call in `init`, on the same
        // (dedicated, about-to-exit) thread.
        unsafe { CoUninitialize() };
    }
}

fn get_smtc_for_window(hwnd: HWND) -> windows::core::Result<SystemMediaTransportControls> {
    let interop: ISystemMediaTransportControlsInterop = windows::core::factory::<
        SystemMediaTransportControls,
        ISystemMediaTransportControlsInterop,
    >()?;
    unsafe { interop.GetForWindow(hwnd) }
}
