Feature: Audio Playback and Control Controls
  As a user
  I want to play, pause, seek, and control volume of my audio
  So that I can listen to my music collections

  Scenario: Playing a track loads decoder and starts CPAL output stream
    Given the player is stopped
    When I play the track with ID 1
    Then the audio engine should transition to the "Playing" state
    And the track position should begin advancing in real-time

  Scenario: Pausing and resuming audio
    Given the player is playing a track
    When I press pause
    Then the audio stream should pause
    And the audio engine state should transition to "Paused"
    And the position tracker should freeze at its current value
    When I press resume
    Then the audio stream should play again
    And the state should transition back to "Playing"

  Scenario: Seeking to a target timecode
    Given the player is playing a track
    When I seek to 45 seconds (45,000,000,000 nanoseconds)
    Then the decoder should seek the format stream to 45 seconds
    And the CPAL output buffer should clear to avoid stale audio
    And the position tracker should jump immediately to 45 seconds

  Scenario: Volume control adjustment
    Given the player is playing a track
    When I adjust the volume to 75%
    Then the playback gain should scale all output float samples by 0.75
