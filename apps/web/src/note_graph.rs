use crate::links::{link_target_base, normalize_title, parse_wiki_links, rename_wiki_links};
use crate::models::Note;
use js_sys::Date;
use leptos::web_sys::Element;
use std::collections::{HashMap, HashSet};

pub fn build_title_index(notes: &[Note]) -> HashMap<String, String> {
    let mut by_title = HashMap::new();
    for note in notes {
        let key = normalize_title(&note.title);
        if !key.is_empty() {
            by_title.entry(key).or_insert_with(|| note.id.clone());
        }
    }
    by_title
}

pub fn backlink_ids_for(notes: &[Note], active_note: &Note) -> Vec<String> {
    let active_norm = normalize_title(&active_note.title);
    if active_norm.is_empty() {
        return Vec::new();
    }

    let mut seen = HashSet::new();
    let mut backlink_ids = Vec::new();
    for note in notes {
        if note.id == active_note.id {
            continue;
        }
        let links_to_active = parse_wiki_links(&note.content)
            .into_iter()
            .any(|link| normalize_title(link_target_base(&link.target)) == active_norm);
        if links_to_active && seen.insert(note.id.clone()) {
            backlink_ids.push(note.id.clone());
        }
    }
    backlink_ids
}

pub fn closest_wiki_anchor(mut current: Element) -> Option<Element> {
    loop {
        if current.get_attribute("data-note-id").is_some()
            || current.get_attribute("data-note-title").is_some()
        {
            return Some(current);
        }
        if let Some(parent) = current.parent_element() {
            current = parent;
        } else {
            return None;
        }
    }
}

pub fn propagate_renamed_title(notes: &mut [Note], old_title: &str, new_title: &str) -> Vec<Note> {
    let mut changed = Vec::new();
    for note in notes.iter_mut() {
        let renamed = rename_wiki_links(&note.content, old_title, new_title);
        if renamed != note.content {
            note.content = renamed;
            note.updated_at = Date::now();
            changed.push(note.clone());
        }
    }
    changed
}
