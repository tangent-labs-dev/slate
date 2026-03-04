use crate::models::MediaAsset;
use crate::store::db::{MEDIA_STORE, open_db};
use rexie::TransactionMode;
use wasm_bindgen::JsValue;

pub async fn load_all_media_assets() -> Result<Vec<MediaAsset>, JsValue> {
    let db = open_db().await?;
    let tx = db
        .transaction(&[MEDIA_STORE], TransactionMode::ReadOnly)
        .map_err(|e| JsValue::from_str(&format!("readonly tx failed: {e}")))?;
    let store = tx
        .store(MEDIA_STORE)
        .map_err(|e| JsValue::from_str(&format!("store failed: {e}")))?;

    let values = store
        .get_all(None, None)
        .await
        .map_err(|e| JsValue::from_str(&format!("get_all failed: {e}")))?;

    tx.done()
        .await
        .map_err(|e| JsValue::from_str(&format!("tx done failed: {e}")))?;

    let mut assets = Vec::new();
    for value in values {
        if let Ok(asset) = serde_wasm_bindgen::from_value::<MediaAsset>(value) {
            assets.push(asset);
        }
    }
    assets.sort_by(|a, b| b.created_at.total_cmp(&a.created_at));
    Ok(assets)
}

pub async fn upsert_media_asset(asset: &MediaAsset) -> Result<(), JsValue> {
    let db = open_db().await?;
    let tx = db
        .transaction(&[MEDIA_STORE], TransactionMode::ReadWrite)
        .map_err(|e| JsValue::from_str(&format!("readwrite tx failed: {e}")))?;
    let store = tx
        .store(MEDIA_STORE)
        .map_err(|e| JsValue::from_str(&format!("store failed: {e}")))?;

    let value = serde_wasm_bindgen::to_value(asset)
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

pub async fn delete_media_assets_by_ids(ids: &[String]) -> Result<(), JsValue> {
    if ids.is_empty() {
        return Ok(());
    }

    let db = open_db().await?;
    let tx = db
        .transaction(&[MEDIA_STORE], TransactionMode::ReadWrite)
        .map_err(|e| JsValue::from_str(&format!("readwrite tx failed: {e}")))?;
    let store = tx
        .store(MEDIA_STORE)
        .map_err(|e| JsValue::from_str(&format!("store failed: {e}")))?;

    for id in ids {
        store
            .delete(JsValue::from_str(id))
            .await
            .map_err(|e| JsValue::from_str(&format!("delete failed: {e}")))?;
    }

    tx.done()
        .await
        .map_err(|e| JsValue::from_str(&format!("tx done failed: {e}")))?;
    Ok(())
}
