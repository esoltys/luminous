Feature: Local Music Library Scanning
  As a user of Luminous Music Player
  I want to register my music folders
  So that my local audio files are indexed and available in the library

  Scenario: Adding a directory to watched directories
    Given the database is initialized and empty
    When I add the directory "/home/user/Music" to watched folders
    Then the directory "/home/user/Music" should be saved in the database
    And the watched directories list should return "/home/user/Music"

  Scenario: Rescanning folders indexes audio files
    Given a watched directory "/home/user/Music" containing:
      | path                      | artist      | album          | title         | filetype | length_sec |
      | /home/user/Music/track1.mp3| Artist One  | Album Gold     | Song Alpha    | MP3      | 180        |
      | /home/user/Music/track2.wav| Artist Two  | Album Silver   | Song Beta     | WAV      | 240        |
    When I trigger a library scan
    Then 2 songs should be indexed in the database
    And searching for "Song Alpha" should return the first song
    And searching for "Silver" should return the second song

  Scenario: Incremental scan skips unmodified files
    Given the library has already been scanned
    And the file "/home/user/Music/track1.mp3" has not been modified
    When I trigger a library scan
    Then the database should skip re-parsing "/home/user/Music/track1.mp3"
