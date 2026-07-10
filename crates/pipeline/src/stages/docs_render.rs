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
/// The per-term join key is each finding's FIRST `Location` whose `logical` is
/// `Some` — matched by EXACT string equality against `known_term_iris`, never a
/// heuristic/fuzzy match. A finding whose first logical location names no known
/// term (or that carries no logical location at all) simply has no `by_term`
/// entry (an honest absence, not a bug).
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

        let term_candidate = finding
            .locations
            .iter()
            .find_map(|loc| loc.logical.as_deref());
        if let Some(term_iri) = term_candidate
            && known_term_iris.contains(term_iri)
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
                if let RdfTerm::Iri(object) = &q.object
                    && object == LOGIC_PROJECTION_TARGET_TYPE
                {
                    target_subjects.insert(subject.clone());
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
    for rows in by_term.values_mut() {
        rows.sort_by(|a, b| a.target.cmp(&b.target));
    }

    Ok(gmeow_docs::model::TermLossDigest {
        by_term,
        total_property_path_rows,
    })
}

/// Fold the per-term JSON Schema / OpenAPI fragment digest
/// [`gmeow_docs::model::SchemaFragmentDigest`] from the LIVE
/// `stage-export-json-schema` product's committed `gmeow.schema.json` /
/// `gmeow.openapi.json` artifacts ([`crate::stages::json_schema::JSON_SCHEMA_PATH`]
/// / [`crate::stages::json_schema::OPENAPI_PATH`]) — the SAME bytes the carrier
/// folds into the packed `schemas-archive`, read in-memory off the already-emitted
/// product (never a re-run of the emitter, never a `generated/` disk read). Each
/// documented CLASS whose emitter def key
/// ([`Namespaces::def_key`](purrdf::shapes::json_schema::Namespaces::def_key): a
/// bare local name for a primary-namespace class, a CURIE otherwise) names a
/// `$defs` (respectively `components/schemas`) entry gets that fragment,
/// pretty-printed deterministically. A class with no matching entry is honestly
/// absent (no fabricated stub); the emitter's synthetic `Node`/`Annotation` keys
/// (a whole-schema discriminator + the RDF-1.2 reifier-metadata fragment) are
/// never joined. Hard-fails when the declared `stage-export-json-schema` product /
/// artifact is absent or its bytes fail to parse as JSON (never a silently empty
/// digest).
pub(crate) fn schema_fragments_from_upstream(
    upstream: &BTreeMap<String, StageProduct>,
    terms: &[gmeow_docs::model::DocTerm],
) -> Result<gmeow_docs::model::SchemaFragmentDigest, gmeow_errors::Diag> {
    let product = upstream.get("stage-export-json-schema").ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-docs-render".to_string(),
            message: "missing stage-export-json-schema product for the schema-fragment digest"
                .to_string(),
        })
    })?;
    let read_json = |path: &str| -> Result<serde_json::Value, gmeow_errors::Diag> {
        let bytes = product.artifact(path).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-docs-render".to_string(),
                message: format!(
                    "missing stage-export-json-schema artifact {path} for the schema-fragment digest"
                ),
            })
        })?;
        serde_json::from_slice(bytes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-docs-render".to_string(),
                message: format!("parse {path} JSON for the schema-fragment digest: {e}"),
            })
        })
    };
    let schema = read_json(crate::stages::json_schema::JSON_SCHEMA_PATH)?;
    let openapi = read_json(crate::stages::json_schema::OPENAPI_PATH)?;
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

    Ok(gmeow_docs::model::SchemaFragmentDigest {
        schema_by_term,
        openapi_by_term,
    })
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
/// surface never fabricates a "carried exactly" claim. The per-term
/// content-address provenance is read from the committed manifest (self-healing
/// on a term-adding build; see `gmeow_docs::model::DocsModel::discover`).
pub fn render_docs_graph(
    root: &Path,
    verdict: ReasoningVerdict,
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<String, gmeow_errors::Diag> {
    let mut model = DocsModel::discover(root).map_err(|e| {
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
    Ok(to_gmeow_rdf(&model))
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
    // The committed term content manifest: the docs model reads it for each term's
    // content digest, first-seen version, and computed changelog, so a manifest edit
    // must bust the docs cache (cache soundness).
    let term_manifest = root.join(crate::stages::term_manifest::TERM_MANIFEST_RDF_PATH);
    if term_manifest.is_file() {
        files.push(term_manifest);
    }
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
    /// join digest (the term page's "Diagnostics you might hit" surface), and
    /// `stage-mappings` so it carries the dynamic per-term projection-loss join
    /// digest (the term page's "how this term degrades under projection" surface).
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-compile-logic".to_string(),
                "stage-gts-compose".to_string(),
                "stage-mappings".to_string(),
                "stage-reason".to_string(),
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
    fn impl_version(&self) -> &str {
        // v6: adds `term_loss_digest_from_upstream`, folding the dynamic per-term
        // projection-loss join from the `stage-mappings` product's live
        // `GRAPH_PROJECTION_LEDGER` graph. Bumped so the cache re-derives the
        // rendered graph now that a new upstream product feeds it.
        "docs_render.v6"
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
}
