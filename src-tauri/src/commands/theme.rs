use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemeColors {
    #[serde(rename = "bg-main")]
    pub bg_main: String,
    #[serde(rename = "bg-sidebar")]
    pub bg_sidebar: String,
    #[serde(rename = "bg-playerbar")]
    pub bg_playerbar: String,
    #[serde(rename = "color-accent")]
    pub color_accent: String,
    #[serde(rename = "color-accent-hover")]
    pub color_accent_hover: String,
    #[serde(rename = "color-text-primary")]
    pub color_text_primary: String,
    #[serde(rename = "color-text-secondary")]
    pub color_text_secondary: String,
    #[serde(rename = "color-border")]
    pub color_border: String,
}

impl ThemeColors {
    pub fn validate(&self) -> Result<(), String> {
        let fields = [
            ("bg-main", &self.bg_main),
            ("bg-sidebar", &self.bg_sidebar),
            ("bg-playerbar", &self.bg_playerbar),
            ("color-accent", &self.color_accent),
            ("color-accent-hover", &self.color_accent_hover),
            ("color-text-primary", &self.color_text_primary),
            ("color-text-secondary", &self.color_text_secondary),
            ("color-border", &self.color_border),
        ];

        for (name, value) in fields {
            if value.trim().is_empty() {
                return Err(format!("Theme color '{name}' cannot be empty"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Theme {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub colors: ThemeColors,
    #[serde(rename = "isCustom", default = "default_true")]
    pub is_custom: bool,
}

fn default_true() -> bool {
    true
}

#[tauri::command]
pub async fn export_theme(theme: Theme, export_path: String) -> Result<(), String> {
    if theme.name.trim().is_empty() {
        return Err("Theme name cannot be empty".to_string());
    }
    theme.colors.validate()?;

    let json = serde_json::to_string_pretty(&theme)
        .map_err(|e| format!("Failed to serialize theme: {e}"))?;

    fs::write(&export_path, json)
        .map_err(|e| format!("Failed to write theme to '{export_path}': {e}"))?;

    Ok(())
}

#[tauri::command]
pub async fn import_theme(file_path: String) -> Result<Theme, String> {
    let path = Path::new(&file_path);
    if !path.exists() {
        return Err(format!("Theme file not found: '{file_path}'"));
    }

    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read theme file '{file_path}': {e}"))?;

    let mut theme: Theme = serde_json::from_str(&content)
        .map_err(|e| format!("Invalid theme file format: {e}"))?;

    theme.colors.validate()?;

    let resolved_name = if !theme.name.trim().is_empty() {
        theme.name.trim().to_string()
    } else {
        path.file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("Imported Theme")
            .to_string()
    };

    let sanitized_slug: String = resolved_name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let sanitized_slug = sanitized_slug.trim_matches('-');
    let sanitized_slug = if sanitized_slug.is_empty() {
        "theme"
    } else {
        sanitized_slug
    };

    let short_uuid = &Uuid::new_v4().to_string()[..8];
    theme.id = format!("custom-{sanitized_slug}-{short_uuid}");
    theme.name = resolved_name;
    theme.is_custom = true;

    Ok(theme)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_colors() -> ThemeColors {
        ThemeColors {
            bg_main: "#0d0b18".to_string(),
            bg_sidebar: "#07050e".to_string(),
            bg_playerbar: "#0a0813".to_string(),
            color_accent: "#8b5cf6".to_string(),
            color_accent_hover: "#a78bfa".to_string(),
            color_text_primary: "#f3f4f6".to_string(),
            color_text_secondary: "#9ca3af".to_string(),
            color_border: "#1f1b2e".to_string(),
        }
    }

    #[tokio::test]
    async fn test_export_and_import_theme_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("test_theme_{}.json", Uuid::new_v4()));
        let file_str = file_path.to_str().unwrap().to_string();

        let original_theme = Theme {
            id: "custom-synthwave-original".to_string(),
            name: "Synthwave Glow".to_string(),
            colors: sample_colors(),
            is_custom: true,
        };

        // Export
        let export_res = export_theme(original_theme.clone(), file_str.clone()).await;
        assert!(export_res.is_ok());

        // Import
        let imported = import_theme(file_str.clone()).await.expect("import failed");
        assert_eq!(imported.name, "Synthwave Glow");
        assert_eq!(imported.colors, original_theme.colors);
        assert!(imported.is_custom);
        // Regenerated id should not match the original id
        assert_ne!(imported.id, original_theme.id);
        assert!(imported.id.starts_with("custom-synthwave-glow-"));

        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn test_import_theme_missing_colors() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("test_theme_bad_{}.json", Uuid::new_v4()));
        let file_str = file_path.to_str().unwrap().to_string();

        // Missing color-border
        let json = r##"{
            "name": "Incomplete Theme",
            "colors": {
                "bg-main": "#000",
                "bg-sidebar": "#111",
                "bg-playerbar": "#222",
                "color-accent": "#333",
                "color-accent-hover": "#444",
                "color-text-primary": "#555",
                "color-text-secondary": "#666"
            }
        }"##;

        fs::write(&file_path, json).unwrap();

        let res = import_theme(file_str).await;
        assert!(res.is_err());
        let err_msg = res.unwrap_err();
        assert!(err_msg.contains("Invalid theme file format"));

        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn test_import_theme_empty_color_value() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("test_theme_empty_val_{}.json", Uuid::new_v4()));
        let file_str = file_path.to_str().unwrap().to_string();

        let json = r##"{
            "name": "Empty Val Theme",
            "colors": {
                "bg-main": "   ",
                "bg-sidebar": "#111",
                "bg-playerbar": "#222",
                "color-accent": "#333",
                "color-accent-hover": "#444",
                "color-text-primary": "#555",
                "color-text-secondary": "#666",
                "color-border": "#777"
            }
        }"##;

        fs::write(&file_path, json).unwrap();

        let res = import_theme(file_str).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Theme color 'bg-main' cannot be empty"));

        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn test_import_theme_fallback_name_from_file_stem() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("Cyberpunk_2077.json");
        let file_str = file_path.to_str().unwrap().to_string();

        let json = r##"{
            "name": "",
            "colors": {
                "bg-main": "#000",
                "bg-sidebar": "#111",
                "bg-playerbar": "#222",
                "color-accent": "#333",
                "color-accent-hover": "#444",
                "color-text-primary": "#555",
                "color-text-secondary": "#666",
                "color-border": "#777"
            }
        }"##;

        fs::write(&file_path, json).unwrap();

        let imported = import_theme(file_str).await.expect("should derive name");
        assert_eq!(imported.name, "Cyberpunk_2077");
        assert!(imported.id.starts_with("custom-cyberpunk-2077-"));
        assert!(imported.is_custom);

        let _ = fs::remove_file(file_path);
    }

    #[tokio::test]
    async fn test_export_theme_validation() {
        let bad_theme = Theme {
            id: "bad".to_string(),
            name: "   ".to_string(),
            colors: sample_colors(),
            is_custom: true,
        };
        let res = export_theme(bad_theme, "irrelevant.json".to_string()).await;
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Theme name cannot be empty");
    }
}
