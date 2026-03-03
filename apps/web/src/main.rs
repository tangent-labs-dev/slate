mod app;
mod markdown;
mod models;
mod store;

use leptos::prelude::*;

fn main() {
    leptos::mount::mount_to_body(|| view! { <app::App /> });
}
