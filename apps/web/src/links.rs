use crate::models::WikiLink;

pub fn normalize_title(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn parse_wiki_links(input: &str) -> Vec<WikiLink> {
    let mut out = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;

    while i + 1 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            let start = i;
            i += 2;
            let inner_start = i;
            while i + 1 < bytes.len() {
                if bytes[i] == b']' && bytes[i + 1] == b']' {
                    let inner = &input[inner_start..i];
                    if let Some(link) = parse_inner_link(inner, start, i + 2) {
                        out.push(link);
                    }
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }
        i += 1;
    }

    out
}

pub fn rename_wiki_links(input: &str, old_title: &str, new_title: &str) -> String {
    let old_norm = normalize_title(old_title);
    let new_trimmed = new_title.trim();
    if old_norm.is_empty() || new_trimmed.is_empty() || old_norm == normalize_title(new_trimmed) {
        return input.to_string();
    }

    let links = parse_wiki_links(input);
    if links.is_empty() {
        return input.to_string();
    }

    let mut changed = false;
    let mut out = String::with_capacity(input.len());
    let mut cursor = 0;

    for link in links {
        out.push_str(&input[cursor..link.start]);

        let (base, suffix) = split_target_and_suffix(&link.target);
        if normalize_title(base) == old_norm {
            changed = true;
            let mut replacement = String::from("[[");
            replacement.push_str(new_trimmed);
            replacement.push_str(suffix);
            if let Some(alias) = &link.alias {
                replacement.push('|');
                replacement.push_str(alias);
            }
            replacement.push_str("]]");
            out.push_str(&replacement);
        } else {
            out.push_str(&input[link.start..link.end]);
        }

        cursor = link.end;
    }

    out.push_str(&input[cursor..]);
    if changed { out } else { input.to_string() }
}

pub fn link_target_base(target: &str) -> &str {
    split_target_and_suffix(target).0
}

fn parse_inner_link(inner: &str, start: usize, end: usize) -> Option<WikiLink> {
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut pieces = trimmed.splitn(2, '|');
    let target = pieces.next().unwrap_or_default().trim();
    if target.is_empty() {
        return None;
    }

    let alias = pieces.next().map(str::trim).filter(|s| !s.is_empty());
    Some(WikiLink {
        start,
        end,
        target: target.to_string(),
        alias: alias.map(ToString::to_string),
    })
}

fn split_target_and_suffix(target: &str) -> (&str, &str) {
    if let Some(idx) = target.find('#') {
        (&target[..idx], &target[idx..])
    } else {
        (target, "")
    }
}
