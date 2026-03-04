use crate::models::Note;
use crate::store::db::{NOTES_STORE, open_db};
use rexie::TransactionMode;
use wasm_bindgen::JsValue;

pub async fn load_all_notes() -> Result<Vec<Note>, JsValue> {
    let db = open_db().await?;
    let tx = db
        .transaction(&[NOTES_STORE], TransactionMode::ReadOnly)
        .map_err(|e| JsValue::from_str(&format!("readonly tx failed: {e}")))?;
    let store = tx
        .store(NOTES_STORE)
        .map_err(|e| JsValue::from_str(&format!("store failed: {e}")))?;

    let values = store
        .get_all(None, None)
        .await
        .map_err(|e| JsValue::from_str(&format!("get_all failed: {e}")))?;

    tx.done()
        .await
        .map_err(|e| JsValue::from_str(&format!("tx done failed: {e}")))?;

    let mut notes = Vec::new();
    for v in values {
        if let Ok(note) = serde_wasm_bindgen::from_value::<Note>(v) {
            if !note.is_deleted {
                notes.push(note);
            }
        }
    }

    notes.sort_by(|a, b| b.created_at.total_cmp(&a.created_at));
    Ok(notes)
}

pub async fn upsert_note(note: &Note) -> Result<(), JsValue> {
    let db = open_db().await?;
    let tx = db
        .transaction(&[NOTES_STORE], TransactionMode::ReadWrite)
        .map_err(|e| JsValue::from_str(&format!("readwrite tx failed: {e}")))?;
    let store = tx
        .store(NOTES_STORE)
        .map_err(|e| JsValue::from_str(&format!("store failed: {e}")))?;

    let existing = store
        .get(JsValue::from_str(&note.id))
        .await
        .map_err(|e| JsValue::from_str(&format!("get existing failed: {e}")))?;
    if let Some(existing_value) = existing
        && let Ok(existing_note) = serde_wasm_bindgen::from_value::<Note>(existing_value)
        && existing_note.updated_at > note.updated_at
    {
        tx.done()
            .await
            .map_err(|e| JsValue::from_str(&format!("tx done failed: {e}")))?;
        return Ok(());
    }

    let value = serde_wasm_bindgen::to_value(note)
        .map_err(|e| JsValue::from_str(&format!("serialize failed: {e}")))?;

    store
        .put(&value, None)
        .await
        .map_err(|e| JsValue::from_str(&format!("put failed: {e}")))?;

    tx.done()
        .await
        .map_err(|e| JsValue::from_str(&format!("tx done failed: {e}")))?;
    Ok(())
}
