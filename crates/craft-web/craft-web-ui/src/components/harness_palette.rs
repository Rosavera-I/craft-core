//! Harness palette component with drag handles

use crate::api::{ApiClient, HarnessInfo};
use leptos::*;

/// HarnessPalette displays the list of installed harnesses with drag handles
#[component]
pub fn HarnessPalette(
    #[prop(into)] on_select: Callback<HarnessInfo>,
    #[prop(optional)] selected: Option<HarnessInfo>,
) -> impl IntoView {
    let (harnesses, set_harnesses) = create_signal::<Vec<HarnessInfo>>(vec![]);
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);
    
    // Load harnesses on mount
    create_effect(move |_| {
        let client = use_context::<ApiClient>().expect("ApiClient not provided");
        spawn_local(async move {
            set_loading.set(true);
            set_error.set(None);
            
            match client.list_harnesses().await {
                Ok(list) => set_harnesses.set(list),
                Err(e) => set_error.set(Some(e)),
            }
            set_loading.set(false);
        });
    });
    
    view! {
        <div class="harness-palette">
            <h2>Installed Harnesses</h2>
            <div class="harness-list">
                {move || {
                    if loading.get() {
                        view! { <p class="loading">"Loading..."</p> }.into_view()
                    } else if let Some(err) = error.get() {
                        view! { <p class="error">{err}</p> }.into_view()
                    } else {
                        let items = harnesses.get();
                        if items.is_empty() {
                            view! { <p class="empty">"No harnesses installed"</p> }.into_view()
                        } else {
                            items
                                .into_iter()
                                .map(|h| {
                                    let is_selected = selected
                                        .as_ref()
                                        .map(|s| s.name == h.name)
                                        .unwrap_or(false);
                                    let h_clone = h.clone();
                                    
                                    view! {
                                        <div
                                            class={move || {
                                                if is_selected {
                                                    "harness-item harness-selected"
                                                } else {
                                                    "harness-item"
                                                }
                                            }}
                                            draggable="true"
                                            on:dragstart=move |e| {
                                                let data = serde_json::to_string(&h).unwrap_or_default();
                                                let _ = e.data_transfer().map(|dt| {
                                                    dt.set_data("harness", &data).ok()
                                                });
                                            }
                                            on:click=move |_| on_select.call(h_clone.clone())
                                        >
                                            <div class="harness-drag-handle">≡</div>
                                            <div class="harness-info">
                                                <span class="harness-name">{h.name.clone()}</span>
                                                <span class="harness-version">{h.version.clone()}</span>
                                                {if !h.description.is_empty() {
                                                    view! { <p class="harness-desc">{h.description.clone()}</p> }.into_view()
                                                } else {
                                                    view! { <span /> }.into_view()
                                                }}
                                            </div>
                                        </div>
                                    }
                                })
                                .collect_view()
                        }
                    }
                }}
            </div>
        </div>
        
        <style>
            "
            .harness-palette {
                background-color: #16213e;
                border-radius: 8px;
                padding: 1.5rem;
                width: 300px;
                max-height: 80vh;
                overflow-y: auto;
            }
            .harness-palette h2 {
                margin: 0 0 1rem 0;
                color: #e94560;
                font-size: 1.2rem;
            }
            .harness-list {
                display: flex;
                flex-direction: column;
                gap: 0.5rem;
            }
            .harness-item {
                display: flex;
                align-items: center;
                gap: 0.75rem;
                padding: 0.75rem;
                background-color: #1a1a2e;
                border: 1px solid #0f3460;
                border-radius: 4px;
                cursor: pointer;
                transition: all 0.2s;
            }
            .harness-item:hover {
                border-color: #e94560;
            }
            .harness-selected {
                border-color: #e94560;
                background-color: rgba(233, 69, 96, 0.1);
            }
            .harness-drag-handle {
                color: #666;
                cursor: grab;
                user-select: none;
            }
            .harness-info {
                flex: 1;
                min-width: 0;
            }
            .harness-name {
                font-weight: 600;
                color: #e0e0e0;
                display: block;
            }
            .harness-version {
                font-size: 0.8rem;
                color: #666;
                display: block;
            }
            .harness-desc {
                margin: 0.25rem 0 0 0;
                font-size: 0.85rem;
                color: #888;
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
            }
            .loading, .error, .empty {
                text-align: center;
                color: #888;
                padding: 2rem;
            }
            .error {
                color: #e94560;
            }
            "
        </style>
    }
}
