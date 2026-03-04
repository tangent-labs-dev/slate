use crate::links::normalize_title;
use crate::markdown::{
    collect_slate_media_ids, render_markdown, resolve_slate_media_urls, rewrite_video_image_tags,
};
use crate::models::{EditorMode, MediaAsset, Note, derive_title};
use crate::note_graph::{
    backlink_ids_for, build_title_index, closest_wiki_anchor, propagate_renamed_title,
};
use crate::store::{
    delete_media_assets_by_ids, load_all_media_assets, load_all_notes, upsert_media_asset,
    upsert_note,
};
use icondata::{BsChevronLeft, BsChevronRight, BsPlusLg, BsXLg};
use js_sys::{Date, Uint8Array};
use leptos::web_sys::{Element, HtmlInputElement};
use leptos::{ev::MouseEvent, prelude::*};
use leptos_icons::Icon;
use wasm_bindgen::JsCast;
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

export function auto_resize_editors() {
  requestAnimationFrame(() => {
    document.querySelectorAll('.editor').forEach((el) => {
      if (!(el instanceof HTMLTextAreaElement)) return;
      el.style.height = 'auto';
      el.style.overflowY = 'hidden';
      el.style.height = `${el.scrollHeight}px`;
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

export function ui_prompt(message, defaultValue = '') {
  const value = globalThis.prompt(message, defaultValue);
  return value ?? '';
}

export function click_by_id(id) {
  const node = document.getElementById(id);
  if (node instanceof HTMLInputElement) {
    node.click();
  }
}

export function media_create_object_url(bytes, mimeType) {
  const typed = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  const blob = new Blob([typed], { type: mimeType || 'application/octet-stream' });
  return URL.createObjectURL(blob);
}

export function media_revoke_object_url(url) {
  if (url) {
    URL.revokeObjectURL(url);
  }
}
"#)]
extern "C" {
    fn highlight_markdown_code();
    fn auto_resize_editors();
    fn init_sidebar_resizer();
    fn ui_pref_get(key: &str) -> String;
    fn ui_pref_set(key: &str, value: &str);
    fn ui_prompt(message: &str, default_value: &str) -> String;
    fn click_by_id(id: &str);
    fn media_create_object_url(bytes: &[u8], mime_type: &str) -> String;
    fn media_revoke_object_url(url: &str);
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

const IMAGE_UPLOAD_INPUT_ID: &str = "media-upload-image";
const VIDEO_UPLOAD_INPUT_ID: &str = "media-upload-video";
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_VIDEO_BYTES: usize = 100 * 1024 * 1024;

fn is_safe_remote_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

fn escape_html_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn normalize_video_url(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if !is_safe_remote_url(trimmed) {
        return None;
    }
    Some(trimmed.to_string())
}

fn youtube_embed_src(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    if lower.contains("youtube.com/watch?v=") {
        let marker = "watch?v=";
        let start = lower.find(marker)? + marker.len();
        let id = &url[start..];
        let id = id.split('&').next()?.trim();
        if id.is_empty() {
            return None;
        }
        return Some(format!("https://www.youtube.com/embed/{id}"));
    }
    if lower.contains("youtu.be/") {
        let marker = "youtu.be/";
        let start = lower.find(marker)? + marker.len();
        let id = &url[start..];
        let id = id.split('?').next()?.trim();
        if id.is_empty() {
            return None;
        }
        return Some(format!("https://www.youtube.com/embed/{id}"));
    }
    if lower.contains("youtube.com/embed/") {
        return Some(url.to_string());
    }
    None
}

fn vimeo_embed_src(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    if !lower.contains("vimeo.com/") {
        return None;
    }

    if let Some(pos) = lower.find("player.vimeo.com/video/") {
        let start = pos + "player.vimeo.com/video/".len();
        let id = &url[start..];
        let id = id.split('?').next()?.trim();
        if id.chars().all(|ch| ch.is_ascii_digit()) {
            return Some(format!("https://player.vimeo.com/video/{id}"));
        }
    }

    let marker = "vimeo.com/";
    let start = lower.find(marker)? + marker.len();
    let id = &url[start..];
    let id = id.split('?').next()?.trim();
    if id.chars().all(|ch| ch.is_ascii_digit()) {
        return Some(format!("https://player.vimeo.com/video/{id}"));
    }
    None
}

fn video_embed_markdown(url: &str) -> Option<String> {
    let normalized = normalize_video_url(url)?;
    if let Some(embed) = youtube_embed_src(&normalized).or_else(|| vimeo_embed_src(&normalized)) {
        let safe_src = escape_html_attr(&embed);
        return Some(format!(
            r#"<iframe class="video-embed" src="{safe_src}" title="Embedded video" loading="lazy" referrerpolicy="strict-origin-when-cross-origin" allowfullscreen></iframe>"#
        ));
    }

    let safe_src = escape_html_attr(&normalized);
    Some(format!(r#"<video controls src="{safe_src}"></video>"#))
}

fn image_markdown(url: &str) -> Option<String> {
    let normalized = url.trim();
    if !is_safe_remote_url(normalized) {
        return None;
    }
    if !looks_like_image_url(normalized) {
        return None;
    }
    Some(format!("![Image]({normalized})"))
}

fn looks_like_image_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".bmp", ".avif",
    ]
    .iter()
    .any(|ext| lower.contains(ext))
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
    let (title_before_edit, set_title_before_edit) = signal::<Option<String>>(None);
    let (media_assets, set_media_assets) = signal::<Vec<MediaAsset>>(vec![]);
    let (media_url_index, set_media_url_index) =
        signal::<std::collections::HashMap<String, String>>(std::collections::HashMap::new());

    let sorted_notes = Memo::new(move |_| {
        let mut n = notes.get();
        n.sort_by(|a, b| b.created_at.total_cmp(&a.created_at));
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

    let title_index = Memo::new(move |_| build_title_index(&notes.get()));

    let backlinks = Memo::new(move |_| {
        if let Some(active) = active_note.get() {
            let all_notes = notes.get();
            backlink_ids_for(&all_notes, &active)
        } else {
            Vec::new()
        }
    });

    let preview_html = Memo::new(move |_| {
        if matches!(mode.get(), EditorMode::Preview | EditorMode::Split) {
            let title_map = title_index.get();
            let urls = media_url_index.get();
            active_note
                .get()
                .map(|n| {
                    let rendered = render_markdown(&n.content, &title_map);
                    let resolved = resolve_slate_media_urls(&rendered, &urls);
                    rewrite_video_image_tags(&resolved)
                })
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
        let _ = mode.get();
        let _ = active_note_id.get();
        auto_resize_editors();
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
            let set_media_assets = set_media_assets.clone();
            let set_media_url_index = set_media_url_index.clone();

            async move {
                match load_all_notes().await {
                    Ok(mut loaded) => {
                        if loaded.is_empty() {
                            let starter = Note::new(
                                "Welcome",
                                r#"# Welcome to Slate

Slate is a local-first note app with markdown editing and Obsidian-style wiki links.

## Notes features showcase

### Headings and emphasis

Use headings, **bold**, *italic*, and ~~strikethrough~~.

### Lists

- Bullet item
- Another item
  - Nested bullet

1. Numbered item
2. Another numbered item

### Tasks

- [x] Build wiki links
- [ ] Add graph view later

### Quote

> Notes are only useful if you can find and connect them later.

### Code block

```rust
fn hello(name: &str) -> String {
    format!("Hello, {name}!")
}
```

### Table

| Feature | Supported |
| --- | --- |
| Markdown preview | Yes |
| Wiki links | Yes |
| Backlinks | Yes |

### Horizontal rule

---

### Wiki links

- Basic link: [[Project Ideas]]
- Alias link: [[Project Ideas|Ideas]]
- Link with heading: [[Project Ideas#Next Steps]]

Click wiki links in Preview to open the target note (or create it if missing).

### Media (images + videos)

Use the toolbar buttons (`Image URL`, `Upload Image`, `Video URL`, `Upload Video`) while editing.

#### Image example (direct image file URL)

![Slate image example](https://upload.wikimedia.org/wikipedia/commons/thumb/a/a7/React-icon.svg/512px-React-icon.svg.png)

#### Video example (YouTube URL auto-embeds)

![YouTube video](https://www.youtube.com/watch?v=ysz5S6PUM-U)

#### Local upload syntax (inserted automatically after upload)

```md
![My uploaded image](slate-media://asset-id)
<video controls src="slate-media://asset-id"></video>
```

## App features

- Raw, Preview, and Split editor modes
- Multi-tab notes
- Search by title/content
- Backlinks in "Linked mentions"
- Auto-update wiki links when a note is renamed
- Duplicate/delete notes from context menu
- Resizable/collapsible sidebar
- Theme switcher

Happy writing and linking.
"#,
                            );
                            if let Err(e) = upsert_note(&starter).await {
                                set_db_error.set(Some(format!("{e:?}")));
                            }
                            loaded.push(starter);
                        }

                        loaded.sort_by(|a, b| b.created_at.total_cmp(&a.created_at));
                        let first_id = loaded[0].id.clone();
                        set_notes.set(loaded);
                        set_active_note_id.set(Some(first_id.clone()));
                        set_open_tabs.set(vec![first_id]);
                    }
                    Err(e) => set_db_error.set(Some(format!("{e:?}"))),
                }

                match load_all_media_assets().await {
                    Ok(loaded_assets) => {
                        let mut urls = std::collections::HashMap::new();
                        for asset in &loaded_assets {
                            let object_url = media_create_object_url(&asset.data, &asset.mime_type);
                            urls.insert(asset.id.clone(), object_url);
                        }
                        set_media_assets.set(loaded_assets);
                        set_media_url_index.set(urls);
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

    let open_or_create_note = move |title: String| {
        let wanted = title.trim().to_string();
        if wanted.is_empty() {
            return;
        }
        let wanted_norm = normalize_title(&wanted);
        if let Some(existing) = notes
            .get_untracked()
            .into_iter()
            .find(|n| normalize_title(&n.title) == wanted_norm)
        {
            open_note(existing.id);
            return;
        }

        let new_note = Note::new(wanted, "");
        let new_note_id = new_note.id.clone();
        set_notes.update(|all| all.push(new_note.clone()));
        open_note(new_note_id);
        spawn_local({
            let set_db_error = set_db_error.clone();
            async move {
                if let Err(e) = upsert_note(&new_note).await {
                    set_db_error.set(Some(format!("{e:?}")));
                }
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

    let cleanup_orphaned_media = move || {
        let all_notes = notes.get_untracked();
        let referenced_ids = all_notes
            .iter()
            .flat_map(|note| collect_slate_media_ids(&note.content))
            .collect::<std::collections::HashSet<_>>();

        let orphan_ids = media_assets
            .get_untracked()
            .into_iter()
            .filter(|asset| !referenced_ids.contains(&asset.id))
            .map(|asset| asset.id)
            .collect::<Vec<_>>();

        if orphan_ids.is_empty() {
            return;
        }

        set_media_assets.update(|all| all.retain(|asset| !orphan_ids.contains(&asset.id)));

        let mut url_index = media_url_index.get_untracked();
        for orphan_id in &orphan_ids {
            if let Some(url) = url_index.remove(orphan_id) {
                media_revoke_object_url(&url);
            }
        }
        set_media_url_index.set(url_index);

        spawn_local({
            let set_db_error = set_db_error.clone();
            async move {
                if let Err(e) = delete_media_assets_by_ids(&orphan_ids).await {
                    set_db_error.set(Some(format!("{e:?}")));
                }
            }
        });
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

        cleanup_orphaned_media();
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

    let append_media_snippet = move |snippet: String| {
        if let Some(note_id) = active_note_id.get_untracked() {
            let now = Date::now();
            set_notes.update(|all| {
                if let Some(note) = all.iter_mut().find(|note| note.id == note_id) {
                    if !note.content.ends_with('\n') {
                        note.content.push('\n');
                    }
                    note.content.push_str(&snippet);
                    note.content.push('\n');
                    note.updated_at = now;
                }
            });
            auto_resize_editors();
            save_note(note_id);
        }
    };

    let insert_image_by_url = move || {
        let input = ui_prompt("Image URL", "https://");
        if input.trim().is_empty() {
            return;
        }
        match image_markdown(&input) {
            Some(snippet) => append_media_snippet(snippet),
            None => set_db_error.set(Some(
                "Use a direct image URL ending in .png/.jpg/.jpeg/.gif/.webp/.svg/.bmp/.avif"
                    .to_string(),
            )),
        }
    };

    let insert_video_by_url = move || {
        let input = ui_prompt(
            "Video URL (YouTube/Vimeo links can embed as iframe)",
            "https://",
        );
        if input.trim().is_empty() {
            return;
        }
        match video_embed_markdown(&input) {
            Some(snippet) => append_media_snippet(snippet),
            None => set_db_error.set(Some(
                "Invalid video URL. Use a valid http/https URL.".to_string(),
            )),
        }
    };

    let on_image_upload = move |ev| {
        let input = event_target::<HtmlInputElement>(&ev);
        let file = input.files().and_then(|files| files.get(0));
        input.set_value("");
        let Some(file) = file else { return };
        let Some(note_id) = active_note_id.get_untracked() else {
            set_db_error.set(Some("Open a note before uploading media.".to_string()));
            return;
        };

        let mime = file.type_();
        if !mime.starts_with("image/") {
            set_db_error.set(Some("Only image files are allowed here.".to_string()));
            return;
        }
        if file.size() > MAX_IMAGE_BYTES as f64 {
            set_db_error.set(Some("Image is too large (max 10MB).".to_string()));
            return;
        }

        let filename = file.name();
        spawn_local({
            let set_db_error = set_db_error.clone();
            let set_media_assets = set_media_assets.clone();
            let set_media_url_index = set_media_url_index.clone();
            async move {
                let bytes = match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                    Ok(buffer) => {
                        let array = Uint8Array::new(&buffer);
                        let mut data = vec![0; array.length() as usize];
                        array.copy_to(&mut data);
                        data
                    }
                    Err(e) => {
                        set_db_error.set(Some(format!("Failed to read image: {e:?}")));
                        return;
                    }
                };

                let asset = MediaAsset::new(note_id, filename.clone(), mime.clone(), bytes);
                if let Err(e) = upsert_media_asset(&asset).await {
                    set_db_error.set(Some(format!("{e:?}")));
                    return;
                }

                let object_url = media_create_object_url(&asset.data, &asset.mime_type);
                set_media_assets.update(|all| all.push(asset.clone()));
                set_media_url_index.update(|index| {
                    index.insert(asset.id.clone(), object_url);
                });
                append_media_snippet(format!("![{}](slate-media://{})", filename, asset.id));
            }
        });
    };

    let on_video_upload = move |ev| {
        let input = event_target::<HtmlInputElement>(&ev);
        let file = input.files().and_then(|files| files.get(0));
        input.set_value("");
        let Some(file) = file else { return };
        let Some(note_id) = active_note_id.get_untracked() else {
            set_db_error.set(Some("Open a note before uploading media.".to_string()));
            return;
        };

        let mime = file.type_();
        if !mime.starts_with("video/") {
            set_db_error.set(Some("Only video files are allowed here.".to_string()));
            return;
        }
        if file.size() > MAX_VIDEO_BYTES as f64 {
            set_db_error.set(Some("Video is too large (max 100MB).".to_string()));
            return;
        }

        let filename = file.name();
        spawn_local({
            let set_db_error = set_db_error.clone();
            let set_media_assets = set_media_assets.clone();
            let set_media_url_index = set_media_url_index.clone();
            async move {
                let bytes = match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                    Ok(buffer) => {
                        let array = Uint8Array::new(&buffer);
                        let mut data = vec![0; array.length() as usize];
                        array.copy_to(&mut data);
                        data
                    }
                    Err(e) => {
                        set_db_error.set(Some(format!("Failed to read video: {e:?}")));
                        return;
                    }
                };

                let asset = MediaAsset::new(note_id, filename.clone(), mime.clone(), bytes);
                if let Err(e) = upsert_media_asset(&asset).await {
                    set_db_error.set(Some(format!("{e:?}")));
                    return;
                }

                let object_url = media_create_object_url(&asset.data, &asset.mime_type);
                set_media_assets.update(|all| all.push(asset.clone()));
                set_media_url_index.update(|index| {
                    index.insert(asset.id.clone(), object_url);
                });
                append_media_snippet(format!(
                    r#"<video controls src="slate-media://{}"></video>"#,
                    asset.id
                ));
            }
        });
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
                .media-actions {
                    display: inline-flex;
                    align-items: center;
                    gap: 0.24rem;
                }
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
                .media-hidden-input { display: none; }
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
                .backlinks-wrap {
                    margin: 0 0 0.9rem;
                    border: 1px solid var(--line-soft);
                    border-radius: 8px;
                    padding: 0.55rem 0.65rem;
                    background: color-mix(in srgb, var(--bg-alt), transparent 22%);
                }
                .backlinks-title {
                    margin: 0 0 0.45rem;
                    font-size: 0.72rem;
                    text-transform: uppercase;
                    letter-spacing: 0.04em;
                    color: var(--text-muted);
                    font-weight: 600;
                }
                .backlinks-list {
                    display: flex;
                    flex-wrap: wrap;
                    gap: 0.35rem;
                }
                .backlink-item {
                    border: 1px solid var(--line);
                    border-radius: 999px;
                    background: transparent;
                    color: var(--preview-link);
                    padding: 0.2rem 0.55rem;
                    font-size: 0.78rem;
                    cursor: pointer;
                }
                .backlink-item:hover {
                    border-color: color-mix(in srgb, var(--preview-link), transparent 45%);
                    background: color-mix(in srgb, var(--preview-link), transparent 88%);
                }
                .backlinks-empty {
                    margin: 0;
                    color: var(--text-muted);
                    font-size: 0.78rem;
                }
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
                    height: auto;
                    border: none;
                    background: transparent;
                    color: var(--text);
                    padding: 0;
                    font: 15px/1.7 ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
                    resize: none;
                    overflow-x: hidden;
                    overflow-y: hidden;
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
                .preview a.wiki-link.missing {
                    opacity: 0.8;
                    text-decoration-style: dashed;
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
                .preview video,
                .preview iframe {
                    display: block;
                    width: min(100%, 960px);
                    margin: 0.75rem auto;
                    aspect-ratio: 16 / 9;
                    height: auto;
                    border: 1px solid var(--line-soft);
                    border-radius: 8px;
                    background: #000;
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
                    <div class="media-actions">
                        <button class="mode-btn" on:click=move |_| insert_image_by_url()>
                            "Image URL"
                        </button>
                        <button class="mode-btn" on:click=move |_| click_by_id(IMAGE_UPLOAD_INPUT_ID)>
                            "Upload Image"
                        </button>
                        <button class="mode-btn" on:click=move |_| insert_video_by_url()>
                            "Video URL"
                        </button>
                        <button class="mode-btn" on:click=move |_| click_by_id(VIDEO_UPLOAD_INPUT_ID)>
                            "Upload Video"
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
                <input
                    id=IMAGE_UPLOAD_INPUT_ID
                    class="media-hidden-input"
                    type="file"
                    accept="image/*"
                    on:change=on_image_upload
                />
                <input
                    id=VIDEO_UPLOAD_INPUT_ID
                    class="media-hidden-input"
                    type="file"
                    accept="video/*"
                    on:change=on_video_upload
                />

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
                                    save_note(note_id);
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
                                            changed =
                                                propagate_renamed_title(all, &old, &new_title);
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
                                    save_note(note_id);
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
                                                <button class="backlink-item" on:click=move |_| open_note(id_for_open.clone())>
                                                    {label}
                                                </button>
                                            }
                                        }
                                    />
                                </div>
                            </Show>
                        </div>

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
                                                                auto_resize_editors();
                                                                save_note(note_id);
                                                            }
                                                        }
                                                        on:blur=move |_| {
                                                            if let Some(note_id) = active_note_id.get_untracked() {
                                                                save_note(note_id);
                                                            }
                                                            cleanup_orphaned_media();
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
                                                                    open_note(note_id);
                                                                } else if let Some(title) = anchor.get_attribute("data-note-title") {
                                                                    open_or_create_note(title);
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
                                                        open_note(note_id);
                                                    } else if let Some(title) = anchor.get_attribute("data-note-title") {
                                                        open_or_create_note(title);
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
                                            auto_resize_editors();
                                            save_note(note_id);
                                        }
                                    }
                                    on:blur=move |_| {
                                        if let Some(note_id) = active_note_id.get_untracked() {
                                            save_note(note_id);
                                        }
                                        cleanup_orphaned_media();
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
