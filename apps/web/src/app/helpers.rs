use crate::models::EditorMode;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppTheme {
    Dark,
    Light,
    Sepia,
    Midnight,
}

impl AppTheme {
    pub fn as_attr(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::Sepia => "sepia",
            Self::Midnight => "midnight",
        }
    }

    pub fn from_attr(value: &str) -> Self {
        match value {
            "light" => Self::Light,
            "sepia" => Self::Sepia,
            "midnight" => Self::Midnight,
            _ => Self::Dark,
        }
    }
}

pub fn mode_to_pref(mode: EditorMode) -> &'static str {
    match mode {
        EditorMode::Raw => "raw",
        EditorMode::Preview => "preview",
        EditorMode::Split => "split",
    }
}

pub fn mode_from_pref(value: &str) -> EditorMode {
    match value {
        "preview" => EditorMode::Preview,
        "split" => EditorMode::Split,
        _ => EditorMode::Raw,
    }
}

pub fn video_embed_markdown(url: &str) -> Option<String> {
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

pub fn image_markdown(url: &str) -> Option<String> {
    let normalized = url.trim();
    if !is_safe_remote_url(normalized) || !looks_like_image_url(normalized) {
        return None;
    }
    Some(format!("![Image]({normalized})"))
}

pub fn format_bytes(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if size >= MB {
        format!("{:.1} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.1} KB", size as f64 / KB as f64)
    } else {
        format!("{size} B")
    }
}

pub fn normalized_storage_path(storage_path: &str, asset_id: &str) -> String {
    if storage_path.trim().is_empty() {
        format!("uploads/{asset_id}")
    } else {
        storage_path.to_string()
    }
}

pub fn strip_media_ref_lines(content: &str, key_a: &str, key_b: &str) -> String {
    let mut changed = false;
    let mut kept = Vec::new();

    for line in content.lines() {
        if line.contains(key_a) || line.contains(key_b) {
            changed = true;
            continue;
        }
        kept.push(line);
    }

    if changed {
        kept.join("\n")
    } else {
        content.to_string()
    }
}

pub fn strip_ink_ref_blocks(content: &str, asset_id: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut in_ink_block = false;
    let mut remove_current = false;
    let inline_marker = format!("\"id\":\"{asset_id}\"");

    for line in content.lines() {
        let trimmed = line.trim();
        if !in_ink_block
            && (trimmed.starts_with(":::whiteboard") || trimmed.starts_with(":::ink"))
        {
            remove_current = trimmed.contains(&inline_marker);
            if trimmed.ends_with(":::") {
                // Single-line token style, no block close expected.
                if !remove_current {
                    out.push_str(line);
                    out.push('\n');
                }
                remove_current = false;
                continue;
            }
            in_ink_block = true;
            if !remove_current {
                out.push_str(line);
                out.push('\n');
            }
            continue;
        }

        if in_ink_block {
            if !remove_current {
                out.push_str(line);
                out.push('\n');
            }
            if trimmed == ":::" {
                in_ink_block = false;
                remove_current = false;
            }
            continue;
        }

        out.push_str(line);
        out.push('\n');
    }

    out
}

fn is_safe_remote_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

fn looks_like_image_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".bmp", ".avif",
    ]
    .iter()
    .any(|ext| lower.contains(ext))
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
