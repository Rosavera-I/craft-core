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
                background-color: #16213e;
                border-bottom: 1px solid #0f3460;
            }
            .nav-brand {
                display: flex;
                align-items: center;
                gap: 0.5rem;
            }
            .logo {
                font-size: 1.5rem;
                font-weight: bold;
                color: #e94560;
            }
            .tagline {
                font-size: 0.9rem;
                color: #a0a0a0;
            }
            .nav-links {
                display: flex;
                gap: 1.5rem;
            }
            .nav-link {
                color: #a0a0a0;
                text-decoration: none;
                padding: 0.5rem 1rem;
                border-radius: 4px;
                transition: all 0.2s;
            }
            .nav-link:hover {
                color: #e0e0e0;
                background-color: #0f3460;
            }
            .nav-link-active {
                color: #e94560;
                background-color: rgba(233, 69, 96, 0.1);
            }
            "
        </style>
    }
}
