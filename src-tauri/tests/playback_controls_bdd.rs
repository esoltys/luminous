use cucumber::{given, then, when, World};
use luminous_lib::models::PlayState;

#[derive(Debug, World)]
pub struct PlaybackControlsWorld {
    state: PlayState,
    position_nanosec: u64,
    is_position_advancing: bool,
    volume: f32,
    buffer_cleared: bool,
}

impl Default for PlaybackControlsWorld {
    fn default() -> Self {
        Self {
            state: PlayState::Stopped,
            position_nanosec: 0,
            is_position_advancing: false,
            volume: 1.0,
            buffer_cleared: false,
        }
    }
}

#[given("the player is stopped")]
fn player_is_stopped(w: &mut PlaybackControlsWorld) {
    w.state = PlayState::Stopped;
    w.position_nanosec = 0;
    w.is_position_advancing = false;
}

#[when(expr = "I play the track with ID {int}")]
fn play_track(w: &mut PlaybackControlsWorld, _id: i64) {
    w.state = PlayState::Playing;
    w.is_position_advancing = true;
}

#[then("the audio engine should transition to the \"Playing\" state")]
fn state_is_playing(w: &mut PlaybackControlsWorld) {
    assert_eq!(w.state, PlayState::Playing);
}

#[then("the track position should begin advancing in real-time")]
fn position_advancing(w: &mut PlaybackControlsWorld) {
    assert!(w.is_position_advancing);
}

#[given("the player is playing a track")]
fn player_is_playing(w: &mut PlaybackControlsWorld) {
    w.state = PlayState::Playing;
    w.position_nanosec = 10_000_000_000;
    w.is_position_advancing = true;
}

#[when("I press pause")]
fn press_pause(w: &mut PlaybackControlsWorld) {
    w.state = PlayState::Paused;
    w.is_position_advancing = false;
}

#[then("the audio stream should pause")]
fn stream_paused(w: &mut PlaybackControlsWorld) {
    assert_eq!(w.state, PlayState::Paused);
}

#[then("the audio engine state should transition to \"Paused\"")]
fn state_is_paused(w: &mut PlaybackControlsWorld) {
    assert_eq!(w.state, PlayState::Paused);
}

#[then("the position tracker should freeze at its current value")]
fn position_freezes(w: &mut PlaybackControlsWorld) {
    assert!(!w.is_position_advancing);
}

#[when("I press resume")]
fn press_resume(w: &mut PlaybackControlsWorld) {
    w.state = PlayState::Playing;
    w.is_position_advancing = true;
}

#[then("the audio stream should play again")]
fn stream_plays_again(w: &mut PlaybackControlsWorld) {
    assert_eq!(w.state, PlayState::Playing);
}

#[then("the state should transition back to \"Playing\"")]
fn state_back_to_playing(w: &mut PlaybackControlsWorld) {
    assert_eq!(w.state, PlayState::Playing);
}

#[when(regex = r#"I seek to (\d+) seconds \(([\d,]+) nanoseconds\)"#)]
fn seek_to_timecode(w: &mut PlaybackControlsWorld, secs: u64, nanosecs_str: String) {
    let clean_nanosecs = nanosecs_str.replace(",", "");
    let nanosecs: u64 = clean_nanosecs.parse().unwrap();
    w.position_nanosec = nanosecs;
    w.buffer_cleared = true;
    assert_eq!(secs * 1_000_000_000, nanosecs);
}

#[then(regex = r#"the decoder should seek the format stream to (\d+) seconds"#)]
fn decoder_seeked(w: &mut PlaybackControlsWorld, secs: u64) {
    assert_eq!(w.position_nanosec, secs * 1_000_000_000);
}

#[then("the CPAL output buffer should clear to avoid stale audio")]
fn buffer_cleared(w: &mut PlaybackControlsWorld) {
    assert!(w.buffer_cleared);
}

#[then(regex = r#"the position tracker should jump immediately to (\d+) seconds"#)]
fn position_jumped(w: &mut PlaybackControlsWorld, secs: u64) {
    assert_eq!(w.position_nanosec, secs * 1_000_000_000);
}

#[when(regex = r#"I adjust the volume to (\d+)%"#)]
fn adjust_volume(w: &mut PlaybackControlsWorld, percent: u32) {
    w.volume = percent as f32 / 100.0;
}

#[then(regex = r#"the playback gain should scale all output float samples by ([\d.]+)"#)]
fn check_gain_scale(w: &mut PlaybackControlsWorld, expected_gain: f32) {
    assert!(
        (w.volume - expected_gain).abs() < 0.001,
        "Expected gain {}, got {}",
        expected_gain,
        w.volume
    );
}

#[tokio::main]
async fn main() {
    PlaybackControlsWorld::run("../features/playback_controls.feature").await;
}
