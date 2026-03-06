mod actions;
mod bindings;
mod components;
mod constants;
mod helpers;

use self::actions::create_note_and_open;
use self::bindings::{
    auto_resize_editors, click_by_id, highlight_markdown_code, init_sidebar_resizer,
    media_create_object_url, media_revoke_object_url, ui_pref_get, ui_pref_set, ui_prompt,
};
use self::components::{EditorPane, InkCanvasModal, Sidebar, Toolbar};
use self::constants::{
    APP_STYLES, IMAGE_UPLOAD_INPUT_ID, MAX_IMAGE_BYTES, MAX_VIDEO_BYTES, VIDEO_UPLOAD_INPUT_ID,
    WELCOME_NOTE_CONTENT,
};
use self::helpers::{
    AppTheme, image_markdown, mode_from_pref, mode_to_pref, normalized_storage_path,
    strip_ink_ref_blocks, strip_media_ref_lines, video_embed_markdown,
};
use crate::links::normalize_title;
use crate::markdown::{
    collect_slate_ink_ids, collect_slate_media_ids, render_markdown, resolve_slate_media_urls,
    rewrite_ink_blocks_to_html, rewrite_video_image_tags,
};
use crate::models::{EditorMode, InkDocument, MediaAsset, Note};
use crate::note_graph::{backlink_ids_for, build_title_index};
use crate::store::{
    delete_media_assets_by_ids, load_all_media_assets, load_all_notes, upsert_media_asset,
    upsert_note,
};
use js_sys::{Date, Uint8Array};
use leptos::prelude::*;
use leptos::web_sys::HtmlInputElement;
use serde_json::{from_slice, to_vec};
use wasm_bindgen_futures::spawn_local;

#[derive(Clone, Debug, PartialEq)]
struct InkEditorSession {
    asset_id: String,
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
    let (ink_editor_session, set_ink_editor_session) = signal::<Option<InkEditorSession>>(None);

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

    let sorted_uploads = Memo::new(move |_| {
        let mut uploads = media_assets
            .get()
            .into_iter()
            .filter(|asset| asset.mime_type != "application/vnd.slate.ink+json")
            .collect::<Vec<_>>();
        uploads.sort_by(|a, b| b.created_at.total_cmp(&a.created_at));
        uploads
    });

    let sorted_whiteboards = Memo::new(move |_| {
        let mut boards = media_assets
            .get()
            .into_iter()
            .filter(|asset| asset.mime_type == "application/vnd.slate.ink+json")
            .collect::<Vec<_>>();
        boards.sort_by(|a, b| b.created_at.total_cmp(&a.created_at));
        boards
    });

    let active_note_whiteboards = Memo::new(move |_| {
        if let Some(note) = active_note.get() {
            let ink_ids = collect_slate_ink_ids(&note.content);
            sorted_whiteboards
                .get()
                .into_iter()
                .filter(|asset| ink_ids.iter().any(|id| id == &asset.id))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    });

    let active_note_uploads = Memo::new(move |_| {
        if let Some(note) = active_note.get() {
            sorted_uploads
                .get()
                .into_iter()
                .filter(|asset| {
                    let storage_path = normalized_storage_path(&asset.storage_path, &asset.id);
                    note.content
                        .contains(&format!("slate-media://{storage_path}"))
                        || note
                            .content
                            .contains(&format!("slate-media://{}", asset.id))
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    });

    let ink_documents = Memo::new(move |_| {
        media_assets
            .get()
            .into_iter()
            .filter(|asset| asset.mime_type == "application/vnd.slate.ink+json")
            .filter_map(|asset| {
                from_slice::<InkDocument>(&asset.data)
                    .ok()
                    .map(|doc| (asset.id, doc))
            })
            .collect::<std::collections::HashMap<_, _>>()
    });

    let ink_thumbnail_index = Memo::new(move |_| {
        ink_documents
            .get()
            .into_iter()
            .filter_map(|(id, doc)| {
                doc.thumbnail_data_url.map(|preview| (id, preview))
            })
            .collect::<std::collections::HashMap<_, _>>()
    });
    let ink_name_index = Memo::new(move |_| {
        ink_documents
            .get()
            .into_iter()
            .map(|(id, doc)| (id, doc.name))
            .collect::<std::collections::HashMap<_, _>>()
    });

    let preview_html = Memo::new(move |_| {
        if matches!(mode.get(), EditorMode::Preview | EditorMode::Split) {
            let title_map = title_index.get();
            let urls = media_url_index.get();
            let thumbnails = ink_thumbnail_index.get();
            let ink_names = ink_name_index.get();
            active_note
                .get()
                .map(|n| {
                    let with_ink = rewrite_ink_blocks_to_html(&n.content, &thumbnails, &ink_names);
                    let rendered = render_markdown(&with_ink, &title_map);
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
                            let starter = Note::new("Welcome", WELCOME_NOTE_CONTENT);
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
                            let storage_key =
                                normalized_storage_path(&asset.storage_path, &asset.id);
                            urls.insert(storage_key, object_url.clone());
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
        let mut referenced_ids = all_notes
            .iter()
            .flat_map(|note| collect_slate_media_ids(&note.content))
            .collect::<std::collections::HashSet<_>>();
        for ink_id in all_notes
            .iter()
            .flat_map(|note| collect_slate_ink_ids(&note.content))
        {
            referenced_ids.insert(ink_id);
        }

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

    let delete_media_asset_by_id = move |asset_id: String| {
        let asset = media_assets
            .get_untracked()
            .into_iter()
            .find(|item| item.id == asset_id);
        let Some(asset) = asset else { return };
        let storage_path = normalized_storage_path(&asset.storage_path, &asset.id);

        set_media_assets.update(|all| all.retain(|item| item.id != asset_id));

        let mut revoked = std::collections::HashSet::<String>::new();
        set_media_url_index.update(|index| {
            if let Some(url) = index.remove(&asset.id)
                && revoked.insert(url.clone())
            {
                media_revoke_object_url(&url);
            }
            if let Some(url) = index.remove(&storage_path)
                && revoked.insert(url.clone())
            {
                media_revoke_object_url(&url);
            }
        });

        let key_with_path = format!("slate-media://{storage_path}");
        let key_legacy = format!("slate-media://{}", asset.id);

        let now = Date::now();
        let mut changed_notes = Vec::<Note>::new();
        set_notes.update(|all| {
            for note in all.iter_mut() {
                let stripped_media =
                    strip_media_ref_lines(&note.content, &key_with_path, &key_legacy);
                let next_content = strip_ink_ref_blocks(&stripped_media, &asset_id);
                if next_content != note.content {
                    note.content = next_content;
                    note.updated_at = now;
                    changed_notes.push(note.clone());
                }
            }
        });

        spawn_local({
            let set_db_error = set_db_error.clone();
            async move {
                if let Err(e) = delete_media_assets_by_ids(std::slice::from_ref(&asset_id)).await {
                    set_db_error.set(Some(format!("{e:?}")));
                    return;
                }
                for note in changed_notes {
                    if let Err(e) = upsert_note(&note).await {
                        set_db_error.set(Some(format!("{e:?}")));
                        break;
                    }
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

    let insert_ink_block = move || {
        let Some(note_id) = active_note_id.get_untracked() else {
            set_db_error.set(Some("Open a note before inserting ink.".to_string()));
            return;
        };
        let doc = InkDocument::blank(1400.0, 900.0);
        let payload = match to_vec(&doc) {
            Ok(bytes) => bytes,
            Err(e) => {
                set_db_error.set(Some(format!("Failed to serialize ink document: {e}")));
                return;
            }
        };
        let asset = MediaAsset::new(
            note_id,
            "ink.json",
            "application/vnd.slate.ink+json",
            payload,
        );
        let asset_id = asset.id.clone();
        set_media_assets.update(|all| all.push(asset.clone()));
        append_media_snippet(format!(":::ink {{\"id\":\"{}\"}} :::", asset_id));
        set_ink_editor_session.set(Some(InkEditorSession {
            asset_id: asset_id.clone(),
        }));
        spawn_local({
            let set_db_error = set_db_error.clone();
            async move {
                if let Err(e) = upsert_media_asset(&asset).await {
                    set_db_error.set(Some(format!("{e:?}")));
                }
            }
        });
    };

    let open_ink_editor = move |asset_id: String| {
        let exists = media_assets
            .get_untracked()
            .into_iter()
            .any(|asset| asset.id == asset_id);
        if !exists {
            set_db_error.set(Some("Ink asset not found.".to_string()));
            return;
        }
        set_ink_editor_session.set(Some(InkEditorSession { asset_id }));
    };

    let save_ink_document = move |asset_id: String, document: InkDocument| {
        let payload = match to_vec(&document) {
            Ok(bytes) => bytes,
            Err(e) => {
                set_db_error.set(Some(format!("Failed to save ink document: {e}")));
                return;
            }
        };

        let mut maybe_asset = None::<MediaAsset>;
        set_media_assets.update(|all| {
            if let Some(asset) = all.iter_mut().find(|asset| asset.id == asset_id) {
                asset.data = payload.clone();
                asset.size_bytes = asset.data.len() as u64;
                maybe_asset = Some(asset.clone());
            }
        });

        let Some(asset) = maybe_asset else {
            set_db_error.set(Some("Ink asset disappeared before save.".to_string()));
            return;
        };
        spawn_local({
            let set_db_error = set_db_error.clone();
            async move {
                if let Err(e) = upsert_media_asset(&asset).await {
                    set_db_error.set(Some(format!("{e:?}")));
                }
            }
        });
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
                    let storage_path = normalized_storage_path(&asset.storage_path, &asset.id);
                    index.insert(storage_path.clone(), object_url.clone());
                    index.insert(asset.id.clone(), object_url);
                });
                append_media_snippet(format!(
                    "![{}](slate-media://{})",
                    filename,
                    normalized_storage_path(&asset.storage_path, &asset.id)
                ));
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
                    let storage_path = normalized_storage_path(&asset.storage_path, &asset.id);
                    index.insert(storage_path, object_url.clone());
                    index.insert(asset.id.clone(), object_url);
                });
                append_media_snippet(format!(
                    r#"<video controls src="slate-media://{}"></video>"#,
                    normalized_storage_path(&asset.storage_path, &asset.id)
                ));
            }
        });
    };

    let on_delete_upload = Callback::new(move |id: String| {
        delete_media_asset_by_id(id);
    });
    let on_open_note = Callback::new(move |id: String| {
        open_note(id);
    });
    let on_open_or_create_note = Callback::new(move |title: String| {
        open_or_create_note(title);
    });
    let on_close_tab = Callback::new(move |id: String| {
        close_tab_by_id(id);
    });
    let on_new_note = Callback::new(move |_| {
        create_note_and_open(set_notes, set_active_note_id, set_open_tabs, set_db_error);
    });
    let on_save_note = Callback::new(move |id: String| {
        save_note(id);
    });
    let on_cleanup_orphaned_media = Callback::new(move |_| {
        cleanup_orphaned_media();
    });
    let on_insert_image_url = Callback::new(move |_| {
        insert_image_by_url();
    });
    let on_insert_video_url = Callback::new(move |_| {
        insert_video_by_url();
    });
    let on_insert_ink = Callback::new(move |_| {
        insert_ink_block();
    });
    let on_click_upload_image = Callback::new(move |_| {
        click_by_id(IMAGE_UPLOAD_INPUT_ID);
    });
    let on_click_upload_video = Callback::new(move |_| {
        click_by_id(VIDEO_UPLOAD_INPUT_ID);
    });
    let on_open_ink = Callback::new(move |id: String| {
        open_ink_editor(id);
    });

    view! {
        <main
            class="app"
            data-theme=move || theme.get().as_attr()
            data-sidebar=move || if sidebar_collapsed.get() { "collapsed" } else { "open" }
            style="--sidebar-width: 280px;"
            on:click=move |_| set_context_menu.set(None)
        >
            <style>{APP_STYLES}</style>

            <Sidebar
                sidebar_collapsed=sidebar_collapsed
                set_sidebar_collapsed=set_sidebar_collapsed
                search_query=search_query
                set_search_query=set_search_query
                filtered_notes=Signal::derive(move || filtered_notes.get())
                notes=notes
                active_note_id=active_note_id
                sorted_uploads=Signal::derive(move || sorted_uploads.get())
                sorted_whiteboards=Signal::derive(move || sorted_whiteboards.get())
                whiteboard_name_index=Signal::derive(move || ink_name_index.get())
                set_context_menu=set_context_menu
                on_open_note=on_open_note
                on_open_ink=on_open_ink
                on_new_note=on_new_note
                on_delete_upload=on_delete_upload
            />
            <div class="sidebar-resizer" aria-hidden="true"></div>

            <section class="workspace">
                <Toolbar
                    active_note=Signal::derive(move || active_note.get())
                    mode=mode
                    set_mode=set_mode
                    theme=theme
                    set_theme=set_theme
                    on_insert_image_url=on_insert_image_url
                    on_insert_video_url=on_insert_video_url
                    on_insert_ink=on_insert_ink
                    on_click_upload_image=on_click_upload_image
                    on_click_upload_video=on_click_upload_video
                />
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
                <EditorPane
                    open_tabs=Signal::derive(move || open_tabs.get())
                    notes=notes
                    set_notes=set_notes
                    active_note_id=active_note_id
                    active_note=Signal::derive(move || active_note.get())
                    mode=mode
                    preview_html=Signal::derive(move || preview_html.get())
                    title_before_edit=title_before_edit
                    set_title_before_edit=set_title_before_edit
                    backlinks=Signal::derive(move || backlinks.get())
                    active_note_uploads=Signal::derive(move || active_note_uploads.get())
                    active_note_whiteboards=Signal::derive(move || active_note_whiteboards.get())
                    whiteboard_name_index=Signal::derive(move || ink_name_index.get())
                    set_db_error=set_db_error
                    on_open_note=on_open_note
                    on_open_ink=on_open_ink
                    on_open_or_create_note=on_open_or_create_note
                    on_close_tab=on_close_tab
                    on_new_note=on_new_note
                    save_note=on_save_note
                    cleanup_orphaned_media=on_cleanup_orphaned_media
                    on_delete_upload=on_delete_upload
                />

                <Show when=move || ink_editor_session.get().is_some()>
                    {move || {
                        let Some(session) = ink_editor_session.get() else {
                            return view! { <></> }.into_any();
                        };
                        let asset_id = session.asset_id.clone();
                        let document = ink_documents
                            .get()
                            .get(&asset_id)
                            .cloned()
                            .unwrap_or_else(|| InkDocument::blank(1400.0, 900.0));
                        view! {
                            <InkCanvasModal
                                initial_document=document
                                on_cancel=Callback::new({
                                    let set_ink_editor_session = set_ink_editor_session;
                                    move |_| set_ink_editor_session.set(None)
                                })
                                on_save=Callback::new({
                                    let set_ink_editor_session = set_ink_editor_session;
                                    let asset_id = asset_id.clone();
                                    move |doc: InkDocument| {
                                        save_ink_document(asset_id.clone(), doc);
                                        set_ink_editor_session.set(None);
                                    }
                                })
                            />
                        }
                            .into_any()
                    }}
                </Show>

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
