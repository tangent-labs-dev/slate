mod app;
mod links;
mod markdown;
mod models;
mod note_graph;
mod store;

use leptos::prelude::*;
use leptos::web_sys;

fn main() {
    let is_app_route = web_sys::window()
        .and_then(|window| window.location().pathname().ok())
        .map(|path| path == "/home" || path.starts_with("/home/"))
        .unwrap_or(false);

    if !is_app_route {
        return;
    }

    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        if let Some(landing_root) = document.get_element_by_id("landing-root") {
            landing_root.set_class_name("hidden");
        }
    }

    leptos::mount::mount_to_body(|| view! { <app::App /> });
}
