//! Harness browser page

use crate::api::{ApiClient, HarnessInfo};
use leptos::*;

#[component]
pub fn HarnessPage() -> impl IntoView {
let (harnesses, set_harnesses) = create_signal::<Vec<HarnessInfo>>(vec![]);
let (loading, set_loading) = create_signal(true);
let (error, set_error) = create_signal::<Option<String>>(None);
let (selected, set_selected) = create_signal::<Option<HarnessInfo>>(None);

// Load harnesses on mount
create_effect(move |_| {
spawn_local(async move {
set_loading.set(true);
set_error.set(None);

let client = use_context::<ApiClient>().expect("ApiClient not provided");
match client.list_harnesses().await {
Ok(list) => set_harnesses.set(list),
Err(e) => set_error.set(Some(e)),
}
set_loading.set(false);
});
});

view! {
<div class="harness-page">
<header class="page-header">
<h1>"Harness Browser"</h1>
<p>"Browse and explore your installed harnesses"</p>
</header>

<div class="harness-container">
<div class="harness-list-panel">
{move || {
if loading.get() {
view! { <div class="loading">"Loading harnesses..."</div> }.into_view()
} else if let Some(err) = error.get() {
view! { <div class="error">{err}</div> }.into_view()
} else {
let items = harnesses.get();
if items.is_empty() {
view! { <div class="empty">"No harnesses installed"</div> }.into_view()
} else {
items
.into_iter()
.map(|h| {
let is_selected = selected
.get()
.as_ref()
.map(|s| s.name == h.name)
.unwrap_or(false);
let h_clone = h.clone();

view! {
<div
class={move || {
if is_selected {
"harness-list-item selected"
} else {
"harness-list-item"
}
}}
on:click=move |_| set_selected.set(Some(h_clone.clone()))
>
<div class="harness-list-name">{h.name}</div>
<div class="harness-list-meta">
<span>"v" {h.version}</span>
<span class="harness-list-source">{h.source}</span>
</div>
</div>
}
})
.collect_view()
}
}
}}
</div>

<div class="harness-detail-panel">
{move || {
if let Some(h) = selected.get() {
view! {
<div class="harness-detail">
<div class="detail-header">
<h2>{h.name}</h2>
<span class="version-badge">{h.version}</span>
</div>

<div class="detail-section">
<h3>"Description"</h3>
<p>{if h.description.is_empty() { "No description available" } else { &h.description }}</p>
</div>

<div class="detail-section">
<h3>"Source"</h3>
<code>{h.source}</code>
</div>

<div class="detail-section">
<h3>"Authors"</h3>
<p>{if h.authors.is_empty() {
"Unknown"
} else {
h.authors.join(", ")
}}</p>
</div>

<div class="detail-section">
<h3>"Installed"</h3>
<p>{h.installed_at}</p>
</div>

<div class="detail-actions">
<a href="/compose" class="action-btn">
"Use in Compose"
</a>
</div>
</div>
}.into_view()
} else {
view! {
<div class="no-selection">
<div class="placeholder-icon">🛠️</div>
<p>"Select a harness from the list to view details"</p>
</div>
}.into_view()
}
}}
</div>
</div>
</div>

<style>
"
.harness-page {
padding: 1.5rem;
max-width: 1400px;
margin: 0 auto;
}
.page-header {
margin-bottom: 1.5rem;
}
.page-header h1 {
margin: 0;
color: #e94560;
}
.page-header p {
color: #888;
margin: 0.25rem 0 0 0;
}
.harness-container {
display: grid;
grid-template-columns: 350px 1fr;
gap: 1.5rem;
min-height: 600px;
}
.harness-list-panel {
background-color: #16213e;
border-radius: 8px;
overflow-y: auto;
max-height: 80vh;
}
.loading,
.error,
.empty {
padding: 2rem;
text-align: center;
color: #888;
}
.error {
color: #e94560;
}
.harness-list-item {
padding: 1rem;
border-bottom: 1px solid #0f3460;
cursor: pointer;
transition: all 0.2s;
}
.harness-list-item:hover {
background-color: #1a2646;
}
.harness-list-item.selected {
background-color: rgba(233, 69, 96, 0.1);
border-left: 3px solid #e94560;
}
.harness-list-name {
font-weight: 600;
color: #e0e0e0;
margin-bottom: 0.25rem;
}
.harness-list-meta {
display: flex;
gap: 0.75rem;
font-size: 0.85rem;
color: #666;
}
.harness-list-source {
color: #888;
}
.harness-detail-panel {
background-color: #16213e;
border-radius: 8px;
padding: 1.5rem;
}
.harness-detail {
animation: fadeIn 0.3s ease;
}
@keyframes fadeIn {
from { opacity: 0; transform: translateY(10px); }
to { opacity: 1; transform: translateY(0); }
}
.detail-header {
display: flex;
align-items: center;
gap: 1rem;
margin-bottom: 1.5rem;
padding-bottom: 1rem;
border-bottom: 1px solid #0f3460;
}
.detail-header h2 {
margin: 0;
color: #e94560;
font-size: 1.75rem;
}
.version-badge {
background-color: #0f3460;
color: #e0e0e0;
padding: 0.25rem 0.75rem;
border-radius: 4px;
font-size: 0.9rem;
}
.detail-section {
margin-bottom: 1.5rem;
}
.detail-section h3 {
color: #a0a0a0;
font-size: 0.9rem;
text-transform: uppercase;
letter-spacing: 0.05em;
margin: 0 0 0.5rem 0;
}
.detail-section p {
color: #e0e0e0;
margin: 0;
line-height: 1.6;
}
.detail-section code {
background-color: #0d1b2a;
padding: 0.5rem;
border-radius: 4px;
color: #e94560;
font-size: 0.9rem;
display: inline-block;
}
.detail-actions {
margin-top: 2rem;
padding-top: 1rem;
border-top: 1px solid #0f3460;
}
.action-btn {
display: inline-block;
padding: 0.75rem 1.5rem;
background-color: #e94560;
color: white;
text-decoration: none;
border-radius: 4px;
transition: all 0.2s;
}
.action-btn:hover {
background-color: #d63d56;
}
.no-selection {
display: flex;
flex-direction: column;
align-items: center;
justify-content: center;
height: 100%;
color: #666;
text-align: center;
}
.placeholder-icon {
font-size: 4rem;
margin-bottom: 1rem;
}
"
</style>
}
}
