Feature: Design Tools for Theme Customization
  As a user
  I want to access advanced design tools to customize the app's appearance
  So that I can create personalized themes that match my aesthetic preferences

  Scenario: Access theme builder from settings
    Given the app is running
    When I navigate to Settings > UI Themes
    Then I should see the custom theme builder interface
    And I should see color picker inputs for all theme colors
    And I should see a live preview of theme changes

  Scenario: Create custom theme with color pickers
    Given the theme builder is open
    When I enter a theme name "My Theme"
    And I select custom colors for main background, sidebar, player bar, accent, and text
    And I click "Save Custom Theme"
    Then the theme should be saved to custom themes list
    And the new theme should become the active theme

  Scenario: Live preview of theme changes
    Given the theme builder is open
    When I adjust a color picker value
    Then the app UI should immediately update with the new color
    And no additional click/save is needed for preview

  Scenario: Import colors from active theme
    Given I have an active theme selected
    When I click "Import Active Colors"
    Then the theme builder form should populate with the current theme colors
    And I can modify these colors for a new custom theme

  Scenario: Delete custom theme
    Given I have a custom theme
    When I click the delete button next to the custom theme
    Then the theme should be removed from the custom themes list
    And if it was active, the app should switch to a default theme
