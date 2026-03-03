use js_sys::Date;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorMode {
    Raw,
    Preview,
    Split,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub created_at: f64,
    pub updated_at: f64,
    #[serde(default)]
    pub is_deleted: bool,
    #[serde(default)]
    pub deleted_at: Option<f64>,
    #[serde(default)]
    pub last_synced_at: Option<f64>,
}

impl Note {
    pub fn new(title: impl Into<String>, content: impl Into<String>) -> Self {
        let now = Date::now();
        let content = content.into();

        let mut title = title.into();
        if title.trim().is_empty() {
            title = derive_title(&content);
        }

        Self {
            id: Uuid::new_v4().to_string(),
            title,
            content,
            created_at: now,
            updated_at: now,
            is_deleted: false,
            deleted_at: None,
            last_synced_at: None,
        }
    }
}

pub fn derive_title(markdown: &str) -> String {
    let line = markdown
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("Untitled");
    let title = line.trim().trim_start_matches('#').trim();
    let title = title.chars().take(60).collect::<String>();
    if title.trim().is_empty() {
        "Untitled".to_string()
    } else {
        title
    }
}
