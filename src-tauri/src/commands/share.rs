//! Backend half of the social share card export (#97). The card image itself
//! is rasterized entirely in the frontend (SVG -> canvas -> PNG blob); this
//! command just writes the resulting bytes to a path the user already chose
//! via the save dialog, since there's no `@tauri-apps/plugin-fs` dependency
//! for the frontend to write files directly (see `playlist::import_export`
//! for the same pattern used by playlist export).

use base64::{engine::general_purpose::STANDARD, Engine as _};

#[tauri::command]
pub async fn save_share_card_image(path: String, data_base64: String) -> Result<(), String> {
    let bytes = STANDARD.decode(&data_base64).map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| e.to_string())
}
