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
            :root {
                --craft-bg: #1a1a2e;
                --craft-surface: #16213e;
                --craft-surface-raised: #111b30;
                --craft-surface-muted: #0d1b2a;
                --craft-surface-hover: #1a2646;
                --craft-border: #25324f;
                --craft-border-strong: #0f3460;
                --craft-accent: #e94560;
                --craft-accent-hover: #d63d56;
                --craft-accent-soft: rgba(233, 69, 96, 0.12);
                --craft-text: #e0e0e0;
                --craft-text-strong: #f3f6fa;
                --craft-text-muted: #a0a0a0;
                --craft-text-subtle: #888;
                --craft-text-dim: #666;
                --craft-success: #2ecc71;
                --craft-danger: #e74c3c;
                --craft-focus: #8bd3ff;
            }
            .app-container {
                min-height: 100vh;
                background-color: var(--craft-bg);
                color: var(--craft-text);
                font-family: system-ui, -apple-system, sans-serif;
            }
            .app-container *,
            .app-container *::before,
            .app-container *::after {
                box-sizing: border-box;
            }
            .app-container :where(a, button, input, select, [role=\"button\"]):focus-visible {
                outline: 2px solid var(--craft-focus);
                outline-offset: 2px;
            }
            @media (prefers-reduced-motion: reduce) {
                .app-container *,
                .app-container *::before,
                .app-container *::after {
                    animation-duration: 0.01ms !important;
                    animation-iteration-count: 1 !important;
                    scroll-behavior: auto !important;
                    transition-duration: 0.01ms !important;
                }
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
