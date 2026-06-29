//! Memory inspector - search and browse memory facts with FTS

use crate::api::{ApiClient, MemoryFact, MemorySearchResult};
use leptos::*;

/// MemoryInspector provides search and browse capabilities
#[component]
pub fn MemoryInspector() -> impl IntoView {
let (facts, set_facts) = create_signal::<Vec<MemoryFact>>(vec![]);
let (query, set_query) = create_signal(String::new());
let (scope, set_scope) = create_signal::<Option<String>>(None);
let (loading, set_loading) = create_signal(false);
let (error, set_error) = create_signal::<Option<String>>(None);
let (selected_scope, set_selected_scope) = create_signal("all".to_string());

// Search handler
let search = move || {
let q = query.get();
if q.trim().is_empty() {
// Load all facts instead
spawn_local(async move {
set_loading.set(true);
set_error.set(None);
let client = use_context::<ApiClient>().expect("ApiClient not provided");
let scope_filter = scope.get();

match client.list_memory_facts(scope_filter.as_deref()).await {
Ok(list) => set_facts.set(list),
Err(e) => set_error.set(Some(e)),
}
set_loading.set(false);
});
} else {
spawn_local(async move {
set_loading.set(true);
set_error.set(None);
let client = use_context::<ApiClient>().expect("ApiClient not provided");
let scope_filter = scope.get();

match client.search_memory(&q, scope_filter.as_deref()).await {
Ok(result) => set_facts.set(result.facts),
Err(e) => set_error.set(Some(e)),
}
set_loading.set(false);
});
}
};

// Clear search
let clear_search = move |_| {
set_query.set(String::new());
set_scope.set(None);
set_selected_scope.set("all".to_string());
set_facts.set(vec![]);
};

// Scope filter change
let on_scope_change = move |ev| {
let value = event_target_value(&ev);
set_selected_scope.set(value.clone());
if value == "all" {
set_scope.set(None);
} else {
set_scope.set(Some(value.clone()));
}
};

// Create new fact (simplified)
let create_fact = move |_| {
spawn_local(async move {
let client = use_context::<ApiClient>().expect("ApiClient not provided");
// Create a sample fact
match client.create_memory_fact("global", "test-key", "test-value").await {
Ok(fact) => {
log::info!("Created fact: {:?}", fact);
// Reload
search();
}
Err(e) => {
log::error!("Failed to create fact: {}", e);
}
}
});
};

// Initial load
create_effect(move |_| {
search();
});

view! {
<div class="memory-inspector">
<div class="memory-header">
<h2>"Memory Inspector"</h2>
<p class="subtitle">"Search and browse memory facts"</p>
</div>

<div class="memory-controls">
<div class="search-bar">
<input
prop:value=query.get()
on:input=move |ev| set_query.set(event_target_value(&ev))
placeholder="Search memory facts..."
type="text"
/>
<button on:click=move |_| search()>
"Search"
</button>
<button class="btn-secondary" on:click=clear_search>
"Clear"
</button>
</div>

<div class="scope-filter">
<label>"Scope: "</label>
<select prop:value=selected_scope.get() on:change=on_scope_change>
<option value="all">"All Scopes"</option>
<option value="global">"Global"</option>
<option value="user">"User"</option>
<option value="project">"Project"</option>
<option value="session">"Session"</option>
</select>
<button class="btn-add" on:click=create_fact>
"+ Add Fact"
</button>
</div>
</div>

<div class="memory-content">
{move || {
if loading.get() {
view! { <div class="memory-loading">"Loading..."</div> }.into_view()
} else if let Some(err) = error.get() {
view! { <div class="memory-error">{err}</div> }.into_view()
} else {
let items = facts.get();
if items.is_empty() {
view! {
<div class="memory-empty">
"No memory facts found. Try a different search or add new facts."
</div>
}.into_view()
} else {
view! {
<div class="memory-results">
<div class="memory-count">
{format!("Showing {} fact(s)", items.len())}
</div>
<div class="memory-list">
{items.into_iter().map(|fact| {
let timestamp = chrono::DateTime::from_timestamp(fact.created_at, 0)
.map(|dt| dt.to_rfc3339())
.unwrap_or_else(|| "Unknown".to_string());

view! {
<div class="memory-fact">
<div class="fact-header">
<span class="fact-scope">{fact.scope}</span>
<span class="fact-key">{fact.key}</span>
<span class="fact-timestamp">{timestamp}</span>
</div>
<div class="fact-value">{fact.value}</div>
</div>
}
}).collect_view()}
</div>
</div>
}.into_view()
}
}
}}
</div>
</div>

<style>
"
.memory-inspector {
padding: 1.5rem;
max-width: 1200px;
margin: 0 auto;
}
.memory-header {
margin-bottom: 1.5rem;
}
.memory-header h2 {
margin: 0;
color: #e94560;
}
.subtitle {
color: #666;
margin: 0.25rem 0 0 0;
}
.memory-controls {
display: flex;
flex-direction: column;
gap: 1rem;
background-color: #16213e;
padding: 1rem;
border-radius: 8px;
margin-bottom: 1rem;
}
.search-bar {
display: flex;
gap: 0.5rem;
}
.search-bar input {
flex: 1;
padding: 0.75rem;
background-color: #1a1a2e;
color: #e0e0e0;
border: 1px solid #0f3460;
border-radius: 4px;
font-size: 1rem;
}
.search-bar input:focus {
outline: none;
border-color: #e94560;
}
.search-bar button {
padding: 0.75rem 1.5rem;
background-color: #e94560;
color: white;
border: none;
border-radius: 4px;
cursor: pointer;
transition: all 0.2s;
}
.search-bar button:hover {
background-color: #d63d56;
}
.btn-secondary {
background-color: #0f3460 !important;
}
.btn-secondary:hover {
background-color: #1a5490 !important;
}
.scope-filter {
display: flex;
align-items: center;
gap: 0.5rem;
}
.scope-filter label {
color: #a0a0a0;
}
.scope-filter select {
padding: 0.5rem;
background-color: #1a1a2e;
color: #e0e0e0;
border: 1px solid #0f3460;
border-radius: 4px;
}
.btn-add {
padding: 0.5rem 1rem;
background-color: #533483;
color: #e0e0e0;
border: none;
border-radius: 4px;
cursor: pointer;
margin-left: auto;
}
.btn-add:hover {
background-color: #6a4a9e;
}
.memory-content {
background-color: #16213e;
border-radius: 8px;
min-height: 400px;
}
.memory-loading,
.memory-error,
.memory-empty {
padding: 3rem;
text-align: center;
color: #888;
}
.memory-error {
color: #e94560;
}
.memory-results {
padding: 1rem;
}
.memory-count {
color: #666;
font-size: 0.9rem;
margin-bottom: 1rem;
padding-bottom: 0.5rem;
border-bottom: 1px solid #0f3460;
}
.memory-list {
display: flex;
flex-direction: column;
gap: 0.5rem;
}
.memory-fact {
background-color: #1a1a2e;
padding: 1rem;
border-radius: 4px;
border: 1px solid #0f3460;
transition: all 0.2s;
}
.memory-fact:hover {
border-color: #e94560;
}
.fact-header {
display: flex;
align-items: center;
gap: 0.75rem;
margin-bottom: 0.5rem;
}
.fact-scope {
background-color: #e94560;
color: white;
padding: 0.2rem 0.5rem;
border-radius: 3px;
font-size: 0.75rem;
text-transform: uppercase;
}
.fact-key {
font-weight: 600;
color: #e0e0e0;
}
.fact-timestamp {
color: #666;
font-size: 0.8rem;
margin-left: auto;
}
.fact-value {
color: #a0a0a0;
white-space: pre-wrap;
word-break: break-word;
}
"
</style>
}
}
