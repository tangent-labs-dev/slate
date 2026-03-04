use crate::links::{link_target_base, normalize_title, parse_wiki_links};
use pulldown_cmark::{Options, Parser, html};
use std::collections::HashMap;

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
