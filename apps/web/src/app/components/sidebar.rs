use crate::app::components::UploadRows;
use crate::models::{MediaAsset, Note};
use icondata::{BsChevronLeft, BsChevronRight, BsPlusLg};
use leptos::{ev::MouseEvent, prelude::*};
use leptos_icons::Icon;

#[component]
pub fn Sidebar(
    sidebar_collapsed: ReadSignal<bool>,
    set_sidebar_collapsed: WriteSignal<bool>,
    search_query: ReadSignal<String>,
    set_search_query: WriteSignal<String>,
    filtered_notes: Signal<Vec<Note>>,
    notes: ReadSignal<Vec<Note>>,
    active_note_id: ReadSignal<Option<String>>,
    sorted_uploads: Signal<Vec<MediaAsset>>,
    set_context_menu: WriteSignal<Option<(String, i32, i32)>>,
    on_open_note: Callback<String>,
    on_new_note: Callback<()>,
    on_delete_upload: Callback<String>,
) -> impl IntoView {
    view! {
        <aside class="sidebar">
            <div class="sidebar-header">
                <h2 class="sidebar-title">"Notes"</h2>
                <button
                    class="icon-btn collapse-btn"
                    title=move || if sidebar_collapsed.get() { "Expand sidebar" } else { "Collapse sidebar" }
                    on:click=move |_| set_sidebar_collapsed.update(|v| *v = !*v)
                >
                    {move || {
                        if sidebar_collapsed.get() {
                            view! { <Icon icon=BsChevronRight /> }.into_any()
                        } else {
                            view! { <Icon icon=BsChevronLeft /> }.into_any()
                        }
                    }}
                </button>
            </div>

            <button
                class="icon-btn collapse-btn collapsed-new-btn"
                title="New note"
                on:click=move |_| on_new_note.run(())
            >
                <Icon icon=BsPlusLg />
            </button>

            <button class="primary-btn new-note-btn" on:click=move |_| on_new_note.run(())>
                <Icon icon=BsPlusLg />
                " New note"
            </button>

            <div class="search-wrap">
                <input
                    class="search-input"
                    type="text"
                    placeholder="Search notes..."
                    prop:value=move || search_query.get()
                    on:input=move |ev| set_search_query.set(event_target_value(&ev))
                />
            </div>

            <div class="notes-scroll">
                <p class="sidebar-section-title">"Notes"</p>
                <For
                    each=move || filtered_notes.get()
                    key=|n| n.id.clone()
                    children=move |n: Note| {
                        let id = n.id.clone();
                        let id_for_label = id.clone();
                        let id_for_open = id.clone();
                        let id_for_open_active = id.clone();
                        let id_for_menu = id.clone();
                        let note_label = move || {
                            notes
                                .get()
                                .into_iter()
                                .find(|note| note.id == id_for_label)
                                .map(|note| note.title)
                                .unwrap_or_else(|| "Untitled".to_string())
                        };
                        view! {
                            <div class="note-row" on:contextmenu={
                                let id_for_menu = id_for_menu.clone();
                                move |ev: MouseEvent| {
                                    ev.prevent_default();
                                    set_context_menu.set(Some((id_for_menu.clone(), ev.client_x(), ev.client_y())));
                                }
                            }>
                                <button
                                    class=move || {
                                        if active_note_id.get().as_deref() == Some(id_for_open_active.as_str()) {
                                            "note-main active"
                                        } else {
                                            "note-main"
                                        }
                                    }
                                    on:click={
                                        let id_for_open = id_for_open.clone();
                                        move |_| on_open_note.run(id_for_open.clone())
                                    }
                                >
                                    <span class="note-label">{note_label}</span>
                                </button>
                            </div>
                        }
                    }
                />
                <div class="sidebar-uploads-folder">
                    <p class="sidebar-section-title">
                        {move || format!("Uploads ({})", sorted_uploads.get().len())}
                    </p>
                    <Show
                        when=move || !sorted_uploads.get().is_empty()
                        fallback=move || {
                            view! { <p class="uploads-empty">"No uploads yet."</p> }
                        }
                    >
                        <UploadRows
                            assets=Signal::derive(move || sorted_uploads.get())
                            on_delete=on_delete_upload
                        />
                    </Show>
                </div>
            </div>
        </aside>
    }
}
