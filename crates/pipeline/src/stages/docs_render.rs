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

use crate::node::{Stage, StageInput, StageOutput, StageProduct};

/// Logical path of the documentation named graph (N-Quads, in-memory dataflow).
pub const DOCS_GRAPH_PATH: &str = "pipeline/documentation.nq";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";

/// Derive the docs [`ReasoningVerdict`] from the `stage-reason` product's inferred
/// closure — the SOLE reasoning pass, never re-run here.
///
/// A class is unsatisfiable exactly when the native DL reasoner entailed
/// `<class> rdfs:subClassOf owl:Nothing` (the same signal
/// [`gmeow_logic::reason::dl::unsatisfiable_from_inferred`] keys on); the ontology
/// is inconsistent exactly when some individual is entailed `rdf:type owl:Nothing`
/// (a witnessed clash). Both are read off the already-carried closure, so no new
/// reasoning runs and no `crates/logic` type grows a field. Shared by the
/// docs-graph stage (here) and the rendered-site archive (`carrier`), so the two
/// surfaces report the SAME verdict. Hard-fails if the closure is absent — never a
/// silent "consistent" default.
pub(crate) fn reasoning_verdict_from_reason(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<ReasoningVerdict, gmeow_errors::Diag> {
    let closure = upstream
        .get("stage-reason")
        .and_then(|p| p.artifact(crate::stages::reason::CLOSURE_PATH))
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-docs-render".to_string(),
                message: format!(
                    "missing stage-reason artifact {} for the reasoning verdict",
                    crate::stages::reason::CLOSURE_PATH
                ),
            })
        })?;
    let dataset = crate::stages::source_load::turtle_bytes_to_dataset(closure, "reason-closure")
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-docs-render".to_string(),
                message: format!("parse reasoned closure for the reasoning verdict: {e}"),
            })
        })?;
    let mut unsatisfiable: BTreeSet<String> = BTreeSet::new();
    let mut is_consistent = true;
    for q in dataset.owned_quads() {
        let RdfTerm::Iri(object) = &q.object else {
            continue;
        };
        if object != OWL_NOTHING {
            continue;
        }
        if q.predicate == RDFS_SUBCLASSOF {
            if let RdfTerm::Iri(subject) = &q.subject
                && subject != OWL_NOTHING
            {
                unsatisfiable.insert(subject.clone());
            }
        } else if q.predicate == RDF_TYPE {
            is_consistent = false;
        }
    }
    Ok(ReasoningVerdict {
        is_consistent,
        unsatisfiable,
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

/// Fold the diagnostics→term join [`DiagnosticsDigest`] from the `stage-validate` +
/// `stage-compile-logic` products' committed **JSON** diagnostics artifacts
/// ([`crate::stages::validate::SHACL_JSON_PATH`] /
/// [`crate::stages::compile_logic::DIAG_JSON_PATH`]) — never a re-run of SHACL or
/// the logic compiler (reason/validate-once). This reads the full-fidelity
/// `gmeow_errors::Report` (`gmeow_errors::render::to_json`'s exact wire form), NOT
/// the lossy `diagnostics:nodes` blob: the forward `Finding → DiagNode` fold
/// (`diag_render::finding_nodes`, `rdf_location_lossy`) deliberately drops
/// `location.logical` (and the intermediate `purrdf::RdfDiagnostic` carries no
/// attributions at all) so the diagnostics RDF projection and the run-ledger stay
/// byte-identical — that lane can NEVER carry a term/slice join, no matter how
/// many diagnostics exist. The JSON artifact is a plain `serde` serialization of
/// the `Report`/`Finding` model itself, so `Location.logical` and
/// `Finding.attributions` survive intact. Hard-fails when either declared upstream
/// product/artifact is absent, or when the artifact bytes fail to parse as a
/// `Report` (never a silently empty digest).
///
/// The per-term join has two legs, both EXACT string matches against
/// `known_term_iris` (never heuristic/fuzzy). The PRIMARY leg reads each finding's
/// purpose-built [`documented_terms`](gmeow_errors::Finding::documented_terms) — the
/// DOCUMENTED term the finding structurally concerns, e.g. a SHACL violation's
/// constrained `sh:path` property (a documented `gmeow:` term), recorded at the
/// finding-construction site (`gmeow_validate::findings`). This is preferred over the
/// raw focus node because a SHACL finding's focus is an ABox data individual that
/// never names a documented term. The SECONDARY leg, retained for findings whose
/// PRIMARY `Location.logical` genuinely names a documented term, matches the first
/// such logical location (skipped when it duplicates a documented-term hit). A finding
/// that resolves to no known term on either leg simply has no `by_term` entry (an
/// honest absence, not a bug).
///
/// `by_slice` is keyed on EVERY recorded [`gmeow_errors::DiagnosticAttribution`]
/// (a coarser join, available whenever the finding carries an attribution);
/// `help_uri` resolves through `constraint_rules` by exact `code` match.
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
    let shacl_json = validate
        .artifact(crate::stages::validate::SHACL_JSON_PATH)
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-docs-render".to_string(),
                message: format!(
                    "missing stage-validate artifact {} for the diagnostics digest",
                    crate::stages::validate::SHACL_JSON_PATH
                ),
            })
        })?;
    let compile_json = compile_logic
        .artifact(crate::stages::compile_logic::DIAG_JSON_PATH)
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-docs-render".to_string(),
                message: format!(
                    "missing stage-compile-logic artifact {} for the diagnostics digest",
                    crate::stages::compile_logic::DIAG_JSON_PATH
                ),
            })
        })?;
    let parse_report = |bytes: &[u8],
                        source: &str|
     -> Result<gmeow_errors::Report, gmeow_errors::Diag> {
        serde_json::from_slice(bytes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-docs-render".to_string(),
                message: format!("parse {source} diagnostics JSON for the diagnostics digest: {e}"),
            })
        })
    };
    let shacl_report = parse_report(shacl_json, crate::stages::validate::SHACL_JSON_PATH)?;
    let compile_report = parse_report(compile_json, crate::stages::compile_logic::DIAG_JSON_PATH)?;

    let findings: Vec<&gmeow_errors::Finding> = shacl_report
        .findings
        .iter()
        .chain(compile_report.findings.iter())
        .collect();
    let total = findings.len();

    let by_code: BTreeMap<&str, &str> = constraint_rules
        .iter()
        .map(|r| (r.code.as_str(), r.help_uri.as_str()))
        .collect();

    let mut by_term: BTreeMap<String, Vec<DocDiagFinding>> = BTreeMap::new();
    let mut by_slice: BTreeMap<String, Vec<DocDiagFinding>> = BTreeMap::new();
    for finding in &findings {
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

    Ok(DiagnosticsDigest {
        by_term,
        by_slice,
        total,
    })
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
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
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
/// disk-sourced reader for the standalone `make sync SYNC_OUTPUTS=docs` fanout
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

    let ns = crate::gmeow_ns::gmeow_json_schema_namespaces();
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
/// `gmeow_docs::model::DocsModel::discover_with_manifest`, never the committed
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
    let mut model = DocsModel::discover_with_manifest(root, manifest_bytes).map_err(|e| {
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
    // the fanout flushes (the stale-disk-fold class). The standalone `make sync SYNC_OUTPUTS=docs`
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

/// Every raw source file `gmeow_docs::DocsModel::discover` reads: slice modules,
/// per-slice `docs.md` guides, slice `examples/*.ttl`, `docs/four-boxes.md`,
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
        let docs = dir.join("docs.md");
        if docs.is_file() {
            files.push(docs);
        }
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
    fn impl_version(&self) -> &str {
        // v9: the per-term content-address manifest (definition digest + first-seen
        // version + computed changelog) is read from THIS run's consumed
        // stage-term-manifest product (DocsModel::discover_with_manifest) instead of
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
        "docs_render.v9"
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
mod tests {
    use super::*;
    use crate::stages::source_load::rdf_bytes_to_dataset;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn docs_source_files_includes_new_inputs() {
        let root = repo_root();
        let files = docs_source_files(&root).expect("docs_source_files");
        let has_shapes_ttl = files
            .iter()
            .any(|p| p.file_name().and_then(|n| n.to_str()) == Some("shapes.ttl"));
        assert!(
            has_shapes_ttl,
            "docs_source_files must include at least one per-slice shapes.ttl"
        );
        let has_competency = files.iter().any(|p| {
            p.file_name().and_then(|n| n.to_str()) == Some("competency.ttl")
                && p.parent()
                    .and_then(|parent| parent.file_name())
                    .and_then(|n| n.to_str())
                    == Some("tests")
        });
        assert!(
            has_competency,
            "docs_source_files must include at least one per-slice tests/competency.ttl"
        );
        let shapes_dir = root.join("shapes");
        let has_root_shapes = files.iter().any(|p| {
            p.extension().and_then(|s| s.to_str()) == Some("ttl")
                && p.parent()
                    .map(|parent| parent == shapes_dir)
                    .unwrap_or(false)
        });
        assert!(
            has_root_shapes,
            "docs_source_files must include root shapes/*.ttl files"
        );
        // A CQ's `cqQueryFile` may resolve into a slice's own `queries/competency/`
        // tree (`slices/grounding/logic/queries/competency/named-parametric-paths.rq`)
        // or the shared repo-root tree (`queries/competency/citation-intents.rq`) —
        // both must be cache-salted (`gmeow_docs::model::apply_competency_query_text`).
        let has_slice_query = files
            .iter()
            .any(|p| p.ends_with("queries/competency/named-parametric-paths.rq"));
        assert!(
            has_slice_query,
            "docs_source_files must include at least one per-slice queries/competency/*.rq"
        );
        let root_query = root.join("queries/competency/citation-intents.rq");
        assert!(
            files.contains(&root_query),
            "docs_source_files must include the shared root queries/competency/*.rq tree"
        );
        // The notation-grammar exhibits (`gmeow_docs::model::DocGrammar`) — a
        // `grammars/*.ebnf` edit must bust the docs cache.
        let has_grammar = files
            .iter()
            .any(|p| p.ends_with("slices/grounding/lang/grammars/gmn.ebnf"));
        assert!(
            has_grammar,
            "docs_source_files must include the lang slice's grammars/*.ebnf files"
        );
    }

    /// A `stage-validate` + `stage-compile-logic` + `stage-mappings` upstream
    /// triple whose JSON diagnostics artifacts carry an EMPTY
    /// [`gmeow_errors::Report`] (zero findings, never an absent artifact — a
    /// missing artifact must hard-fail, see [`diagnostics_digest_from_upstream`])
    /// and whose `stage-mappings` product carries an EMPTY dataset (zero
    /// `GRAPH_PROJECTION_LEDGER` rows, never an absent product — a missing
    /// product must hard-fail, see [`term_loss_digest_from_upstream`]) — the
    /// minimal upstream `render_docs_graph` needs so a source-lane test can
    /// render the whole-repo docs graph without a real pipeline run.
    fn empty_render_docs_graph_upstream() -> BTreeMap<String, StageProduct> {
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-validate".to_string(),
            report_json_product(
                "stage-validate",
                crate::stages::validate::SHACL_JSON_PATH,
                &gmeow_errors::Report::new("shacl"),
            ),
        );
        upstream.insert(
            "stage-compile-logic".to_string(),
            report_json_product(
                "stage-compile-logic",
                crate::stages::compile_logic::DIAG_JSON_PATH,
                &gmeow_errors::Report::new("logic-compile"),
            ),
        );
        upstream.insert(
            "stage-mappings".to_string(),
            StageProduct::new("stage-mappings", "test-empty-mappings-digest"),
        );
        // The docs graph now folds the per-term entailment DAG parsed from
        // stage-reason's materialized reasoning-explanations (reason-once); provide a
        // valid-but-empty explanations artifact so the read-back joins to zero
        // derivations (an honest absence) rather than hard-failing on a missing
        // artifact.
        let mut reason_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        reason_artifacts.insert(
            crate::stages::reason::EXPLANATIONS_PATH.to_string(),
            b"# no derivations\n".to_vec(),
        );
        upstream.insert(
            "stage-reason".to_string(),
            StageProduct::from_artifacts("stage-reason", reason_artifacts),
        );
        // The per-term schema-fragment digest is attached from THIS run's
        // stage-export-json-schema product (a missing artifact must hard-fail, see
        // `schema_fragments_from_upstream`); provide valid-but-empty JSON documents
        // so the join resolves to zero fragments (an honest absence).
        let mut schema_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        schema_artifacts.insert(
            crate::stages::json_schema::JSON_SCHEMA_PATH.to_string(),
            b"{}".to_vec(),
        );
        schema_artifacts.insert(
            crate::stages::json_schema::OPENAPI_PATH.to_string(),
            b"{}".to_vec(),
        );
        upstream.insert(
            "stage-export-json-schema".to_string(),
            StageProduct::from_artifacts("stage-export-json-schema", schema_artifacts),
        );
        // The per-term content-address manifest is now read from THIS run's
        // stage-term-manifest product (a missing artifact must hard-fail, see
        // `render_docs_graph`); feed the freshly-rendered manifest for the live repo
        // so the join carries every documented term's content-address exactly as a
        // real pipeline run would (never the empty digest a stale/absent read gives).
        let manifest_bytes = crate::stages::term_manifest::render_term_manifest(&repo_root())
            .expect("render term manifest for the docs-graph render test");
        let mut manifest_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        manifest_artifacts.insert(
            crate::stages::term_manifest::TERM_MANIFEST_RDF_PATH.to_string(),
            manifest_bytes,
        );
        upstream.insert(
            "stage-term-manifest".to_string(),
            StageProduct::from_artifacts("stage-term-manifest", manifest_artifacts),
        );
        upstream
    }

    /// Build a synthetic `stage_id` product carrying `report`'s JSON
    /// serialization ([`gmeow_errors::render::to_json`]) at `json_path` — the
    /// EXACT artifact lane `diagnostics_digest_from_upstream` reads, mirroring
    /// the real `stage-validate`/`stage-compile-logic` producers
    /// (`diag_render::render_diagnostics_artifacts`), so this test exercises the
    /// real production code path rather than the lossy `diagnostics:nodes` blob.
    fn report_json_product(
        stage_id: &str,
        json_path: &str,
        report: &gmeow_errors::Report,
    ) -> StageProduct {
        let json = gmeow_errors::render::to_json(report).expect("encode report json");
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(json_path.to_string(), json.into_bytes());
        StageProduct::from_artifacts(stage_id, artifacts)
    }

    /// A minimal synthetic [`gmeow_errors::Finding`] with the given code/
    /// category/term-location/slice-attribution — a plain builder, since (unlike
    /// the retired `DiagNode` lane) the JSON-artifact join needs no ledger
    /// fingerprint.
    fn synthetic_finding(
        code: &str,
        category: gmeow_errors::FindingCategory,
        term_iri: Option<&str>,
        slice_iri: Option<&str>,
    ) -> gmeow_errors::Finding {
        let mut finding = gmeow_errors::Finding::new(
            gmeow_errors::Severity::Warning,
            code,
            format!("synthetic finding for {code}"),
        )
        .with_category(category);
        if let Some(term_iri) = term_iri {
            finding.add_location(gmeow_errors::Location::new(
                None,
                None,
                None,
                Some(term_iri.to_string()),
            ));
        }
        if let Some(slice_iri) = slice_iri {
            finding
                .attributions
                .push(gmeow_errors::DiagnosticAttribution {
                    slice_iri: slice_iri.to_string(),
                    role: "focus-origin".to_string(),
                    evidence: None,
                });
        }
        finding
    }

    #[test]
    fn diagnostics_digest_joins_on_documented_term_attribution_not_abox_focus() {
        // The PRIMARY join leg: a SHACL-shaped finding whose FOCUS is an ABox data
        // individual (names no documented term) but whose `documented_terms` carries
        // the constrained property (a documented term) joins by_term on the PROPERTY,
        // never on the focus. This is the exact real-repo shape: the MinCount
        // violations' focus nodes are fixture individuals; their constrained
        // `gmeow:hasReferenceFrame` property is the documented term the panel lights up.
        let property = "https://blackcatinformatics.ca/gmeow/hasReferenceFrame";
        let mut known_terms: BTreeSet<String> = BTreeSet::new();
        known_terms.insert(property.to_string());

        let mut shacl_report = gmeow_errors::Report::new("shacl");
        let mut finding = synthetic_finding(
            "shacl.MinCountConstraintComponent",
            gmeow_errors::FindingCategory::DataShapeViolation,
            // The focus node is an ABox fixture individual — NOT a documented term.
            Some("https://blackcatinformatics.ca/gmeow/fixtureRagaYamanImprovised1975"),
            None,
        );
        finding = finding.with_documented_term(property);
        shacl_report.findings.push(finding);

        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-validate".to_string(),
            report_json_product(
                "stage-validate",
                crate::stages::validate::SHACL_JSON_PATH,
                &shacl_report,
            ),
        );
        upstream.insert(
            "stage-compile-logic".to_string(),
            report_json_product(
                "stage-compile-logic",
                crate::stages::compile_logic::DIAG_JSON_PATH,
                &gmeow_errors::Report::new("logic-compile"),
            ),
        );

        let digest =
            diagnostics_digest_from_upstream(&upstream, &known_terms, &[]).expect("digest folds");
        // Joined on the documented PROPERTY, exactly once, via the primary leg.
        assert_eq!(
            digest.by_term.get(property).map(Vec::len),
            Some(1),
            "the finding joins by_term on its documented constrained property"
        );
        assert_eq!(
            digest.by_term[property][0].code,
            "shacl.MinCountConstraintComponent"
        );
        // The ABox focus individual never fabricates a by_term key.
        assert!(
            !digest.by_term.contains_key(
                "https://blackcatinformatics.ca/gmeow/fixtureRagaYamanImprovised1975"
            ),
            "the ABox focus node must never enter by_term"
        );
    }

    #[test]
    fn diagnostics_digest_joins_term_and_slice_and_hard_fails_on_missing_upstream() {
        let term_iri = "https://blackcatinformatics.ca/gmeow/Cat";
        let mut known_terms: BTreeSet<String> = BTreeSet::new();
        known_terms.insert(term_iri.to_string());

        let mut shacl_report = gmeow_errors::Report::new("shacl");
        shacl_report.findings.push(synthetic_finding(
            "shacl.MinCountConstraintComponent",
            gmeow_errors::FindingCategory::DataShapeViolation,
            Some(term_iri),
            Some("https://blackcatinformatics.ca/gmeow/slices/core"),
        ));
        // A finding whose location names no KNOWN term: honestly absent from
        // `by_term`, never a fuzzy/heuristic join.
        shacl_report.findings.push(synthetic_finding(
            "shacl.NodeKindConstraintComponent",
            gmeow_errors::FindingCategory::DataShapeViolation,
            Some("https://example.test/not-a-known-term"),
            None,
        ));

        let mut compile_report = gmeow_errors::Report::new("logic-compile");
        compile_report.findings.push(synthetic_finding(
            "logic-compile.UNKNOWN_PROFILE",
            gmeow_errors::FindingCategory::ModelingDisciplineViolation,
            None,
            None,
        ));

        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-validate".to_string(),
            report_json_product(
                "stage-validate",
                crate::stages::validate::SHACL_JSON_PATH,
                &shacl_report,
            ),
        );
        upstream.insert(
            "stage-compile-logic".to_string(),
            report_json_product(
                "stage-compile-logic",
                crate::stages::compile_logic::DIAG_JSON_PATH,
                &compile_report,
            ),
        );

        let rule = gmeow_docs::model::ConstraintRule {
            code: "shacl.MinCountConstraintComponent".to_string(),
            slug: "shacl-min-count-constraint-component".to_string(),
            category: "https://blackcatinformatics.ca/gmeow/FindingDataShapeViolation".to_string(),
            severity: "binding".to_string(),
            help_uri:
                "https://blackcatinformatics.ca/gmeow/rules#shacl-min-count-constraint-component"
                    .to_string(),
            label: None,
            definition: None,
            applies_to_terms: Vec::new(),
            formalizes: None,
        };

        let digest =
            diagnostics_digest_from_upstream(&upstream, &known_terms, std::slice::from_ref(&rule))
                .expect("digest folds from synthetic upstream");
        assert_eq!(digest.total, 3, "3 findings folded across both producers");
        assert_eq!(
            digest.by_term.get(term_iri).map(Vec::len),
            Some(1),
            "only the finding whose location names a KNOWN term joins by_term"
        );
        let joined = &digest.by_term[term_iri][0];
        assert_eq!(joined.code, "shacl.MinCountConstraintComponent");
        assert_eq!(joined.help_uri.as_deref(), Some(rule.help_uri.as_str()));
        assert_eq!(
            digest
                .by_slice
                .get("https://blackcatinformatics.ca/gmeow/slices/core")
                .map(Vec::len),
            Some(1)
        );
        // The unattributed / unresolved-code findings never fabricate a slice or help_uri.
        assert!(
            !digest
                .by_slice
                .values()
                .flatten()
                .any(|f| f.code == "logic-compile.UNKNOWN_PROFILE" && f.help_uri.is_some()),
            "an unresolved code must never carry a fabricated help_uri"
        );

        // Missing EITHER declared upstream product hard-fails (never a silent empty digest).
        let mut only_validate: BTreeMap<String, StageProduct> = BTreeMap::new();
        only_validate.insert(
            "stage-validate".to_string(),
            report_json_product(
                "stage-validate",
                crate::stages::validate::SHACL_JSON_PATH,
                &gmeow_errors::Report::new("shacl"),
            ),
        );
        assert!(
            diagnostics_digest_from_upstream(&only_validate, &known_terms, &[]).is_err(),
            "missing stage-compile-logic must hard-fail"
        );
        assert!(
            diagnostics_digest_from_upstream(&BTreeMap::new(), &known_terms, &[]).is_err(),
            "missing both upstream products must hard-fail"
        );

        // A declared upstream product present but MISSING the JSON artifact (e.g. a
        // stale/partial product) hard-fails too — never silently treated as empty.
        let mut missing_artifact: BTreeMap<String, StageProduct> = BTreeMap::new();
        missing_artifact.insert(
            "stage-validate".to_string(),
            StageProduct::from_artifacts("stage-validate", BTreeMap::new()),
        );
        missing_artifact.insert(
            "stage-compile-logic".to_string(),
            report_json_product(
                "stage-compile-logic",
                crate::stages::compile_logic::DIAG_JSON_PATH,
                &gmeow_errors::Report::new("logic-compile"),
            ),
        );
        assert!(
            diagnostics_digest_from_upstream(&missing_artifact, &known_terms, &[]).is_err(),
            "a stage-validate product missing the SHACL JSON artifact must hard-fail"
        );
    }

    /// Build a synthetic `stage-mappings` product whose `GRAPH_PROJECTION_LEDGER`
    /// named graph carries EXACTLY the given Turtle `body`, parsed and re-rooted
    /// via [`crate::stages::carrier::parse_into_graph`] — the SAME producer-
    /// attached-graph lane the real `stage-mappings` stage rides
    /// (`mappings::run`'s own `parse_into_graph(..., GRAPH_PROJECTION_LEDGER)`
    /// call), so this test exercises the real production read path
    /// (`term_loss_digest_from_upstream` → `producer_graph`) rather than a stub.
    fn mappings_product_with_ledger(turtle_body: &str) -> StageProduct {
        let dataset = crate::stages::carrier::parse_into_graph(
            turtle_body.as_bytes(),
            "text/turtle",
            crate::stages::carrier::GRAPH_PROJECTION_LEDGER,
        )
        .expect("parse synthetic projection-ledger turtle");
        StageProduct::from_artifacts_over("stage-mappings", dataset, BTreeMap::new())
    }

    #[test]
    fn term_loss_digest_joins_property_path_rows_and_hard_fails_on_missing_upstream() {
        use gmeow_docs::model::{DocShape, DocTerm, DocTermCategory};

        // (a) resolves via a DocShape whose shape_iri matches the ledger row and
        // whose target_term names a documented term.
        let shape_a = "https://blackcatinformatics.ca/gmeow/examples/logic/nearbyOrgs";
        let term_a = "https://blackcatinformatics.ca/gmeow/PredicatePath";
        // (b) a property-path row whose shape IRI resolves to NEITHER a DocShape
        // NOR a known DocTerm — honestly absent from `by_term`.
        let shape_b = "https://example.test/shapes/unresolvable";
        // (c) resolves via the FALLBACK: no DocShape claims it, but the bare shape
        // IRI itself names a known DocTerm.
        let shape_c = "https://blackcatinformatics.ca/gmeow/AncestorsTo3";
        let term_c = shape_c;

        let preservation_kind_val = format!("{LOGIC_NS}SoundUnderApproximation");
        let turtle = format!(
            "<https://example.test/target/a> <{rdf_type}> <{pt}> .\n\
             <https://example.test/target/a> <{label}> \"property-path:{shape_a}\" .\n\
             <https://example.test/target/a> <{pk}> <{pk_val}> .\n\
             <https://example.test/target/a> <{cc}> \"PTIME\" .\n\
             <https://example.test/target/a> <{drop}> \"structural note B\" .\n\
             <https://example.test/target/a> <{drop}> \"structural note A\" .\n\
             <https://example.test/target/b> <{rdf_type}> <{pt}> .\n\
             <https://example.test/target/b> <{label}> \"property-path:{shape_b}\" .\n\
             <https://example.test/target/b> <{pk}> <{pk_val}> .\n\
             <https://example.test/target/b> <{cc}> \"PTIME\" .\n\
             <https://example.test/target/c> <{rdf_type}> <{pt}> .\n\
             <https://example.test/target/c> <{label}> \"property-path:{shape_c}\" .\n\
             <https://example.test/target/c> <{pk}> <{pk_val}> .\n\
             <https://example.test/target/c> <{cc}> \"PTIME\" .\n\
             <https://example.test/target/whole-program> <{rdf_type}> <{pt}> .\n\
             <https://example.test/target/whole-program> <{label}> \"owl-dl\" .\n\
             <https://example.test/target/whole-program> <{pk}> <{pk_val}> .\n\
             <https://example.test/target/whole-program> <{cc}> \"PTIME\" .\n",
            rdf_type = RDF_TYPE,
            pt = LOGIC_PROJECTION_TARGET_TYPE,
            label = RDFS_LABEL,
            pk = LOGIC_PRESERVATION_KIND,
            pk_val = preservation_kind_val,
            cc = LOGIC_COMPLEXITY_CLASS,
            drop = GMEOW_LOSSY_DROP,
            shape_a = shape_a,
            shape_b = shape_b,
            shape_c = shape_c,
        );

        let shapes = vec![DocShape {
            shape_iri: shape_a.to_string(),
            target_term: term_a.to_string(),
            messages: Vec::new(),
            owner_slice: "test-slice".to_string(),
        }];
        let terms = vec![
            DocTerm {
                iri: term_a.to_string(),
                curie: "gmeow:PredicatePath".to_string(),
                category: DocTermCategory::Class,
                owner_slice: "test-slice".to_string(),
                ..Default::default()
            },
            DocTerm {
                iri: term_c.to_string(),
                curie: "gmeow:AncestorsTo3".to_string(),
                category: DocTermCategory::Class,
                owner_slice: "test-slice".to_string(),
                ..Default::default()
            },
        ];

        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-mappings".to_string(),
            mappings_product_with_ledger(&turtle),
        );

        let digest = term_loss_digest_from_upstream(&upstream, &shapes, &terms)
            .expect("digest folds from synthetic stage-mappings upstream");

        assert_eq!(
            digest.total_property_path_rows, 3,
            "3 property-path rows (a, b, c) counted; the whole-program row must not count"
        );
        assert_eq!(
            digest.by_term.get(term_a).map(Vec::len),
            Some(1),
            "shape_a joins via DocShape.shape_iri -> target_term"
        );
        let row_a = &digest.by_term[term_a][0];
        assert_eq!(row_a.target, format!("property-path:{shape_a}"));
        assert_eq!(row_a.preservation_kind, "SoundUnderApproximation");
        assert_eq!(row_a.complexity_class, "PTIME");
        assert_eq!(
            row_a.lossy_drops,
            vec![
                "structural note A".to_string(),
                "structural note B".to_string()
            ],
            "lossy_drops must be sorted"
        );
        assert_eq!(
            digest.by_term.get(term_c).map(Vec::len),
            Some(1),
            "shape_c joins via the bare-shape-IRI == DocTerm.iri fallback"
        );
        assert!(
            !digest
                .by_term
                .values()
                .flatten()
                .any(|r| r.target.contains("unresolvable")),
            "shape_b names no DocShape and no DocTerm — honestly absent from by_term"
        );
        assert!(
            !digest
                .by_term
                .values()
                .flatten()
                .any(|r| r.target == "owl-dl"),
            "a whole-program row must never enter by_term"
        );

        // Missing the declared `stage-mappings` upstream product hard-fails (never a
        // silent empty digest).
        assert!(
            term_loss_digest_from_upstream(&BTreeMap::new(), &shapes, &terms).is_err(),
            "missing stage-mappings must hard-fail"
        );
    }

    /// The GENERAL per-term attribution join (the source-term-attribution correction): a
    /// `logic:TermProjectionLoss` node on ANY projection target attributes its drops to the
    /// DOCUMENTED source term named by its structured `gmeow:lossySourceTerm` IRI — the
    /// canonical core's loss when projected DOWN lands on the term's page. A node whose
    /// source term is NOT documented is honestly absent (never forced onto a term).
    #[test]
    fn term_loss_digest_attributes_term_projection_loss_nodes_to_documented_source_terms() {
        use gmeow_docs::model::{DocTerm, DocTermCategory};

        let core_term = "https://blackcatinformatics.ca/gmeow/Agent";
        let undocumented = "https://blackcatinformatics.ca/gmeow/NotDocumented";
        let preservation_kind_val = format!("{LOGIC_NS}SoundUnderApproximation");
        // Two term-loss nodes: one attributing to a documented CORE term (joins), one to an
        // undocumented term (honest absence). Each carries the projection target label, the
        // preservation kind, the complexity class, and a dropped feature.
        let turtle = format!(
            "<https://example.test/target/sssom:abc/termloss/agent> <{rdf_type}> <{tpl}> .\n\
             <https://example.test/target/sssom:abc/termloss/agent> <{src}> <{core}> .\n\
             <https://example.test/target/sssom:abc/termloss/agent> <{label}> \"sssom:abc\" .\n\
             <https://example.test/target/sssom:abc/termloss/agent> <{pk}> <{pk_val}> .\n\
             <https://example.test/target/sssom:abc/termloss/agent> <{cc}> \"1:1 lattice band\" .\n\
             <https://example.test/target/sssom:abc/termloss/agent> <{drop}> \"gmeow:Agent equivalentClass prov:Agent loses the caveat structure\" .\n\
             <https://example.test/target/sssom:def/termloss/nd> <{rdf_type}> <{tpl}> .\n\
             <https://example.test/target/sssom:def/termloss/nd> <{src}> <{nd}> .\n\
             <https://example.test/target/sssom:def/termloss/nd> <{label}> \"sssom:def\" .\n\
             <https://example.test/target/sssom:def/termloss/nd> <{pk}> <{pk_val}> .\n\
             <https://example.test/target/sssom:def/termloss/nd> <{drop}> \"orphan drop\" .\n",
            rdf_type = RDF_TYPE,
            tpl = LOGIC_TERM_PROJECTION_LOSS_TYPE,
            src = GMEOW_LOSSY_SOURCE_TERM,
            label = RDFS_LABEL,
            pk = LOGIC_PRESERVATION_KIND,
            pk_val = preservation_kind_val,
            cc = LOGIC_COMPLEXITY_CLASS,
            drop = GMEOW_LOSSY_DROP,
            core = core_term,
            nd = undocumented,
        );

        let terms = vec![DocTerm {
            iri: core_term.to_string(),
            curie: "gmeow:Agent".to_string(),
            category: DocTermCategory::Class,
            owner_slice: "test-slice".to_string(),
            ..Default::default()
        }];

        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-mappings".to_string(),
            mappings_product_with_ledger(&turtle),
        );

        let digest = term_loss_digest_from_upstream(&upstream, &[], &terms)
            .expect("digest folds from synthetic term-loss upstream");

        // The documented CORE term carries the attributed row (target = the projection
        // target label, preservation kind + complexity + dropped feature all present).
        let rows = digest
            .by_term
            .get(core_term)
            .expect("documented source term must carry its attributed projection-loss row");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].target, "sssom:abc");
        assert_eq!(rows[0].preservation_kind, "SoundUnderApproximation");
        assert_eq!(rows[0].complexity_class, "1:1 lattice band");
        assert_eq!(
            rows[0].lossy_drops,
            vec!["gmeow:Agent equivalentClass prov:Agent loses the caveat structure".to_string()]
        );
        // The undocumented source term is honestly absent — never fabricated onto a term.
        assert!(
            !digest.by_term.contains_key(undocumented),
            "a term-loss node whose source term is undocumented must not enter by_term"
        );
        // Whole-program `property-path` count is untouched by the general attribution join.
        assert_eq!(digest.total_property_path_rows, 0);
    }

    #[test]
    fn docs_graph_is_nonempty_and_parses() {
        let root = repo_root();
        let upstream = empty_render_docs_graph_upstream();
        let nq = render_docs_graph(&root, ReasoningVerdict::default(), &upstream)
            .expect("render docs graph");
        let dataset =
            rdf_bytes_to_dataset(nq.as_bytes(), "application/n-quads", "docs-graph").unwrap();
        let count = dataset.quad_count();
        // The documentation graph covers 50+ slices and their terms.
        assert!(
            count > 200,
            "docs named graph unexpectedly small: {count} quads"
        );
        // With a verdict attached, every documented term carries a reasoning status.
        assert!(
            nq.contains("docReasoningStatus"),
            "docs graph must carry per-term reasoning status when a verdict is attached"
        );
    }

    #[test]
    fn reasoning_verdict_reads_unsat_and_inconsistency_from_closure() {
        // A closure with one unsat class and one Nothing-typed individual.
        let closure = concat!(
            "<https://x/Empty> <http://www.w3.org/2000/01/rdf-schema#subClassOf> ",
            "<http://www.w3.org/2002/07/owl#Nothing> .\n",
            "<https://x/i> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ",
            "<http://www.w3.org/2002/07/owl#Nothing> .\n",
        );
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(
            crate::stages::reason::CLOSURE_PATH.to_string(),
            closure.as_bytes().to_vec(),
        );
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert(
            "stage-reason".to_string(),
            StageProduct::from_artifacts("stage-reason", artifacts),
        );
        let verdict = reasoning_verdict_from_reason(&upstream).expect("verdict");
        assert!(
            !verdict.is_consistent,
            "Nothing-typed individual ⇒ inconsistent"
        );
        assert!(verdict.unsatisfiable.contains("https://x/Empty"));
        assert!(
            !verdict
                .unsatisfiable
                .contains("http://www.w3.org/2002/07/owl#Nothing")
        );

        // A missing stage-reason product hard-fails (never a silent default).
        assert!(reasoning_verdict_from_reason(&BTreeMap::new()).is_err());
    }

    /// B1 (real-repo non-vacuity, symmetric with the B2/B3 gates): the
    /// diagnostics→term digest folded from the REAL `stage-validate` +
    /// `stage-compile-logic` products over the whole ontology must carry BOTH a
    /// non-zero raw finding total AND a NON-EMPTY `by_term` — i.e. at least one
    /// DOCUMENTED term whose "Diagnostics you might hit" panel renders a real
    /// diagnostic. The weaker `total > 0` alone passed while the HEADLINE per-term
    /// surface shipped vacuous on every one of the ~2357 term pages (the real SHACL
    /// findings' focus nodes are ABox individuals that name no documented term), so
    /// it is retained only as a precondition. The TRUE bar is the term join: the four
    /// real `ExpressionFrameRequirement` MinCount violations concern the documented
    /// property `gmeow:hasReferenceFrame` (their constrained `sh:path`), so that
    /// term's page must carry a `shacl.MinCountConstraintComponent` row. Runs the real
    /// source-load → validate / compile-logic chain directly (each `Stage::run` is
    /// pure in-memory — the committed `generated/` tree is untouched), mirroring the
    /// B3 real-repo chaining.
    #[test]
    fn diagnostics_digest_total_is_non_vacuous_on_the_real_repo() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap();
        let empty: BTreeMap<String, StageProduct> = BTreeMap::new();

        let source_load = crate::stages::source_load::SourceLoadStage::new()
            .run(StageInput {
                root: &root,
                upstream: &empty,
            })
            .expect("real source-load");

        // compile-logic reads the narrowed graph/logic-compile-inputs corpus off the
        // source-load product, so its upstream must carry that product.
        let mut compile_upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        compile_upstream.insert("stage-source-load".to_string(), source_load.product.clone());
        let compile = crate::stages::compile_logic::CompileLogicStage::new()
            .run(StageInput {
                root: &root,
                upstream: &compile_upstream,
            })
            .expect("real compile-logic");

        // The validate stage enforces the FRESH shape union: its upstream must carry
        // the four generated-shape producers (compile-logic + the three shape export
        // leaves), never a disk read of generated/shapes (the stale-disk-fold class).
        let frame = crate::stages::frame_shapes::FrameShapesStage
            .run(StageInput {
                root: &root,
                upstream: &empty,
            })
            .expect("real frame-shapes");
        let constraint = crate::stages::constraint_shapes::ConstraintShapesStage
            .run(StageInput {
                root: &root,
                upstream: &empty,
            })
            .expect("real constraint-shapes");
        let result_shapes = crate::stages::result_shapes::ResultShapesStage
            .run(StageInput {
                root: &root,
                upstream: &empty,
            })
            .expect("real result-shapes");

        let mut with_source: BTreeMap<String, StageProduct> = BTreeMap::new();
        with_source.insert("stage-source-load".to_string(), source_load.product);
        with_source.insert("stage-compile-logic".to_string(), compile.product.clone());
        with_source.insert("stage-export-frame-shapes".to_string(), frame.product);
        with_source.insert(
            "stage-export-constraint-shapes".to_string(),
            constraint.product,
        );
        with_source.insert(
            "stage-export-result-shapes".to_string(),
            result_shapes.product,
        );

        let validate = crate::stages::validate::ValidateStage::new()
            .run(StageInput {
                root: &root,
                upstream: &with_source,
            })
            .expect("real validate");

        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert("stage-validate".to_string(), validate.product);
        upstream.insert("stage-compile-logic".to_string(), compile.product);

        let model = DocsModel::discover(&root).expect("real docs model discovery");
        let known_term_iris: BTreeSet<String> = model.terms.iter().map(|t| t.iri.clone()).collect();
        let digest =
            diagnostics_digest_from_upstream(&upstream, &known_term_iris, &model.constraint_rules)
                .expect("real diagnostics digest folds from validate + compile-logic products");

        // Precondition (weaker proxy): some findings were folded at all.
        assert!(
            digest.total > 0,
            "the diagnostics digest total must be non-vacuous on the real repo (B1) — the docs \
             diagnostics surface must never ship with zero folded findings"
        );

        // The TRUE B1 bar: the per-term join must actually connect — at least one
        // DOCUMENTED term carries a "Diagnostics you might hit" row. An empty `by_term`
        // means every term page's diagnostics panel renders blank ("No diagnostics
        // recorded against this term in the current build.") on real data.
        assert!(
            !digest.by_term.is_empty(),
            "B1 (true bar): DiagnosticsDigest.by_term must be NON-EMPTY on the real repo — at \
             least one DOCUMENTED term must carry a real diagnostic on its page, not just a \
             non-zero raw total. An empty by_term means the diagnostics→term join is vacuous \
             and every one of the ~2357 term pages ships the blank 'No diagnostics recorded' panel"
        );

        // The concrete documented term the real MinCount violations honestly concern:
        // the constrained property `gmeow:hasReferenceFrame` (the `sh:path` of the
        // ExpressionFrameRequirement shape), NOT the ABox fixture individuals that
        // tripped it. Its page must carry the SHACL MinCount diagnostic.
        let has_reference_frame = "https://blackcatinformatics.ca/gmeow/hasReferenceFrame";
        let rows = digest.by_term.get(has_reference_frame).unwrap_or_else(|| {
            panic!(
                "B1 (true bar): the documented property <{has_reference_frame}> must carry its \
                 constrained-property MinCount diagnostics in by_term; got keys {:?}",
                digest.by_term.keys().collect::<Vec<_>>()
            )
        });
        assert!(
            rows.iter()
                .any(|r| r.code == "shacl.MinCountConstraintComponent"),
            "B1 (true bar): <{has_reference_frame}>'s per-term rows must include the \
             shacl.MinCountConstraintComponent violation it constrains; got {rows:?}"
        );
    }

    /// B2 (real-repo non-vacuity, symmetric with the B1/B3 gates above): the per-term
    /// projection-loss digest [`gmeow_docs::model::TermLossDigest`] folded from the REAL
    /// `stage-mappings` product over the real ontology must carry at least one
    /// `property-path:<iri>` row. Root cause (gap G1): `compile_logic`'s
    /// `SOURCE_PATH` (`slices/grounding/logic/module.ttl`) carries only the
    /// `logic:PathShape` VOCABULARY; the two authored INSTANCES (`ex:nearbyOrgs`,
    /// `ex:ancestorsTo3`) live in the worked example
    /// `slices/grounding/logic/examples/predicate-paths.ttl` and were never ingested, so
    /// `program.path_shapes` was empty and `paths::project_path_shapes` emitted zero rows
    /// — this is the hard-fail gate that catches a regression back to that vacuous state.
    /// Runs the real `source_load` → `compile_logic` → `mappings` stage chain directly
    /// (each `Stage::run` call is pure in-memory — no disk write; the committed
    /// `generated/` tree is untouched), mirroring the B3 `term_entailments_are_non_vacuous_
    /// on_the_real_repo` chaining pattern in `carrier.rs`.
    #[test]
    fn term_loss_digest_is_non_vacuous_on_the_real_repo() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap();
        let empty: BTreeMap<String, StageProduct> = BTreeMap::new();

        // compile-logic reads the narrowed graph/logic-compile-inputs corpus off the
        // source-load product, so run source-load first and carry it as its upstream.
        let source_load = crate::stages::source_load::SourceLoadStage::new()
            .run(StageInput {
                root: &root,
                upstream: &empty,
            })
            .expect("real source-load");
        let mut compile_upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        compile_upstream.insert("stage-source-load".to_string(), source_load.product);
        let compile = crate::stages::compile_logic::CompileLogicStage::new()
            .run(StageInput {
                root: &root,
                upstream: &compile_upstream,
            })
            .expect("real compile-logic");
        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        upstream.insert("stage-compile-logic".to_string(), compile.product);

        let constraint_shapes = crate::stages::constraint_shapes::ConstraintShapesStage
            .run(StageInput {
                root: &root,
                upstream: &empty,
            })
            .expect("real constraint-shapes");
        upstream.insert(
            "stage-export-constraint-shapes".to_string(),
            constraint_shapes.product,
        );

        let mappings = crate::stages::mappings::MappingsStage::new()
            .run(StageInput {
                root: &root,
                upstream: &upstream,
            })
            .expect("real mappings");
        upstream.insert("stage-mappings".to_string(), mappings.product);

        let model = DocsModel::discover(&root).expect("real docs model discovery");
        let digest = term_loss_digest_from_upstream(&upstream, &model.shapes, &model.terms)
            .expect("real term-loss digest folds from the stage-mappings product");

        // The weaker proxy: at least one `property-path:<iri>` ledger row exists.
        // This alone passed while the HEADLINE per-term surface (`by_term`) was
        // empty, so it is NOT the true bar — it is retained only as a precondition.
        assert!(
            digest.total_property_path_rows >= 1,
            "G1/B2: total_property_path_rows must be non-vacuous on the real repo — the \
             authored logic:PathShape worked examples (ex:nearbyOrgs, ex:ancestorsTo3) must \
             produce real property-path:<iri> ledger rows, not zero"
        );

        // The TRUE B2 bar: the per-term join must actually connect — at least one
        // DOCUMENTED term carries a per-term projection-loss row. A `property-path`
        // row joins a term only if its bare shape IRI resolves to a documented
        // `DocTerm.iri` (or a `DocShape.target_term`), so an empty `by_term` means
        // the authored PathShapes never became documented terms and the headline
        // surface ships vacuous on every one of the ~2357 term pages. The two
        // authored worked-example PathShapes are documented Individual terms, so
        // each must carry its own per-term loss row here.
        assert!(
            !digest.by_term.is_empty(),
            "G1/B2 (true bar): TermLossDigest.by_term must be NON-EMPTY on the real repo — \
             at least one DOCUMENTED term must carry a per-term projection-loss row, not just \
             a whole-program `property-path:<iri>` count. An empty by_term means the per-term \
             join is vacuous and every term page's projection-loss table renders blank"
        );
        for shape_iri in [
            "https://blackcatinformatics.ca/gmeow/examples/logic/nearbyOrgs",
            "https://blackcatinformatics.ca/gmeow/examples/logic/ancestorsTo3",
        ] {
            let rows = digest.by_term.get(shape_iri).unwrap_or_else(|| {
                panic!(
                    "G1/B2 (true bar): the authored logic:PathShape term <{shape_iri}> must be a \
                     documented term carrying its own per-term projection-loss row in by_term; \
                     got keys {:?}",
                    digest.by_term.keys().collect::<Vec<_>>()
                )
            });
            assert!(
                rows.iter()
                    .any(|r| r.target == format!("property-path:{shape_iri}")),
                "G1/B2 (true bar): term <{shape_iri}>'s per-term rows must include its own \
                 property-path:<iri> projection-loss row; got {rows:?}"
            );
        }

        // The CORRECTED B2 bar (source-term-attribution reframing): the per-term loss table must surface
        // what the canonical `logic:`/`gmeow:` core loses when projected DOWN to a lossy
        // surface (OWL/EL/Datalog/SPARQL/SSSOM/…). At least one CORE documented term — NOT an
        // example worked shape under `.../gmeow/examples/` — must carry a projection-loss row
        // attributed structurally via `gmeow:lossySourceTerm` (the correspondence/SSSOM
        // down-projections attribute each aligned `gmeow:` term's dropped alignment
        // distinction to its page). An empty CORE set means the general term-attribution join
        // is inert and only the two worked-example path shapes ever light up.
        const EXAMPLE_NS: &str = "https://blackcatinformatics.ca/gmeow/examples/";
        let core_terms: Vec<&String> = digest
            .by_term
            .keys()
            .filter(|iri| !iri.starts_with(EXAMPLE_NS))
            .collect();
        assert!(
            !core_terms.is_empty(),
            "G1/B2 (corrected bar): at least one CORE documented term (NOT a \
             .../gmeow/examples/ worked shape) must carry a per-term projection-loss row — the \
             canonical core's loss when projected DOWN must land on real term pages. Only \
             example path shapes lit up; by_term keys = {:?}",
            digest.by_term.keys().collect::<Vec<_>>()
        );
        // Every CORE row must be honestly attributed (a real projection target + at least one
        // dropped feature), never an empty placeholder.
        for term in &core_terms {
            let rows = &digest.by_term[*term];
            assert!(
                rows.iter()
                    .all(|r| !r.target.is_empty() && !r.lossy_drops.is_empty()),
                "G1/B2 (corrected bar): CORE term <{term}> carries a malformed loss row \
                 (empty target or no dropped feature); rows = {rows:?}"
            );
        }
    }

    /// The EXACT production `make sync SYNC_OUTPUTS=docs` fanout path: build the docs model via
    /// `DocsModel::discover` (no live pipeline product — the standalone render),
    /// source the schema-fragment digest off the committed `generated/schemas/*.json`
    /// via [`schema_fragments_from_generated`] (the production sibling reader), attach
    /// it, and render a real modeled class's term page. Proves that after the
    /// production discover+attach the model's `schema_fragments` is populated AND that
    /// the per-term Python (Pydantic) + Rust example tabs actually render — the gate the
    /// dark feature lacked (the tabs return `None` whenever `schema_fragments` is
    /// `None`, which was ALWAYS the case on the shipped surface before this wiring).
    /// This deliberately does NOT hand-call `attach_schema_fragments` with a synthetic
    /// digest: it exercises the real disk-sourced producer end-to-end.
    #[test]
    fn make_docs_render_populates_schema_fragments_and_renders_python_rust_tabs() {
        use gmeow_docs::model::{DocTermCategory, DocsModel};
        use gmeow_docs::render::{Page, term_slug, to_markdown};

        let root = repo_root();
        let mut model = DocsModel::discover(&root).expect("discover live docs model");

        // The production disk-sourced digest — NOT a hand-attached synthetic one.
        let digest = schema_fragments_from_generated(&root, &model.terms)
            .expect("build schema-fragment digest from committed generated/schemas/*.json");
        assert!(
            !digest.schema_by_term.is_empty(),
            "the committed JSON Schema must join at least one documented class \
             (an empty digest means the tabs would never render)"
        );
        model.attach_schema_fragments(digest);
        assert!(
            model.schema_fragments.is_some(),
            "attach_schema_fragments must populate the model's schema_fragments"
        );

        // Pick a real modeled CLASS carrying a schema fragment — the exact key both
        // example-tab providers read — deterministically (first by iri).
        let fragments = model.schema_fragments.as_ref().unwrap();
        let mut modeled: Vec<&gmeow_docs::model::DocTerm> = model
            .terms
            .iter()
            .filter(|t| {
                t.category == DocTermCategory::Class
                    && fragments.schema_by_term.contains_key(&t.iri)
            })
            .collect();
        modeled.sort_by(|a, b| a.iri.cmp(&b.iri));
        let term = modeled
            .first()
            .copied()
            .expect("at least one modeled class carries a schema fragment")
            .clone();

        let md = to_markdown(&model, &Page::Term(term_slug(&term)));
        assert!(
            md.contains("## Example in multiple syntaxes"),
            "the modeled class term page must render the multi-syntax example section"
        );
        assert!(
            md.contains("from gmeow_models.") && md.contains(".model_validate("),
            "the Python (Pydantic) example tab must render on the production term page"
        );
        assert!(
            md.contains("purrdf::parse_turtle(") && md.contains("gmeow_validate::validate("),
            "the Rust example tab must render on the production term page"
        );
    }
}
