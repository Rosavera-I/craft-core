//! Memory page - search and browse memory facts

use crate::components::MemoryInspector;
use leptos::*;

#[component]
pub fn MemoryPage() -> impl IntoView {
view! {
<div class="memory-page">
<header class="page-header">
<h1>"Memory Inspector"</h1>
<p>"Search and browse memory facts with full-text search"</p>
</header>

<MemoryInspector />
</div>

<style>
"
.memory-page {
padding: 0;
}
.page-header {
padding: 1.5rem;
max-width: 1200px;
margin: 0 auto;
}
.page-header h1 {
margin: 0;
color: #e94560;
font-size: 2rem;
}
.page-header p {
color: #888;
margin: 0.25rem 0 0 0;
}
"
</style>
}
}
