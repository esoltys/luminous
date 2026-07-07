Feature: Graphic Equalizer
  As an audiophile user of Luminous Music Player
  I want to adjust the frequency bands of my audio
  So that I can customize the playback sound signature to my room or headphones

  Scenario: Enabling and disabling the Equalizer
    Given the player is playing a track
    And the equalizer is currently disabled
    When I toggle the equalizer "On"
    Then the audio engine should process all playback samples through the 10-band filter cascade
    When I toggle the equalizer "Off"
    Then the audio engine should bypass the filter cascade and output dry samples

  Scenario: Adjusting equalizer band gains
    Given the equalizer is enabled
    When I set the gain of the "1kHz" band (index 5) to "+6.0dB"
    Then the 1kHz band filter coefficients should recalculate
    And the audio engine should boost frequencies around 1kHz by 6.0dB

  Scenario: Loading an Equalizer Preset
    Given the equalizer is enabled
    When I select the "Rock" equalizer preset
    Then the gains for all 10 bands should update to preset values:
      | Band   | Gain (dB) |
      | 31.5Hz | +4.0      |
      | 63Hz   | +3.0      |
      | 125Hz  | +2.0      |
      | 250Hz  | -1.0      |
      | 500Hz  | -2.0      |
      | 1kHz   | -1.0      |
      | 2kHz   | +1.0      |
      | 4kHz   | +2.0      |
      | 8kHz   | +3.0      |
      | 16kHz  | +4.0      |
    And all biquad filter coefficients should recalculate
