Feature: Playlists and Undo-Redo Stack
  As a user
  I want to organize tracks into playlists and modify their order
  So that I can customize my listening queue

  Scenario: Creating and deleting playlists
    Given the playlist manager is initialized
    When I create a new playlist named "Study Vibes"
    Then the database should store a new playlist with name "Study Vibes"
    When I delete the playlist "Study Vibes"
    Then the playlist should be removed from the database

  Scenario: Drag-and-drop track reordering with Undo and Redo
    Given a playlist "My Hits" containing:
      | index | song_id | title        |
      | 0     | 101     | Song Alpha   |
      | 1     | 102     | Song Beta    |
      | 2     | 103     | Song Gamma   |
    When I move track at index 2 to index 0
    Then the playlist track order should become:
      | index | title        |
      | 0     | Song Gamma   |
      | 1     | Song Alpha   |
      | 2     | Song Beta    |
    When I click the "Undo" button
    Then the playlist track order should restore to:
      | index | title        |
      | 0     | Song Alpha   |
      | 1     | Song Beta    |
      | 2     | Song Gamma   |
    When I click the "Redo" button
    Then the playlist track order should apply the move again:
      | index | title        |
      | 0     | Song Gamma   |
      | 1     | Song Alpha   |
      | 2     | Song Beta    |
