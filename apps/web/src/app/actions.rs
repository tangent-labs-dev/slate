use crate::models::Note;
use crate::store::upsert_note;
use leptos::prelude::{Set, Update, WriteSignal};
use wasm_bindgen_futures::spawn_local;

pub fn create_note_and_open(
    set_notes: WriteSignal<Vec<Note>>,
    set_active_note_id: WriteSignal<Option<String>>,
    set_open_tabs: WriteSignal<Vec<String>>,
    set_db_error: WriteSignal<Option<String>>,
) {
    let note = Note::new("Untitled", "");
    let note_id = note.id.clone();

    set_notes.update(|all| all.push(note.clone()));
    set_active_note_id.set(Some(note_id.clone()));
    set_open_tabs.update(|tabs| {
        if !tabs.iter().any(|t| t == &note_id) {
            tabs.push(note_id.clone());
        }
    });

    spawn_local(async move {
        if let Err(e) = upsert_note(&note).await {
            set_db_error.set(Some(format!("{e:?}")));
        }
    });
}
