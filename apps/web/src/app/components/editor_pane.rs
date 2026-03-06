use crate::app::components::{ContentPanel, NoteHeader, TabStrip};
use crate::models::{EditorMode, MediaAsset, Note};
use leptos::prelude::*;
use std::collections::HashMap;

#[component]
pub fn EditorPane(
    open_tabs: Signal<Vec<String>>,
    notes: ReadSignal<Vec<Note>>,
    set_notes: WriteSignal<Vec<Note>>,
    active_note_id: ReadSignal<Option<String>>,
    active_note: Signal<Option<Note>>,
    mode: ReadSignal<EditorMode>,
    preview_html: Signal<String>,
    title_before_edit: ReadSignal<Option<String>>,
    set_title_before_edit: WriteSignal<Option<String>>,
    backlinks: Signal<Vec<String>>,
    active_note_uploads: Signal<Vec<MediaAsset>>,
    active_note_whiteboards: Signal<Vec<MediaAsset>>,
    whiteboard_name_index: Signal<HashMap<String, String>>,
    set_db_error: WriteSignal<Option<String>>,
    on_open_note: Callback<String>,
    on_open_ink: Callback<String>,
    on_open_or_create_note: Callback<String>,
    on_close_tab: Callback<String>,
    on_new_note: Callback<()>,
    save_note: Callback<String>,
    cleanup_orphaned_media: Callback<()>,
    on_delete_upload: Callback<String>,
) -> impl IntoView {
    view! {
        <TabStrip
            open_tabs=open_tabs
            notes=notes
            active_note_id=active_note_id
            on_open_note=on_open_note
            on_close_tab=on_close_tab
            on_new_note=on_new_note
        />

        <div class="editor-wrap">
            <Show
                when=move || active_note_id.get().is_some()
                fallback=move || view! { <p>"No active note selected."</p> }
            >
                <NoteHeader
                    notes=notes
                    set_notes=set_notes
                    active_note_id=active_note_id
                    active_note=active_note
                    title_before_edit=title_before_edit
                    set_title_before_edit=set_title_before_edit
                    backlinks=backlinks
                    active_note_uploads=active_note_uploads
                    active_note_whiteboards=active_note_whiteboards
                    whiteboard_name_index=whiteboard_name_index
                    set_db_error=set_db_error
                    on_open_note=on_open_note
                    on_open_ink=on_open_ink
                    save_note=save_note
                    on_delete_upload=on_delete_upload
                />
                <ContentPanel
                    mode=mode
                    active_note_id=active_note_id
                    active_note=active_note
                    preview_html=preview_html
                    set_notes=set_notes
                    on_open_note=on_open_note
                    on_open_or_create_note=on_open_or_create_note
                    on_open_ink=on_open_ink
                    save_note=save_note
                    cleanup_orphaned_media=cleanup_orphaned_media
                />
            </Show>
        </div>
    }
}
