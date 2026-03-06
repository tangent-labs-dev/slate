use crate::app::components::UploadRows;
use crate::links::normalize_title;
use crate::models::{MediaAsset, Note};
use crate::note_graph::propagate_renamed_title;
use crate::store::upsert_note;
use js_sys::Date;
use leptos::prelude::*;
use leptos::ev::MouseEvent;
use std::collections::HashMap;
use wasm_bindgen_futures::spawn_local;

#[component]
pub fn NoteHeader(
    notes: ReadSignal<Vec<Note>>,
    set_notes: WriteSignal<Vec<Note>>,
    active_note_id: ReadSignal<Option<String>>,
    active_note: Signal<Option<Note>>,
    title_before_edit: ReadSignal<Option<String>>,
    set_title_before_edit: WriteSignal<Option<String>>,
    backlinks: Signal<Vec<String>>,
    active_note_uploads: Signal<Vec<MediaAsset>>,
    active_note_whiteboards: Signal<Vec<MediaAsset>>,
    whiteboard_name_index: Signal<HashMap<String, String>>,
    set_db_error: WriteSignal<Option<String>>,
    on_open_note: Callback<String>,
    on_open_ink: Callback<String>,
    save_note: Callback<String>,
    on_delete_upload: Callback<String>,
) -> impl IntoView {
    view! {
        <input
            class="title"
            prop:value=move || active_note.get().map(|n| n.title).unwrap_or_default()
            on:input=move |ev| {
                if let Some(note_id) = active_note_id.get_untracked() {
                    if title_before_edit.get_untracked().is_none() {
                        let previous = notes
                            .get_untracked()
                            .into_iter()
                            .find(|n| n.id == note_id)
                            .map(|n| n.title);
                        set_title_before_edit.set(previous);
                    }
                    let value = event_target_value(&ev);
                    let now = Date::now();
                    set_notes.update(|all| {
                        if let Some(note) = all.iter_mut().find(|x| x.id == note_id) {
                            note.title = value.clone();
                            note.updated_at = now;
                        }
                    });
                    save_note.run(note_id);
                }
            }
            on:blur=move |_| {
                if let Some(note_id) = active_note_id.get_untracked() {
                    let old_title = title_before_edit.get_untracked();
                    let new_title = notes
                        .get_untracked()
                        .into_iter()
                        .find(|n| n.id == note_id)
                        .map(|n| n.title)
                        .unwrap_or_default();
                    if let Some(old) = old_title
                        && normalize_title(&old) != normalize_title(&new_title)
                    {
                        let mut changed = Vec::<Note>::new();
                        set_notes.update(|all| {
                            changed = propagate_renamed_title(all, &old, &new_title);
                        });
                        if !changed.is_empty() {
                            spawn_local({
                                let set_db_error = set_db_error.clone();
                                async move {
                                    for note in changed {
                                        if let Err(e) = upsert_note(&note).await {
                                            set_db_error.set(Some(format!("{e:?}")));
                                            break;
                                        }
                                    }
                                }
                            });
                        }
                    }
                    save_note.run(note_id);
                }
                set_title_before_edit.set(None);
            }
        />

        <div class="backlinks-wrap">
            <p class="backlinks-title">"Linked mentions"</p>
            <Show
                when=move || !backlinks.get().is_empty()
                fallback=move || view! { <p class="backlinks-empty">"No backlinks yet."</p> }
            >
                <div class="backlinks-list">
                    <For
                        each=move || backlinks.get()
                        key=|id| id.clone()
                        children=move |id: String| {
                            let id_for_open = id.clone();
                            let id_for_label = id.clone();
                            let label = move || {
                                notes
                                    .get()
                                    .into_iter()
                                    .find(|n| n.id == id_for_label)
                                    .map(|n| n.title)
                                    .unwrap_or_else(|| "Untitled".to_string())
                            };
                            view! {
                                <button class="backlink-item" on:click=move |_| on_open_note.run(id_for_open.clone())>
                                    {label}
                                </button>
                            }
                        }
                    />
                </div>
            </Show>
        </div>

        <div class="note-uploads-wrap">
            <p class="backlinks-title">
                {move || format!("Uploads in this note ({})", active_note_uploads.get().len())}
            </p>
            <Show
                when=move || !active_note_uploads.get().is_empty()
                fallback=move || {
                    view! { <p class="backlinks-empty">"No uploads in this note yet."</p> }
                }
            >
                <UploadRows
                    assets=Signal::derive(move || active_note_uploads.get())
                    on_delete=on_delete_upload
                />
            </Show>
        </div>
        <div class="note-uploads-wrap">
            <p class="backlinks-title">
                {move || format!("Whiteboards in this note ({})", active_note_whiteboards.get().len())}
            </p>
            <Show
                when=move || !active_note_whiteboards.get().is_empty()
                fallback=move || {
                    view! { <p class="backlinks-empty">"No whiteboards in this note yet."</p> }
                }
            >
                <div class="uploads-list">
                    <For
                        each=move || active_note_whiteboards.get()
                        key=|asset| asset.id.clone()
                        children=move |asset: MediaAsset| {
                            let board_id = asset.id.clone();
                            let board_id_open = board_id.clone();
                            let board_id_delete = board_id.clone();
                            let board_id_for_name = board_id.clone();
                            let name = move || {
                                whiteboard_name_index
                                    .get()
                                    .get(&board_id_for_name)
                                    .cloned()
                                    .unwrap_or_else(|| "Whiteboard".to_string())
                            };
                            view! {
                                <div class="upload-row">
                                    <button class="upload-main" on:click=move |_| on_open_ink.run(board_id_open.clone())>
                                        <div class="upload-meta">
                                            <span class="upload-name">{name}</span>
                                            <span class="upload-path">{board_id.clone()}</span>
                                        </div>
                                    </button>
                                    <button
                                        class="upload-remove"
                                        title="Delete whiteboard"
                                        on:click=move |ev: MouseEvent| {
                                            ev.stop_propagation();
                                            on_delete_upload.run(board_id_delete.clone());
                                        }
                                    >
                                        "Delete"
                                    </button>
                                </div>
                            }
                        }
                    />
                </div>
            </Show>
        </div>
    }
}
