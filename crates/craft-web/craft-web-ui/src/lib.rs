//! # CRAFT Web Dashboard - Leptos Frontend
//!
//! WASM frontend for the CRAFT Web Dashboard with:
//! - Harness Palette component
//! - Composition Canvas (visual node editor)
//! - Memory Inspector
//! - Runtime Monitor

use leptos::*;
use leptos_router::*;

mod api;
mod components;
mod pages;

use components::{AppLayout, Navigation};
use pages::{ComposePage, HarnessPage, HomePage, MemoryPage};

/// Main application component
#[component]
pub fn App() -> impl IntoView {
    // Provide the API client context
    provide_context(api::ApiClient::default());

    view! {
        <Router>
            <AppLayout>
                <Navigation />
                <main class="main-content">
                    <Routes>
                        <Route path="/" view=HomePage />
                        <Route path="/harnesses" view=HarnessPage />
                        <Route path="/compose" view=ComposePage />
                        <Route path="/memory" view=MemoryPage />
                    </Routes>
                </main>
            </AppLayout>
        </Router>
    }
}

/// Initialize the app when loaded in browser
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn main() {
    wasm_logger::init(wasm_logger::Config::new(log::Level::Info));
    console_error_panic_hook::set_once();
    mount_to_body(App);
}
