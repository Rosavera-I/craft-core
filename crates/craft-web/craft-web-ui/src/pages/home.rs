//! Home/Dashboard page

use crate::components::{HarnessPalette, RuntimeMonitor};
use crate::api::HarnessInfo;
use leptos::*;

#[component]
pub fn HomePage() -> impl IntoView {
let (selected_harness, set_selected_harness) = create_signal::<Option<HarnessInfo>>(None);

view! {
<div class="home-page">
<header class="page-header">
<h1>"CRAFT Dashboard"</h1>
<p class="description">
"Visual harness composition and memory inspection interface for CRAFT."
</p>
</header>

<div class="dashboard-grid">
<div class="left-panel">
<h2>"Quick Actions"</h2>
<div class="quick-actions">
<a href="/harnesses" class="quick-card">
<div class="quick-icon">🛠️</div>
<div class="quick-text">
<h3>"Browse Harnesses"</h3>
<p>"View and manage your installed harnesses"</p>
</div>
</a>
<a href="/compose" class="quick-card">
<div class="quick-icon">🔗</div>
<div class="quick-text">
<h3>"Compose"</h3>
<p>"Build compositions by combining harnesses"</p>
</div>
</a>
<a href="/memory" class="quick-card">
<div class="quick-icon">🧠</div>
<div class="quick-text">
<h3>"Memory"</h3>
<p>"Search and browse memory facts"</p>
</div>
</a>
</div>

<RuntimeMonitor />
</div>

<div class="right-panel">
<HarnessPalette
on_select=Callback::new(move |h: HarnessInfo| {
set_selected_harness.set(Some(h));
})
selected=selected_harness.get()
/>

{move || {
if let Some(h) = selected_harness.get() {
view! {
<div class="harness-preview">
<h3>{h.name}</h3>
<p class="harness-description">{h.description}</p>
<div class="harness-meta">
<span>"Version: " {h.version}</span>
<span>"Source: " {h.source}</span>
<span>"Authors: " {h.authors.join(", ")}</span>
</div>
<a href={format!("/harnesses?selected={}", h.name)} class="btn-view">
"View Details"
</a>
</div>
}.into_view()
} else {
view! { <span /> }.into_view()
}
}}
</div>
</div>
</div>

<style>
"
.home-page {
padding: 1.5rem;
max-width: 1400px;
margin: 0 auto;
}
.page-header {
margin-bottom: 2rem;
}
.page-header h1 {
margin: 0;
color: #e94560;
font-size: 2.5rem;
}
.description {
color: #888;
margin: 0.5rem 0 0 0;
}
.dashboard-grid {
display: grid;
grid-template-columns: 1fr 320px;
gap: 1.5rem;
align-items: start;
}
.left-panel {
display: flex;
flex-direction: column;
gap: 1.5rem;
}
.left-panel h2 {
margin: 0;
color: #e0e0e0;
font-size: 1.25rem;
}
.right-panel {
display: flex;
flex-direction: column;
gap: 1rem;
}
.quick-actions {
display: flex;
flex-direction: column;
gap: 0.75rem;
}
.quick-card {
display: flex;
align-items: center;
gap: 1rem;
padding: 1.25rem;
background-color: #16213e;
border-radius: 8px;
text-decoration: none;
color: inherit;
transition: all 0.2s;
border: 1px solid transparent;
}
.quick-card:hover {
border-color: #e94560;
background-color: #1a2646;
transform: translateX(4px);
}
.quick-icon {
font-size: 2rem;
}
.quick-text h3 {
margin: 0 0 0.25rem 0;
color: #e94560;
font-size: 1.1rem;
}
.quick-text p {
margin: 0;
color: #888;
font-size: 0.9rem;
}
.harness-preview {
background-color: #16213e;
padding: 1rem;
border-radius: 8px;
border: 1px solid #0f3460;
}
.harness-preview h3 {
margin: 0 0 0.5rem 0;
color: #e94560;
}
.harness-description {
color: #888;
margin: 0 0 0.75rem 0;
font-size: 0.9rem;
}
.harness-meta {
display: flex;
flex-direction: column;
gap: 0.25rem;
font-size: 0.85rem;
color: #666;
}
.btn-view {
display: inline-block;
margin-top: 1rem;
padding: 0.5rem 1rem;
background-color: #e94560;
color: white;
text-decoration: none;
border-radius: 4px;
text-align: center;
transition: all 0.2s;
}
.btn-view:hover {
background-color: #d63d56;
}
"
</style>
}
}
