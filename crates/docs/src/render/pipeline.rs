// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Build pipeline and provenance sections.

use gmeow_docs_model::slug::provenance_chain;

use crate::model::{DocTerm, DocsModel};

use super::{
    Page, blank, code_escape, curie_link, heading, line, push_line, rel, root_href, to_curie,
};

/// The build-pipeline DAG page: the deterministic SVG plus a stage table (impl,
/// capabilities, resources, consumes count) and the plan's goal + success mode.
/// A pure render of `model.pipeline` (the source-lane build graph).
pub(super) fn md_pipeline_dag(model: &DocsModel) -> String {
    let from = Page::PipelineDag.dir();
    let mut out = String::new();
    heading(&mut out, 1, model.ui("body_build_pipeline"));
    let Some(pipeline) = &model.pipeline else {
        line(&mut out, model.ui("body_no_pipeline"));
        return out;
    };
    line(
        &mut out,
        &format!(
            "The dogfooded GMEOW build DAG, authored as data in \
             `slices/core/pipeline/module.ttl` and read back by the `gmeow-pipeline` \
             executor: **{}** `gmeow:PipelineStage` node(s) wired by **{}** dataflow \
             edge(s). Exactly one stage holds `gmeow:sinkCapability` — the single \
             serialization exit (the gts narrow waist), highlighted in the diagram.",
            pipeline.stages.len(),
            pipeline.edges.len(),
        ),
    );

    // The plan facets (a `gmeow:Pipeline` IS a `logic:Plan`).
    if pipeline.goal.is_some() || pipeline.success_mode.is_some() {
        heading(&mut out, 2, model.ui("body_goal"));
        if let Some(goal) = &pipeline.goal {
            push_line(
                &mut out,
                &format!(
                    "- **{}:** {}",
                    model.ui("body_goal"),
                    curie_link(model, &from, goal)
                ),
            );
        }
        if let Some(mode) = &pipeline.success_mode {
            push_line(
                &mut out,
                &format!(
                    "- **{}:** `{}`",
                    model.ui("body_pipeline_success_mode"),
                    code_escape(mode)
                ),
            );
        }
        blank(&mut out);
    }

    // The deterministic DAG SVG (emitted in `render_site_lang_exec`).
    heading(&mut out, 2, model.ui("body_pipeline_diagram"));
    push_line(
        &mut out,
        &format!(
            "![Build pipeline DAG]({}diagrams/pipeline.svg)",
            root_href(&from)
        ),
    );
    blank(&mut out);

    // The stage table.
    heading(&mut out, 2, model.ui("body_pipeline_stages"));
    push_line(
        &mut out,
        &format!(
            "| Stage | {} | {} | {} | {} |",
            model.ui("body_pipeline_implementation"),
            model.ui("body_pipeline_capabilities"),
            model.ui("body_pipeline_consumes"),
            model.ui("body_box_role"),
        ),
    );
    push_line(&mut out, "| --- | --- | --- | --- | --- |");
    for stage in &pipeline.stages {
        let name = curie_link(model, &from, &to_curie(&stage.iri));
        let impl_cell = match &stage.stage_impl {
            Some(module) => format!("`crates/pipeline/src/stages/{}.rs`", code_escape(module)),
            None => "—".to_string(),
        };
        let caps = pipeline_curie_cell(&stage.capabilities, &stage.resources);
        let box_role = stage
            .box_role
            .as_deref()
            .map(|r| format!("`{}`", code_escape(r)))
            .unwrap_or_else(|| "—".to_string());
        push_line(
            &mut out,
            &format!(
                "| {} | {} | {} | {} | {} |",
                name,
                impl_cell,
                caps,
                stage.consumes.len(),
                box_role,
            ),
        );
    }
    blank(&mut out);
    out
}

/// Render a stage's capability + resource CURIEs into one compact table cell
/// (each a code span), or an em-dash when the stage is a plain transform leaf.
fn pipeline_curie_cell(capabilities: &[String], resources: &[String]) -> String {
    let all: Vec<String> = capabilities
        .iter()
        .chain(resources.iter())
        .map(|c| format!("`{}`", code_escape(c)))
        .collect();
    if all.is_empty() {
        "—".to_string()
    } else {
        all.join(" ")
    }
}

/// Append the enriched build-pipeline stage section to a term page when the term
/// IS a `gmeow:PipelineStage` (its IRI matches a stage in `model.pipeline`). Renders
/// the Rust module binding, the consumes / consumed-by dataflow tables (from
/// `model.pipeline.edges`), the flowing named graphs (reified `gmeow:flowEntity`
/// where authored — honest absence otherwise), and the capabilities/resources.
///
/// No per-stage gate-verdict chip is rendered: `gmeow_errors::grade::GateVerdict`
/// is finding-scoped, and no genuine substrate attributes findings to one of the
/// 36 pipeline STAGE IRIs, so a per-stage verdict would be fabricated. Per the
/// proof-carrying doctrine, the honest static facts are rendered and the chip is
/// omitted (an honest computed-absence, not a scope gap).
pub(super) fn append_stage_section(
    out: &mut String,
    model: &DocsModel,
    term: &DocTerm,
    from: &str,
) {
    let Some(pipeline) = &model.pipeline else {
        return;
    };
    let Some(stage) = pipeline.stages.iter().find(|s| s.iri == term.iri) else {
        return;
    };

    heading(out, 2, model.ui("body_pipeline_stage"));
    line(
        out,
        &format!(
            "This term is a stage of the [build pipeline]({}index.md) — a typed unit of \
             build work the `gmeow-pipeline` executor runs single-pass over the in-memory \
             dataset.",
            rel(from, &Page::PipelineDag.dir()),
        ),
    );

    if let Some(module) = &stage.stage_impl {
        push_line(
            out,
            &format!(
                "- **{}:** `crates/pipeline/src/stages/{}.rs`",
                model.ui("body_pipeline_implementation"),
                code_escape(module)
            ),
        );
    }
    if !stage.capabilities.is_empty() || !stage.resources.is_empty() {
        push_line(
            out,
            &format!(
                "- **{}:** {}",
                model.ui("body_pipeline_capabilities"),
                pipeline_curie_cell(&stage.capabilities, &stage.resources)
            ),
        );
    }
    if let Some(role) = &stage.box_role {
        push_line(
            out,
            &format!(
                "- **{}:** `{}`",
                model.ui("body_box_role"),
                code_escape(role)
            ),
        );
    }
    blank(out);

    // Consumes: the upstream producer stages this stage reads (each a term page).
    if !stage.consumes.is_empty() {
        heading(out, 3, model.ui("body_pipeline_consumes"));
        for producer in &stage.consumes {
            push_line(
                out,
                &format!("- {}", curie_link(model, from, &to_curie(producer))),
            );
        }
        blank(out);
    }

    // Attaches: the named graphs / blob-rep lanes this stage contributes to the carrier
    // as its delta (gmeow:attachesGraph / gmeow:attachesBlobRep) — the run-verified
    // declaration of what the stage produced, so `gmeow docs-on <stage>` self-explains.
    if !stage.attaches_graphs.is_empty() || !stage.attaches_blob_reps.is_empty() {
        heading(out, 3, model.ui("body_pipeline_attaches"));
        for graph in &stage.attaches_graphs {
            push_line(out, &format!("- `{}`", code_escape(graph)));
        }
        for rep in &stage.attaches_blob_reps {
            push_line(
                out,
                &format!(
                    "- {} `{}`",
                    model.ui("body_pipeline_attaches_blob"),
                    code_escape(rep)
                ),
            );
        }
        blank(out);
    }

    // Consumed by: the downstream stages that read THIS stage's product (the edge
    // reverse — `edges.from == this stage`).
    let mut consumed_by: Vec<&str> = pipeline
        .edges
        .iter()
        .filter(|e| e.from == stage.iri)
        .map(|e| e.to.as_str())
        .collect();
    consumed_by.sort_unstable();
    consumed_by.dedup();
    if !consumed_by.is_empty() {
        heading(out, 3, model.ui("body_pipeline_consumed_by"));
        for consumer in consumed_by {
            push_line(
                out,
                &format!("- {}", curie_link(model, from, &to_curie(consumer))),
            );
        }
        blank(out);
    }

    // Flowing graphs: the reified `gmeow:flowEntity` named graphs on any edge
    // touching this stage (sorted/deduped). Absent unless a `gmeow:BuildDataFlow`
    // authors them — honest computed-absence, so the heading is emitted only when
    // at least one flowing graph exists.
    let mut flowing: Vec<&str> = pipeline
        .edges
        .iter()
        .filter(|e| e.from == stage.iri || e.to == stage.iri)
        .flat_map(|e| e.flow_entities.iter().map(String::as_str))
        .collect();
    flowing.sort_unstable();
    flowing.dedup();
    if !flowing.is_empty() {
        heading(out, 3, model.ui("body_pipeline_flowing_graphs"));
        for graph in flowing {
            push_line(out, &format!("- `{}`", code_escape(graph)));
        }
        blank(out);
    }
}

/// Append the per-page provenance footer: the producing-stage chain, the
/// build-grain projection of the single `gmeow:docGroundedBy` provenance relation.
/// A no-op when the model carries no pipeline (a bare unit-test model) — honest
/// absence, so the source-model goldens without a pipeline are unaffected.
pub(super) fn append_provenance_footer(out: &mut String, model: &DocsModel) {
    let Some(pipeline) = &model.pipeline else {
        return;
    };
    let chain = provenance_chain(pipeline, "stage-docs-render");
    if chain.is_empty() {
        return;
    }
    let rendered = chain
        .iter()
        .map(|s| format!("`{}`", code_escape(s)))
        .collect::<Vec<_>>()
        .join(" ← ");
    blank(out);
    push_line(out, "---");
    blank(out);
    push_line(
        out,
        &format!(
            "**{}:** this page ← {}",
            model.ui("body_provenance"),
            rendered
        ),
    );
    push_line(
        out,
        "Rendered from `gmeow.gts` by the dogfooded build pipeline.",
    );
    blank(out);
}
