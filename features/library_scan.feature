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
    Given a watched directory containing:
      | fixture         |
      | song_alpha.mp3   |
      | song_beta.wav     |
    When I trigger a library scan
    Then 2 songs should be indexed in the database
    And searching for "Song Alpha" should return the first song
    And searching for "Silver" should return the second song

  Scenario: Incremental scan skips unmodified files
    Given the library has already been scanned
    And the file "song_alpha.mp3" has not been modified
    When I trigger a library scan
    Then the database should skip re-parsing "song_alpha.mp3"

  Scenario: Scanning one folder only adds the files inside it
    Given a watched directory with these files:
      | fixture        | folder |
      | song_alpha.mp3 | Alpha  |
    When I trigger a library scan
    And I copy "song_gamma.flac" into the folder "Alpha"
    And I copy "song_beta.wav" into the folder "Beta"
    And I scan only the folder "Alpha"
    Then 2 songs should be indexed in the database
    And the library should contain "Song Alpha"
    And the library should contain "Song Gamma"
    And the library should not contain "Song Beta"

  Scenario: Scanning one folder keeps songs elsewhere in the library
    Given a watched directory with these files:
      | fixture        | folder |
      | song_alpha.mp3 | Alpha  |
      | song_beta.wav  | Beta   |
    When I trigger a library scan
    And I scan only the folder "Alpha"
    Then 2 songs should be indexed in the database
    And the library should contain "Song Beta"

  Scenario: Reserved folders are never scanned
    Given a watched directory with these files:
      | fixture        | folder           |
      | song_alpha.mp3 | Alpha            |
      | song_beta.wav  | Alpha/Duplicates |
    When I trigger a library scan
    And I scan only the folder "Alpha/Duplicates"
    Then 1 songs should be indexed in the database
    And the library should not contain "Song Beta"
