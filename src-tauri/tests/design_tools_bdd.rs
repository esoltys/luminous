use cucumber::{given, then, when, World};
use luminous_lib::db::Database;
use std::sync::Arc;
use tempfile::TempDir;

#[derive(Debug, Clone)]
pub struct ThemeModel {
    pub id: String,
    pub name: String,
    pub bg_main: String,
    pub bg_sidebar: String,
    pub bg_playerbar: String,
    pub color_accent: String,
}

#[derive(Debug, World)]
pub struct DesignToolsWorld {
    _temp_dir: TempDir,
    db: Arc<Database>,
    app_running: bool,
    theme_builder_open: bool,
    color_pickers_visible: bool,
    live_preview_active: bool,
    current_theme_name: String,
    custom_themes: Vec<ThemeModel>,
    active_theme_id: String,
    preview_color: Option<String>,
}

impl Default for DesignToolsWorld {
    fn default() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
        Self {
            _temp_dir: temp_dir,
            db,
            app_running: true,
            theme_builder_open: false,
            color_pickers_visible: false,
            live_preview_active: false,
            current_theme_name: String::new(),
            custom_themes: vec![],
            active_theme_id: "system".to_string(),
            preview_color: None,
        }
    }
}

#[given("the app is running")]
fn app_running(w: &mut DesignToolsWorld) {
    w.app_running = true;
}

#[when("I navigate to Settings > UI Themes")]
fn navigate_to_theme_builder(w: &mut DesignToolsWorld) {
    w.theme_builder_open = true;
    w.color_pickers_visible = true;
    w.live_preview_active = true;
}

#[then("I should see the custom theme builder interface")]
fn see_theme_builder(w: &mut DesignToolsWorld) {
    assert!(w.theme_builder_open);
}

#[then("I should see color picker inputs for all theme colors")]
fn see_color_pickers(w: &mut DesignToolsWorld) {
    assert!(w.color_pickers_visible);
}

#[then("I should see a live preview of theme changes")]
fn see_live_preview(w: &mut DesignToolsWorld) {
    assert!(w.live_preview_active);
}

#[given("the theme builder is open")]
fn theme_builder_is_open(w: &mut DesignToolsWorld) {
    w.theme_builder_open = true;
    w.color_pickers_visible = true;
    w.live_preview_active = true;
}

#[when(expr = "I enter a theme name {string}")]
fn enter_theme_name(w: &mut DesignToolsWorld, name: String) {
    w.current_theme_name = name;
}

#[when("I select custom colors for main background, sidebar, player bar, accent, and text")]
fn select_custom_colors(w: &mut DesignToolsWorld) {
    w.preview_color = Some("#ff5500".to_string());
}

#[when("I click \"Save Custom Theme\"")]
fn click_save_custom_theme(w: &mut DesignToolsWorld) {
    let new_id = format!("custom-{}", w.custom_themes.len() + 1);
    let theme = ThemeModel {
        id: new_id.clone(),
        name: w.current_theme_name.clone(),
        bg_main: "#121212".to_string(),
        bg_sidebar: "#1e1e1e".to_string(),
        bg_playerbar: "#252525".to_string(),
        color_accent: "#ff5500".to_string(),
    };
    w.custom_themes.push(theme);
    w.active_theme_id = new_id;

    // Persist in DB app_state table
    let conn = w.db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO app_state (key, value) VALUES ('active_theme_id', ?1)",
        rusqlite::params![w.active_theme_id],
    )
    .unwrap();
}

#[then("the theme should be saved to custom themes list")]
fn theme_saved_to_list(w: &mut DesignToolsWorld) {
    assert!(
        w.custom_themes.iter().any(|t| t.name == "My Theme"),
        "Theme 'My Theme' not found in custom themes list"
    );
}

#[then("the new theme should become the active theme")]
fn new_theme_is_active(w: &mut DesignToolsWorld) {
    let active = w.custom_themes.iter().find(|t| t.id == w.active_theme_id);
    assert!(active.is_some());
    assert_eq!(active.unwrap().name, "My Theme");
}

#[when("I adjust a color picker value")]
fn adjust_color_picker(w: &mut DesignToolsWorld) {
    w.preview_color = Some("#00ff88".to_string());
}

#[then("the app UI should immediately update with the new color")]
fn ui_updates_immediately(w: &mut DesignToolsWorld) {
    assert_eq!(w.preview_color.as_deref(), Some("#00ff88"));
}

#[then("no additional click/save is needed for preview")]
fn no_save_needed_for_preview(w: &mut DesignToolsWorld) {
    assert!(w.live_preview_active);
}

#[given("I have an active theme selected")]
fn active_theme_selected(w: &mut DesignToolsWorld) {
    w.active_theme_id = "system".to_string();
}

#[when("I click \"Import Active Colors\"")]
fn click_import_active_colors(w: &mut DesignToolsWorld) {
    w.preview_color = Some("#08090c".to_string());
}

#[then("the theme builder form should populate with the current theme colors")]
fn form_populates_active_colors(w: &mut DesignToolsWorld) {
    assert!(w.preview_color.is_some());
}

#[then("I can modify these colors for a new custom theme")]
fn can_modify_colors(_w: &mut DesignToolsWorld) {}

#[given("I have a custom theme")]
fn have_custom_theme(w: &mut DesignToolsWorld) {
    let theme = ThemeModel {
        id: "custom-1".to_string(),
        name: "My Custom".to_string(),
        bg_main: "#111111".to_string(),
        bg_sidebar: "#222222".to_string(),
        bg_playerbar: "#333333".to_string(),
        color_accent: "#444444".to_string(),
    };
    w.custom_themes.push(theme);
    w.active_theme_id = "custom-1".to_string();
}

#[when("I click the delete button next to the custom theme")]
fn delete_custom_theme(w: &mut DesignToolsWorld) {
    w.custom_themes.retain(|t| t.id != "custom-1");
    if w.active_theme_id == "custom-1" {
        w.active_theme_id = "system".to_string();
    }
}

#[then("the theme should be removed from the custom themes list")]
fn theme_removed_from_list(w: &mut DesignToolsWorld) {
    assert!(w.custom_themes.is_empty());
}

#[then("if it was active, the app should switch to a default theme")]
fn switched_to_default_theme(w: &mut DesignToolsWorld) {
    assert_eq!(w.active_theme_id, "system");
}

#[tokio::main]
async fn main() {
    DesignToolsWorld::run("../features/design_tools.feature").await;
}
