use crate::links::{link_target_base, normalize_title, parse_wiki_links};
use pulldown_cmark::{Options, Parser, html};
use std::collections::HashMap;

pub const SLATE_MEDIA_SCHEME: &str = "slate-media://";

pub fn render_markdown(input: &str, note_title_index: &HashMap<String, String>) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let with_links = rewrite_wiki_links(input, note_title_index);
    let parser = Parser::new_ext(&with_links, options);
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

pub fn resolve_slate_media_urls(html: &str, media_url_index: &HashMap<String, String>) -> String {
    if !html.contains(SLATE_MEDIA_SCHEME) || media_url_index.is_empty() {
        return html.to_string();
    }

    let mut out = String::with_capacity(html.len());
    let mut cursor = 0;

    while let Some(found) = html[cursor..].find(SLATE_MEDIA_SCHEME) {
        let scheme_start = cursor + found;
        out.push_str(&html[cursor..scheme_start]);
        let key_start = scheme_start + SLATE_MEDIA_SCHEME.len();
        let mut key_end = key_start;
        while key_end < html.len() && is_media_key_char(html.as_bytes()[key_end] as char) {
            key_end += 1;
        }

        let media_key = &html[key_start..key_end];
        if let Some(url) = media_url_index.get(media_key) {
            out.push_str(url);
        } else if let Some(asset_id) = asset_id_from_media_key(media_key) {
            if let Some(url) = media_url_index.get(&asset_id) {
                out.push_str(url);
            } else {
                out.push_str(SLATE_MEDIA_SCHEME);
                out.push_str(media_key);
            }
        } else {
            out.push_str(SLATE_MEDIA_SCHEME);
            out.push_str(media_key);
        }
        cursor = key_end;
    }

    out.push_str(&html[cursor..]);
    out
}

pub fn rewrite_video_image_tags(html: &str) -> String {
    if !html.contains("<img") {
        return html.to_string();
    }

    let mut out = String::with_capacity(html.len());
    let mut cursor = 0;

    while let Some(found) = html[cursor..].find("<img") {
        let tag_start = cursor + found;
        out.push_str(&html[cursor..tag_start]);

        let Some(relative_tag_end) = html[tag_start..].find('>') else {
            out.push_str(&html[tag_start..]);
            return out;
        };
        let tag_end = tag_start + relative_tag_end + 1;
        let tag = &html[tag_start..tag_end];
        let src = extract_attr(tag, "src");

        if let Some(src) = src {
            if let Some(embed) = video_embed_src(src) {
                out.push_str(&format!(
                    r#"<iframe class="video-embed" src="{embed}" title="Embedded video" loading="lazy" referrerpolicy="strict-origin-when-cross-origin" allowfullscreen></iframe>"#
                ));
            } else if is_direct_video_url(src) {
                out.push_str(&format!(r#"<video controls src="{src}"></video>"#));
            } else {
                out.push_str(tag);
            }
        } else {
            out.push_str(tag);
        }

        cursor = tag_end;
    }

    out.push_str(&html[cursor..]);
    out
}

pub fn collect_slate_media_ids(markdown: &str) -> Vec<String> {
    if !markdown.contains(SLATE_MEDIA_SCHEME) {
        return Vec::new();
    }

    let mut ids = Vec::new();
    let mut cursor = 0;

    while let Some(found) = markdown[cursor..].find(SLATE_MEDIA_SCHEME) {
        let key_start = cursor + found + SLATE_MEDIA_SCHEME.len();
        let mut key_end = key_start;
        while key_end < markdown.len() && is_media_key_char(markdown.as_bytes()[key_end] as char) {
            key_end += 1;
        }
        if key_end > key_start {
            let key = &markdown[key_start..key_end];
            if let Some(asset_id) = asset_id_from_media_key(key) {
                ids.push(asset_id);
            }
        }
        cursor = key_end;
    }

    ids.sort();
    ids.dedup();
    ids
}

pub fn collect_slate_ink_ids(markdown: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut in_code_fence = false;
    let mut pending_ink_id: Option<String> = None;

    for line in markdown.lines() {
        let trimmed = line.trim();

        if is_fence_toggle(trimmed) {
            in_code_fence = !in_code_fence;
            continue;
        }
        if in_code_fence {
            continue;
        }

        if pending_ink_id.is_none() && is_whiteboard_block_start(trimmed) {
            if let Some(id) = parse_ink_id(trimmed) {
                // Single-line marker is canonical.
                if !trimmed.ends_with(":::") {
                    pending_ink_id = Some(id.clone());
                }
                ids.push(id);
            }
            continue;
        }

        if pending_ink_id.is_some() && trimmed == ":::" {
            pending_ink_id = None;
        }
    }

    ids.sort();
    ids.dedup();
    ids
}

pub fn rewrite_ink_blocks_to_html(
    markdown: &str,
    thumbnail_index: &HashMap<String, String>,
    name_index: &HashMap<String, String>,
) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut in_code_fence = false;
    let mut waiting_for_legacy_close = false;

    for line in markdown.lines() {
        let trimmed = line.trim();

        if is_fence_toggle(trimmed) {
            in_code_fence = !in_code_fence;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_code_fence {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        if waiting_for_legacy_close && trimmed == ":::" {
            waiting_for_legacy_close = false;
            continue;
        }

        if is_whiteboard_block_start(trimmed) {
            if let Some(id) = parse_ink_id(trimmed) {
                let safe_id = escape_html(&id);
                let raw_name = name_index
                    .get(&id)
                    .map(std::string::String::as_str)
                    .unwrap_or("Whiteboard");
                let safe_name = escape_html(raw_name);
                if let Some(thumb) = thumbnail_index.get(&id) {
                    let safe_thumb = escape_html(thumb);
                    out.push_str(&format!(
                        r#"<div class="ink-embed" data-ink-id="{safe_id}" title="{safe_name}"><img class="ink-embed-thumb" src="{safe_thumb}" alt="{safe_name}" loading="lazy" /><div class="ink-embed-name">{safe_name}</div></div>"#
                    ));
                } else {
                    out.push_str(&format!(
                        r#"<div class="ink-embed" data-ink-id="{safe_id}" title="{safe_name}"><div class="ink-embed-empty">{safe_name}</div></div>"#
                    ));
                }
                out.push('\n');
                // Legacy multiline syntax may include a closing ::: on next line.
                if !trimmed.ends_with(":::") {
                    waiting_for_legacy_close = true;
                }
            } else {
                out.push_str(line);
                out.push('\n');
            }
            continue;
        }

        out.push_str(line);
        out.push('\n');
    }

    out
}

fn rewrite_wiki_links(input: &str, note_title_index: &HashMap<String, String>) -> String {
    let links = parse_wiki_links(input);
    if links.is_empty() {
        return input.to_string();
    }

    let mut out = String::with_capacity(input.len());
    let mut cursor = 0;

    for link in links {
        out.push_str(&input[cursor..link.start]);

        let base = normalize_title(link_target_base(&link.target));
        let display = link.alias.as_ref().unwrap_or(&link.target);
        if let Some(note_id) = note_title_index.get(&base) {
            out.push_str(&format!(
                r##"<a href="#" class="wiki-link" data-note-id="{}">{}</a>"##,
                escape_html(note_id),
                escape_html(display)
            ));
        } else {
            out.push_str(&format!(
                r##"<a href="#" class="wiki-link missing" data-note-title="{}">{}</a>"##,
                escape_html(&link.target),
                escape_html(display)
            ));
        }

        cursor = link.end;
    }

    out.push_str(&input[cursor..]);
    out
}

fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn is_media_key_char(ch: char) -> bool {
    !matches!(ch, '"' | '\'' | ')' | ' ' | '\n' | '\r' | '\t' | '<' | '>')
}

fn asset_id_from_media_key(key: &str) -> Option<String> {
    if key.is_empty() {
        return None;
    }

    if let Some(rest) = key.strip_prefix("uploads/") {
        let id = rest.split('/').next().unwrap_or_default().trim();
        if id.is_empty() {
            return None;
        }
        return Some(id.to_string());
    }

    Some(key.to_string())
}

fn extract_attr<'a>(tag: &'a str, attr_name: &str) -> Option<&'a str> {
    let marker = format!(r#"{attr_name}=""#);
    let start = tag.find(&marker)? + marker.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn parse_ink_id(line: &str) -> Option<String> {
    let marker = "\"id\":\"";
    let idx = line.find(marker)?;
    let rest = &line[idx + marker.len()..];
    let end = rest.find('"')?;
    let id = rest[..end].trim();
    if id.is_empty() {
        return None;
    }
    Some(id.to_string())
}

fn is_whiteboard_block_start(trimmed: &str) -> bool {
    trimmed.starts_with(":::whiteboard") || trimmed.starts_with(":::ink")
}

fn is_fence_toggle(trimmed: &str) -> bool {
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

fn video_embed_src(url: &str) -> Option<String> {
    if let Some(embed) = youtube_embed_src(url) {
        return Some(embed);
    }
    vimeo_embed_src(url)
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

fn is_direct_video_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    [".mp4", ".webm", ".mov", ".ogg", ".m4v"]
        .iter()
        .any(|ext| lower.contains(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_unique_media_ids_from_markdown() {
        let input = "![img](slate-media://uploads/abc)\n<video src=\"slate-media://uploads/def\"></video>\n![dup](slate-media://uploads/abc)";
        let ids = collect_slate_media_ids(input);
        assert_eq!(ids, vec!["abc".to_string(), "def".to_string()]);
    }

    #[test]
    fn resolves_media_urls_when_available() {
        let mut index = HashMap::new();
        index.insert(
            "uploads/abc".to_string(),
            "blob:https://local/1".to_string(),
        );
        let html =
            r#"<img src="slate-media://uploads/abc"><img src="slate-media://uploads/missing">"#;
        let resolved = resolve_slate_media_urls(html, &index);
        assert!(resolved.contains("blob:https://local/1"));
        assert!(resolved.contains("slate-media://uploads/missing"));
    }

    #[test]
    fn rewrites_youtube_img_to_iframe() {
        let html = r#"<p><img src="https://www.youtube.com/watch?v=abc123" alt="yt"></p>"#;
        let rewritten = rewrite_video_image_tags(html);
        assert!(rewritten.contains("<iframe"));
        assert!(rewritten.contains("youtube.com/embed/abc123"));
    }

    #[test]
    fn collects_ink_ids_from_blocks() {
        let input = r#"
:::whiteboard {"id":"ink-a"}
:::

:::ink {"id":"ink-b"}
some ignored payload
:::
"#;
        let ids = collect_slate_ink_ids(input);
        assert_eq!(ids, vec!["ink-a".to_string(), "ink-b".to_string()]);
    }

    #[test]
    fn rewrites_ink_blocks_to_placeholders() {
        let mut thumbs = HashMap::new();
        thumbs.insert("ink-a".to_string(), "data:image/png;base64,abc".to_string());
        let input = "before\n:::whiteboard {\"id\":\"ink-a\"}\n:::\nafter";
        let mut names = HashMap::new();
        names.insert("ink-a".to_string(), "Session Diagram".to_string());
        let out = rewrite_ink_blocks_to_html(input, &thumbs, &names);
        assert!(out.contains("data-ink-id=\"ink-a\""));
        assert!(out.contains("ink-embed-thumb"));
        assert!(out.contains("Session Diagram"));
        assert!(out.contains("before"));
        assert!(out.contains("after"));
    }
}
