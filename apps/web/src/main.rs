mod app;
mod links;
mod markdown;
mod models;
mod note_graph;
mod store;

use leptos::prelude::*;

fn main() {
    leptos::mount::mount_to_body(|| view! { <app::App /> });
}
