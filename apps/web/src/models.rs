use js_sys::{Date, Math};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorMode {
    Raw,
    Preview,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub created_at: f64,
    pub updated_at: f64,
}

impl Note {
    pub fn new(title: impl Into<String>, content: impl Into<String>) -> Self {
        let now = Date::now();
        let random = (Math::random() * 1_000_000.0) as u64;
        let content = content.into();

        let mut title = title.into();
        if title.trim().is_empty() {
            title = derive_title(&content);
        }

        Self {
            id: format!("note-{}-{}", now as u64, random),
            title,
            content,
            created_at: now,
            updated_at: now,
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