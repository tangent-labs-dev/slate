use crate::models::Note;
use icondata::{BsPlusLg, BsXLg};
use leptos::{ev::MouseEvent, prelude::*};
use leptos_icons::Icon;

#[component]
pub fn TabStrip(
    open_tabs: Signal<Vec<String>>,
    notes: ReadSignal<Vec<Note>>,
    active_note_id: ReadSignal<Option<String>>,
    on_open_note: Callback<String>,
    on_close_tab: Callback<String>,
    on_new_note: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="tabstrip">
            <div class="tabstrip-inner">
                <For
                    each=move || open_tabs.get()
                    key=|id| id.clone()
                    children=move |id: String| {
                        let id_for_label = id.clone();
                        let id_for_click = id.clone();
                        let id_for_active = id.clone();
                        let id_for_close = id.clone();
                        let label = move || {
                            notes
                                .get()
                                .into_iter()
                                .find(|n| n.id == id_for_label)
                                .map(|n| n.title)
                                .unwrap_or_else(|| "Untitled".to_string())
                        };
                        view! {
                            <div class=move || {
                                if active_note_id.get().as_deref() == Some(id_for_active.as_str()) {
                                    "browser-tab active"
                                } else {
                                    "browser-tab"
                                }
                            }>
                                <button
                                    class="browser-tab-main"
                                    on:click={
                                        let id_for_click = id_for_click.clone();
                                        move |_| on_open_note.run(id_for_click.clone())
                                    }
                                >
                                    {label}
                                </button>
                                <button
                                    class="browser-tab-close"
                                    on:click={
                                        let id_for_close = id_for_close.clone();
                                        move |ev: MouseEvent| {
                                            ev.stop_propagation();
                                            on_close_tab.run(id_for_close.clone());
                                        }
                                    }
                                >
                                    <Icon icon=BsXLg />
                                </button>
                            </div>
                        }
                    }
                />
            </div>
            <button class="new-tab-btn icon-btn" on:click=move |_| on_new_note.run(())>
                <Icon icon=BsPlusLg />
            </button>
        </div>
    }
}
