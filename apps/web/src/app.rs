use crate::markdown::render_markdown;
use crate::models::{derive_title, EditorMode, Note};
use crate::store::indexed_db::{delete_note, load_all_notes, upsert_note};
use icondata::{BsMoonStarsFill, BsPlusLg, BsSunFill, BsXLg};
use js_sys::Date;
use leptos::{ev::MouseEvent, prelude::*};
use leptos_icons::Icon;
use wasm_bindgen_futures::spawn_local;

const WRAP_COLS: usize = 88;

fn wrapped_line_number_rows(content: &str, cols: usize) -> Vec<Option<usize>> {
    let cols = cols.max(1);
    let mut rows = Vec::new();

    for (idx, line) in content.split('\n').enumerate() {
        // Keep numbering by logical lines, but reserve extra blank rows for wrapped visuals.
        let wrapped_rows = line.chars().count().max(1).div_ceil(cols);
        rows.push(Some(idx + 1));
        for _ in 1..wrapped_rows {
            rows.push(None);
        }
    }

    if rows.is_empty() {
        rows.push(Some(1));
    }

    rows
}

#[component]
pub fn App() -> impl IntoView {
    let (notes, set_notes) = signal::<Vec<Note>>(vec![]);
    let (active_note_id, set_active_note_id) = signal::<Option<String>>(None);
    let (open_tabs, set_open_tabs) = signal::<Vec<String>>(vec![]);
    let (mode, set_mode) = signal(EditorMode::Raw);
    let (db_error, set_db_error) = signal::<Option<String>>(None);
    let (context_menu, set_context_menu) = signal::<Option<(String, i32, i32)>>(None);
    let (is_dark, set_is_dark) = signal(true);

    let sorted_notes = Memo::new(move |_| {
        let mut n = notes.get();
        n.sort_by(|a, b| b.updated_at.total_cmp(&a.updated_at));
        n
    });

    let active_note = Memo::new(move |_| {
        let id = active_note_id.get();
        notes
            .get()
            .into_iter()
            .find(|n| Some(n.id.as_str()) == id.as_deref())
    });

    let preview_html = Memo::new(move |_| {
        if mode.get() == EditorMode::Preview {
            active_note
                .get()
                .map(|n| render_markdown(&n.content))
                .unwrap_or_default()
        } else {
            String::new()
        }
    });

    // Initial load
    Effect::new(move |_| {
        spawn_local({
            let set_notes = set_notes.clone();
            let set_active_note_id = set_active_note_id.clone();
            let set_open_tabs = set_open_tabs.clone();
            let set_db_error = set_db_error.clone();

            async move {
                match load_all_notes().await {
                    Ok(mut loaded) => {
                        if loaded.is_empty() {
                            let starter = Note::new("Welcome", "# Welcome\n\nStart writing.");
                            if let Err(e) = upsert_note(&starter).await {
                                set_db_error.set(Some(format!("{e:?}")));
                            }
                            loaded.push(starter);
                        }

                        loaded.sort_by(|a, b| b.updated_at.total_cmp(&a.updated_at));
                        let first_id = loaded[0].id.clone();
                        set_notes.set(loaded);
                        set_active_note_id.set(Some(first_id.clone()));
                        set_open_tabs.set(vec![first_id]);
                    }
                    Err(e) => set_db_error.set(Some(format!("{e:?}"))),
                }
            }
        });
    });

    let save_note = move |id: String| {
        if let Some(note) = notes.get_untracked().into_iter().find(|n| n.id == id) {
            spawn_local({
                let set_db_error = set_db_error.clone();
                async move {
                    if let Err(e) = upsert_note(&note).await {
                        set_db_error.set(Some(format!("{e:?}")));
                    }
                }
            });
        }
    };

    let open_note = move |id: String| {
        set_active_note_id.set(Some(id.clone()));
        set_open_tabs.update(|tabs| {
            if !tabs.iter().any(|t| t == &id) {
                tabs.push(id);
            }
        });
    };

    let close_tab_by_id = move |id: String| {
        let tabs_before = open_tabs.get_untracked();
        let closed_index = tabs_before.iter().position(|t| t == &id).unwrap_or(0);
        let remaining_tabs: Vec<String> = tabs_before
            .into_iter()
            .filter(|t| t != &id)
            .collect();

        set_open_tabs.set(remaining_tabs.clone());

        if active_note_id.get_untracked().as_deref() == Some(id.as_str()) {
            let next_active = if remaining_tabs.is_empty() {
                notes.get_untracked().first().map(|n| n.id.clone())
            } else if closed_index == 0 {
                remaining_tabs.first().cloned()
            } else {
                remaining_tabs
                    .get(closed_index.saturating_sub(1))
                    .cloned()
                    .or_else(|| remaining_tabs.first().cloned())
            };
            set_active_note_id.set(next_active);
        }
    };

    let delete_note_by_id = move |id: String| {
        set_notes.update(|all| all.retain(|x| x.id != id));
        close_tab_by_id(id.clone());

        if active_note_id.get_untracked().as_deref() == Some(id.as_str()) {
            let next_open = open_tabs.get_untracked().first().cloned();
            if next_open.is_some() {
                set_active_note_id.set(next_open);
            } else {
                let next_any = notes.get_untracked().first().map(|n| n.id.clone());
                set_active_note_id.set(next_any);
            }
        }

        spawn_local({
            let set_db_error = set_db_error.clone();
            async move {
                if let Err(e) = delete_note(&id).await {
                    set_db_error.set(Some(format!("{e:?}")));
                }
            }
        });
    };

    let duplicate_note = move |id: String| {
        if let Some(source) = notes.get_untracked().into_iter().find(|n| n.id == id) {
            let duplicated = Note::new(format!("{} Copy", source.title), source.content);
            let duplicated_id = duplicated.id.clone();
            set_notes.update(|all| all.push(duplicated.clone()));
            open_note(duplicated_id);

            spawn_local({
                let set_db_error = set_db_error.clone();
                async move {
                    if let Err(e) = upsert_note(&duplicated).await {
                        set_db_error.set(Some(format!("{e:?}")));
                    }
                }
            });
        }
    };

    view! {
        <main
            class="app"
            data-theme=move || if is_dark.get() { "dark" } else { "light" }
            on:click=move |_| set_context_menu.set(None)
        >
            <style>{r#"
                * { box-sizing: border-box; }
                :root, body { margin: 0; }
                .app[data-theme="light"] {
                    --bg: #f5f6fa;
                    --bg-panel: #ffffff;
                    --bg-alt: #f1f3f8;
                    --bg-tab: #ebeef5;
                    --line: #d8dde8;
                    --line-soft: #e4e8f1;
                    --text: #1f2937;
                    --text-muted: #6b7280;
                    --accent: #4f46e5;
                    --danger: #dc2626;
                    color-scheme: light;
                }
                .app[data-theme="dark"] {
                    --bg: #1f1f1f;
                    --bg-panel: #262626;
                    --bg-alt: #202020;
                    --bg-tab: #2d2d2d;
                    --line: #373737;
                    --line-soft: #424242;
                    --text: #d7dce2;
                    --text-muted: #9aa3af;
                    --accent: #8b5cf6;
                    --danger: #f87171;
                    color-scheme: dark;
                }
                .app {
                    display: grid;
                    grid-template-columns: 280px minmax(0, 1fr);
                    min-height: 100vh;
                    background: var(--bg);
                    color: var(--text);
                    font-family: Inter, system-ui, -apple-system, Segoe UI, Roboto, sans-serif;
                }
                .sidebar {
                    border-right: 1px solid var(--line);
                    background: var(--bg-panel);
                    padding: 0.6rem 0.65rem;
                }
                .sidebar-header {
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    margin-bottom: 0.45rem;
                }
                .sidebar-title {
                    font-size: 0.88rem;
                    text-transform: uppercase;
                    letter-spacing: 0.04em;
                    color: var(--text-muted);
                    margin: 0;
                }
                .chip {
                    border: 1px solid var(--line);
                    border-radius: 999px;
                    background: var(--bg-alt);
                    color: var(--text-muted);
                    font-size: 0.72rem;
                    padding: 0.12rem 0.45rem;
                }
                .primary-btn,
                .secondary-btn {
                    border: 1px solid var(--line);
                    background: var(--bg-alt);
                    color: var(--text);
                    border-radius: 7px;
                    cursor: pointer;
                    font-size: 0.82rem;
                    padding: 0.32rem 0.6rem;
                }
                .primary-btn {
                    width: 100%;
                    margin-bottom: 0.55rem;
                    background: color-mix(in srgb, var(--accent), transparent 82%);
                    border-color: color-mix(in srgb, var(--accent), transparent 60%);
                }
                .notes-scroll {
                    max-height: calc(100vh - 120px);
                    overflow: auto;
                    padding-right: 0.12rem;
                }
                .note-row {
                    display: grid;
                    grid-template-columns: 1fr auto;
                    gap: 0.28rem;
                    margin-bottom: 0.22rem;
                }
                .note-main {
                    border: 1px solid transparent;
                    background: transparent;
                    color: var(--text-muted);
                    border-radius: 6px;
                    padding: 0.3rem 0.42rem;
                    cursor: pointer;
                    text-align: left;
                }
                .note-main:hover {
                    background: var(--bg-tab);
                    color: var(--text);
                }
                .note-main.active {
                    background: color-mix(in srgb, var(--accent), transparent 86%);
                    color: var(--text);
                    border-color: color-mix(in srgb, var(--accent), transparent 65%);
                }
                .note-label {
                    display: block;
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                    font-size: 0.84rem;
                }
                .danger-btn {
                    border: none;
                    background: transparent;
                    color: var(--text-muted);
                    border-radius: 6px;
                    width: 24px;
                    cursor: pointer;
                }
                .danger-btn:hover { color: var(--danger); background: var(--bg-tab); }
                .workspace {
                    display: grid;
                    grid-template-rows: auto auto 1fr auto;
                    min-width: 0;
                }
                .toolbar {
                    height: 34px;
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    gap: 0.45rem;
                    border-bottom: 1px solid var(--line);
                    background: var(--bg-alt);
                    padding: 0 0.55rem;
                }
                .toolbar-title {
                    margin: 0;
                    font-size: 0.78rem;
                    color: var(--text-muted);
                    font-weight: 600;
                }
                .mode-switch { display: inline-flex; gap: 0.24rem; }
                .mode-btn {
                    border: 1px solid transparent;
                    background: transparent;
                    color: var(--text-muted);
                    border-radius: 6px;
                    padding: 0.2rem 0.42rem;
                    font-size: 0.73rem;
                    cursor: pointer;
                }
                .mode-btn:hover,
                .mode-btn.active {
                    border-color: var(--line);
                    background: var(--bg-tab);
                    color: var(--text);
                }
                .tabstrip {
                    display: flex;
                    align-items: flex-end;
                    gap: 0.3rem;
                    border-bottom: 1px solid var(--line);
                    background: var(--bg-alt);
                    min-width: 0;
                    padding: 0.15rem 0.45rem 0;
                }
                .tabstrip-inner {
                    display: flex;
                    gap: 0.2rem;
                    overflow-x: auto;
                    flex: 1;
                    min-width: 0;
                    padding-bottom: 0.1rem;
                }
                .browser-tab {
                    display: flex;
                    align-items: center;
                    min-width: 145px;
                    max-width: 220px;
                    border: 1px solid transparent;
                    border-bottom: none;
                    border-radius: 6px 6px 0 0;
                    background: transparent;
                }
                .browser-tab.active {
                    background: var(--bg);
                    border-color: var(--line);
                    position: relative;
                }
                .browser-tab.active::after {
                    content: "";
                    position: absolute;
                    left: 0;
                    right: 0;
                    bottom: -1px;
                    height: 1px;
                    background: var(--bg);
                }
                .browser-tab-main {
                    border: none;
                    background: transparent;
                    color: var(--text-muted);
                    text-align: left;
                    flex: 1;
                    min-width: 0;
                    padding: 0.38rem 0.52rem;
                    white-space: nowrap;
                    overflow: hidden;
                    text-overflow: ellipsis;
                    font-size: 0.81rem;
                    cursor: pointer;
                }
                .browser-tab.active .browser-tab-main { color: var(--text); }
                .browser-tab-close {
                    border: none;
                    background: transparent;
                    color: var(--text-muted);
                    border-radius: 5px;
                    width: 20px;
                    height: 20px;
                    cursor: pointer;
                    margin-right: 0.22rem;
                }
                .browser-tab-close:hover { background: var(--bg-tab); color: var(--text); }
                .new-tab-btn {
                    border: 1px solid transparent;
                    background: transparent;
                    color: var(--text-muted);
                    border-radius: 6px;
                    height: 24px;
                    width: 24px;
                    margin-bottom: 0.16rem;
                    cursor: pointer;
                }
                .new-tab-btn:hover { border-color: var(--line); background: var(--bg-tab); color: var(--text); }
                .editor-wrap {
                    width: min(860px, calc(100% - 3rem));
                    margin: 1.15rem auto 0.75rem;
                    min-width: 0;
                }
                .title {
                    width: 100%;
                    border: none;
                    border-radius: 0;
                    background: transparent;
                    color: var(--text);
                    padding: 0 0 0.55rem 0;
                    font-size: 1.85rem;
                    font-weight: 700;
                }
                .title::placeholder { color: var(--text-muted); }
                .title:focus { outline: none; }
                .panel {
                    border: none;
                    border-radius: 0;
                    min-height: calc(100vh - 170px);
                    background: transparent;
                    overflow: visible;
                }
                .editor-grid {
                    display: grid;
                    grid-template-columns: 48px 1fr;
                    gap: 0.65rem;
                    align-items: start;
                }
                .line-numbers {
                    margin: 0;
                    user-select: none;
                    color: var(--text-muted);
                    text-align: right;
                    font: 15px/1.7 ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
                    min-height: calc(100vh - 240px);
                    display: flex;
                    flex-direction: column;
                }
                .line-number {
                    line-height: 1.7;
                    white-space: nowrap;
                    min-height: 1.7em;
                }
                .editor {
                    width: 100%;
                    min-height: calc(100vh - 240px);
                    height: calc(100vh - 240px);
                    border: none;
                    background: transparent;
                    color: var(--text);
                    padding: 0;
                    font: 15px/1.7 ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
                    resize: none;
                    overflow-x: hidden;
                    overflow-y: auto;
                }
                .editor:focus { outline: none; }
                .preview {
                    padding: 0;
                    line-height: 1.7;
                    color: var(--text);
                    white-space: pre-wrap;
                    overflow-wrap: anywhere;
                    word-break: break-word;
                }
                .preview pre {
                    background: #101827;
                    border: 1px solid var(--line-soft);
                    border-radius: 8px;
                    padding: 0.78rem;
                    overflow-x: auto;
                    white-space: pre;
                    word-break: normal;
                    overflow-wrap: normal;
                }
                .preview code {
                    background: var(--bg-tab);
                    border-radius: 5px;
                    padding: 0.08rem 0.3rem;
                }
                .error {
                    color: var(--danger);
                    background: color-mix(in srgb, var(--danger), transparent 88%);
                    border: 1px solid color-mix(in srgb, var(--danger), transparent 72%);
                    border-radius: 8px;
                    font-size: 0.82rem;
                    width: min(860px, calc(100% - 3rem));
                    margin: 0 auto 1rem;
                    padding: 0.5rem 0.62rem;
                }
                .context-menu {
                    position: fixed;
                    z-index: 220;
                    min-width: 172px;
                    border: 1px solid var(--line);
                    border-radius: 8px;
                    background: var(--bg-panel);
                    padding: 0.28rem;
                }
                .context-menu button {
                    width: 100%;
                    border: none;
                    text-align: left;
                    background: transparent;
                    color: var(--text);
                    border-radius: 6px;
                    padding: 0.4rem 0.5rem;
                    font-size: 0.82rem;
                    cursor: pointer;
                }
                .context-menu button:hover { background: var(--bg-tab); }
                .context-menu .danger-item { color: var(--danger); }
                .icon-btn {
                    display: inline-flex;
                    align-items: center;
                    justify-content: center;
                    width: 24px;
                    height: 24px;
                    border: 1px solid transparent;
                    border-radius: 6px;
                    color: var(--text-muted);
                    background: transparent;
                    cursor: pointer;
                }
                .icon-btn:hover {
                    color: var(--text);
                    background: var(--bg-tab);
                    border-color: var(--line);
                }
            "#}</style>

            <aside class="sidebar">
                    <div class="sidebar-header">
                        <h2 class="sidebar-title">"Notes"</h2>
                        <span class="chip">{move || format!("{}", notes.get().len())}</span>
                    </div>

                    <button
                        class="primary-btn"
                        on:click=move |_| {
                            let n = Note::new("Untitled", "");
                            let id = n.id.clone();
                            set_notes.update(|all| all.push(n.clone()));
                            open_note(id.clone());

                            spawn_local({
                                let set_db_error = set_db_error.clone();
                                async move {
                                    if let Err(e) = upsert_note(&n).await {
                                        set_db_error.set(Some(format!("{e:?}")));
                                    }
                                }
                            });
                        }
                    >
                        "+ New note"
                    </button>

                    <div class="notes-scroll">
                        <For
                            each=move || sorted_notes.get()
                            key=|n| n.id.clone()
                            children=move |n: Note| {
                                let id = n.id.clone();
                                let id_for_label = id.clone();
                                let id_for_open = id.clone();
                                let id_for_open_active = id.clone();
                                let id_for_delete = id.clone();
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
                                            move |_| open_note(id_for_open.clone())
                                        }>
                                            <span class="note-label">{note_label}</span>
                                        </button>
                                        <button class="danger-btn icon-btn" on:click={
                                            let id_for_delete = id_for_delete.clone();
                                            move |_| delete_note_by_id(id_for_delete.clone())
                                        }>
                                            <Icon icon=BsXLg />
                                        </button>
                                    </div>
                                }
                            }
                        />
                    </div>
            </aside>

            <section class="workspace">
                <div class="toolbar">
                    <h3 class="toolbar-title">
                        {move || active_note.get().map(|n| n.title).unwrap_or_else(|| "Untitled".to_string())}
                    </h3>
                    <div class="mode-switch">
                        <button
                            class=move || if mode.get() == EditorMode::Raw { "mode-btn active" } else { "mode-btn" }
                            on:click=move |_| set_mode.set(EditorMode::Raw)
                        >
                            "Raw"
                        </button>
                        <button
                            class=move || if mode.get() == EditorMode::Preview { "mode-btn active" } else { "mode-btn" }
                            on:click=move |_| set_mode.set(EditorMode::Preview)
                        >
                            "Preview"
                        </button>
                    </div>
                    <button class="icon-btn" title="Toggle theme" on:click=move |_| set_is_dark.update(|v| *v = !*v)>
                        {move || {
                            if is_dark.get() {
                                view! { <Icon icon=BsSunFill /> }.into_any()
                            } else {
                                view! { <Icon icon=BsMoonStarsFill /> }.into_any()
                            }
                        }}
                    </button>
                </div>

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
                                notes.get()
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
                                        move |_| open_note(id_for_click.clone())
                                }>
                                        {label}
                                    </button>
                                    <button
                                        class="browser-tab-close"
                                        on:click={
                                            let id_for_close = id_for_close.clone();
                                            move |ev: MouseEvent| {
                                                ev.stop_propagation();
                                                close_tab_by_id(id_for_close.clone());
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
                    <button
                        class="new-tab-btn icon-btn"
                        on:click=move |_| {
                            let n = Note::new("Untitled", "");
                            let id = n.id.clone();
                            set_notes.update(|all| all.push(n.clone()));
                            open_note(id.clone());
                            spawn_local({
                                let set_db_error = set_db_error.clone();
                                async move {
                                    if let Err(e) = upsert_note(&n).await {
                                        set_db_error.set(Some(format!("{e:?}")));
                                    }
                                }
                            });
                        }
                    >
                        <Icon icon=BsPlusLg />
                    </button>
                </div>

                <div class="editor-wrap">
                    <Show
                        when=move || active_note_id.get().is_some()
                        fallback=move || view! { <p>"No active note selected."</p> }
                    >
                        <input
                            class="title"
                            prop:value=move || active_note.get().map(|n| n.title).unwrap_or_default()
                            on:input=move |ev| {
                                if let Some(note_id) = active_note_id.get_untracked() {
                                    let value = event_target_value(&ev);
                                    let now = Date::now();
                                    set_notes.update(|all| {
                                        if let Some(note) = all.iter_mut().find(|x| x.id == note_id) {
                                            note.title = value.clone();
                                            note.updated_at = now;
                                        }
                                    });
                                }
                            }
                            on:blur=move |_| {
                                if let Some(note_id) = active_note_id.get_untracked() {
                                    save_note(note_id);
                                }
                            }
                        />

                        <section class="panel">
                            <Show
                                when=move || mode.get() == EditorMode::Raw
                                fallback=move || view! {
                                    <article class="preview" inner_html=move || preview_html.get()></article>
                                }
                            >
                                <div class="editor-grid">
                                    <div class="line-numbers">
                                        <For
                                            each=move || {
                                                let content = active_note
                                                    .get()
                                                    .map(|n| n.content)
                                                    .unwrap_or_default();
                                                wrapped_line_number_rows(&content, WRAP_COLS)
                                                    .into_iter()
                                                    .enumerate()
                                                    .collect::<Vec<_>>()
                                            }
                                            key=|(idx, _)| *idx
                                            children=move |(_, line): (usize, Option<usize>)| view! {
                                                <span class="line-number">
                                                    {line.map(|n| n.to_string()).unwrap_or_default()}
                                                </span>
                                            }
                                        />
                                    </div>
                                    <textarea
                                        class="editor"
                                        wrap="soft"
                                        cols=WRAP_COLS
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
                                            }
                                        }
                                        on:blur=move |_| {
                                            if let Some(note_id) = active_note_id.get_untracked() {
                                                save_note(note_id);
                                            }
                                        }
                                        spellcheck="false"
                                    />
                                </div>
                            </Show>
                        </section>
                    </Show>
                </div>

                <Show when=move || db_error.get().is_some()>
                    <p class="error">{move || db_error.get().unwrap_or_default()}</p>
                </Show>
            </section>

            <Show when=move || context_menu.get().is_some()>
                {move || {
                    if let Some((note_id, x, y)) = context_menu.get() {
                        let id_for_open = note_id.clone();
                        let id_for_duplicate = note_id.clone();
                        let id_for_delete = note_id.clone();
                        view! {
                            <div class="context-menu" style=move || format!("left: {}px; top: {}px;", x, y)>
                                <button on:click={
                                    let id_for_open = id_for_open.clone();
                                    move |_| {
                                        open_note(id_for_open.clone());
                                        set_context_menu.set(None);
                                    }
                                }>
                                    "Open Note"
                                </button>
                                <button on:click={
                                    let id_for_duplicate = id_for_duplicate.clone();
                                    move |_| {
                                        duplicate_note(id_for_duplicate.clone());
                                        set_context_menu.set(None);
                                    }
                                }>
                                    "Duplicate Note"
                                </button>
                                <button class="danger-item" on:click={
                                    let id_for_delete = id_for_delete.clone();
                                    move |_| {
                                        delete_note_by_id(id_for_delete.clone());
                                        set_context_menu.set(None);
                                    }
                                }>
                                    "Delete Note"
                                </button>
                            </div>
                        }
                            .into_any()
                    } else {
                        view! { <></> }.into_any()
                    }
                }}
            </Show>
        </main>
    }
}