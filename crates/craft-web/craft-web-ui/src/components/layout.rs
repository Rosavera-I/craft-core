//! Application layout components

use leptos::*;

/// AppLayout provides the main page structure
#[component]
pub fn AppLayout(children: Children) -> impl IntoView {
    view! {
        <div class="app-container">
            {children()}
        </div>
        
        // Basic styles
        <style>
            "
            .app-container {
                min-height: 100vh;
                background-color: #1a1a2e;
                color: #e0e0e0;
                font-family: system-ui, -apple-system, sans-serif;
            }
            "
        </style>
    }
}

/// Props for layout components
#[derive(Debug, Clone)]
pub struct LayoutProps {
    // Could add theme, sidebar visibility, etc.
}
