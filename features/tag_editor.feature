Feature: Tag Editor and AcoustID Fingerprinting
  As a music collector using Luminous
  I want to edit track metadata tags and automatically fetch missing tags using audio fingerprinting
  So that my collection is perfectly organized and search-friendly

  Scenario: Editing track tags and saving back to audio file
    Given I have a song in the library
    When I open the tag editor for the song
    And I change the Title to "Yellow (Acoustic)"
    And I change the Artist to "Coldplay"
    And I click "Save Tags"
    Then the backend should write the new tags to the audio file's metadata on disk (using lofty)
    And it should update the song details in the SQLite database
    And the library views should immediately reflect the updated metadata

  Scenario: AcoustID fingerprinting lookup suggesting metadata
    Given I have a song with missing or incorrect tags
    And `fpcalc` is installed and available
    When I open the tag editor for the song
    And I click "Lookup Tags via AcoustID"
    Then the backend should run `fpcalc` to generate the audio fingerprint
    And query the AcoustID Web Service with the fingerprint and duration
    And return suggested tags:
      | Field  | Value                 |
      | Title  | Clocks                |
      | Artist | Coldplay              |
      | Album  | A Rush of Blood to the Head |
    And the tag editor form fields should be populated with the suggested values
