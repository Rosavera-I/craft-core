//! Navigation component with CRAFT branding

use leptos::*;
use leptos_router::*;

#[component]
pub fn Navigation() -> impl IntoView {
    view! {
        <nav class="navbar">
            <div class="nav-brand">
                <span class="logo">🛠️ CRAFT</span>
                <span class="tagline">Web Dashboard</span>
            </div>
            <div class="nav-links">
                <NavLink href="/" class="nav-link" active_class="nav-link-active">
                    "Dashboard"
                </NavLink>
                <NavLink href="/harnesses" class="nav-link" active_class="nav-link-active">
                    "Harnesses"
                </NavLink>
                <NavLink href="/compose" class="nav-link" active_class="nav-link-active">
                    "Compose"
                </NavLink>
                <NavLink href="/memory" class="nav-link" active_class="nav-link-active">
                    "Memory"
                </NavLink>
            </div>
        </nav>
        
        <style>
            "
            .navbar {
                display: flex;
                justify-content: space-between;
                align-items: center;
                padding: 1rem 2rem;
                background-color: var(--craft-surface);
                border-bottom: 1px solid var(--craft-border-strong);
            }
            .nav-brand {
                display: flex;
                align-items: center;
                gap: 0.5rem;
            }
            .logo {
                font-size: 1.5rem;
                font-weight: bold;
                color: var(--craft-accent);
            }
            .tagline {
                font-size: 0.9rem;
                color: var(--craft-text-muted);
            }
            .nav-links {
                display: flex;
                gap: 1.5rem;
            }
            .nav-link {
                color: var(--craft-text-muted);
                text-decoration: none;
                padding: 0.5rem 1rem;
                border-radius: 4px;
                transition: all 0.2s;
            }
            .nav-link:hover {
                color: var(--craft-text);
                background-color: var(--craft-border-strong);
            }
            .nav-link-active {
                color: var(--craft-accent);
                background-color: var(--craft-accent-soft);
            }
            "
        </style>
    }
}
