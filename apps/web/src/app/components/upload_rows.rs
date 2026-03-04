use crate::app::helpers::{format_bytes, normalized_storage_path};
use crate::models::MediaAsset;
use leptos::{ev::MouseEvent, prelude::*};

#[component]
pub fn UploadRows(assets: Signal<Vec<MediaAsset>>, on_delete: Callback<String>) -> impl IntoView {
    view! {
        <div class="uploads-list">
            <For
                each=move || assets.get()
                key=|asset| asset.id.clone()
                children=move |asset: MediaAsset| {
                    let asset_id = asset.id.clone();
                    let storage_path = normalized_storage_path(&asset.storage_path, &asset.id);
                    let file_size = format_bytes(asset.size_bytes);
                    view! {
                        <div class="upload-row">
                            <div class="upload-meta">
                                <span class="upload-name">{asset.filename}</span>
                                <span class="upload-path">{storage_path}</span>
                                <span class="upload-size">{file_size}</span>
                            </div>
                            <button
                                class="upload-remove"
                                title="Delete upload"
                                on:click=move |ev: MouseEvent| {
                                    ev.stop_propagation();
                                    on_delete.run(asset_id.clone());
                                }
                            >
                                "Delete"
                            </button>
                        </div>
                    }
                }
            />
        </div>
    }
}
