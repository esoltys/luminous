Feature: Synced and Unsynced Lyrics View
  As a music lover using Luminous
  I want to view the lyrics of the currently playing song
  So that I can sing along and understand the song's meaning in real-time

  Scenario: Loading cached lyrics from the database
    Given a song is playing
    And the database already has cached lyrics for this song
    When I open the lyrics panel
    Then the system should display the cached lyrics immediately without making a network request

  Scenario: Fetching synced lyrics from LRCLIB (Primary Provider)
    Given a song is playing with artist "Coldplay" and title "Yellow"
    And there are no lyrics cached in the database
    When I open the lyrics panel
    Then the system should query LRCLIB for synced lyrics
    And it should display the lyrics with timestamp markers
    And it should cache the retrieved lyrics (synced format) in the database

  Scenario: Falling back to plain text lyrics via Lyrics.ovh
    Given a song is playing with artist "The Beatles" and title "Yesterday"
    And LRCLIB does not return any lyrics for this song
    When I open the lyrics panel
    Then the system should fallback and query Lyrics.ovh for plain text lyrics
    And it should display the plain text lyrics without sync highlights
    And it should cache the plain text lyrics in the database

  Scenario: Synced lyric highlight and auto-scroll
    Given a song is playing
    And the lyrics are in LRC (synced) format
    When the playback position matches a lyric line timestamp
    Then that lyric line should be highlighted as active
    And the lyrics panel should smoothly scroll to center the active line
