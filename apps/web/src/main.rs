use leptos::prelude::*;
use pulldown_cmark::{html, Options, Parser};

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditorMode {
    Raw,
    Preview,
}

fn render_markdown(input: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(input, options);
    let mut output = String::new();
    html::push_html(&mut output, parser);

    output
}

#[component]
fn App() -> impl IntoView {
    let (markdown, set_markdown) = signal(String::from(
        "# Slate\n\nStart typing markdown here...\n\n- fast\n- rust-native parsing\n",
    ));

    let (mode, set_mode) = signal(EditorMode::Raw);

    let preview_html = Memo::new(move |_| {
        if mode.get() == EditorMode::Preview {
            render_markdown(&markdown.get())
        } else {
            String::new()

        }
    });

    view! {
        <main class="editor-shell">
            <style>
                {r#"
                    .editor-shell { max-width: 900px; margin: 2rem auto; padding: 0 1rem; font-family: Inter, system-ui, sans-serif; }
                    .toolbar { display: flex; gap: 0.5rem; margin-bottom: 0.75rem; }
                    .btn { padding: 0.45rem 0.75rem; border: 1px solid #ccc; border-radius: 8px; background: #fff; cursor: pointer; }
                    .btn:disabled { opacity: 0.65; cursor: default; }
                    .panel { border: 1px solid #ddd; border-radius: 10px; min-height: 420px; }
                    .editor-input { width: 100%; min-height: 420px; border: 0; resize: vertical; padding: 1rem; font: 14px/1.5 ui-monospace, SFMono-Regular, Menlo, monospace; }
                    .preview { padding: 1rem; line-height: 1.65; }
                    .preview pre { background: #f7f7f8; padding: 0.75rem; border-radius: 8px; overflow-x: auto; }
                    .preview code { background: #f7f7f8; padding: 0.1rem 0.3rem; border-radius: 5px; }
                "#}
            </style>

            <div class="toolbar">
                <button
                    class="btn"
                    on:click=move |_| set_mode.set(EditorMode::Raw)
                    disabled=move || mode.get() == EditorMode::Raw
                >
                    "Raw"
                </button>
                <button
                    class="btn"
                    on:click=move |_| set_mode.set(EditorMode::Preview)
                    disabled=move || mode.get() == EditorMode::Preview
                >
                    "Preview"
                </button>
            </div>

            <section class="panel">
                <Show
                    when=move || mode.get() == EditorMode::Raw
                    fallback=move || {
                        view! {
                            <article class="preview" inner_html=move || preview_html.get()></article>
                        }
                    }
                >
                    <textarea
                        class="editor-input"
                        prop:value=move || markdown.get()
                        on:input=move |ev| set_markdown.set(event_target_value(&ev))
                        placeholder="Write markdown..."
                        spellcheck="false"
                    />
                </Show>
            </section>
        </main>
    }

}

fn main() {
    leptos::mount::mount_to_body(|| view! { <App /> })
}
