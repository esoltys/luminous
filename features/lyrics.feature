Feature: Lyrics View
  As a music lover using Luminous
  I want to view the lyrics of the currently playing song
  So that I can sing along and understand the song's meaning in real-time

  Scenario: Loading cached lyrics from the database
    Given a song is playing
    And the database already has cached lyrics for this song
    When I open the lyrics panel
    Then the system should display the cached lyrics immediately without making a network request

  Scenario: Loading sidecar .lrc lyrics from disk
    Given a song is playing with a local audio file
    And a sidecar .lrc file exists next to the audio file
    When I open the lyrics panel
    Then the system should display the sidecar lyrics immediately without making a network request

  Scenario: Sidecar .lrc takes priority over embedded lyrics and online fetch
    Given a song is playing with a local audio file
    And the database already has cached lyrics for this song
    And a sidecar .lrc file exists next to the audio file
    When I open the lyrics panel
    Then the system should display the sidecar lyrics immediately without making a network request

