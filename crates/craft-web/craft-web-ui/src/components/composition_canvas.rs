//! Drag-and-drop harness composition canvas.

use crate::api::{ApiClient, CompositionPlan, HarnessInfo};
use crate::components::HarnessPalette;
use leptos::*;

fn harness_names(harnesses: &[HarnessInfo]) -> Vec<String> {
    harnesses
        .iter()
        .map(|harness| harness.name.clone())
        .collect()
}

fn has_harness(harnesses: &[HarnessInfo], name: &str) -> bool {
    harnesses.iter().any(|harness| harness.name == name)
}

/// CompositionCanvas lets operators build a draft harness composition.
#[component]
pub fn CompositionCanvas() -> impl IntoView {
    let (draft, set_draft) = create_signal::<Vec<HarnessInfo>>(vec![]);
    let (selected, set_selected) = create_signal::<Option<HarnessInfo>>(None);
    let (strategy, set_strategy) = create_signal("ordered-merge".to_string());
    let (plan, set_plan) = create_signal::<Option<CompositionPlan>>(None);
    let (loading, set_loading) = create_signal(false);
    let (error, set_error) = create_signal::<Option<String>>(None);
    let (notice, set_notice) = create_signal::<Option<String>>(None);
    let (drag_active, set_drag_active) = create_signal(false);

    let add_harness = move |harness: HarnessInfo| {
        set_notice.set(None);
        set_error.set(None);
        set_plan.set(None);

        set_draft.update(|draft| {
            if has_harness(draft, &harness.name) {
                set_notice.set(Some(format!("{} is already in the draft.", harness.name)));
            } else {
                draft.push(harness);
            }
        });
    };

    let on_select = Callback::new(move |harness: HarnessInfo| {
        set_selected.set(Some(harness.clone()));
        add_harness(harness);
    });

    let remove_harness = move |name: String| {
        set_draft.update(|draft| draft.retain(|harness| harness.name != name));
        set_plan.set(None);
        set_notice.set(None);
        set_error.set(None);
    };

    let move_harness = move |name: String, direction: i32| {
        set_draft.update(|draft| {
            if let Some(index) = draft.iter().position(|harness| harness.name == name) {
                let next = index as i32 + direction;
                if next >= 0 && (next as usize) < draft.len() {
                    draft.swap(index, next as usize);
                }
            }
        });
        set_plan.set(None);
    };

    let clear = move |_| {
        set_draft.set(vec![]);
        set_selected.set(None);
        set_plan.set(None);
        set_notice.set(None);
        set_error.set(None);
    };

    let plan_composition = move |_| {
        let names = harness_names(&draft.get());
        if names.is_empty() {
            set_error.set(Some(
                "Add at least one harness before planning.".to_string(),
            ));
            return;
        }

        let selected_strategy = strategy.get();
        spawn_local(async move {
            set_loading.set(true);
            set_error.set(None);
            set_notice.set(None);

            let client = use_context::<ApiClient>().expect("ApiClient not provided");
            match client.compose_plan(names, &selected_strategy).await {
                Ok(result) => {
                    set_plan.set(Some(result));
                    set_notice.set(Some("Composition plan ready.".to_string()));
                }
                Err(err) => {
                    set_plan.set(None);
                    set_error.set(Some(err));
                }
            }

            set_loading.set(false);
        });
    };

    let validate = move |_| {
        let names = harness_names(&draft.get());
        if names.is_empty() {
            set_error.set(Some(
                "Add at least one harness before validating.".to_string(),
            ));
            set_notice.set(None);
            return;
        }

        let unique_count = names
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len();

        if unique_count != names.len() {
            set_error.set(Some("Draft contains duplicate harnesses.".to_string()));
            set_notice.set(None);
        } else {
            set_error.set(None);
            set_notice.set(Some(format!(
                "{} harness{} ready for planning.",
                names.len(),
                if names.len() == 1 { "" } else { "es" }
            )));
        }
    };

    view! {
        <section class="composition-workspace">
            <HarnessPalette on_select=on_select selected=selected.get() />

            <div class="composer-panel">
                <div class="composer-toolbar">
                    <div>
                        <h2>"Composition Draft"</h2>
                        <p>{move || {
                            let count = draft.get().len();
                            format!("{} harness{} selected", count, if count == 1 { "" } else { "es" })
                        }}</p>
                    </div>

                    <div class="toolbar-actions">
                        <select prop:value=strategy.get() on:change=move |ev| {
                            set_strategy.set(event_target_value(&ev));
                            set_plan.set(None);
                        }>
                            <option value="ordered-merge">"Ordered merge"</option>
                            <option value="memory-aware">"Memory aware"</option>
                            <option value="strict">"Strict"</option>
                        </select>
                        <button class="btn-secondary" on:click=validate>"Validate"</button>
                        <button
                            class="btn-primary"
                            disabled=move || loading.get() || draft.get().is_empty()
                            on:click=plan_composition
                        >
                            {move || if loading.get() { "Planning..." } else { "Plan Composition" }}
                        </button>
                        <button class="btn-danger" on:click=clear>"Clear"</button>
                    </div>
                </div>

                <div
                    class=move || if drag_active.get() {
                        "composition-dropzone drop-active"
                    } else {
                        "composition-dropzone"
                    }
                    on:dragover=move |ev| {
                        ev.prevent_default();
                        set_drag_active.set(true);
                    }
                    on:dragleave=move |_| set_drag_active.set(false)
                    on:drop=move |ev| {
                        ev.prevent_default();
                        set_drag_active.set(false);

                        if let Some(data_transfer) = ev.data_transfer() {
                            match data_transfer.get_data("harness") {
                                Ok(payload) if !payload.is_empty() => {
                                    match serde_json::from_str::<HarnessInfo>(&payload) {
                                        Ok(harness) => add_harness(harness),
                                        Err(err) => set_error.set(Some(format!("Invalid harness payload: {}", err))),
                                    }
                                }
                                _ => set_error.set(Some("Drop did not include harness data.".to_string())),
                            }
                        }
                    }
                >
                    {move || {
                        let items = draft.get();
                        if items.is_empty() {
                            view! {
                                <div class="empty-canvas">
                                    <h3>"Drop harnesses here"</h3>
                                    <p>"Drag from the palette or click a harness to add it to the draft."</p>
                                </div>
                            }.into_view()
                        } else {
                            view! {
                                <div class="draft-nodes">
                                    {items.into_iter().enumerate().map(|(index, harness)| {
                                        let name_for_remove = harness.name.clone();
                                        let name_for_up = harness.name.clone();
                                        let name_for_down = harness.name.clone();

                                        view! {
                                            <article class="draft-node">
                                                <div class="node-index">{index + 1}</div>
                                                <div class="node-body">
                                                    <div class="node-title">
                                                        <strong>{harness.name}</strong>
                                                        <span>"v" {harness.version}</span>
                                                    </div>
                                                    <p>{if harness.description.is_empty() {
                                                        "No description available".to_string()
                                                    } else {
                                                        harness.description
                                                    }}</p>
                                                    <div class="node-meta">
                                                        <span>{harness.source}</span>
                                                    </div>
                                                </div>
                                                <div class="node-actions">
                                                    <button
                                                        title="Move earlier"
                                                        disabled=index == 0
                                                        on:click=move |_| move_harness(name_for_up.clone(), -1)
                                                    >"↑"</button>
                                                    <button
                                                        title="Move later"
                                                        disabled=index + 1 >= draft.get().len()
                                                        on:click=move |_| move_harness(name_for_down.clone(), 1)
                                                    >"↓"</button>
                                                    <button
                                                        title="Remove"
                                                        on:click=move |_| remove_harness(name_for_remove.clone())
                                                    >"×"</button>
                                                </div>
                                            </article>
                                        }
                                    }).collect_view()}
                                </div>
                            }.into_view()
                        }
                    }}
                </div>

                <div class="composer-feedback">
                    {move || error.get().map(|err| view! { <div class="feedback error">{err}</div> })}
                    {move || notice.get().map(|message| view! { <div class="feedback notice">{message}</div> })}
                </div>

                <div class="composition-details">
                    <section class="draft-list">
                        <h3>"Draft Harness Names"</h3>
                        {move || {
                            let names = harness_names(&draft.get());
                            if names.is_empty() {
                                view! { <p class="muted">"No harnesses selected."</p> }.into_view()
                            } else {
                                view! {
                                    <ol>
                                        {names.into_iter().map(|name| view! { <li>{name}</li> }).collect_view()}
                                    </ol>
                                }.into_view()
                            }
                        }}
                    </section>

                    <section class="plan-preview">
                        <h3>"Plan Preview"</h3>
                        {move || {
                            if let Some(plan) = plan.get() {
                                view! {
                                    <div>
                                        <div class="plan-summary">
                                            <span>{plan.strategy}</span>
                                            <span>{format!("{} harnesses", plan.harnesses.len())}</span>
                                            <span>{format!("{} warnings", plan.warnings.len())}</span>
                                        </div>
                                        <div class="plan-harnesses">
                                            {plan.harnesses.into_iter().map(|harness| view! {
                                                <div class="plan-row">
                                                    <strong>{harness.name}</strong>
                                                    <span>"v" {harness.version}</span>
                                                    <span>{harness.source}</span>
                                                </div>
                                            }).collect_view()}
                                        </div>
                                        {if plan.warnings.is_empty() {
                                            view! { <p class="muted">"No warnings returned."</p> }.into_view()
                                        } else {
                                            view! {
                                                <ul class="warnings">
                                                    {plan.warnings.into_iter().map(|warning| {
                                                        view! { <li>{warning}</li> }
                                                    }).collect_view()}
                                                </ul>
                                            }.into_view()
                                        }}
                                    </div>
                                }.into_view()
                            } else {
                                view! { <p class="muted">"Run Plan Composition to preview backend ordering and warnings."</p> }.into_view()
                            }
                        }}
                    </section>
                </div>
            </div>
        </section>

        <style>
            "
            .composition-workspace {
                align-items: flex-start;
                display: flex;
                gap: 1rem;
                margin: 0 auto;
                max-width: 1400px;
                padding: 0 1.5rem 1.5rem;
            }
            .composer-panel {
                background: #16213e;
                border: 1px solid #25324f;
                border-radius: 8px;
                flex: 1;
                min-width: 0;
                padding: 1rem;
            }
            .composer-toolbar {
                align-items: flex-start;
                display: flex;
                gap: 1rem;
                justify-content: space-between;
                margin-bottom: 1rem;
            }
            .composer-toolbar h2 {
                color: #f05d5e;
                font-size: 1rem;
                margin: 0;
            }
            .composer-toolbar p {
                color: #8ea0b8;
                font-size: 0.85rem;
                margin: 0.25rem 0 0;
            }
            .toolbar-actions {
                display: flex;
                flex-wrap: wrap;
                gap: 0.5rem;
                justify-content: flex-end;
            }
            .toolbar-actions button,
            .toolbar-actions select {
                background: #0d1b2a;
                border: 1px solid #25324f;
                border-radius: 4px;
                color: #e0e6ed;
                cursor: pointer;
                font-size: 0.85rem;
                min-height: 2.25rem;
                padding: 0.45rem 0.7rem;
            }
            .toolbar-actions button:disabled {
                cursor: not-allowed;
                opacity: 0.55;
            }
            .toolbar-actions .btn-primary {
                background: #f05d5e;
                border-color: #f05d5e;
                color: #101624;
                font-weight: 700;
            }
            .toolbar-actions .btn-secondary {
                background: #0f3460;
                border-color: #1b548b;
            }
            .toolbar-actions .btn-danger {
                color: #ffb4b8;
            }
            .composition-dropzone {
                background: #0d1b2a;
                border: 1px dashed #33415f;
                border-radius: 8px;
                min-height: 360px;
                padding: 1rem;
                transition: background-color 0.15s, border-color 0.15s;
            }
            .composition-dropzone.drop-active {
                background: #132342;
                border-color: #f05d5e;
            }
            .empty-canvas {
                align-items: center;
                color: #8ea0b8;
                display: flex;
                flex-direction: column;
                justify-content: center;
                min-height: 320px;
                text-align: center;
            }
            .empty-canvas h3 {
                color: #e0e6ed;
                margin: 0 0 0.25rem;
            }
            .empty-canvas p {
                margin: 0;
            }
            .draft-nodes {
                display: grid;
                gap: 0.75rem;
            }
            .draft-node {
                align-items: center;
                background: #111b30;
                border: 1px solid #25324f;
                border-radius: 6px;
                display: grid;
                gap: 0.75rem;
                grid-template-columns: 2rem minmax(0, 1fr) auto;
                padding: 0.75rem;
            }
            .node-index {
                align-items: center;
                background: #24385e;
                border-radius: 999px;
                color: #f3f6fa;
                display: flex;
                font-weight: 700;
                height: 2rem;
                justify-content: center;
                width: 2rem;
            }
            .node-body {
                min-width: 0;
            }
            .node-title {
                align-items: baseline;
                display: flex;
                gap: 0.5rem;
            }
            .node-title strong {
                color: #f3f6fa;
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
            }
            .node-title span,
            .node-meta {
                color: #8ea0b8;
                font-size: 0.78rem;
            }
            .node-body p {
                color: #aab6c5;
                font-size: 0.83rem;
                line-height: 1.35;
                margin: 0.25rem 0;
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
            }
            .node-actions {
                display: flex;
                gap: 0.35rem;
            }
            .node-actions button {
                background: #0d1b2a;
                border: 1px solid #25324f;
                border-radius: 4px;
                color: #e0e6ed;
                cursor: pointer;
                height: 2rem;
                width: 2rem;
            }
            .node-actions button:disabled {
                cursor: not-allowed;
                opacity: 0.45;
            }
            .composer-feedback {
                display: grid;
                gap: 0.5rem;
                margin-top: 0.75rem;
            }
            .feedback {
                border-radius: 4px;
                font-size: 0.85rem;
                padding: 0.65rem 0.75rem;
            }
            .feedback.error {
                background: rgba(240, 93, 94, 0.12);
                border: 1px solid rgba(240, 93, 94, 0.35);
                color: #ffb4b8;
            }
            .feedback.notice {
                background: rgba(81, 177, 120, 0.12);
                border: 1px solid rgba(81, 177, 120, 0.35);
                color: #9ee2b6;
            }
            .composition-details {
                display: grid;
                gap: 1rem;
                grid-template-columns: minmax(220px, 0.8fr) minmax(0, 1.2fr);
                margin-top: 1rem;
            }
            .draft-list,
            .plan-preview {
                background: #111b30;
                border: 1px solid #25324f;
                border-radius: 6px;
                padding: 1rem;
            }
            .draft-list h3,
            .plan-preview h3 {
                color: #e0e6ed;
                font-size: 0.92rem;
                margin: 0 0 0.75rem;
            }
            .draft-list ol,
            .warnings {
                color: #c9d1dc;
                margin: 0;
                padding-left: 1.25rem;
            }
            .draft-list li,
            .warnings li {
                margin: 0.35rem 0;
            }
            .muted {
                color: #8ea0b8;
                font-size: 0.85rem;
                margin: 0;
            }
            .plan-summary {
                display: flex;
                flex-wrap: wrap;
                gap: 0.5rem;
                margin-bottom: 0.75rem;
            }
            .plan-summary span {
                background: #0d1b2a;
                border: 1px solid #25324f;
                border-radius: 999px;
                color: #c9d1dc;
                font-size: 0.75rem;
                padding: 0.2rem 0.55rem;
            }
            .plan-harnesses {
                display: grid;
                gap: 0.35rem;
                margin-bottom: 0.75rem;
            }
            .plan-row {
                align-items: center;
                color: #aab6c5;
                display: grid;
                font-size: 0.82rem;
                gap: 0.5rem;
                grid-template-columns: minmax(0, 1fr) auto auto;
            }
            .plan-row strong {
                color: #f3f6fa;
                overflow: hidden;
                text-overflow: ellipsis;
                white-space: nowrap;
            }
            @media (max-width: 980px) {
                .composition-workspace {
                    flex-direction: column;
                }
                .harness-palette {
                    max-height: none;
                    width: 100%;
                }
                .composer-panel {
                    width: 100%;
                }
                .composer-toolbar {
                    flex-direction: column;
                }
                .toolbar-actions {
                    justify-content: flex-start;
                }
                .composition-details {
                    grid-template-columns: 1fr;
                }
            }
            @media (max-width: 640px) {
                .composition-workspace {
                    padding: 0 1rem 1rem;
                }
                .draft-node {
                    align-items: flex-start;
                    grid-template-columns: 2rem minmax(0, 1fr);
                }
                .node-actions {
                    grid-column: 2;
                }
                .toolbar-actions button,
                .toolbar-actions select {
                    flex: 1 1 10rem;
                }
            }
            "
        </style>
    }
}
