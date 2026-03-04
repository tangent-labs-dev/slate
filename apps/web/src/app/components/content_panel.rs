use crate::models::{EditorMode, Note, derive_title};
use crate::note_graph::closest_wiki_anchor;
use js_sys::Date;
use leptos::prelude::*;
use leptos::{ev::MouseEvent, web_sys::Element};
use wasm_bindgen::JsCast;

#[component]
pub fn ContentPanel(
    mode: ReadSignal<EditorMode>,
    active_note_id: ReadSignal<Option<String>>,
    active_note: Signal<Option<Note>>,
    preview_html: Signal<String>,
    set_notes: WriteSignal<Vec<Note>>,
    on_open_note: Callback<String>,
    on_open_or_create_note: Callback<String>,
    save_note: Callback<String>,
    cleanup_orphaned_media: Callback<()>,
) -> impl IntoView {
    view! {
        <section class="panel">
            <Show
                when=move || mode.get() == EditorMode::Raw
                fallback=move || view! {
                    <Show
                        when=move || mode.get() == EditorMode::Preview
                        fallback=move || view! {
                            <div class="live-grid">
                                <div class="live-pane">
                                    <p class="live-pane-title">"Editor"</p>
                                    <textarea
                                        class="editor"
                                        wrap="soft"
                                        prop:value=move || active_note.get().map(|n| n.content).unwrap_or_default()
                                        on:input=move |ev| {
                                            if let Some(note_id) = active_note_id.get_untracked() {
                                                let value = event_target_value(&ev);
                                                let now = Date::now();
                                                let auto_title = derive_title(&value);

                                                set_notes.update(|all| {
                                                    if let Some(note) = all.iter_mut().find(|x| x.id == note_id) {
                                                        note.content = value.clone();
                                                        note.updated_at = now;
                                                        if note.title.trim().is_empty() || note.title == "Untitled" {
                                                            note.title = auto_title.clone();
                                                        }
                                                    }
                                                });
                                                crate::app::bindings::auto_resize_editors();
                                                save_note.run(note_id);
                                            }
                                        }
                                        on:blur=move |_| {
                                            if let Some(note_id) = active_note_id.get_untracked() {
                                                save_note.run(note_id);
                                            }
                                            cleanup_orphaned_media.run(());
                                        }
                                        spellcheck="false"
                                    />
                                </div>
                                <div class="live-pane">
                                    <p class="live-pane-title">"Preview"</p>
                                    <article
                                        class="preview live-preview"
                                        on:click=move |ev: MouseEvent| {
                                            if let Some(target) = ev.target().and_then(|t| t.dyn_into::<Element>().ok())
                                                && let Some(anchor) = closest_wiki_anchor(target)
                                            {
                                                ev.prevent_default();
                                                if let Some(note_id) = anchor.get_attribute("data-note-id") {
                                                    on_open_note.run(note_id);
                                                } else if let Some(title) = anchor.get_attribute("data-note-title") {
                                                    on_open_or_create_note.run(title);
                                                }
                                            }
                                        }
                                        inner_html=move || preview_html.get()
                                    ></article>
                                </div>
                            </div>
                        }
                    >
                        <article
                            class="preview"
                            on:click=move |ev: MouseEvent| {
                                if let Some(target) = ev.target().and_then(|t| t.dyn_into::<Element>().ok())
                                    && let Some(anchor) = closest_wiki_anchor(target)
                                {
                                    ev.prevent_default();
                                    if let Some(note_id) = anchor.get_attribute("data-note-id") {
                                        on_open_note.run(note_id);
                                    } else if let Some(title) = anchor.get_attribute("data-note-title") {
                                        on_open_or_create_note.run(title);
                                    }
                                }
                            }
                            inner_html=move || preview_html.get()
                        ></article>
                    </Show>
                }
            >
                <textarea
                    class="editor"
                    wrap="soft"
                    prop:value=move || active_note.get().map(|n| n.content).unwrap_or_default()
                    on:input=move |ev| {
                        if let Some(note_id) = active_note_id.get_untracked() {
                            let value = event_target_value(&ev);
                            let now = Date::now();
                            let auto_title = derive_title(&value);

                            set_notes.update(|all| {
                                if let Some(note) = all.iter_mut().find(|x| x.id == note_id) {
                                    note.content = value.clone();
                                    note.updated_at = now;
                                    if note.title.trim().is_empty() || note.title == "Untitled" {
                                        note.title = auto_title.clone();
                                    }
                                }
                            });
                            crate::app::bindings::auto_resize_editors();
                            save_note.run(note_id);
                        }
                    }
                    on:blur=move |_| {
                        if let Some(note_id) = active_note_id.get_untracked() {
                            save_note.run(note_id);
                        }
                        cleanup_orphaned_media.run(());
                    }
                    spellcheck="false"
                />
            </Show>
        </section>
    }
}
