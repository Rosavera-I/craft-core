//! Runtime monitor - live execution status

use crate::api::{ApiClient, RuntimeStatus};
use leptos::*;

/// RuntimeMonitor displays system status and statistics
#[component]
pub fn RuntimeMonitor() -> impl IntoView {
let (status, set_status) = create_signal::<Option<RuntimeStatus>>(None);
let (loading, set_loading) = create_signal(true);
let (error, set_error) = create_signal::<Option<String>>(None);
let (refresh_interval, _set_refresh_interval) = create_signal(5000u64); // 5 seconds

let fetch_status = move || {
spawn_local(async move {
set_loading.set(true);
set_error.set(None);

let client = use_context::<ApiClient>().expect("ApiClient not provided");
match client.get_status().await {
Ok(s) => set_status.set(Some(s)),
Err(e) => {
set_error.set(Some(e));
set_status.set(None);
}
}

set_loading.set(false);
});
};

// Initial fetch
create_effect(move |_| {
fetch_status();
});

// Auto-refresh
let refresh_handle = create_effect(move |prev: Option<IntervalHandle>| {
if let Some(handle) = prev {
handle.clear();
}

set_interval_with_handle(
move || {
fetch_status();
},
std::time::Duration::from_millis(refresh_interval.get()),
)
.expect("Failed to create interval")
});

// IPC: Stop refresh on cleanup
on_cleanup(move || {
refresh_handle.get().clear();
});

view! {
<div class="runtime-monitor">
<div class="monitor-header">
<h2>"Runtime Monitor"</h2>
{move || {
if loading.get() {
view! { <span class="refresh-indicator">"⟳"</span> }.into_view()
} else {
view! {
<button class="refresh-btn" on:click=move |_| fetch_status()>
"↻ Refresh"
</button>
}.into_view()
}
}}
</div>

{move || {
if let Some(err) = error.get() {
view! {
<div class="monitor-error">
<span class="error-icon">"⚠"</span>
<span>{err}</span>
</div>
}.into_view()
} else if let Some(s) = status.get() {
let stats = s.stats;
view! {
<div class="monitor-content">
<div class="status-indicator" class:active=s.active>
<div class="status-dot"></div>
<span class="status-text">
{if s.active {
"Active".to_string()
} else {
"Inactive".to_string()
}}
</span>
</div>

<div class="stats-grid">
<div class="stat-card">
<div class="stat-value">{stats.installed_harnesses}</div>
<div class="stat-label">"Installed Harnesses"</div>
</div>
<div class="stat-card">
<div class="stat-value">{stats.memory_facts_count}</div>
<div class="stat-label">"Memory Facts"</div>
</div>
<div class="stat-card">
<div class="stat-value">{stats.compositions_created}</div>
<div class="stat-label">"Compositions"</div>
</div>
</div>

{if let Some(current) = &s.current_harness {
view! {
<div class="current-activity">
<h3>"Current Activity"</h3>
<p>"Active harness: " <strong>{current}</strong></p>
</div>
}.into_view()
} else {
view! { <span /> }.into_view()
}}

{if let Some(last) = &s.last_activity {
view! {
<div class="last-activity">
<span class="label">"Last activity: "</span>
<span>{last}</span>
</div>
}.into_view()
} else {
view! { <span /> }.into_view()
}}
</div>
}.into_view()
} else {
view! {
<div class="monitor-loading">
<p>"Loading runtime status..."</p>
</div>
}.into_view()
}
}}
</div>

<style>
"
.runtime-monitor {
background-color: #16213e;
border-radius: 8px;
padding: 1.5rem;
max-width: 800px;
margin: 1.5rem;
}
.monitor-header {
display: flex;
justify-content: space-between;
align-items: center;
margin-bottom: 1.5rem;
}
.monitor-header h2 {
margin: 0;
color: #e94560;
}
.refresh-indicator {
color: #a0a0a0;
animation: spin 1s linear infinite;
}
@keyframes spin {
from { transform: rotate(0deg); }
to { transform: rotate(360deg); }
}
.refresh-btn {
padding: 0.5rem 1rem;
background-color: #0f3460;
color: #e0e0e0;
border: none;
border-radius: 4px;
cursor: pointer;
transition: all 0.2s;
}
.refresh-btn:hover {
background-color: #1a5490;
}
.monitor-error {
padding: 1rem;
background-color: rgba(233, 69, 96, 0.1);
border-left: 3px solid #e94560;
display: flex;
align-items: center;
gap: 0.75rem;
color: #e94560;
}
.error-icon {
font-size: 1.5rem;
}
.monitor-content {
display: flex;
flex-direction: column;
gap: 1.5rem;
}
.status-indicator {
display: flex;
align-items: center;
gap: 0.5rem;
padding: 0.75rem 1rem;
background-color: #1a1a2e;
border-radius: 4px;
}
.status-indicator.active .status-dot {
background-color: #2ecc71;
box-shadow: 0 0 8px #2ecc71;
}
.status-indicator:not(.active) .status-dot {
background-color: #e74c3c;
}
.status-dot {
width: 12px;
height: 12px;
border-radius: 50%;
transition: all 0.3s;
}
.status-text {
color: #a0a0a0;
font-weight: 500;
}
.status-indicator.active .status-text {
color: #2ecc71;
}
.stats-grid {
display: grid;
grid-template-columns: repeat(3, 1fr);
gap: 1rem;
}
.stat-card {
background-color: #1a1a2e;
padding: 1.5rem;
border-radius: 4px;
text-align: center;
border: 1px solid #0f3460;
transition: all 0.2s;
}
.stat-card:hover {
border-color: #e94560;
}
.stat-value {
font-size: 2.5rem;
font-weight: bold;
color: #e94560;
}
.stat-label {
color: #a0a0a0;
font-size: 0.9rem;
margin-top: 0.25rem;
}
.current-activity {
background-color: #1a1a2e;
padding: 1rem;
border-radius: 4px;
border-left: 3px solid #e94560;
}
.current-activity h3 {
margin: 0 0 0.5rem 0;
color: #e0e0e0;
font-size: 1rem;
}
.current-activity p {
margin: 0;
color: #a0a0a0;
}
.last-activity {
text-align: right;
color: #666;
font-size: 0.85rem;
}
.last-activity .label {
color: #888;
}
.monitor-loading {
text-align: center;
padding: 3rem;
color: #666;
}
"
</style>
}
}
