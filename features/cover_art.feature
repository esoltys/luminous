Feature: Cover Art Management
  As a user of Luminous Music Player
  I want my albums to display their cover art
  So that I can visually browse and identify my music collection

  Scenario: Extracting embedded cover art on library scan
    Given a watched directory containing a song with embedded cover art
    When I trigger a library scan
    Then the scanner should extract the image from the audio file
    And save it to the covers cache directory with an FNV-1a hash filename
    And the song in the database should have "art_embedded" set to true
    And "art_automatic" set to the cached filename

  Scenario: Scanning local folder for cover art
    Given a song without embedded cover art
    And the song's parent directory contains a file named "cover.jpg"
    When I trigger a library scan
    Then the scanner should find "cover.jpg" in the song's folder
    And the song in the database should have "art_automatic" set to the absolute path of "cover.jpg"

  Scenario: Fetching remote cover art fallback
    Given a song has no embedded cover art
    And there is no cover image file in the song's directory
    When the song is played or loaded in the player
    Then the player should query the iTunes Search API for the album's cover art
    And download the artwork to the covers cache directory
    And update the database with the cached artwork filename in "art_automatic"
