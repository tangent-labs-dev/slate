use rexie::{ObjectStore, Rexie};
use wasm_bindgen::JsValue;

const DB_NAME: &str = "slate-db";
pub const NOTES_STORE: &str = "notes";
pub const MEDIA_STORE: &str = "media_assets";

pub async fn open_db() -> Result<Rexie, JsValue> {
    Rexie::builder(DB_NAME)
        .version(2)
        .add_object_store(ObjectStore::new(NOTES_STORE).key_path("id"))
        .add_object_store(
            ObjectStore::new(MEDIA_STORE)
                .key_path("id")
                .add_index(rexie::Index::new("note_id", "note_id"))
                .add_index(rexie::Index::new("created_at", "created_at")),
        )
        .build()
        .await
        .map_err(|e| JsValue::from_str(&format!("open_db failed: {e}")))
}
