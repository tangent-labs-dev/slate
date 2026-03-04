use crate::app::helpers::AppTheme;
use crate::models::{EditorMode, Note};
use leptos::prelude::*;

#[component]
pub fn Toolbar(
    active_note: Signal<Option<Note>>,
    mode: ReadSignal<EditorMode>,
    set_mode: WriteSignal<EditorMode>,
    theme: ReadSignal<AppTheme>,
    set_theme: WriteSignal<AppTheme>,
    on_insert_image_url: Callback<()>,
    on_insert_video_url: Callback<()>,
    on_click_upload_image: Callback<()>,
    on_click_upload_video: Callback<()>,
) -> impl IntoView {
    view! {
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
                <button class="mode-btn" on:click=move |_| on_insert_image_url.run(())>
                    "Image URL"
                </button>
                <button class="mode-btn" on:click=move |_| on_click_upload_image.run(())>
                    "Upload Image"
                </button>
                <button class="mode-btn" on:click=move |_| on_insert_video_url.run(())>
                    "Video URL"
                </button>
                <button class="mode-btn" on:click=move |_| on_click_upload_video.run(())>
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
    }
}
