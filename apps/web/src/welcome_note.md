# Welcome to Slate

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
![My uploaded image](slate-media://uploads/asset-id)
<video controls src="slate-media://uploads/asset-id"></video>
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
