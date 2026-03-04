# Slate

Simple local-first markdown notes app.

Live: [Slate](https://app.slate.tangentlabs.dev)

## Features

- Markdown editor with Raw, Preview, and Split modes
- Obsidian-style wiki links: `[[Note]]`
- Backlinks ("Linked mentions") for two-way navigation
- Auto-update wiki links when a note title is renamed
- Multi-tab note workflow
- Search notes by title or content
- Sidebar resize and collapse
- Multiple themes
- IndexedDB persistence in the browser

## Project layout

- `apps/web` - Leptos web app
- `apps/web/src/models` - App models
- `apps/web/src/store` - Persistence layer
- `apps/web/src/links.rs` - Wiki link parsing/rewriting
- `apps/web/src/note_graph.rs` - Backlink and rename propagation logic

## Run

From the repository root:

```bash
cargo check
```

Use your existing web dev workflow in `apps/web` to run in the browser.
