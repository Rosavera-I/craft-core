//! Harness composition page

use crate::components::CompositionCanvas;
use leptos::*;

#[component]
pub fn ComposePage() -> impl IntoView {
view! {
<div class="compose-page">
<header class="page-header">
<h1>"Harness Composition"</h1>
<p>"Drag and drop harnesses to build compositions"</p>
</header>

<CompositionCanvas />
</div>

<style>
"
.compose-page {
padding: 0;
}
.page-header {
padding: 1.5rem;
max-width: 1400px;
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
