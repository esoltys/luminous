Feature: Tag Editor
  As a music collector using Luminous
  I want to edit track metadata tags
  So that my collection is perfectly organized and search-friendly

  Scenario: Editing track tags and saving back to audio file
    Given I have a song in the library
    When I open the tag editor for the song
    And I change the Title to "Yellow (Acoustic)"
    And I change the Artist to "Coldplay"
    And I click "Save Tags"
    Then it should update the song details in the SQLite database
    And the library views should immediately reflect the updated metadata
