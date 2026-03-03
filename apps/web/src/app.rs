use crate::markdown::render_markdown;
use crate::models::{EditorMode, Note, derive_title};
use crate::store::indexed_db::{load_all_notes, upsert_note};
use icondata::{BsChevronLeft, BsChevronRight, BsPlusLg, BsXLg};
use js_sys::Date;
use leptos::{ev::MouseEvent, prelude::*};
use leptos_icons::Icon;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen_futures::spawn_local;

#[wasm_bindgen(inline_js = r#"
export function highlight_markdown_code() {
  const hljs = globalThis.hljs;
  if (!hljs) {
    setTimeout(highlight_markdown_code, 120);
    return;
  }

  function getLanguageFromClass(codeEl) {
    for (const cls of codeEl.classList) {
      if (cls.startsWith('language-')) return cls.slice('language-'.length).toLowerCase();
      if (cls.startsWith('lang-')) return cls.slice('lang-'.length).toLowerCase();
    }
    return '';
  }

  function normalizeLanguage(raw) {
    const lang = (raw ?? '').toLowerCase().trim();
    if (lang === 'py') return 'python';
    if (lang === 'js') return 'javascript';
    if (lang === 'ts') return 'typescript';
    if (lang === 'sh' || lang === 'shell' || lang === 'zsh') return 'bash';
    if (lang === 'yml') return 'yaml';
    if (lang === 'md') return 'markdown';
    return lang;
  }

  requestAnimationFrame(() => {
    document.querySelectorAll('.preview pre').forEach((pre) => {
      const code = pre.querySelector('code');
      if (!code) return;

      const lang = normalizeLanguage(getLanguageFromClass(code));
      const source = code.textContent ?? '';

      if (!lang) {
        pre.removeAttribute('data-language');
        code.textContent = source;
        code.classList.remove('hljs');
        return;
      }

      pre.setAttribute('data-language', lang);
      if (!hljs.getLanguage(lang)) {
        code.textContent = source;
        code.classList.add('hljs');
        return;
      }

      const highlighted = hljs.highlight(source, { language: lang, ignoreIllegals: true });
      code.innerHTML = highlighted.value;
      code.classList.add('hljs');
    });
  });
}

export function init_sidebar_resizer() {
  const app = document.querySelector('.app');
  if (!app || app.dataset.sidebarResizeInit === '1') return;
  app.dataset.sidebarResizeInit = '1';

  const handle = app.querySelector('.sidebar-resizer');
  if (!handle) return;

  const minWidth = 220;
  const maxWidth = 520;
  const prefKey = 'slate.ui.sidebar_width';

  try {
    const stored = globalThis.localStorage?.getItem(prefKey);
    if (stored) {
      const parsed = Number.parseInt(stored, 10);
      if (!Number.isNaN(parsed)) {
        const clamped = Math.max(minWidth, Math.min(maxWidth, parsed));
        app.style.setProperty('--sidebar-width', `${clamped}px`);
      }
    }
  } catch {}

  const onPointerMove = (event) => {
    const raw = event.clientX;
    const clamped = Math.max(minWidth, Math.min(maxWidth, raw));
    app.style.setProperty('--sidebar-width', `${clamped}px`);
    try {
      globalThis.localStorage?.setItem(prefKey, String(clamped));
    } catch {}
  };

  const onPointerUp = () => {
    document.body.style.userSelect = '';
    document.removeEventListener('pointermove', onPointerMove);
    document.removeEventListener('pointerup', onPointerUp);
  };

  handle.addEventListener('pointerdown', (event) => {
    if (app.getAttribute('data-sidebar') === 'collapsed') return;
    event.preventDefault();
    document.body.style.userSelect = 'none';
    document.addEventListener('pointermove', onPointerMove);
    document.addEventListener('pointerup', onPointerUp);
  });
}

export function ui_pref_get(key) {
  try {
    return globalThis.localStorage?.getItem(`slate.ui.${key}`) ?? '';
  } catch {
    return '';
  }
}

export function ui_pref_set(key, value) {
  try {
    globalThis.localStorage?.setItem(`slate.ui.${key}`, value);
  } catch {}
}
"#)]
extern "C" {
    fn highlight_markdown_code();
    fn init_sidebar_resizer();
    fn ui_pref_get(key: &str) -> String;
    fn ui_pref_set(key: &str, value: &str);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AppTheme {
    Dark,
    Light,
    Sepia,
    Midnight,
}

impl AppTheme {
    fn as_attr(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Sepia => "sepia",
            Self::Midnight => "midnight",
        }
    }

    fn from_attr(value: &str) -> Self {
        match value {
            "light" => Self::Light,
            "sepia" => Self::Sepia,
            "midnight" => Self::Midnight,
            _ => Self::Dark,
        }
    }
}

fn mode_to_pref(mode: EditorMode) -> &'static str {
    match mode {
        EditorMode::Raw => "raw",
        EditorMode::Preview => "preview",
        EditorMode::Split => "split",
    }
}

fn mode_from_pref(value: &str) -> EditorMode {
    match value {
        "preview" => EditorMode::Preview,
        "split" => EditorMode::Split,
        _ => EditorMode::Raw,
    }
}

#[component]
pub fn App() -> impl IntoView {
    let theme_pref = ui_pref_get("theme");
    let mode_pref = ui_pref_get("mode");
    let sidebar_pref = ui_pref_get("sidebar_collapsed");

    let (notes, set_notes) = signal::<Vec<Note>>(vec![]);
    let (active_note_id, set_active_note_id) = signal::<Option<String>>(None);
    let (open_tabs, set_open_tabs) = signal::<Vec<String>>(vec![]);
    let (mode, set_mode) = signal(mode_from_pref(&mode_pref));
    let (db_error, set_db_error) = signal::<Option<String>>(None);
    let (context_menu, set_context_menu) = signal::<Option<(String, i32, i32)>>(None);
    let (theme, set_theme) = signal(AppTheme::from_attr(&theme_pref));
    let (sidebar_collapsed, set_sidebar_collapsed) = signal(sidebar_pref == "1");
    let (search_query, set_search_query) = signal(String::new());

    let sorted_notes = Memo::new(move |_| {
        let mut n = notes.get();
        n.sort_by(|a, b| b.updated_at.total_cmp(&a.updated_at));
        n
    });

    let filtered_notes = Memo::new(move |_| {
        let query = search_query.get().trim().to_lowercase();
        if query.is_empty() {
            return sorted_notes.get();
        }

        sorted_notes
            .get()
            .into_iter()
            .filter(|note| {
                note.title.to_lowercase().contains(&query)
                    || note.content.to_lowercase().contains(&query)
                    || note.id.to_lowercase().contains(&query)
            })
            .collect::<Vec<_>>()
    });

    let active_note = Memo::new(move |_| {
        let id = active_note_id.get();
        notes
            .get()
            .into_iter()
            .find(|n| Some(n.id.as_str()) == id.as_deref())
    });

    let preview_html = Memo::new(move |_| {
        if matches!(mode.get(), EditorMode::Preview | EditorMode::Split) {
            active_note
                .get()
                .map(|n| render_markdown(&n.content))
                .unwrap_or_default()
        } else {
            String::new()
        }
    });

    Effect::new(move |_| {
        if matches!(mode.get(), EditorMode::Preview | EditorMode::Split) {
            let _ = preview_html.get();
            highlight_markdown_code();
        }
    });

    Effect::new(move |_| {
        init_sidebar_resizer();
    });

    Effect::new(move |_| {
        ui_pref_set("theme", theme.get().as_attr());
    });

    Effect::new(move |_| {
        ui_pref_set("mode", mode_to_pref(mode.get()));
    });

    Effect::new(move |_| {
        ui_pref_set(
            "sidebar_collapsed",
            if sidebar_collapsed.get() { "1" } else { "0" },
        );
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
        let remaining_tabs: Vec<String> = tabs_before.into_iter().filter(|t| t != &id).collect();

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
        let now = Date::now();
        let tombstone = notes
            .get_untracked()
            .into_iter()
            .find(|x| x.id == id)
            .map(|mut note| {
                note.is_deleted = true;
                note.deleted_at = Some(now);
                note.updated_at = now;
                note
            });

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

        if let Some(tombstone_note) = tombstone {
            spawn_local({
                let set_db_error = set_db_error.clone();
                async move {
                    if let Err(e) = upsert_note(&tombstone_note).await {
                        set_db_error.set(Some(format!("{e:?}")));
                    }
                }
            });
        }
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
            data-theme=move || theme.get().as_attr()
            data-sidebar=move || if sidebar_collapsed.get() { "collapsed" } else { "open" }
            style="--sidebar-width: 280px;"
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
                    --preview-code-bg: #f8fafc;
                    --preview-quote-bg: #eef2ff;
                    --preview-quote-line: #818cf8;
                    --preview-link: #4338ca;
                    --hljs-base: #0f172a;
                    --hljs-keyword: #7c3aed;
                    --hljs-string: #047857;
                    --hljs-number: #2563eb;
                    --hljs-comment: #6b7280;
                    --hljs-title: #be123c;
                    --hljs-meta: #0f766e;
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
                    --preview-code-bg: #101827;
                    --preview-quote-bg: #1f2735;
                    --preview-quote-line: #8b5cf6;
                    --preview-link: #a78bfa;
                    --hljs-base: #dbe4f3;
                    --hljs-keyword: #c4b5fd;
                    --hljs-string: #6ee7b7;
                    --hljs-number: #93c5fd;
                    --hljs-comment: #9ca3af;
                    --hljs-title: #fda4af;
                    --hljs-meta: #5eead4;
                    color-scheme: dark;
                }
                .app[data-theme="sepia"] {
                    --bg: #f6efe4;
                    --bg-panel: #fffaf1;
                    --bg-alt: #efe5d3;
                    --bg-tab: #e8dcc8;
                    --line: #d5c6af;
                    --line-soft: #e2d4be;
                    --text: #4a3928;
                    --text-muted: #7d6851;
                    --accent: #b45309;
                    --danger: #b91c1c;
                    --preview-code-bg: #f4ebdd;
                    --preview-quote-bg: #f4e7cf;
                    --preview-quote-line: #d97706;
                    --preview-link: #9a3412;
                    --hljs-base: #4a3928;
                    --hljs-keyword: #9a3412;
                    --hljs-string: #047857;
                    --hljs-number: #1d4ed8;
                    --hljs-comment: #7d6851;
                    --hljs-title: #be123c;
                    --hljs-meta: #0f766e;
                    color-scheme: light;
                }
                .app[data-theme="midnight"] {
                    --bg: #0b1320;
                    --bg-panel: #101a2c;
                    --bg-alt: #111f36;
                    --bg-tab: #15253d;
                    --line: #273856;
                    --line-soft: #1d2b45;
                    --text: #dbe7fb;
                    --text-muted: #95a7c4;
                    --accent: #22d3ee;
                    --danger: #fda4af;
                    --preview-code-bg: #0d1a2e;
                    --preview-quote-bg: #0f253e;
                    --preview-quote-line: #22d3ee;
                    --preview-link: #67e8f9;
                    --hljs-base: #e0ecff;
                    --hljs-keyword: #7dd3fc;
                    --hljs-string: #6ee7b7;
                    --hljs-number: #f9a8d4;
                    --hljs-comment: #9fb4d6;
                    --hljs-title: #fcd34d;
                    --hljs-meta: #5eead4;
                    color-scheme: dark;
                }
                .app {
                    display: grid;
                    grid-template-columns: var(--sidebar-width, 280px) 8px minmax(0, 1fr);
                    min-height: 100vh;
                    background: var(--bg);
                    color: var(--text);
                    font-family: Inter, system-ui, -apple-system, Segoe UI, Roboto, sans-serif;
                }
                .app[data-sidebar="collapsed"] {
                    grid-template-columns: 42px 0 minmax(0, 1fr);
                }
                .sidebar {
                    border-right: 1px solid var(--line);
                    background: var(--bg-panel);
                    padding: 0.6rem 0.65rem;
                }
                .app[data-sidebar="collapsed"] .sidebar {
                    padding: 0.55rem 0.35rem;
                }
                .sidebar-header {
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    margin-bottom: 0.45rem;
                }
                .app[data-sidebar="collapsed"] .sidebar-header {
                    display: grid;
                    justify-items: center;
                }
                .app[data-sidebar="collapsed"] .sidebar-title,
                .app[data-sidebar="collapsed"] .primary-btn,
                .app[data-sidebar="collapsed"] .search-wrap,
                .app[data-sidebar="collapsed"] .notes-scroll {
                    display: none;
                }
                .collapsed-new-btn { margin: 0 auto 0.5rem; }
                .collapse-btn {
                    width: 28px;
                    height: 28px;
                    font-size: 1.05rem;
                    line-height: 1;
                    font-weight: 600;
                    background: color-mix(in srgb, var(--accent), transparent 82%);
                    border-color: color-mix(in srgb, var(--accent), transparent 60%);
                    color: var(--text);
                }
                .collapse-btn svg {
                    width: 16px;
                    height: 16px;
                }
                .sidebar-title {
                    font-size: 0.88rem;
                    text-transform: uppercase;
                    letter-spacing: 0.04em;
                    color: var(--text-muted);
                    margin: 0;
                }
                .sidebar-resizer {
                    cursor: col-resize;
                    position: relative;
                    background: transparent;
                }
                .sidebar-resizer::before {
                    content: "";
                    position: absolute;
                    left: 3px;
                    top: 0;
                    bottom: 0;
                    width: 2px;
                    background: var(--line);
                    opacity: 0.75;
                    transition: opacity 120ms ease;
                }
                .sidebar-resizer:hover::before {
                    opacity: 1;
                    background: var(--accent);
                }
                .app[data-sidebar="collapsed"] .sidebar-resizer {
                    cursor: default;
                }
                .app[data-sidebar="collapsed"] .sidebar-resizer::before {
                    opacity: 0;
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
                .new-note-btn {
                    display: inline-flex;
                    align-items: center;
                    justify-content: center;
                    gap: 0.35rem;
                    font-weight: 600;
                }
                .notes-scroll {
                    max-height: calc(100vh - 120px);
                    overflow: auto;
                    padding-right: 0.12rem;
                }
                .search-wrap {
                    margin-bottom: 0.5rem;
                }
                .search-input {
                    width: 100%;
                    border: 1px solid var(--line);
                    border-radius: 7px;
                    background: var(--bg-alt);
                    color: var(--text);
                    font-size: 0.8rem;
                    padding: 0.38rem 0.52rem;
                }
                .search-input::placeholder {
                    color: var(--text-muted);
                }
                .search-input:focus {
                    outline: none;
                    border-color: color-mix(in srgb, var(--accent), transparent 45%);
                }
                .note-row {
                    display: block;
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
                .theme-select {
                    border: 1px solid var(--line);
                    background: var(--bg-tab);
                    color: var(--text);
                    border-radius: 6px;
                    padding: 0.2rem 0.4rem;
                    font-size: 0.73rem;
                    cursor: pointer;
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
                    width: 100%;
                    margin: 0;
                    padding: 1.15rem 1.4rem 0.75rem;
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
                .live-grid {
                    display: grid;
                    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
                    gap: 1rem;
                    min-height: calc(100vh - 240px);
                }
                .live-pane {
                    min-width: 0;
                }
                .live-pane-title {
                    margin: 0 0 0.45rem;
                    font-size: 0.75rem;
                    text-transform: uppercase;
                    letter-spacing: 0.04em;
                    color: var(--text-muted);
                }
                .live-preview {
                    min-height: calc(100vh - 272px);
                }
                @media (max-width: 1100px) {
                    .live-grid {
                        grid-template-columns: 1fr;
                    }
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
                    font-size: 1rem;
                }
                .preview > *:first-child { margin-top: 0; }
                .preview h1,
                .preview h2,
                .preview h3,
                .preview h4,
                .preview h5,
                .preview h6 {
                    margin: 1.2rem 0 0.5rem;
                    line-height: 1.3;
                }
                .preview p {
                    margin: 0.62rem 0;
                    overflow-wrap: anywhere;
                }
                .preview ul,
                .preview ol {
                    margin: 0.6rem 0 0.9rem 0;
                    padding-left: 1.4rem;
                }
                .preview li { margin: 0.2rem 0; }
                .preview li > p { margin: 0.25rem 0; }
                .preview blockquote {
                    margin: 0.95rem 0;
                    padding: 0.62rem 0.9rem;
                    border-left: 4px solid var(--preview-quote-line);
                    border-radius: 0 8px 8px 0;
                    background: var(--preview-quote-bg);
                    color: color-mix(in srgb, var(--text), var(--text-muted) 34%);
                }
                .preview blockquote > :first-child { margin-top: 0; }
                .preview blockquote > :last-child { margin-bottom: 0; }
                .preview a {
                    color: var(--preview-link);
                    text-underline-offset: 3px;
                }
                .preview hr {
                    border: none;
                    border-top: 1px solid var(--line-soft);
                    margin: 1rem 0;
                }
                .preview pre {
                    margin: 0.95rem 0;
                    background: var(--preview-code-bg);
                    border: 1px solid var(--line-soft);
                    border-radius: 8px;
                    padding: 1.55rem 0.78rem 0.78rem;
                    overflow-x: auto;
                    white-space: pre;
                    word-break: normal;
                    overflow-wrap: normal;
                    position: relative;
                }
                .preview pre[data-language]::before {
                    content: attr(data-language);
                    position: absolute;
                    top: 0.35rem;
                    right: 0.62rem;
                    border: 1px solid var(--line-soft);
                    border-radius: 999px;
                    background: color-mix(in srgb, var(--bg-alt), transparent 15%);
                    color: var(--text-muted);
                    font-size: 0.66rem;
                    line-height: 1;
                    letter-spacing: 0.04em;
                    text-transform: uppercase;
                    padding: 0.16rem 0.42rem;
                }
                .preview code {
                    background: var(--bg-tab);
                    border-radius: 5px;
                    padding: 0.08rem 0.3rem;
                    font-size: 0.94em;
                    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
                }
                .preview pre code {
                    display: block;
                    padding: 0;
                    border-radius: 0;
                    background: transparent;
                }
                .preview table {
                    width: 100%;
                    margin: 0.9rem 0;
                    border-collapse: collapse;
                    border: 1px solid var(--line-soft);
                    border-radius: 8px;
                    overflow: hidden;
                }
                .preview th,
                .preview td {
                    border: 1px solid var(--line-soft);
                    padding: 0.42rem 0.55rem;
                    text-align: left;
                }
                .preview th {
                    background: var(--bg-alt);
                    font-weight: 600;
                }
                .preview img {
                    max-width: 100%;
                    border-radius: 8px;
                }
                .preview pre code.hljs,
                .preview code.hljs {
                    color: var(--hljs-base);
                    background: transparent;
                }
                .preview .hljs-keyword,
                .preview .hljs-selector-tag,
                .preview .hljs-literal,
                .preview .hljs-built_in { color: var(--hljs-keyword); }
                .preview .hljs-string,
                .preview .hljs-attr,
                .preview .hljs-regexp { color: var(--hljs-string); }
                .preview .hljs-number,
                .preview .hljs-symbol,
                .preview .hljs-bullet { color: var(--hljs-number); }
                .preview .hljs-comment,
                .preview .hljs-quote,
                .preview .hljs-doctag { color: var(--hljs-comment); }
                .preview .hljs-title,
                .preview .hljs-section,
                .preview .hljs-function .hljs-title { color: var(--hljs-title); }
                .preview .hljs-meta,
                .preview .hljs-meta .hljs-keyword { color: var(--hljs-meta); }
                .preview .hljs-emphasis { font-style: italic; }
                .preview .hljs-strong { font-weight: 700; }
                .preview .hljs-deletion { color: #ef4444; }
                .preview .hljs-addition { color: #22c55e; }
                .preview input[type="checkbox"] {
                    transform: translateY(1px);
                    margin-right: 0.4rem;
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
                .sidebar .collapse-btn {
                    background: color-mix(in srgb, var(--accent), transparent 82%);
                    border-color: color-mix(in srgb, var(--accent), transparent 60%);
                    color: var(--text);
                }
                .sidebar .collapse-btn:hover {
                    background: color-mix(in srgb, var(--accent), transparent 74%);
                    border-color: color-mix(in srgb, var(--accent), transparent 52%);
                }
                .app[data-sidebar="collapsed"] .sidebar .collapse-btn,
                .app[data-sidebar="collapsed"] .sidebar .collapsed-new-btn {
                    margin-left: auto;
                    margin-right: auto;
                }
                .sidebar .collapsed-new-btn {
                    display: none;
                    background: color-mix(in srgb, var(--accent), transparent 82%);
                    border-color: color-mix(in srgb, var(--accent), transparent 60%);
                    color: var(--text);
                }
                .app[data-sidebar="collapsed"] .sidebar .collapsed-new-btn {
                    display: inline-flex;
                }
            "#}</style>

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

                    <button
                        class="primary-btn new-note-btn"
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
                                            move |_| open_note(id_for_open.clone())
                                        }>
                                            <span class="note-label">{note_label}</span>
                                        </button>
                                    </div>
                                }
                            }
                        />
                    </div>
            </aside>
            <div class="sidebar-resizer" aria-hidden="true"></div>

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
                        <button
                            class=move || if mode.get() == EditorMode::Split { "mode-btn active" } else { "mode-btn" }
                            on:click=move |_| set_mode.set(EditorMode::Split)
                        >
                            "Split"
                        </button>
                    </div>
                    <select
                        class="theme-select"
                        title="Select theme"
                        on:change=move |ev| {
                            let value = event_target_value(&ev);
                            set_theme.set(AppTheme::from_attr(&value));
                        }
                    >
                        <option value="dark" selected=move || theme.get() == AppTheme::Dark>
                            "Dark"
                        </option>
                        <option value="light" selected=move || theme.get() == AppTheme::Light>
                            "Light"
                        </option>
                        <option value="sepia" selected=move || theme.get() == AppTheme::Sepia>
                            "Sepia"
                        </option>
                        <option value="midnight" selected=move || theme.get() == AppTheme::Midnight>
                            "Midnight"
                        </option>
                    </select>
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
                                    save_note(note_id);
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
                                                                save_note(note_id);
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
                                                <div class="live-pane">
                                                    <p class="live-pane-title">"Preview"</p>
                                                    <article class="preview live-preview" inner_html=move || preview_html.get()></article>
                                                </div>
                                            </div>
                                        }
                                    >
                                        <article class="preview" inner_html=move || preview_html.get()></article>
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
                                            save_note(note_id);
                                        }
                                    }
                                    on:blur=move |_| {
                                        if let Some(note_id) = active_note_id.get_untracked() {
                                            save_note(note_id);
                                        }
                                    }
                                    spellcheck="false"
                                />
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
