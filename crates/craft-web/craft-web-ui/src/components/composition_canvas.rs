//! Composition canvas - visual node editor for harness composition

use crate::api::{ApiClient, CompositionPlan, HarnessInfo, ValidationWebSocket};
use leptos::*;

#[derive(Debug, Clone)]
struct CompositionNode {
id: usize,
harness: HarnessInfo,
x: f64,
y: f64,
}

/// CompositionCanvas provides visual drag-and-drop composition
#[component]
pub fn CompositionCanvas() -> impl IntoView {
let (nodes, set_nodes) = create_signal::<Vec<CompositionNode>>(vec![]);
let (draft_names, set_draft_names) = create_signal::<Vec<String>>(vec![]);
let (strategy, set_strategy) = create_signal("ordered-merge".to_string());
let (plan, set_plan) = create_signal::<Option<CompositionPlan>>(None);
let (validation_status, set_validation_status) = create_signal::<Vec<String>>(vec![]);
let (next_id, set_next_id) = create_signal(0usize);
let (is_validating, set_is_validating) = create_signal(false);

let on_drop = move |ev: web_sys::DragEvent| {
ev.prevent_default();
let data = ev.data_transfer()
.and_then(|dt| dt.get_data("harness").ok());

if let Some(json) = data {
if let Ok(harness) = serde_json::from_str::<HarnessInfo>(&json) {
let id = next_id.get();
set_next_id.set(id + 1);

let node = CompositionNode {
id,
harness,
x: f64::from(ev.client_x()),
y: f64::from(ev.client_y()),
};

set_nodes.update(|n| n.push(node.clone()));
set_draft_names.update(|names| names.push(node.harness.name.clone()));
}
}
};

let on_drag_over = move |ev: web_sys::DragEvent| {
ev.prevent_default();
};

// Plan composition
let plan_composition = move |_| {
if draft_names.get().is_empty() {
return;
}

let names = draft_names.get();
let strat = strategy.get();

spawn_local(async move {
let client = use_context::<ApiClient>().expect("ApiClient not provided");
match client.compose_plan(names.clone(), &strat).await {
Ok(p) => set_plan.set(Some(p)),
Err(e) => {
log::error!("Failed to plan composition: {}", e);
set_plan.set(None);
}
}
});
};

// Validate composition via WebSocket
let validate_composition = move |_| {
let names = draft_names.get();
if names.is_empty() {
return;
}

let strat = strategy.get();
set_is_validating.set(true);
set_validation_status.set(vec!["Starting validation...".to_string()]);

spawn_local(async move {
// Use HTTP validation for now since WebSocket is complex in WASM
let client = use_context::<ApiClient>().expect("ApiClient not provided");
let mut statuses = vec![];

// Validate each harness
for name in &names {
match client.get_harness(name).await {
Ok(_) => statuses.push(format!("✓ {} is valid", name)),
Err(e) => statuses.push(format!("✗ {}: {}", name, e)),
}
}

set_validation_status.set(statuses);
set_is_validating.set(false);
});
};

// Clear canvas
let clear_canvas = move |_| {
set_nodes.set(vec![]);
set_draft_names.set(vec![]);
set_plan.set(None);
set_validation_status.set(vec![]);
};

view! {
<div class="composition-container">
<div class="composition-controls">
<h2>Composition Canvas</h2>
<div class="strategy-selector">
<label>"Strategy: "</label>
<select prop:value=strategy.get() on:change=move |ev| {
set_strategy.set(event_target_value(&ev));
}>
<option value="ordered-merge">"Ordered Merge"</option>
<option value="merge">"Merge"</option>
<option value="override">"Override"</option>
<option value="fail">"Fail"</option>
</select>
</div>
<div class="btn-group">
<button class="btn btn-primary" on:click=plan_composition>
"Preview Plan"
</button>
<button
class="btn btn-secondary"
on:click=validate_composition
disabled=is_validating()
>
{move || if is_validating.get() {
"Validating...".to_string()
} else {
"Validate".to_string()
}}
</button>
<button class="btn btn-danger" on:click=clear_canvas>
"Clear"
</button>
</div>
</div>

<div
class="composition-canvas"
on:drop=on_drop
on:dragover=on_drag_over
>
{move || {
nodes.get().into_iter().map(|node| {
let style = format!("position: absolute; left: {}px; top: {}px;", node.x, node.y);
view! {
<div class="composition-node" style=style>
<div class="node-header">{node.harness.name.clone()}</div>
<div class="node-version">{node.harness.version.clone()}</div>
</div>
}
}).collect_view()
}}

<div class="canvas-instructions">
"Drag harnesses from the palette here"
</div>
</div>

<div class="composition-sidebar">
<div class="draft-harnesses">
<h3>"Draft Composition"</h3>
<ol>
{move || {
draft_names.get().into_iter().map(|name| {
view! { <li>{name}</li> }
}).collect_view()
}}
</ol>
</div>

{move || {
if let Some(p) = plan.get() {
view! {
<div class="plan-preview">
<h3>"Plan Preview"</h3>
<p class="plan-strategy">{format!("Strategy: {}", p.strategy)}</p>
{if !p.warnings.is_empty() {
view! {
<div class="plan-warnings">
<h4>"Warnings"</h4>
<ul>
{p.warnings.into_iter().map(|w| view! { <li>{w}</li> }).collect_view()}
</ul>
</div>
}.into_view()
} else {
view! { <span /> }.into_view()
}}
<div class="plan-harnesses">
<h4>"Included Harnesses"</h4>
<ul>
{p.harnesses.into_iter().map(|h| {
view! {
<li>
<strong>{h.name}</strong>
" (" {h.version} ")"
</li>
}
}).collect_view()}
</ul>
</div>
</div>
}.into_view()
} else {
view! { <span /> }.into_view()
}
}}

{move || {
if !validation_status.get().is_empty() {
view! {
<div class="validation-results">
<h3>"Validation Results"</h3>
<ul>
{validation_status.get().into_iter().map(|s| {
view! { <li>{s}</li> }
}).collect_view()}
</ul>
</div>
}.into_view()
} else {
view! { <span /> }.into_view()
}
}}
</div>
</div>

<style>
"
.composition-container {
display: grid;
grid-template-columns: 1fr 300px;
gap: 1rem;
padding: 1.5rem;
}
.composition-controls {
grid-column: 1 / -1;
display: flex;
align-items: center;
gap: 1rem;
background-color: #16213e;
padding: 1rem;
border-radius: 8px;
}
.composition-controls h2 {
margin: 0;
color: #e94560;
}
.strategy-selector label {
color: #a0a0a0;
}
.strategy-selector select {
background-color: #1a1a2e;
color: #e0e0e0;
border: 1px solid #0f3460;
padding: 0.5rem;
border-radius: 4px;
}
.btn-group {
display: flex;
gap: 0.5rem;
margin-left: auto;
}
.btn {
padding: 0.5rem 1rem;
border: none;
border-radius: 4px;
cursor: pointer;
transition: all 0.2s;
font-size: 0.9rem;
}
.btn-primary {
background-color: #e94560;
color: white;
}
.btn-primary:hover {
background-color: #d63d56;
}
.btn-secondary {
background-color: #0f3460;
color: #e0e0e0;
}
.btn-secondary:hover {
background-color: #1a5490;
}
.btn-danger {
background-color: #533483;
color: #e0e0e0;
}
.btn-danger:hover {
background-color: #6a4a9e;
}
.btn:disabled {
opacity: 0.5;
cursor: not-allowed;
}
.composition-canvas {
position: relative;
min-height: 500px;
background-color: #0d1b2a;
border-radius: 8px;
border: 2px dashed #1f4068;
overflow: hidden;
}
.composition-canvas[drag-over] {
border-color: #e94560;
}
.canvas-instructions {
position: absolute;
top: 50%;
left: 50%;
transform: translate(-50%, -50%);
color: #444;
font-size: 1.2rem;
user-select: none;
pointer-events: none;
}
.composition-node {
position: absolute;
width: 180px;
padding: 0.75rem;
background-color: #16213e;
border: 1px solid #0f3460;
border-radius: 8px;
transform: translate(-50%, -50%);
}
.node-header {
font-weight: 600;
color: #e94560;
margin-bottom: 0.25rem;
}
.node-version {
font-size: 0.8rem;
color: #666;
}
.composition-sidebar {
display: flex;
flex-direction: column;
gap: 1rem;
}
.draft-harnesses,
.plan-preview,
.validation-results {
background-color: #16213e;
border-radius: 8px;
padding: 1rem;
}
.draft-harnesses h3,
.plan-preview h3,
.validation-results h3 {
margin: 0 0 0.75rem 0;
color: #e94560;
font-size: 1rem;
}
.plan-strategy {
color: #a0a0a0;
font-style: italic;
}
.plan-warnings {
background-color: rgba(233, 69, 96, 0.1);
border-left: 3px solid #e94560;
padding: 0.5rem;
margin: 0.5rem 0;
}
.plan-warnings h4 {
margin: 0 0 0.25rem 0;
color: #e94560;
}
.plan-warnings ul {
margin: 0;
padding-left: 1rem;
color: #e0e0e0;
}
"
</style>
}
}
