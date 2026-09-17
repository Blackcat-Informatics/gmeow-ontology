// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `docs_render` stage: the typed documentation model as data.
//!
//! Pure WIRING of the Rust docs crate — no port. It discovers the
//! `gmeow_docs::DocsModel` from the slice catalog and projects it to the
//! self-hosting documentation named graph via `gmeow_docs::to_gmeow_rdf` — the
//! exact N-Quads the Python `DocSet.to_gmeow_rdf()` folds into `gmeow.gts`. The
//! rendered HTML/Markdown site blobs (`render_site`) are folded by `gts_sink`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use gmeow_docs::model::{
    ConstraintRule, DiagnosticsDigest, DocDiagFinding, DocsModel, ReasoningVerdict,
};
use gmeow_docs::rdf::to_gmeow_rdf;
use purrdf::RdfTerm;

use crate::node::{CachePolicy, Stage, StageInput, StageOutput, StageProduct};

/// Logical path of the documentation named graph (N-Quads, in-memory dataflow).
pub const DOCS_GRAPH_PATH: &str = "pipeline/documentation.nq";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Select the declared in-DAG reasoning owner. Post-DAG documentation explicitly
/// selects its retained snapshot and uses the same native admission below.
pub(crate) fn reasoning_verdict_from_reason(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<ReasoningVerdict, gmeow_errors::Diag> {
    let reason = upstream.get("stage-reason").ok_or_else(|| {
        reasoning_verdict_error("missing stage-reason product for the reasoning verdict".into())
    })?;
    reasoning_verdict_from_product(reason, "stage-reason")
}

/// Borrow the docs verdict from an explicitly selected, retained native owner.
///
/// Both documentation surfaces share the complete inferred payload and the DL
/// reader's role-sensitive empty-class interpretation. The native information
/// axis decides consistency; an empty class alone is not a witnessed clash.
/// Missing identities, wrong payloads and inconclusive results refuse instead of
/// becoming a fabricated positive verdict. Native hashing verifies the immutable
/// publication contract without rendering, parsing or reasoning over RDF.
pub(crate) fn reasoning_verdict_from_product(
    reason: &StageProduct,
    expected_owner: &str,
) -> Result<ReasoningVerdict, gmeow_errors::Diag> {
    use gmeow_logic::result::{
        CompletenessStatus, EvaluationStatus, InformationState, InputStatus, ResultPayload,
    };
    use gmeow_logic::result_rdf::GRAPH_REASONING;

    let fail = reasoning_verdict_error;
    if reason.stage_id != expected_owner || reason.carrier_released {
        return Err(fail(format!(
            "the reasoning verdict requires the retained {expected_owner} product"
        )));
    }
    let bundle = reason.bundle();
    let entry = bundle
        .handle(GRAPH_REASONING)
        .ok_or_else(|| fail("missing pinned Reasoning handle for the reasoning verdict".into()))?;
    let crate::bundle::PipelineHandle::Reasoning(result) = &entry.payload else {
        return Err(fail(
            "graph/reasoning must carry the Reasoning handle arm".into(),
        ));
    };
    if !crate::handle_identity::contains_graph(bundle, GRAPH_REASONING)
        || entry.content_digest != bundle.graph_digest(GRAPH_REASONING)
    {
        return Err(fail(
            "the Reasoning handle does not match its declared graph pin".into(),
        ));
    }
    // Reuse the publication's native identity recipe, not a whole-bundle digest:
    // recomputing that digest would canonicalize every unrelated closure row.
    let published = reason
        .handle_commitments()
        .get(GRAPH_REASONING)
        .ok_or_else(|| fail("missing published native Reasoning commitment".into()))?;
    let current = crate::handle_identity::handle_commitment(
        GRAPH_REASONING,
        &entry.content_digest.to_hex(),
        &entry.payload,
    );
    if current.identity != published.identity || current.digest != published.digest {
        return Err(fail(
            "native Reasoning payload identity changed after publication".into(),
        ));
    }
    result.validate()?;
    let ResultPayload::Inferred(inferred) = &result.payload else {
        return Err(fail(
            "the reasoning verdict requires a complete native Inferred payload".into(),
        ));
    };
    if result.input != InputStatus::Valid
        || !result.is_conclusive()
        || result.evaluation == EvaluationStatus::Unsupported
    {
        return Err(fail(format!(
            "the reasoning verdict is not conclusive: input={}, evaluation={}",
            result.input.wire(),
            result.evaluation.wire()
        )));
    }
    let is_consistent = match result.information {
        InformationState::Supported
            if result.completeness == CompletenessStatus::CompleteForFragment
                && result.preservation.unsupported_constructs.is_empty() =>
        {
            true
        }
        InformationState::Both => false,
        _ => {
            return Err(fail(format!(
                "the native result does not decide consistency: information={}, completeness={}, unsupported={:?}",
                result.information.wire(),
                result.completeness.wire(),
                result.preservation.unsupported_constructs
            )));
        }
    };
    let unsatisfiable = gmeow_logic::reason::dl::unsatisfiable_from_inferred(inferred)
        .into_iter()
        .map(|empty| empty.class)
        .collect();
    Ok(ReasoningVerdict {
        is_consistent,
        unsatisfiable,
    })
}

/// Keep both documentation consumers' native admission failures explicit.
fn reasoning_verdict_error(message: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-docs-render".to_owned(),
        message,
    })
}

/// Project one full-fidelity `gmeow_errors::Finding` into a [`DocDiagFinding`],
/// resolving `help_uri` ONLY when `finding.code` exactly matches a
/// `ConstraintRule::code` in `by_code` (never a fabricated deep link — mirrors
/// `apply_fixture_catalog_slugs`'s honest-absence contract).
///
/// `category` defaults to `PolicyWarning`'s display spelling when the finding
/// carries no category (mirrors `diagnostics_reader::finding_gate`'s existing
/// non-blocking default) — an honest "uncategorized" rendering, never a hard
/// fail over a field genuinely absent on some findings.
fn doc_diag_finding(
    finding: &gmeow_errors::Finding,
    by_code: &BTreeMap<&str, &str>,
) -> DocDiagFinding {
    DocDiagFinding {
        code: finding.code.clone(),
        severity: finding.severity.as_str().to_string(),
        category: finding
            .category
            .unwrap_or(gmeow_errors::FindingCategory::PolicyWarning)
            .as_str()
            .to_string(),
        message: finding.message.clone(),
        slice_iri: finding.attributions.first().map(|a| a.slice_iri.clone()),
        help_uri: by_code
            .get(finding.code.as_str())
            .map(|uri| (*uri).to_string()),
    }
}

/// Fold exact documented-term and slice joins from the two authenticated native
/// diagnostic reports. These are the final normalized, meta-enriched reports
/// shared by each producer's terminal renderer, including every logical location,
/// standpoint, source attribution and labeled span omitted by the RDF projection.
/// Missing publications refuse; there is no artifact or run-ledger fallback.
pub(crate) fn diagnostics_digest_from_upstream(
    upstream: &BTreeMap<String, StageProduct>,
    known_term_iris: &BTreeSet<String>,
    constraint_rules: &[ConstraintRule],
) -> Result<DiagnosticsDigest, gmeow_errors::Diag> {
    let validate = upstream.get("stage-validate").ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-docs-render".to_string(),
            message: "missing stage-validate product for the diagnostics digest".to_string(),
        })
    })?;
    let compile_logic = upstream.get("stage-compile-logic").ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-docs-render".to_string(),
            message: "missing stage-compile-logic product for the diagnostics digest".to_string(),
        })
    })?;
    let shacl = crate::bundle::diagnostics_from_product(validate, "stage-validate")?;
    let compile = crate::bundle::diagnostics_from_product(compile_logic, "stage-compile-logic")?;
    Ok(diagnostics_digest_from_reports(
        shacl.report(crate::bundle::DiagnosticReportOwner::Validate)?,
        compile.report(crate::bundle::DiagnosticReportOwner::CompileLogic)?,
        known_term_iris,
        constraint_rules,
    ))
}

/// One borrowed fold for in-DAG producer publications and the retained snapshot.
/// Source order is SHACL then compiler; findings never merge across those owners.
/// The final docs rows are presentation projections of the complete native reports.
pub(crate) fn diagnostics_digest_from_reports(
    shacl_report: &gmeow_errors::Report,
    compile_report: &gmeow_errors::Report,
    known_term_iris: &BTreeSet<String>,
    constraint_rules: &[ConstraintRule],
) -> DiagnosticsDigest {
    let total = shacl_report.findings.len() + compile_report.findings.len();
    let findings = shacl_report.findings.iter().chain(&compile_report.findings);

    let by_code: BTreeMap<&str, &str> = constraint_rules
        .iter()
        .map(|r| (r.code.as_str(), r.help_uri.as_str()))
        .collect();

    let mut by_term: BTreeMap<String, Vec<DocDiagFinding>> = BTreeMap::new();
    let mut by_slice: BTreeMap<String, Vec<DocDiagFinding>> = BTreeMap::new();
    for finding in findings {
        let doc_finding = doc_diag_finding(finding, &by_code);

        // Primary join: the purpose-built documented-term attribution — a SHACL
        // violation's CONSTRAINED PROPERTY (its `sh:path`), a documented `gmeow:`
        // term — resolved by EXACT match against `known_term_iris`. This is the
        // honest carrier the finding-construction site records structurally
        // (`gmeow_validate::findings`), preferred over the raw focus node below
        // because the focus is a data individual that never names a documented term.
        for term_iri in &finding.documented_terms {
            if known_term_iris.contains(term_iri.as_str()) {
                by_term
                    .entry(term_iri.clone())
                    .or_default()
                    .push(doc_finding.clone());
            }
        }
        // Secondary join, retained for findings whose PRIMARY location genuinely
        // names a documented term (e.g. a modeling-discipline finding anchored on a
        // documented class): the first `logical` location matched exactly. A finding
        // whose focus is an ABox individual (every real SHACL finding today) has no
        // `by_term` entry from this leg — an honest absence, not a bug.
        let term_candidate = finding
            .locations
            .iter()
            .find_map(|loc| loc.logical.as_deref());
        if let Some(term_iri) = term_candidate
            && known_term_iris.contains(term_iri)
            && !finding.documented_terms.iter().any(|t| t == term_iri)
        {
            by_term
                .entry(term_iri.to_string())
                .or_default()
                .push(doc_finding.clone());
        }

        for attribution in &finding.attributions {
            by_slice
                .entry(attribution.slice_iri.clone())
                .or_default()
                .push(doc_finding.clone());
        }
    }

    DiagnosticsDigest {
        by_term,
        by_slice,
        total,
    }
}

/// The `rdfs:label` prefix the compiler's projection ledger stamps on a per-shape
/// row (`format!("property-path:{}", pp.shape_iri)` in
/// `logic_compile::projections::report::build_projection_report_from`). ONLY
/// labels carrying this EXACT prefix are per-term/per-shape; every other row
/// (`"owl-dl"`, `"datalog"`, `"shacl-json-schema"`, …) is a whole-program row
/// already rendered on the STATIC `Page::LogicLossLedger`
/// (`gmeow_logic_compile::projections::projection_ledger_rows`) and must never be
/// re-rendered per-term here.
const PROPERTY_PATH_LABEL_PREFIX: &str = "property-path:";

/// The `logic:` namespace the compiler's projection ledger mints its vocabulary
/// under (`crate::ir::LOGIC_NAMESPACE`, duplicated here as a literal so this reader
/// needs no dependency on `gmeow-logic-compile`'s internal IR module).
use gmeow_ns::LOGIC_NS;
const LOGIC_PROJECTION_TARGET_TYPE: &str = "https://blackcatinformatics.ca/logic/ProjectionTarget";
const LOGIC_PRESERVATION_KIND: &str = "https://blackcatinformatics.ca/logic/preservationKind";
const LOGIC_COMPLEXITY_CLASS: &str = "https://blackcatinformatics.ca/logic/complexityClass";
const GMEOW_LOSSY_DROP: &str = "https://blackcatinformatics.ca/gmeow/lossyDrop";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
/// The reified per-term projection-loss node type emitted by the projection-report
/// serializer for every actual drop that names a DOCUMENTED source term (the term
/// projected DOWN to a lossy surface). Carries `gmeow:lossySourceTerm` (the term IRI),
/// `rdfs:label` (the projection target name, e.g. `sssom:<hash>`), `logic:preservationKind`,
/// `logic:complexityClass`, and one `gmeow:lossyDrop` per dropped feature.
const LOGIC_TERM_PROJECTION_LOSS_TYPE: &str =
    "https://blackcatinformatics.ca/logic/TermProjectionLoss";
/// The structured source-term IRI a `logic:TermProjectionLoss` attributes its drops to —
/// matched (byte-identical) against a documented `DocTerm.iri`, never scraped from prose.
const GMEOW_LOSSY_SOURCE_TERM: &str = "https://blackcatinformatics.ca/gmeow/lossySourceTerm";

/// Fold the dynamic per-term projection-loss join [`TermLossDigest`] from the LIVE
/// `stage-mappings` product's [`GRAPH_PROJECTION_LEDGER`](crate::stages::carrier::GRAPH_PROJECTION_LEDGER)
/// named graph — the compiler's committed projection report, read off the
/// PRODUCER's already-parsed dataset via
/// [`producer_graph`](crate::stages::carrier::producer_graph) (PIPELINE_SPINE §4:
/// a pure keyed fold, never a re-run of the logic compiler / mappings stage).
/// Hard-fails when `stage-mappings` is absent from `upstream` (no-optionality).
///
/// Only `logic:ProjectionTarget` rows whose `rdfs:label` carries the
/// [`PROPERTY_PATH_LABEL_PREFIX`] are per-term candidates; the shape IRI is
/// recovered by stripping that prefix, then resolved to a documented term by, in
/// order: (a) an exact match against a [`DocShape::shape_iri`](
/// gmeow_docs::model::DocShape::shape_iri), taking its
/// [`target_term`](gmeow_docs::model::DocShape::target_term); (b) failing that, an
/// exact match of the bare shape IRI against a known [`DocTerm::iri`](
/// gmeow_docs::model::DocTerm::iri). A row that resolves to neither is honestly
/// absent from `by_term` — never forced, never fabricated. Whole-program rows
/// (no `property-path:` prefix) are skipped entirely: they apply project-wide,
/// not per-term, and are already rendered on the static loss-ledger page (A4).
pub(crate) fn term_loss_digest_from_upstream(
    upstream: &BTreeMap<String, StageProduct>,
    shapes: &[gmeow_docs::model::DocShape],
    terms: &[gmeow_docs::model::DocTerm],
) -> Result<gmeow_docs::model::TermLossDigest, gmeow_errors::Diag> {
    let ledger = crate::stages::carrier::producer_graph(
        upstream,
        "stage-mappings",
        crate::stages::carrier::GRAPH_PROJECTION_LEDGER,
    )?;

    // First pass: fold every `logic:ProjectionTarget` subject's label/preservation-
    // kind/complexity-class/lossy-drops off the flat quad stream (order-independent —
    // a subject's predicates may arrive in any order).
    let mut target_subjects: BTreeSet<String> = BTreeSet::new();
    // The reified per-term projection-loss subjects (`logic:TermProjectionLoss`) and their
    // structured `gmeow:lossySourceTerm` IRIs — the term-attributed drops the report emits
    // for EVERY projection target (owl-dl/datalog/sssom/…), not just `property-path:`.
    let mut term_loss_subjects: BTreeSet<String> = BTreeSet::new();
    let mut source_terms: BTreeMap<String, String> = BTreeMap::new();
    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    let mut preservation_kinds: BTreeMap<String, String> = BTreeMap::new();
    let mut complexity_classes: BTreeMap<String, String> = BTreeMap::new();
    let mut lossy_drops: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for q in ledger.owned_quads() {
        let RdfTerm::Iri(subject) = &q.subject else {
            continue;
        };
        match q.predicate.as_str() {
            RDF_TYPE => {
                if let RdfTerm::Iri(object) = &q.object {
                    if object == LOGIC_PROJECTION_TARGET_TYPE {
                        target_subjects.insert(subject.clone());
                    } else if object == LOGIC_TERM_PROJECTION_LOSS_TYPE {
                        term_loss_subjects.insert(subject.clone());
                    }
                }
            }
            GMEOW_LOSSY_SOURCE_TERM => {
                if let RdfTerm::Iri(object) = &q.object {
                    source_terms.insert(subject.clone(), object.clone());
                }
            }
            RDFS_LABEL => {
                if let RdfTerm::Literal(lit) = &q.object {
                    labels.insert(subject.clone(), lit.lexical_form.clone());
                }
            }
            LOGIC_PRESERVATION_KIND => {
                if let RdfTerm::Iri(object) = &q.object {
                    let local = object.strip_prefix(LOGIC_NS).unwrap_or(object.as_str());
                    preservation_kinds.insert(subject.clone(), local.to_string());
                }
            }
            LOGIC_COMPLEXITY_CLASS => {
                if let RdfTerm::Literal(lit) = &q.object {
                    complexity_classes.insert(subject.clone(), lit.lexical_form.clone());
                }
            }
            GMEOW_LOSSY_DROP => {
                if let RdfTerm::Literal(lit) = &q.object {
                    lossy_drops
                        .entry(subject.clone())
                        .or_default()
                        .push(lit.lexical_form.clone());
                }
            }
            _ => {}
        }
    }

    // Second pass: join every property-path row to a documented term, in ledger-
    // subject order (deterministic — `target_subjects` is a `BTreeSet`).
    let shape_to_term: BTreeMap<&str, &str> = shapes
        .iter()
        .map(|s| (s.shape_iri.as_str(), s.target_term.as_str()))
        .collect();
    let known_term_iris: BTreeSet<&str> = terms.iter().map(|t| t.iri.as_str()).collect();

    let mut total_property_path_rows = 0usize;
    let mut by_term: BTreeMap<String, Vec<gmeow_docs::model::TermLossRow>> = BTreeMap::new();
    for subject in &target_subjects {
        let Some(label) = labels.get(subject) else {
            continue;
        };
        let Some(shape_iri) = label.strip_prefix(PROPERTY_PATH_LABEL_PREFIX) else {
            // A whole-program row (e.g. "owl-dl") — not per-term, skip.
            continue;
        };
        total_property_path_rows += 1;

        let resolved_term = shape_to_term
            .get(shape_iri)
            .copied()
            .or_else(|| known_term_iris.get(shape_iri).copied());
        let Some(term_iri) = resolved_term else {
            // Genuinely unjoinable: no DocShape claims this shape IRI, and the bare
            // shape IRI names no documented term either. Honest absence.
            continue;
        };

        let mut drops: Vec<String> = lossy_drops.get(subject).cloned().unwrap_or_default();
        drops.sort();
        drops.dedup();

        by_term
            .entry(term_iri.to_string())
            .or_default()
            .push(gmeow_docs::model::TermLossRow {
                target: label.clone(),
                preservation_kind: preservation_kinds.get(subject).cloned().unwrap_or_default(),
                complexity_class: complexity_classes.get(subject).cloned().unwrap_or_default(),
                lossy_drops: drops,
            });
    }
    // Second join: EVERY projection target's term-attributed drops (the reified
    // `logic:TermProjectionLoss` nodes), attributed to their documented source term. This is
    // the general per-term loss surface — a CORE `gmeow:` term projected DOWN to a lossy
    // external surface (e.g. its SSSOM alignment cannot carry a distinction) carries the drop
    // on its own page. A node whose `gmeow:lossySourceTerm` names no documented term is
    // honestly absent (never forced). `term_loss_subjects` is a BTreeSet, so the join order
    // is deterministic.
    for subject in &term_loss_subjects {
        let Some(source_term) = source_terms.get(subject) else {
            // A malformed term-loss node with no structured source term: honest skip.
            continue;
        };
        if !known_term_iris.contains(source_term.as_str()) {
            // The named source term is not a documented term — honest absence.
            continue;
        }

        let mut drops: Vec<String> = lossy_drops.get(subject).cloned().unwrap_or_default();
        drops.sort();
        drops.dedup();

        by_term
            .entry(source_term.clone())
            .or_default()
            .push(gmeow_docs::model::TermLossRow {
                // The projection target this loss belongs to (owl-dl / sssom:<hash> / …),
                // carried on the term-loss node's `rdfs:label`.
                target: labels.get(subject).cloned().unwrap_or_default(),
                preservation_kind: preservation_kinds.get(subject).cloned().unwrap_or_default(),
                complexity_class: complexity_classes.get(subject).cloned().unwrap_or_default(),
                lossy_drops: drops,
            });
    }

    for rows in by_term.values_mut() {
        rows.sort_by(|a, b| {
            a.target
                .cmp(&b.target)
                .then_with(|| a.lossy_drops.cmp(&b.lossy_drops))
        });
        rows.dedup();
    }

    Ok(gmeow_docs::model::TermLossDigest {
        by_term,
        total_property_path_rows,
    })
}

/// Fold the per-term JSON Schema / OpenAPI fragment digest off the COMMITTED
/// `generated/schemas/gmeow.schema.json` / `gmeow.openapi.json` under `root` — the
/// disk-sourced reader for the standalone `make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs` fanout
/// (`gmeow-dev sync --mode update --outputs docs`), which builds the docs model via
/// [`gmeow_docs::model::DocsModel::discover`] WITHOUT a live pipeline product. The
/// two committed files are projections of the `stage-export-json-schema` emitter
/// output, so the resulting digest — and thus every rendered per-term Python/Rust
/// example tab — is a faithful projection of that emitter output (the join is
/// delegated to [`schema_fragments_from_json`]). Hard-fails when either committed
/// schema file is absent or
/// its bytes fail to parse as JSON (no-optionality: a missing required schema
/// source is never papered over with an empty digest).
pub fn schema_fragments_from_generated(
    root: &Path,
    terms: &[gmeow_docs::model::DocTerm],
) -> Result<gmeow_docs::model::SchemaFragmentDigest, gmeow_errors::Diag> {
    let read_json = |rel: &str| -> Result<serde_json::Value, gmeow_errors::Diag> {
        let path = root.join(rel);
        let bytes = std::fs::read(&path).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "sync-docs".to_string(),
                message: format!(
                    "missing committed schema source {} for the schema-fragment digest: {e}",
                    path.display()
                ),
            })
        })?;
        serde_json::from_slice(&bytes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "sync-docs".to_string(),
                message: format!(
                    "parse committed schema source {} for the schema-fragment digest: {e}",
                    path.display()
                ),
            })
        })
    };
    let schema = read_json(crate::stages::json_schema::JSON_SCHEMA_PATH)?;
    let openapi = read_json(crate::stages::json_schema::OPENAPI_PATH)?;
    Ok(schema_fragments_from_json(&schema, &openapi, terms))
}

/// Fold the per-term JSON Schema / OpenAPI fragment digest off THIS run's
/// `stage-export-json-schema` product — the in-pipeline reader
/// ([`DocsRenderStage`]'s run path). The committed `generated/schemas/*.json`
/// files are the PREVIOUS run's projection until the post-phase-1 fanout rewrites
/// them, so a disk read here would lag every schema change by one regenerate (the
/// stale-disk-fold class); the product bytes are the single fresh source (the same
/// bytes the carrier folds into the packed `schemas-archive`). Hard-fails when
/// either artifact is absent from the upstream product or fails to parse as JSON
/// (no-optionality: never a stale on-disk fallback, never an empty digest).
pub(crate) fn schema_fragments_from_upstream(
    upstream: &BTreeMap<String, StageProduct>,
    terms: &[gmeow_docs::model::DocTerm],
) -> Result<gmeow_docs::model::SchemaFragmentDigest, gmeow_errors::Diag> {
    let read_json = |rel: &str| -> Result<serde_json::Value, gmeow_errors::Diag> {
        let bytes = upstream
            .get("stage-export-json-schema")
            .and_then(|p| p.artifact(rel))
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "stage-docs-render".to_string(),
                    message: format!(
                        "stage-export-json-schema produced no {rel} product for the \
                         schema-fragment digest; refusing to fall back to a stale on-disk \
                         read (the stale-disk-fold class, fail-closed)"
                    ),
                })
            })?;
        serde_json::from_slice(bytes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-docs-render".to_string(),
                message: format!(
                    "parse the stage-export-json-schema {rel} product for the \
                     schema-fragment digest: {e}"
                ),
            })
        })
    };
    let schema = read_json(crate::stages::json_schema::JSON_SCHEMA_PATH)?;
    let openapi = read_json(crate::stages::json_schema::OPENAPI_PATH)?;
    Ok(schema_fragments_from_json(&schema, &openapi, terms))
}

/// The pure per-term join both schema-fragment readers share: for each documented
/// CLASS, look its emitter def key
/// ([`Namespaces::def_key`](purrdf::shapes::json_schema::Namespaces::def_key)) up
/// in the parsed `$defs` (JSON Schema) / `components/schemas` (OpenAPI) objects and
/// carry the pretty-printed fragment. A class with no matching entry is honestly
/// absent (no fabricated stub); the emitter's synthetic `Node`/`Annotation` keys
/// (a whole-schema discriminator + the RDF-1.2 reifier-metadata fragment) are
/// never joined. Deterministic (`BTreeMap` keys + stable pretty-print).
pub(crate) fn schema_fragments_from_json(
    schema: &serde_json::Value,
    openapi: &serde_json::Value,
    terms: &[gmeow_docs::model::DocTerm],
) -> gmeow_docs::model::SchemaFragmentDigest {
    let defs = schema.get("$defs").and_then(|v| v.as_object());
    let components = openapi
        .pointer("/components/schemas")
        .and_then(|v| v.as_object());

    let ns = gmeow_ns::gmeow_json_schema_namespaces();
    // The emitter's synthetic def keys (a whole-schema discriminator + the RDF-1.2
    // reifier-metadata fragment) are NOT per-term schemas — never join them.
    const SYNTHETIC_KEYS: &[&str] = &["Node", "Annotation"];

    let mut schema_by_term: BTreeMap<String, String> = BTreeMap::new();
    let mut openapi_by_term: BTreeMap<String, String> = BTreeMap::new();
    for term in terms {
        if term.category != gmeow_docs::model::DocTermCategory::Class {
            continue;
        }
        let key = ns.def_key(&term.iri);
        if SYNTHETIC_KEYS.contains(&key.as_str()) {
            continue;
        }
        if let Some(frag) = defs.and_then(|d| d.get(&key))
            && let Ok(text) = serde_json::to_string_pretty(frag)
        {
            schema_by_term.insert(term.iri.clone(), text);
        }
        if let Some(frag) = components.and_then(|c| c.get(&key))
            && let Ok(text) = serde_json::to_string_pretty(frag)
        {
            openapi_by_term.insert(term.iri.clone(), text);
        }
    }

    gmeow_docs::model::SchemaFragmentDigest {
        schema_by_term,
        openapi_by_term,
    }
}

/// Discover the docs model under `root`, attach the native-reasoner `verdict`, the
/// diagnostics→term join digest, and the dynamic per-term projection-loss join
/// digest (all from `upstream`), and project it to the documentation named graph
/// (N-Quads). The verdict is required so the SPARQL surface always carries the
/// per-term reasoning status (never a fabricated default); the diagnostics digest
/// is required (hard-fails on a missing `stage-validate`/`stage-compile-logic`
/// upstream product) so the per-term "Diagnostics you might hit" surface and any
/// `gmeow:doc*` diagnostics projection never fabricate a "no diagnostics" claim;
/// the term-loss digest is required (hard-fails on a missing `stage-mappings`
/// upstream product) so the per-term "how this term degrades under projection"
/// surface never fabricates a "carried exactly" claim; the schema-fragment digest
/// is required (hard-fails on a missing `stage-export-json-schema` upstream
/// product) so the model reads THIS run's schema bytes, never the previous run's
/// committed `generated/schemas/*.json` (the stale-disk-fold class). The per-term
/// content-address provenance is likewise read from THIS run's `stage-term-manifest`
/// product (hard-fails on a missing artifact) via
/// `gmeow_docs::model::DocsModel::discover_with_manifest_and_catalog`, never the committed
/// `generated/catalog/term-content-manifest.nq`, which lags one regenerate behind
/// whenever a term's definition digest changes (the same stale-disk-fold class).
pub fn render_docs_graph(
    root: &Path,
    verdict: ReasoningVerdict,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<String, gmeow_errors::Diag> {
    // The per-term content manifest, read off THIS run's stage-term-manifest product
    // (hard-fails on a missing artifact) — never the committed
    // generated/catalog/term-content-manifest.nq, which is the PREVIOUS run's bytes
    // until the fanout flushes. A definition-digest change this build mints a fresh
    // "Definition changed" changelog entry in the product; a disk read here would omit
    // it, leaving the documentation graph one regenerate behind the manifest (the
    // stale-disk-fold class). The standalone `make docs` sibling path
    // (`DocsModel::discover`) stays disk-sourced because it runs post-pipeline against
    // the fanout-refreshed committed file.
    let manifest_bytes = upstream
        .get("stage-term-manifest")
        .and_then(|p| p.artifact(crate::stages::term_manifest::TERM_MANIFEST_RDF_PATH))
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-docs-render".to_string(),
                message: format!(
                    "stage-term-manifest produced no {} product for the per-term content \
                     manifest; refusing to fall back to a stale on-disk read (the \
                     stale-disk-fold class, fail-closed)",
                    crate::stages::term_manifest::TERM_MANIFEST_RDF_PATH
                ),
            })
        })?;
    // The constraint catalog, rendered FRESH from THIS run's authored sources (root
    // ontology + slice modules) — never the committed
    // generated/catalog/constraint-catalog.nq, which is absent on a cold tree and the
    // previous run's bytes on a warm one (the stale-disk-fold / cold-absence class).
    // render_constraint_catalog is a pure function of the authored sources this stage's
    // input_files already declare, and the catalog content does not feed the documentation
    // named graph (to_gmeow_rdf ignores constraint_rules), so this render is byte-neutral
    // to the output and needs no new DAG edge — it only keeps the model build from
    // hard-failing on the not-yet-materialized file.
    let catalog_bytes = crate::stages::constraint_catalog::render_constraint_catalog(root)?;
    let mut model =
        DocsModel::discover_with_manifest_and_catalog(root, manifest_bytes, &catalog_bytes)
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "stage-docs-render".to_string(),
                    message: format!("docs model discovery failed: {e}"),
                })
            })?;
    model.attach_reasoning(verdict);
    let known_term_iris: BTreeSet<String> = model.terms.iter().map(|t| t.iri.clone()).collect();
    let diagnostics =
        diagnostics_digest_from_upstream(upstream, &known_term_iris, &model.constraint_rules)?;
    model.attach_diagnostics(diagnostics);
    let term_loss = term_loss_digest_from_upstream(upstream, &model.shapes, &model.terms)?;
    model.attach_term_loss(term_loss);
    // The per-term JSON Schema / OpenAPI fragment join, read off THIS run's
    // stage-export-json-schema product (hard-fails on a missing artifact) — never the
    // committed generated/schemas/*.json, which are the previous run's bytes until
    // the fanout flushes (the stale-disk-fold class). The standalone `make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs`
    // sibling reader (`schema_fragments_from_generated`) stays disk-sourced because
    // it runs post-pipeline against the fanout-refreshed committed files.
    let schema_fragments = schema_fragments_from_upstream(upstream, &model.terms)?;
    model.attach_schema_fragments(schema_fragments);
    // The per-term entailment DAG, parsed from `stage-reason`'s already-materialized
    // `reasoning-explanations` proof skeletons (reason-once — this READS the same
    // upstream product, never a second reasoning pass) and joined against every
    // documented term IRI, so the documentation graph carries each term's
    // derivations (rule → conclusion, all premises) as first-class queryable RDF.
    let entailments =
        crate::stages::carrier::term_entailments_from_upstream(upstream, &known_term_iris)?;
    Ok(to_gmeow_rdf(&model, &entailments))
}

/// Recursively collect every regular file under `dir` into `out` (fail-fast on a
/// `read_dir` entry error; a missing directory yields nothing).
fn walk_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), gmeow_errors::Diag> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Whether one path below `assets_root` is a producer-owned console artifact rather than
/// authored documentation input.
///
/// These paths are git-ignored outputs of `console-assemble`, `npm ci`, or `npm pack`.
/// Folding them back into the docs-stage source digest creates a producer-to-input cycle:
/// a successful gate refreshes the console package after synchronization and invalidates
/// the clean manifest it just wrote. The next gate then pays for another full pipeline even
/// though no authored input changed.
fn is_derived_docs_asset(assets_root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(assets_root) else {
        return false;
    };
    relative.starts_with(Path::new("console/pkg"))
        || relative.starts_with(Path::new("console/node_modules"))
        || relative.starts_with(Path::new("console/smoke/node_modules"))
        || (relative.parent() == Some(Path::new("console"))
            && relative
                .extension()
                .is_some_and(|extension| extension == "tgz"))
}

/// Recursively collect authored vendored assets while pruning producer-owned subtrees
/// before descent, so an installed `node_modules` tree costs neither hashing nor walking.
fn walk_docs_asset_sources(
    assets_root: &Path,
    dir: &Path,
    out: &mut Vec<std::path::PathBuf>,
) -> Result<(), gmeow_errors::Diag> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if is_derived_docs_asset(assets_root, &path) {
            continue;
        }
        if path.is_dir() {
            walk_docs_asset_sources(assets_root, &path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Recursively collect every `*.md` Markdown source under `dir` into `out` — the
/// cache-key mirror of the docs model's recursive `text/markdown` document
/// discovery (fail-fast on a `read_dir` entry error; a missing directory yields
/// nothing).
fn walk_markdown(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), gmeow_errors::Diag> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk_markdown(&path, out)?;
        } else if path.extension().is_some_and(|x| x == "md") {
            out.push(path);
        }
    }
    Ok(())
}

/// Every raw source file `gmeow_docs::DocsModel::discover` reads: slice modules,
/// every recursively-discovered per-slice Markdown source (the `docs.md` guide AND
/// every nested `design/*.md` / other `*.md` — the same `text/markdown` set the
/// model projects as first-class `DocMarkdownDocument`s), the vendored documentation
/// site assets (`crates/docs/assets/**`), slice `examples/*.ttl`, `docs/four-boxes.md`,
/// per-slice `i18n/<lang>.po` gettext translation catalogs, per-slice
/// `shapes.ttl` SHACL constraint files, per-slice `tests/competency.ttl`
/// competency-question overlays, per-slice `tests/conformance-fixtures/*.ttl` /
/// `tests/counter-examples/*.ttl` Do/Don't fixtures + their
/// `tests/example-conformance.ttl` binding overlay, per-slice `queries/*` SPARQL
/// files (a `gmeow:cqQueryFile` may resolve into a slice's own `queries/competency/`
/// tree) plus the shared repo-root `queries/*` tree (the same `cqQueryFile` value
/// may instead point at a root-level shared query, e.g. `queries/competency/…` or
/// `queries/qc/…` — both forms are repo-root-relative, mirroring
/// `crates/slicetest/src/paths.rs::query_file`'s own resolution contract), and
/// root `shapes/*.ttl` aggregate node shapes. These are NOT reflected in the
/// composed `stage-gts-compose` product (guide bodies ride the bundle only as
/// blake3 digests), so any stage that derives an artifact from the docs model
/// must declare them as `input_files` for cache soundness. Shared by
/// `DocsRenderStage` (the documentation graph) and `SnapshotStage` (the
/// embedded rendered site).
pub(crate) fn docs_source_files(
    root: &Path,
) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for module in crate::stages::source_load::module_files(root)? {
        let dir = module.parent().unwrap_or(root);
        files.push(module.clone());
        // Every recursively-discovered Markdown source under the slice directory —
        // the SAME set `gmeow_docs::DocsModel::discover` selects as first-class
        // `DocMarkdownDocument`s (the top-level `docs.md` guide AND every nested
        // `design/*.md` / other `*.md`). Declaring only the top-level `docs.md`
        // silently dropped design-doc edits from the docs cache key, so a rendered
        // page could go stale against its authored source. Over-inclusion of an
        // incidental `.md` is cache-safe (a redundant key input); UNDER-inclusion is
        // the soundness bug this closes.
        walk_markdown(dir, &mut files)?;
        let shapes = dir.join("shapes.ttl");
        if shapes.is_file() {
            files.push(shapes);
        }
        let competency = dir.join("tests").join("competency.ttl");
        if competency.is_file() {
            files.push(competency);
        }
        // Conformance Do/Don't fixtures: the well-formed instances / counter-
        // examples themselves plus the binding overlay that joins them to an
        // expected outcome / violation code / rationale.
        let example_conformance = dir.join("tests").join("example-conformance.ttl");
        if example_conformance.is_file() {
            files.push(example_conformance);
        }
        for fixture_dir in ["conformance-fixtures", "counter-examples"] {
            if let Ok(entries) = std::fs::read_dir(dir.join("tests").join(fixture_dir)) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().is_some_and(|x| x == "ttl") {
                        files.push(p);
                    }
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir(dir.join("examples")) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|x| x == "ttl") {
                    files.push(p);
                }
            }
        }
        if let Ok(entries) = std::fs::read_dir(dir.join("i18n")) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|x| x == "po") {
                    files.push(p);
                }
            }
        }
        // Notation grammars: the first-class W3C EBNF renderings of the
        // project's own serialization surface syntaxes (`gmeow_docs::model::
        // DocGrammar`), authored under `slices/grounding/lang/grammars/*.ebnf`
        // today, but discovered generically per-slice.
        if let Ok(entries) = std::fs::read_dir(dir.join("grammars")) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|x| x == "ebnf") {
                    files.push(p);
                }
            }
        }
        // A slice's own `queries/` tree (typically `queries/competency/*.rq`) — a
        // `gmeow:cqQueryFile` this slice's `competency.ttl` declares may resolve here.
        walk_files(&dir.join("queries"), &mut files)?;
    }
    let four_boxes = root.join("docs").join("four-boxes.md");
    if four_boxes.is_file() {
        files.push(four_boxes);
    }
    // NOTE: the term content manifest is NOT declared here. The in-pipeline
    // `DocsRenderStage` consumes `stage-term-manifest` as an upstream PRODUCT and
    // reads the fresh manifest bytes off it (see `render_docs_graph`), so the cache
    // key already reflects that product edge; declaring the committed on-disk file as
    // a raw source input would re-introduce the previous run's bytes into the cache
    // key (the stale-disk-fold class this stage now avoids). The `SnapshotStage`,
    // which shares this list, embeds the rendered site whose per-term provenance
    // rides in via the same `stage-term-manifest` fold, so it needs no disk read either.
    walk_files(&root.join("i18n"), &mut files)?;
    walk_files(&root.join("shapes"), &mut files)?;
    // The shared repo-root query tree (`queries/competency/*.rq`, `queries/qc/*.rq`,
    // …) — many `gmeow:cqQueryFile` values resolve here rather than into a slice's
    // own directory (both forms are repo-root-relative; see the doc comment above).
    walk_files(&root.join("queries"), &mut files)?;
    // The AUTHORED vendored documentation site assets (`crates/docs/assets/**` — the CSS/JS
    // theme, the offline SPARQL playground's `include_bytes!`'d purrdf wasm engine,
    // and every other vendored wasm surface + its `DIGESTS.blake3` pin). The
    // `SnapshotStage` embeds the rendered site, which carries these bytes verbatim,
    // so refreshing a vendored asset (via its `maint-refresh-*-asset` target) MUST
    // invalidate the docs render cache — otherwise the cache would serve HTML that
    // references a freshly-swapped engine it was not rendered against. `.rs` source
    // (the only thing `GMEOW_BUILD_FINGERPRINT` folds) does not change on an
    // asset-only refresh, so the asset bytes are declared here as first-class cache
    // inputs. New vendored surfaces land under this tree and fold in automatically.
    // Git-ignored console package/install outputs are pruned: they are produced from these
    // sources later in the gate and must never feed back into the sync input identity.
    let assets_root = root.join("crates").join("docs").join("assets");
    walk_docs_asset_sources(&assets_root, &assets_root, &mut files)?;
    files.sort();
    files.dedup();
    Ok(files)
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `docs_render` pipeline stage.
pub struct DocsRenderStage {
    consumes: Vec<String>,
}

impl DocsRenderStage {
    /// Construct the stage. It discovers the docs model from the slice catalog at
    /// the root and consumes `stage-reason` so the projected documentation graph
    /// carries the per-term native-reasoner status (`gmeow:docReasoningStatus`),
    /// `stage-validate` + `stage-compile-logic` so it carries the diagnostics→term
    /// join digest (the term page's "Diagnostics you might hit" surface),
    /// `stage-mappings` so it carries the dynamic per-term projection-loss join
    /// digest (the term page's "how this term degrades under projection" surface),
    /// `stage-export-json-schema` so the model's per-term JSON-Schema/OpenAPI
    /// fragment digest reads THIS run's schema product rather than the previous
    /// run's committed `generated/schemas/*.json` (the stale-disk-fold class), and
    /// `stage-term-manifest` so the model's per-term content-address provenance
    /// (definition digest + first-seen version + computed changelog) reads THIS
    /// run's freshly-computed manifest product rather than the previous run's
    /// committed `generated/catalog/term-content-manifest.nq`, which lags one
    /// regenerate behind whenever a term's definition digest changes (the same
    /// stale-disk-fold class).
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-compile-logic".to_string(),
                "stage-export-json-schema".to_string(),
                "stage-gts-compose".to_string(),
                "stage-mappings".to_string(),
                "stage-reason".to_string(),
                "stage-term-manifest".to_string(),
                "stage-validate".to_string(),
            ],
        }
    }
}

impl Default for DocsRenderStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for DocsRenderStage {
    fn id(&self) -> &str {
        "stage-docs-render"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    /// The named graphs this stage attaches to the carrier (its delta), from the
    /// single Rust-side attach table; mirrored by the slice module.ttl gmeow:attachesGraph
    /// declarations and verified against the run-time delta by the scheduler.
    fn attaches_graphs(&self) -> &[String] {
        crate::stages::attach::graphs(self.id())
    }
    /// The blob-representation lanes this stage attaches (its delta), from the single
    /// Rust-side attach table; mirrored by gmeow:attachesBlobRep and run-time-verified.
    fn attaches_blob_reps(&self) -> &[String] {
        crate::stages::attach::blob_reps(self.id())
    }
    fn cache_policy(&self) -> CachePolicy {
        // Measured contribution: 241.6 MB serialized for a ~6.1 s rebuild. Hydration
        // cannot amortize the disk/RSS cost, so the typed DAG keeps it recompute-only.
        CachePolicy::Recompute
    }
    fn impl_version(&self) -> &str {
        // v11: full native diagnostic reports replace JSON transport parsing.
        // v10: documentation verdicts require the pinned native reasoning result,
        // complete payload identity and an actually decided consistency judgment.
        // v9: the per-term content-address manifest (definition digest + first-seen
        // version + computed changelog) is read from THIS run's consumed
        // stage-term-manifest product (DocsModel::discover_with_manifest_and_catalog) instead of
        // lagging one regenerate behind on the committed
        // generated/catalog/term-content-manifest.nq disk read; the manifest is
        // dropped from input_files since it is now a product edge, not a raw source
        // read (the stale-disk-fold class this fixes for the documentation graph).
        // v8: the per-term JSON-Schema/OpenAPI fragment digest is attached from THIS
        // run's consumed stage-export-json-schema product (schema_fragments_from_
        // upstream) instead of lagging one regenerate behind on the committed
        // generated/schemas/*.json disk read (the stale-disk-fold class).
        // v7: the diagnostics→term digest now joins on each finding's purpose-built
        // `documented_terms` attribution (a SHACL violation's constrained `sh:path`
        // property) as the primary leg, so the per-term "Diagnostics you might hit"
        // panel lights up on documented property terms instead of shipping vacuous.
        // Bumped so the cache re-derives the rendered graph now that `by_term` is
        // populated from the newly-attributed findings.
        // v6: adds `term_loss_digest_from_upstream`, folding the dynamic per-term
        // projection-loss join from the `stage-mappings` product's live
        // `GRAPH_PROJECTION_LEDGER` graph.
        "docs_render.v11-native-diagnostic-reports"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // The raw-source half of this DocsRender leaf — declared so a guide /
        // four-boxes / per-slice i18n catalog edit busts the cache (cache soundness).
        // The snapshot stage embeds the rendered SITE from these same sources,
        // so it shares this list via `docs_source_files`.
        docs_source_files(root)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let verdict = reasoning_verdict_from_reason(input.upstream)?;
        let graph = render_docs_graph(input.root, verdict, input.upstream)?;
        let graph_bytes = graph.into_bytes();
        // Attach the documentation projection as the carrier's `graph/documentation`
        // named graph so the presenter reads it as a pure keyed fold (PIPELINE_SPINE §4),
        // never re-parses the byte artifact. The byte lane is kept for the byte readers.
        let dataset = crate::stages::carrier::parse_into_graph(
            &graph_bytes,
            "application/n-quads",
            crate::stages::carrier::GRAPH_DOCUMENTATION,
        )?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(DOCS_GRAPH_PATH.to_string(), graph_bytes);
        Ok(StageOutput::new(StageProduct::from_artifacts_over(
            self.id(),
            dataset,
            artifacts,
        )))
    }
}

#[cfg(test)]
mod reasoning_tests;

#[path = "docs_render.tests.rs"]
#[cfg(test)]
mod tests;
