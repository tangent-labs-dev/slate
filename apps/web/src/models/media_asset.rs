use js_sys::Date;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MediaAsset {
    pub id: String,
    #[serde(default)]
    pub storage_path: String,
    pub note_id: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub data: Vec<u8>,
    pub created_at: f64,
}

impl MediaAsset {
    pub fn new(
        note_id: impl Into<String>,
        filename: impl Into<String>,
        mime_type: impl Into<String>,
        data: Vec<u8>,
    ) -> Self {
        let size_bytes = data.len() as u64;
        let id = Uuid::new_v4().to_string();
        Self {
            storage_path: format!("uploads/{id}"),
            id,
            note_id: note_id.into(),
            filename: filename.into(),
            mime_type: mime_type.into(),
            size_bytes,
            data,
            created_at: Date::now(),
        }
    }
}
