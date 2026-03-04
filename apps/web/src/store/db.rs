use rexie::{ObjectStore, Rexie};
use wasm_bindgen::JsValue;

const DB_NAME: &str = "slate-db";
pub const NOTES_STORE: &str = "notes";

pub async fn open_db() -> Result<Rexie, JsValue> {
    Rexie::builder(DB_NAME)
        .version(1)
        .add_object_store(ObjectStore::new(NOTES_STORE).key_path("id"))
        .build()
        .await
        .map_err(|e| JsValue::from_str(&format!("open_db failed: {e}")))
}
