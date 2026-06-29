//! Harness palette component with search, filters, and drag handles.

use crate::api::{ApiClient, HarnessInfo};
use leptos::*;

fn matches_harness(harness: &HarnessInfo, query: &str, source_filter: &str) -> bool {
    let q = query.trim().to_lowercase();
    let matches_query = q.is_empty()
        || harness.name.to_lowercase().contains(&q)
        || harness.description.to_lowercase().contains(&q)
        || harness.source.to_lowercase().contains(&q)
        || harness
            .authors
            .iter()
            .any(|author| author.to_lowercase().contains(&q));

    let matches_source =
        source_filter == "all" || harness.source.to_lowercase().contains(source_filter);

    matches_query && matches_source
}

/// HarnessPalette displays installed harnesses with drag handles.
#[component]
pub fn HarnessPalette(
    #[prop(into)] on_select: Callback<HarnessInfo>,
    #[prop(optional)] selected: Option<HarnessInfo>,
) -> impl IntoView {
    let (harnesses, set_harnesses) = create_signal::<Vec<HarnessInfo>>(vec![]);
    let (query, set_query) = create_signal(String::new());
    let (source_filter, set_source_filter) = create_signal("all".to_string());
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);

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

    let filtered = move || {
        let q = query.get();
        let source = source_filter.get();
        harnesses
            .get()
            .into_iter()
            .filter(|h| matches_harness(h, &q, &source))
            .collect::<Vec<_>>()
    };

    view! {
        <aside class="harness-palette">
            <div class="palette-header">
                <h2>"Harnesses"</h2>
                <span class="palette-count">{move || filtered().len()}</span>
            </div>

            <div class="palette-controls">
                <input
                    prop:value=query.get()
                    on:input=move |ev| set_query.set(event_target_value(&ev))
                    placeholder="Search name, source, author"
                    type="search"
                />
                <select prop:value=source_filter.get() on:change=move |ev| {
                    set_source_filter.set(event_target_value(&ev));
                }>
                    <option value="all">"All sources"</option>
                    <option value="github">"GitHub"</option>
                    <option value="local">"Local"</option>
                    <option value="registry">"Registry"</option>
                </select>
            </div>

            <div class="harness-list">
                {move || {
                    if loading.get() {
                        view! { <p class="loading">"Loading harnesses..."</p> }.into_view()
                    } else if let Some(err) = error.get() {
                        view! { <p class="error">{err}</p> }.into_view()
                    } else {
                        let items = filtered();
                        if items.is_empty() {
                            view! { <p class="empty">"No harnesses match the current filters."</p> }.into_view()
                        } else {
                            items
                                .into_iter()
                                .map(|harness| {
                                    let is_selected = selected
                                        .as_ref()
                                        .map(|s| s.name == harness.name)
                                        .unwrap_or(false);
                                    let select_harness = harness.clone();
                                    let drag_harness = harness.clone();
                                    let authors = if harness.authors.is_empty() {
                                        "Unknown author".to_string()
                                    } else {
                                        harness.authors.join(", ")
                                    };

                                    view! {
                                        <article
                                            class={if is_selected {
                                                "harness-item harness-selected"
                                            } else {
                                                "harness-item"
                                            }}
                                            draggable="true"
                                            on:dragstart=move |e| {
                                                let data = serde_json::to_string(&drag_harness).unwrap_or_default();
                                                let _ = e.data_transfer().map(|dt| {
                                                    dt.set_data("harness", &data).ok();
                                                    dt.set_effect_allowed("copy");
                                                });
                                            }
                                            on:click=move |_| on_select.call(select_harness.clone())
                                        >
                                            <div class="harness-drag-handle" title="Drag to composer">"::"</div>
                                            <div class="harness-info">
                                                <div class="harness-title">
                                                    <span class="harness-name">{harness.name}</span>
                                                    <span class="harness-version">"v" {harness.version}</span>
                                                </div>
                                                <p class="harness-desc">
                                                    {if harness.description.is_empty() {
                                                        "No description available".to_string()
                                                    } else {
                                                        harness.description
                                                    }}
                                                </p>
                                                <div class="harness-meta">
                                                    <span>{authors}</span>
                                                    <span>{harness.source}</span>
                                                </div>
                                            </div>
                                        </article>
                                    }
                                })
                                .collect_view()
                        }
                    }
                }}
            </div>
        </aside>

        <style>
            "
            .harness-palette {
                background: #16213e;
                border: 1px solid #25324f;
                border-radius: 8px;
                padding: 1rem;
                width: 320px;
                max-height: 80vh;
                overflow-y: auto;
            }
            .palette-header {
                display: flex;
                align-items: center;
                justify-content: space-between;
                margin-bottom: 0.75rem;
            }
            .palette-header h2 {
                margin: 0;
                color: #f05d5e;
                font-size: 1rem;
            }
            .palette-count {
                background: #0d1b2a;
                border: 1px solid #25324f;
                border-radius: 999px;
                color: #c9d1dc;
                font-size: 0.75rem;
                min-width: 2rem;
                padding: 0.15rem 0.5rem;
                text-align: center;
            }
            .palette-controls {
                display: grid;
                gap: 0.5rem;
                margin-bottom: 0.75rem;
            }
            .palette-controls input,
            .palette-controls select {
                background: #0d1b2a;
                border: 1px solid #25324f;
                border-radius: 4px;
                color: #e0e6ed;
                font-size: 0.9rem;
                padding: 0.55rem 0.65rem;
                width: 100%;
            }
            .harness-list {
                display: flex;
                flex-direction: column;
                gap: 0.5rem;
            }
            .harness-item {
                display: flex;
                gap: 0.75rem;
                padding: 0.75rem;
                background: #111b30;
                border: 1px solid #25324f;
                border-radius: 6px;
                cursor: pointer;
                transition: border-color 0.15s, background-color 0.15s;
            }
            .harness-item:hover,
            .harness-selected {
                border-color: #f05d5e;
                background: #1b2540;
            }
            .harness-drag-handle {
                color: #7f8ca3;
                cursor: grab;
                font-weight: 700;
                line-height: 1.5;
                user-select: none;
            }
            .harness-info {
                min-width: 0;
                flex: 1;
            }
            .harness-title {
                align-items: baseline;
                display: flex;
                gap: 0.5rem;
                justify-content: space-between;
            }
            .harness-name {
                color: #f3f6fa;
                display: block;
                font-weight: 650;
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
            }
            .harness-version {
                color: #8ea0b8;
                flex: none;
                font-size: 0.75rem;
            }
            .harness-desc {
                color: #aab6c5;
                font-size: 0.82rem;
                line-height: 1.35;
                margin: 0.25rem 0 0;
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
            }
            .harness-meta {
                color: #7f8ca3;
                display: flex;
                flex-wrap: wrap;
                gap: 0.4rem 0.65rem;
                font-size: 0.74rem;
                margin-top: 0.45rem;
            }
            .loading, .error, .empty {
                color: #8ea0b8;
                padding: 1.5rem 0.5rem;
                text-align: center;
            }
            .error {
                color: #f05d5e;
            }
            "
        </style>
    }
}
