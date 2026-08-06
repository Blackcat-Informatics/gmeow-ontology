// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The three cell executors and their per-file aggregators.
//!
//! `datatest-stable` invokes one aggregator per discovered `tests/*.ttl` spec
//! file; the aggregator runs every cell in the file and collects each failure,
//! so one nextest case reports ALL failing cells in that file (each anchored to
//! its cell IRI) rather than only the first.
//!
//! * Competency questions run over the merged ontology ([`crate::stores`]):
//!   the asserted graph by default, or its RDFS closure when the question sets
//!   `gmeow:cqReasoning gmeow:reasoningRdfs`.
//! * Structural assertions run a SPARQL ASK over the slice module alone, or the
//!   module plus its `examples/`, per `gmeow:saScope`.
//! * Example-conformance fixtures validate the bound example against the slice
//!   module + shapes via the native SHACL engine and compare finding codes. The
//!   grounding kernel is the sole data-scope exception: its three co-foundational
//!   modules are visible together, while shape authority remains slice-owned.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_errors::{Diag, Result, Severity};
use gmeow_logic::math_expression::check_math_expression_findings;
use gmeow_logic::reason::math_gate::dimension_gate_markers;
use gmeow_logic_compile::result_shape::{ObservedBinding, ObservedTerm};
use gmeow_validate::findings::finding_from_shacl;
use gmeow_validate::lint::{LintConfig, structural_lint_dataset};
use purrdf::shapes::engine::{parse_shapes, validate_dataset};
use purrdf::shapes::shapes::Shapes;
use purrdf::{RdfDataset, RdfTerm, SparqlResult, TermValue};

use crate::dsl::{
    self, CompetencyQuestion, ExampleConformance, ExpectedRow, Outcome, Polarity, ReasoningProfile,
    Scope, StructuralAssertion,
};
use crate::error::{
    CellAggregate, CompetencyMismatch, ConformanceCell, DatasetRead, ExampleDiscovery, QueryLoad,
    ShapeValidation, SparqlEval, StructuralCell,
};
use crate::native_query::{self, render_term, union};
use crate::paths;
use crate::stores::{merged_store, native_closed_store, rdfs_closed_store};

/// A canonical (variable-name, term-N-Triples) binding set for one result row,
/// sorted so row identity is independent of projection/iteration order.
type CanonRow = Vec<(String, String)>;

// ── Per-file aggregators (the datatest-stable entry points) ─────────────────────

/// Run every competency question in a `competency.ttl` spec file.
///
/// # Errors
///
/// Hard-fails with a diagnostic aggregating each failing cell's diagnostic.
pub fn run_competency_file(path: &Path) -> Result<()> {
    let spec = dsl::load_spec(path)?;
    let slice_dir = paths::slice_dir(path);
    // The asserted merged graph is the default lane and is always built once.
    // The RDFS-closed graph is built lazily — only if some question opts into it
    // via gmeow:cqReasoning gmeow:reasoningRdfs — and then reused across cells.
    let merged = merged_store()?;
    let mut rdfs: Option<Arc<RdfDataset>> = None;
    // The native logic:-reasoned closure is built lazily — only if some question opts into
    // gmeow:reasoningLogic — and then reused across cells (it pays the native chase once).
    let mut native: Option<Arc<RdfDataset>> = None;

    // Build an IRI→question index so gmeow:cqConsumes can resolve its producer
    // within this spec file. Built once outside the loop (O(n log n)), reused
    // per-cell (O(log n) lookup).
    let by_iri: std::collections::BTreeMap<&str, &CompetencyQuestion> = spec
        .competency
        .iter()
        .map(|cq| (cq.iri.as_str(), cq))
        .collect();

    let mut results: Vec<(&str, Result<()>)> = Vec::with_capacity(spec.competency.len());
    for cq in &spec.competency {
        // Composition pre-check: if this question declares a gmeow:cqConsumes
        // dependency, verify the producer's output satisfies this question's
        // declared input contract BEFORE running the query. Hard-fail, surfaced.
        if let Some(producer_iri) = &cq.consumes {
            let pre_check = (|| -> Result<()> {
                let producer = by_iri.get(producer_iri.as_str()).ok_or_else(|| {
                    Diag::of_kind(CompetencyMismatch {
                        detail: format!(
                            "gmeow:cqConsumes references unknown question <{producer_iri}> (not declared in this spec file)"
                        ),
                    })
                })?;
                let producer_shape = producer.result_shape.as_ref().ok_or_else(|| {
                    Diag::of_kind(CompetencyMismatch {
                        detail: format!(
                            "producer <{producer_iri}> has no gmeow:cqResultShape — cannot satisfy the input contract"
                        ),
                    })
                })?;
                let input_shape = cq.input_shape.as_ref().ok_or_else(|| {
                    Diag::of_kind(CompetencyMismatch {
                        detail: format!(
                            "gmeow:cqConsumes requires a paired gmeow:cqInputShape on <{}>",
                            cq.iri
                        ),
                    })
                })?;
                input_shape.is_satisfiable_by(producer_shape).map_err(|e| {
                    Diag::of_kind(CompetencyMismatch {
                        detail: format!(
                            "input-shape composition contract (checked before execution): {e}"
                        ),
                    })
                })
            })();
            if let Err(e) = pre_check {
                results.push((cq.iri.as_str(), Err(e)));
                continue;
            }
        }

        let store: &Arc<RdfDataset> = match cq.reasoning {
            ReasoningProfile::None => &merged,
            ReasoningProfile::Rdfs => {
                if rdfs.is_none() {
                    rdfs = Some(rdfs_closed_store()?);
                }
                rdfs.as_ref().expect("rdfs store just built")
            }
            ReasoningProfile::Native => {
                if native.is_none() {
                    native = Some(native_closed_store()?);
                }
                native.as_ref().expect("native store just built")
            }
        };
        results.push((cq.iri.as_str(), run_competency_cell(store, cq, &slice_dir)));
    }
    aggregate(path, "competency", results.into_iter())
}

/// Run every structural assertion in a `structural.ttl` spec file.
///
/// # Errors
///
/// Hard-fails with a diagnostic aggregating each failing cell's diagnostic.
pub fn run_structural_file(path: &Path) -> Result<()> {
    let spec = dsl::load_spec(path)?;
    let slice_dir = paths::slice_dir(path);
    // Every cell in a spec file draws its dataset from one of exactly two scopes
    // (`gmeow:scopeModule` or `gmeow:scopeModuleAndExamples`), and a large rubric
    // module can carry many structural cells over the SAME scope. Build each
    // scoped dataset at most once per file and reuse it across cells (the same
    // lazy-cache-across-cells shape `run_competency_file` already uses for its
    // RDFS/native closures above) rather than re-unioning + re-planning the
    // identical dataset once per cell.
    let mut module_only: Option<Arc<RdfDataset>> = None;
    let mut module_and_examples: Option<Arc<RdfDataset>> = None;
    aggregate(
        path,
        "structural",
        spec.structural.iter().map(|sa| {
            (
                sa.iri.as_str(),
                run_structural_cell(sa, &slice_dir, &mut module_only, &mut module_and_examples),
            )
        }),
    )
}

/// Run every example-conformance fixture in an `example-conformance.ttl` file.
///
/// # Errors
///
/// Hard-fails with a diagnostic aggregating each failing cell's diagnostic.
pub fn run_conformance_file(path: &Path) -> Result<()> {
    let spec = dsl::load_spec(path)?;
    let slice_dir = paths::slice_dir(path);
    // Every cell validates against the SAME enforcing surface (the slice's conformance
    // data + the
    // canonical shape set), and a migrated slice's surface is the whole generated
    // validation/constraint/procedural projection, so the per-cell whole-surface SHACL
    // runs dominate the file's wall time. The cells are independent, so fan them across
    // a bounded set of worker threads. The parsed shape set and module dataset are
    // built ONCE and shared by reference: both are frozen, `Sync` values (the purrdf
    // SPARQL layer's caches live in thread-local engine state, not in `Shapes` /
    // `RdfDataset`, whose lazy indexes are `OnceLock`-guarded), so re-parsing the
    // whole generated surface once per worker would only multiply setup cost —
    // measured as the dominant term on a many-core box, where a migrated slice's
    // ~0.7 MB three-file surface was re-parsed by every one of N workers.
    let shape_paths = paths::shapes_files(&slice_dir);
    let shapes_ttl = shape_paths
        .iter()
        .map(|path| {
            std::fs::read_to_string(path).map_err(|e| {
                Diag::of_kind(DatasetRead {
                    detail: format!("cannot read {}: {e}", path.display()),
                })
            })
        })
        .collect::<Result<Vec<_>>>()?
        .join("\n");
    // Built ONCE from the raw (unscoped) shapes text: a `sh:NodeShape IRI ->
    // gmeow:enforcesFailureClass IRI` index, so a `gmeow:expectedFailureClass` cell can
    // resolve a reported `sh:sourceShape` to the semantic class it enforces. See
    // `shape_failure_class_index`'s own doc comment for why this reads unscoped.
    let failure_class_index = shape_failure_class_index(&shapes_ttl)?;
    // Surface a malformed shape set / module ONCE, as a typed diagnostic, before fanning out.
    let shapes = parse_shapes(&shapes_ttl).map_err(|e| {
        Diag::of_kind(ShapeValidation {
            detail: format!("parsing slice shapes: {e}"),
        })
    })?;
    // Shape ownership is recovered from the tested slice's own module. Validation
    // data is normally that same module, except that the grounding contract makes
    // logic:, lang:, and math: one co-foundational kernel. Their conformance cells
    // therefore see all three canonical modules, which lets a lang: denotation of a
    // math:-owned class observe its authoritative owl:Class type without duplicating
    // that declaration in lang: (Principle 4).
    let owned_module =
        native_query::dataset_from_file(&paths::module_file(&slice_dir)).map_err(|e| {
            Diag::of_kind(DatasetRead {
                detail: format!("building module dataset: {e}"),
            })
        })?;
    // The shape set is the GENERATED SHACL projection, written against the OWL/RDFS
    // surface; the module is the CANONICAL authored surface. Lower the module's
    // canonical subsumption edges into their `rdfs:` projection (once, shared by
    // every cell) so both sides of the validation speak the same surface — see
    // `native_query::with_rdfs_subsumption_projection`.
    let module = native_query::with_rdfs_subsumption_projection(
        &native_query::dataset_from_files(&paths::conformance_module_files(&slice_dir)).map_err(
            |e| {
                Diag::of_kind(DatasetRead {
                    detail: format!("building conformance module dataset: {e}"),
                })
            },
        )?,
    );
    // The module's own constant reading of the two whole-dataset native channels,
    // measured ONCE and subtracted per cell (see `NativeChannelBaseline`).
    let baseline = NativeChannelBaseline::measure(&module)?;
    let local_shapes = slice_dir.join("shapes.ttl");
    let shapes = scope_shapes_to_slice(
        shapes,
        &shapes_ttl,
        &owned_module,
        local_shapes.is_file().then_some(local_shapes.as_path()),
    )?;
    let workers = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
        .min(spec.conformance.len().max(1));
    let cells: Vec<&ExampleConformance> = spec.conformance.iter().collect();
    let mut results: Vec<(usize, Result<()>)> = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for w in 0..workers {
            let cells = &cells;
            let slice_dir = &slice_dir;
            let shapes = &shapes;
            let module = &module;
            let failure_class_index = &failure_class_index;
            let baseline = &baseline;
            handles.push(scope.spawn(move || {
                let mut out = Vec::new();
                for (i, ec) in cells.iter().enumerate() {
                    if i % workers == w {
                        out.push((
                            i,
                            run_conformance_cell(
                                ec,
                                slice_dir,
                                module,
                                shapes,
                                failure_class_index,
                                baseline,
                            ),
                        ));
                    }
                }
                out
            }));
        }
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("conformance worker thread joins"))
            .collect()
    });
    results.sort_by_key(|(i, _)| *i);
    aggregate(
        path,
        "example-conformance",
        results
            .into_iter()
            .map(|(i, r)| (spec.conformance[i].iri.as_str(), r)),
    )
}

/// Collect per-cell results, returning one aggregated error if any failed.
fn aggregate<'a>(
    path: &Path,
    kind: &str,
    cells: impl Iterator<Item = (&'a str, Result<()>)>,
) -> Result<()> {
    let mut count = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for (iri, result) in cells {
        count += 1;
        if let Err(e) = result {
            failures.push(format!("  • [{iri}] {e}"));
        }
    }
    if failures.is_empty() {
        return Ok(());
    }
    Err(Diag::of_kind(CellAggregate {
        detail: format!(
            "{} of {count} {kind} cell(s) failed in {}:\n{}",
            failures.len(),
            path.display(),
            failures.join("\n")
        ),
    }))
}

// ── Competency ──────────────────────────────────────────────────────────────────

fn run_competency_cell(
    store: &Arc<RdfDataset>,
    cq: &CompetencyQuestion,
    slice_dir: &Path,
) -> Result<()> {
    let query = load_query(cq)?;

    // The dataset the cqQuery runs over starts as the (asserted or RDFS) merged store.
    // A gmeow:cqDataFile overlays a slice-relative ABox fixture by UNION (the IR is
    // immutable — `RdfDataset::union` produces a fresh dataset, standardizing the
    // fixture's blanks apart, so the shared base is never mutated and there is nothing
    // to remove afterwards).
    let base: Arc<RdfDataset> = match &cq.data_file {
        None => Arc::clone(store),
        Some(rel) => {
            if cq.reasoning != ReasoningProfile::None {
                // The RDFS/native closure is computed BEFORE the overlay, so an overlaid
                // fixture's entailments would be invisible. Refuse rather than silently
                // under-answer.
                return Err(Diag::of_kind(CompetencyMismatch {
                    detail: format!(
                        "{}: gmeow:cqDataFile is only honoured in the asserted (reasoningNone) lane, \
                         not gmeow:reasoningRdfs / gmeow:reasoningLogic",
                        cq.iri
                    ),
                }));
            }
            let fixture_path = paths::example_file(slice_dir, rel);
            let fixture = native_query::dataset_from_file(&fixture_path).map_err(|e| {
                Diag::of_kind(DatasetRead {
                    detail: format!("parsing cqDataFile {}: {e}", fixture_path.display()),
                })
            })?;
            union(&[Arc::clone(store), fixture])
        }
    };

    // Optional projection step: a gmeow:cqProject names a CONSTRUCT query that
    // MATERIALIZES a computed projection (e.g. the flat upper-projection edges that
    // are the one-hop collapse of the de-conflation canon) over the overlaid dataset
    // BEFORE the cqQuery runs. Its constructed triples are UNIONED in, so the question
    // is answered against the materialized projection rather than a hand-asserted copy
    // — which is what makes the projection-agreement gate non-circular.
    let dataset: Arc<RdfDataset> = match &cq.project_query_file {
        None => base,
        Some(rel) => {
            let path = paths::query_file(rel);
            let construct = std::fs::read_to_string(&path).map_err(|e| {
                Diag::of_kind(DatasetRead {
                    detail: format!("cannot read cqProject {}: {e}", path.display()),
                })
            })?;
            match native_query::query(&base, &construct).map_err(|e| {
                Diag::of_kind(SparqlEval {
                    detail: format!("cqProject query error: {e}"),
                })
            })? {
                SparqlResult::Graph(g) => union(&[base, g]),
                _ => {
                    return Err(Diag::of_kind(CompetencyMismatch {
                        detail: format!(
                            "{}: gmeow:cqProject must be a CONSTRUCT query (returning a graph)",
                            cq.iri
                        ),
                    }));
                }
            }
        }
    };

    execute_competency_query(&dataset, cq, &query)
}

/// Execute a competency question's (already-resolved) query over `store` and
/// check the result against its expectation.
fn execute_competency_query(
    store: &Arc<RdfDataset>,
    cq: &CompetencyQuestion,
    query: &str,
) -> Result<()> {
    let results = native_query::query(store, query).map_err(|e| {
        Diag::of_kind(SparqlEval {
            detail: format!("query error: {e}"),
        })
    })?;

    match results {
        SparqlResult::Boolean(actual) => {
            let expected = cq.expect_ask.ok_or_else(|| {
                Diag::of_kind(CompetencyMismatch {
                    detail: "ASK query but no gmeow:cqExpectAsk on the question".to_owned(),
                })
            })?;
            if actual != expected {
                return Err(Diag::of_kind(CompetencyMismatch {
                    detail: format!("ASK expected {expected}, got {actual}"),
                }));
            }
            Ok(())
        }
        SparqlResult::Solutions {
            variables, rows, ..
        } => {
            let mut actual: Vec<CanonRow> = Vec::new();
            // The observed (var, term-kind) bindings, kept alongside the canonical
            // rows so the declared output shape can type-check them.
            let mut observed: Vec<Vec<ObservedBinding>> = Vec::new();
            for solution in &rows {
                let mut row: CanonRow = Vec::new();
                let mut obs: Vec<ObservedBinding> = Vec::new();
                for (v, cell) in variables.iter().zip(solution) {
                    if let Some(t) = cell {
                        row.push((v.clone(), render_term(t)));
                        obs.push(ObservedBinding::new(
                            v.clone(),
                            observed_term(&cq.iri, v, t)?,
                        ));
                    }
                }
                row.sort();
                actual.push(row);
                observed.push(obs);
            }
            // Output contract: type-check the bindings against the declared result
            // shape (term-kind / datatype / requiredness / cardinality) BEFORE the
            // example-row comparison. Hard-fail, surfaced.
            if let Some(shape) = &cq.result_shape {
                shape.validate_bindings(&observed).map_err(|e| {
                    Diag::of_kind(CompetencyMismatch {
                        detail: format!("result-shape contract: {e}"),
                    })
                })?;
            }
            check_select(cq, &actual)
        }
        SparqlResult::Graph(_) => Err(Diag::of_kind(CompetencyMismatch {
            detail: "competency query must be ASK or SELECT, got CONSTRUCT/DESCRIBE".to_owned(),
        })),
    }
}

/// Compare a SELECT competency question's actual rows against its expectation.
fn check_select(cq: &CompetencyQuestion, actual: &[CanonRow]) -> Result<()> {
    let actual_set: BTreeSet<CanonRow> = actual.iter().cloned().collect();
    let expected_set: BTreeSet<CanonRow> =
        cq.expected_rows.iter().map(canon_expected_row).collect();

    if let Some(want) = cq.expect_row_count {
        // Escape-hatch tier: pin the count, and any enumerated sample rows must
        // be a subset of the actual result.
        let got = actual.len() as u64;
        if got != want {
            return Err(Diag::of_kind(CompetencyMismatch {
                detail: format!("expected {want} rows, got {got}"),
            }));
        }
        return missing_rows(&expected_set, &actual_set).map_err(|m| {
            Diag::of_kind(CompetencyMismatch {
                detail: format!("sample row(s) absent from result: {m}"),
            })
        });
    }

    // Enumerated tier: the question must carry expected rows.
    if cq.expected_rows.is_empty() {
        return Err(Diag::of_kind(CompetencyMismatch {
            detail:
                "SELECT competency question has neither gmeow:cqExpectRowCount nor gmeow:cqExpectRow"
                    .to_owned(),
        }));
    }
    if cq.exact_rows {
        if actual_set != expected_set {
            let missing = set_diff(&expected_set, &actual_set);
            let extra = set_diff(&actual_set, &expected_set);
            return Err(Diag::of_kind(CompetencyMismatch {
                detail: format!(
                    "exact-row mismatch: {} expected-but-absent, {} unexpected (missing={missing}; extra={extra})",
                    expected_set.difference(&actual_set).count(),
                    actual_set.difference(&expected_set).count(),
                ),
            }));
        }
        Ok(())
    } else {
        missing_rows(&expected_set, &actual_set).map_err(|m| {
            Diag::of_kind(CompetencyMismatch {
                detail: format!("expected row(s) absent from result: {m}"),
            })
        })
    }
}

fn missing_rows(expected: &BTreeSet<CanonRow>, actual: &BTreeSet<CanonRow>) -> Result<()> {
    let missing = set_diff(expected, actual);
    if missing.is_empty() {
        Ok(())
    } else {
        Err(Diag::of_kind(CompetencyMismatch { detail: missing }))
    }
}

fn set_diff(a: &BTreeSet<CanonRow>, b: &BTreeSet<CanonRow>) -> String {
    a.difference(b)
        .map(|row| {
            let cells: Vec<String> = row.iter().map(|(v, t)| format!("?{v}={t}")).collect();
            format!("{{{}}}", cells.join(", "))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `rdf:langString` — the stable identity datatype of a language-tagged literal.
const LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";
/// `rdf:dirLangString` — the RDF-1.2 effective datatype of a *directional*
/// language-tagged literal; normalised to [`LANG_STRING`] for contract checking.
const DIR_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString";

/// Project one native RDF 1.2 result binding into the pure-data [`ObservedTerm`]
/// the result-shape contract checks.
fn observed_term(_cq_iri: &str, _var: &str, term: &TermValue) -> Result<ObservedTerm> {
    Ok(match term {
        TermValue::Iri(_) => ObservedTerm::Iri,
        TermValue::Blank { .. } => ObservedTerm::BlankNode,
        TermValue::Literal { datatype, .. } => {
            // An RDF-1.2 directional language-tagged literal reports the effective
            // datatype rdf:dirLangString, but a column declares the stable identity
            // datatype rdf:langString (the same convention crates/rdf-wasm uses):
            // normalise so a directional literal conforms to a langString column
            // rather than false-positiving a DatatypeMismatch.
            let datatype = if datatype == DIR_LANG_STRING {
                LANG_STRING.to_owned()
            } else {
                datatype.clone()
            };
            ObservedTerm::Literal { datatype }
        }
        TermValue::Triple { .. } => ObservedTerm::TripleTerm,
    })
}

fn canon_expected_row(row: &ExpectedRow) -> CanonRow {
    let mut cells: CanonRow = row
        .cells
        .iter()
        .map(|c| (c.var.clone(), render_term(&c.value)))
        .collect();
    cells.sort();
    cells
}

fn load_query(cq: &CompetencyQuestion) -> Result<String> {
    match (&cq.query_inline, &cq.query_file) {
        (Some(q), None) => Ok(q.clone()),
        (None, Some(rel)) => {
            let p = paths::query_file(rel);
            std::fs::read_to_string(&p).map_err(|e| {
                Diag::of_kind(QueryLoad {
                    detail: format!("cannot read cqQueryFile {}: {e}", p.display()),
                })
            })
        }
        (Some(_), Some(_)) => Err(Diag::of_kind(QueryLoad {
            detail: "competency question sets both cqQuery and cqQueryFile".to_owned(),
        })),
        (None, None) => Err(Diag::of_kind(QueryLoad {
            detail: "competency question sets neither cqQuery nor cqQueryFile".to_owned(),
        })),
    }
}

// ── Structural ──────────────────────────────────────────────────────────────────

fn run_structural_cell(
    sa: &StructuralAssertion,
    slice_dir: &Path,
    module_only: &mut Option<Arc<RdfDataset>>,
    module_and_examples: &mut Option<Arc<RdfDataset>>,
) -> Result<()> {
    let pattern = match (&sa.pattern, &sa.shape) {
        (Some(p), None) => p,
        (None, Some(shape)) => {
            // No T2 exemplar exercises saShape; fail loudly rather than silently
            // pass (the no-optionality / hard-fail doctrine).
            return Err(Diag::of_kind(StructuralCell {
                detail: format!(
                    "saShape execution is not yet implemented (shape {shape}); refusing to silently pass"
                ),
            }));
        }
        (Some(_), Some(_)) => {
            return Err(Diag::of_kind(StructuralCell {
                detail: "assertion sets both saPattern and saShape".to_owned(),
            }));
        }
        (None, None) => {
            return Err(Diag::of_kind(StructuralCell {
                detail: "assertion sets neither saPattern nor saShape".to_owned(),
            }));
        }
    };

    let cache = match sa.scope {
        Scope::Module => &mut *module_only,
        Scope::ModuleAndExamples => &mut *module_and_examples,
    };
    let store = match cache {
        Some(store) => Arc::clone(store),
        None => {
            let mut sources = vec![paths::module_file(slice_dir)];
            if sa.scope == Scope::ModuleAndExamples {
                sources.extend(example_ttls(&paths::examples_dir(slice_dir))?);
            }
            let built = native_query::dataset_from_files(&sources).map_err(|e| {
                Diag::of_kind(DatasetRead {
                    detail: format!("building scoped dataset for structural assertion: {e}"),
                })
            })?;
            *cache = Some(Arc::clone(&built));
            built
        }
    };
    let holds = run_ask(&store, pattern)?;

    match (sa.polarity, holds) {
        (Polarity::Must, false) => {
            return Err(Diag::of_kind(StructuralCell {
                detail: "polarity 'must' but the ASK pattern did NOT hold".to_owned(),
            }));
        }
        (Polarity::MustNot, true) => {
            return Err(Diag::of_kind(StructuralCell {
                detail: "polarity 'mustNot' but the ASK pattern HELD".to_owned(),
            }));
        }
        _ => {}
    }

    // Teeth check (gmeow:saFailWitness): a `scopeModule` ban is an ASK over the slice's
    // own module, which by construction never carries the banned triple — so the ban
    // could be a typo or a dead pattern and still pass vacuously. When a fail-witness
    // fixture is declared, run the SAME pattern over module ∪ fixture and REQUIRE the
    // polarity to be violated there (a `mustNot` ban must now HOLD; a `must` ban must
    // now FAIL). The fixture supplies exactly the banned pattern the real module must
    // never hold; if it fails to trip the ban, the ban is vacuous and this is a hard
    // fail. This is deliberately independent of `sa.scope`: the witness is always
    // unioned with the module only (never examples), isolating the injected violation.
    if let Some(witness_rel) = &sa.fail_witness {
        let witness_path = slice_dir.join(witness_rel);
        let witnessed = native_query::dataset_from_files(&[
            paths::module_file(slice_dir),
            witness_path.clone(),
        ])
        .map_err(|e| {
            Diag::of_kind(DatasetRead {
                detail: format!(
                    "building module+fail-witness dataset ({}): {e}",
                    witness_path.display()
                ),
            })
        })?;
        let witness_holds = run_ask(&witnessed, pattern)?;
        let tripped = match sa.polarity {
            Polarity::Must => !witness_holds,
            Polarity::MustNot => witness_holds,
        };
        if !tripped {
            let pol = match sa.polarity {
                Polarity::Must => "must",
                Polarity::MustNot => "mustNot",
            };
            return Err(Diag::of_kind(StructuralCell {
                detail: format!(
                    "fail-witness {witness_rel} did NOT trip the '{pol}' ban — the assertion \
                     is vacuous: the fixture must supply the banned pattern so the ban \
                     demonstrably has teeth"
                ),
            }));
        }
    }

    Ok(())
}

fn run_ask(store: &Arc<RdfDataset>, query: &str) -> Result<bool> {
    match native_query::query(store, query).map_err(|e| {
        Diag::of_kind(SparqlEval {
            detail: format!("saPattern error: {e}"),
        })
    })? {
        SparqlResult::Boolean(b) => Ok(b),
        SparqlResult::Solutions { .. } | SparqlResult::Graph(_) => {
            Err(Diag::of_kind(StructuralCell {
                detail: "saPattern must be a SPARQL ASK query".to_owned(),
            }))
        }
    }
}

/// Every `*.ttl` directly under a slice's `examples/` dir (sorted; empty if the
/// directory is absent).
fn example_ttls(examples_dir: &Path) -> Result<Vec<PathBuf>> {
    // An absent examples/ dir is normal (→ no examples). Any OTHER read error
    // (permissions, I/O) must propagate, not masquerade as "no examples": that
    // would silently run a scopeModuleAndExamples assertion against module-only
    // data and could pass spuriously.
    let entries = match std::fs::read_dir(examples_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(Diag::of_kind(ExampleDiscovery {
                detail: format!("read_dir {}: {e}", examples_dir.display()),
            }));
        }
    };
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| {
            Diag::of_kind(ExampleDiscovery {
                detail: format!("read_dir entry under {}: {e}", examples_dir.display()),
            })
        })?;
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|x| x == "ttl") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

// ── Example conformance ─────────────────────────────────────────────────────────

fn run_conformance_cell(
    ec: &ExampleConformance,
    slice_dir: &Path,
    module: &Arc<RdfDataset>,
    shapes: &purrdf::shapes::shapes::Shapes,
    failure_class_index: &BTreeMap<String, String>,
    baseline: &NativeChannelBaseline,
) -> Result<()> {
    let example_path = paths::example_file(slice_dir, &ec.file);
    // Lowered onto the shape set's own OWL/RDFS surface for the same reason the
    // module is (`native_query::with_rdfs_subsumption_projection`): an example that
    // authors a canonical subsumption edge must be visible to a shape written
    // against the projection.
    let example = native_query::with_rdfs_subsumption_projection(
        &native_query::dataset_from_file(&example_path).map_err(|e| {
            Diag::of_kind(DatasetRead {
                detail: format!("parsing example {}: {e}", example_path.display()),
            })
        })?,
    );

    // Validate (module + example) against the slice shapes. The IR is immutable, so
    // instead of an in-place insert/remove overlay we UNION the module and example
    // into a fresh dataset (blanks standardized apart) and validate that — exactly
    // the validation-path example idiom, no shared store to restore.
    let data = union(&[module.clone(), example]);
    let report = validate_dataset(&data, shapes).map_err(|e| {
        Diag::of_kind(ShapeValidation {
            detail: format!("native SHACL validation failed: {e}"),
        })
    })?;

    let codes: BTreeSet<String> = report
        .results
        .iter()
        .map(|r| finding_from_shacl(r).code)
        .collect();

    match ec.outcome {
        Outcome::Conforms => {
            // SHACL conformance is gated by `sh:Violation` results ONLY — `Info`/
            // `Warning` results (e.g. the advisory-tier `logic:severity "Info"`
            // constraints) are non-gating per spec, so an advisory finding must
            // NOT turn an "expected conformance" cell into a failure. Filter to
            // Violation-severity results before deciding (mirrors
            // `gmeow_validate::advisory::split_advisory_results`'s recomputed
            // `conforms`, and `crates/validate/tests/example_sweep.rs`'s
            // `conforms_to_shacl`).
            let violations = || {
                report
                    .results
                    .iter()
                    .filter(|r| matches!(r.severity, purrdf::shapes::report::Severity::Violation))
            };
            // The NATIVE channel counts here too. A "conforms" cell that consults only
            // SHACL is green while `check_math_expression_findings` reports an error over
            // the very same fixture — which is how three positive fixtures for the
            // content-key contract shipped carrying hand-guessed `math:structuralKey`
            // literals that `gmeow validate --deep` rejects. A fixture the native
            // expression-identity gate ERRORS on does not conform, whichever channel
            // decided it — this arm adds that channel, and claims nothing about SHACL
            // parity between the harness's module-unioned graph and a bare CLI run.
            let native: Vec<gmeow_errors::Finding> = check_math_expression_findings(&data, &data)
                .into_iter()
                .filter(|f| f.severity == Severity::Error)
                .collect();
            if violations().next().is_none() && native.is_empty() {
                Ok(())
            } else if violations().next().is_none() {
                Err(Diag::of_kind(ConformanceCell {
                    detail: format!(
                        "expected conformance, and SHACL agrees, but the native math: \
                         expression gate reports {} error(s): {}",
                        native.len(),
                        native
                            .iter()
                            .map(|f| format!("{} {}", f.code, f.message))
                            .collect::<Vec<_>>()
                            .join("; ")
                    ),
                }))
            } else {
                let codes: BTreeSet<String> =
                    violations().map(|r| finding_from_shacl(r).code).collect();
                let locations = violations()
                    .map(|result| {
                        let finding = finding_from_shacl(result);
                        let path = result
                            .result_path
                            .as_ref()
                            .map_or_else(|| "<node>".to_owned(), ToString::to_string);
                        format!(
                            "{} at {} on {} (shape {})",
                            finding.code, result.focus_node, path, result.source_shape
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                let native_detail = if native.is_empty() {
                    String::new()
                } else {
                    format!(
                        "; native math: expression gate also reports: {}",
                        native
                            .iter()
                            .map(|f| format!("{} {}", f.code, f.message))
                            .collect::<Vec<_>>()
                            .join("; ")
                    )
                };
                Err(Diag::of_kind(ConformanceCell {
                    detail: format!(
                        "expected conformance, got finding(s): {}; {}{native_detail}",
                        join_codes(&codes),
                        locations
                    ),
                }))
            }
        }
        Outcome::Violates => {
            // `gmeow:expectedFailureClass` is a wholly alternative, STRONGER check: it
            // reaches native (non-SHACL) failure classes that carry no finding code at
            // all, and it requires ISOLATION across BOTH channels (no unmatched finding
            // anywhere), not merely "the expected code is present somewhere". A cell
            // that sets it does not also need `gmeow:expectedViolationCode`.
            if let Some(expected_class) = ec.expected_failure_class.as_deref() {
                return check_failure_class_isolation(
                    ec,
                    &report,
                    &data,
                    expected_class,
                    failure_class_index,
                    baseline,
                );
            }
            let expected = ec.violation_code.as_deref().ok_or_else(|| {
                Diag::of_kind(ConformanceCell {
                    detail: "outcome 'violates' but no gmeow:expectedViolationCode".to_owned(),
                })
            })?;
            if !codes.contains(expected) {
                return Err(Diag::of_kind(ConformanceCell {
                    detail: format!(
                        "expected violation {expected}, got finding(s): {}",
                        join_codes(&codes)
                    ),
                }));
            }
            // GAP 4: every `logic:Constraint` projects to the SAME generic finding
            // component (`shacl.SPARQLConstraintComponent`), so a component-code match
            // alone cannot prove the SPECIFIC named rule fired — the counter-example
            // could be tripping a DIFFERENT constraint that happens to share the code.
            // When the cell pins `gmeow:expectedSourceShape`, additionally require that
            // at least one finding carrying the expected code ALSO originates from that
            // source shape. A cell that omits it keeps the pre-GAP-4 behaviour (the
            // component-code match above is conclusive) — so no other slice's cells,
            // which never set it, are affected.
            if let Some(expected_shape) = ec.expected_source_shape.as_deref() {
                let from_shapes: Vec<String> = report
                    .results
                    .iter()
                    .filter(|r| finding_from_shacl(r).code == expected)
                    .map(|r| strip_angle(&r.source_shape.to_string()).to_owned())
                    .collect();
                if !from_shapes
                    .iter()
                    .any(|shape| source_shape_matches(shape, expected_shape))
                {
                    return Err(Diag::of_kind(ConformanceCell {
                        detail: format!(
                            "cell {} expected a {expected} finding from shape {expected_shape}, \
                             but the {expected} finding(s) came from shape(s): [{}]",
                            ec.iri,
                            from_shapes.join(", ")
                        ),
                    }));
                }
            }
            // EXHAUSTIVENESS. Everything above is an EXISTENCE check: some finding
            // carries the expected code, and some finding carrying it comes from the
            // pinned shape. Neither can fail on a fixture that ALSO trips three other
            // laws — which is exactly what a rationale saying "and NO other finding"
            // claims it does not. That claim was therefore unfalsifiable: it could
            // only ever have been established by measuring the fixture once, by hand,
            // and nothing kept it true afterwards. `gmeow:expectedSoleFinding true`
            // makes it checkable. Deliberately opt-in: the phrase appeared on 238 cells
            // across nine slices, and turning the check on for all of them by default
            // would red slices whose fixtures were never measured — the check has to be
            // adopted cell by cell, with the finding set actually read.
            //
            // "Sole" is per-LAW, and the LAW is the `sh:sourceShape` — not the finding
            // code and not the result count. The unit matters:
            //
            // * Per-RESULT would forbid ONE shape from reporting one defect at several
            //   focus nodes, which a class shape targeting a class legitimately does.
            // * Per-CODE would split ONE shape's report of ONE defect into a failure
            //   whenever the shape raises two components over it (a class shape reporting
            //   a missing member as both sh:minCount and sh:qualifiedMinCount is one law
            //   saying one thing twice).
            //
            // Per-SHAPE is therefore the STRICTEST unit that does not misfire on one law
            // speaking more than once, and it is deliberately strict in the other
            // direction: when one defect is reported by a class shape AND by that class's
            // superclass shape, or by a derived shape AND by the residual hand-authored
            // twin a partially-migrated slice still ships in its `shapes.ttl`, that IS two
            // shapes and the cell may not claim soleness. Those are real duplications of
            // authority, and a cell that wants to stay honest about them pins the law with
            // `gmeow:expectedSourceShape` and says in its rationale what else speaks.
            //
            // So: every violation-severity result must originate from the ONE shape the
            // cell names. Any second shape is a second authored law, which is exactly the
            // claim "and NO other finding" makes and could not previously fail on.
            //
            // NAMING THE LAW IS PART OF THE CLAIM, so `gmeow:expectedSourceShape` is
            // REQUIRED here rather than optional. The first cut let an unpinned cell fall
            // back to "no OTHER shape raised a finding carrying the expected code", and
            // that reading is vacuous on every generic component: `MinCountConstraintComponent`
            // and `SPARQLConstraintComponent` are each raised by dozens of shapes, so the
            // fallback accepted any second law that happened to raise the same code — it
            // asserted almost nothing on 151 of the 175 cells that had adopted the flag,
            // and 30 of those cells were in fact tripping two or three distinct laws. Two
            // further reasons the fallback had to go rather than be repaired into "any
            // second shape is an intruder":
            //
            // * A soleness claim is a claim about WHICH law is the only one. "Exactly one
            //   law fired" without naming it still cannot fail when a fixture drifts onto
            //   a DIFFERENT single law raising the same generic component — GAP 4 above,
            //   which is the whole reason `gmeow:expectedSourceShape` exists.
            // * It would give one term two meanings, selected silently by whether a
            //   sibling property happens to be bound. A declared claim whose strength
            //   depends on an absent input is exactly the silent degradation the
            //   no-optionality rule forbids: a missing input is a HARD FAIL.
            //
            // `shapes/test-dsl-shapes.ttl` states the same requirement declaratively, so
            // the pairing is rejected at DSL-lint time as well as here.
            if ec.expected_sole_finding == Some(true) {
                let pinned = ec.expected_source_shape.as_deref().ok_or_else(|| {
                    Diag::of_kind(ConformanceCell {
                        detail: format!(
                            "cell {} declares gmeow:expectedSoleFinding true without \
                             gmeow:expectedSourceShape: soleness is a claim about WHICH law is \
                             the only one, so the law must be named. Bind \
                             gmeow:expectedSourceShape to the sh:sourceShape IRI the {expected} \
                             finding originates from.",
                            ec.iri
                        ),
                    })
                })?;
                let intruders: Vec<String> = report
                    .results
                    .iter()
                    .filter(|r| matches!(r.severity, purrdf::shapes::report::Severity::Violation))
                    .filter(|r| {
                        !source_shape_matches(strip_angle(&r.source_shape.to_string()), pinned)
                    })
                    .map(|r| {
                        format!(
                            "{} on {} (shape {})",
                            finding_from_shacl(r).code,
                            r.focus_node,
                            r.source_shape
                        )
                    })
                    .collect();
                if !intruders.is_empty() {
                    return Err(Diag::of_kind(ConformanceCell {
                        detail: format!(
                            "cell {} declares gmeow:expectedSoleFinding, so the shape raising \
                             {expected} ({pinned}) must be the ONLY law this fixture trips; it \
                             also raised: [{}]",
                            ec.iri,
                            intruders.join(", "),
                        ),
                    }));
                }
            }
            Ok(())
        }
    }
}

/// Whether a finding's reported `sh:sourceShape` IRI satisfies a cell's
/// `gmeow:expectedSourceShape` binding: an exact IRI match, or (fallback) the
/// actual IRI ending in the expected IRI's local name, so a cell may pin the shape
/// by local name even when the validator reports the fully-qualified derived
/// NodeShape IRI.
fn source_shape_matches(actual: &str, expected: &str) -> bool {
    if actual == expected {
        return true;
    }
    let local = expected.rsplit(['/', '#']).next().unwrap_or(expected);
    !local.is_empty() && actual.ends_with(local)
}

/// Strip the surrounding `<>` from a rendered IRI term (`<https://…>` → `https://…`).
fn strip_angle(term: &str) -> &str {
    term.strip_prefix('<')
        .and_then(|t| t.strip_suffix('>'))
        .unwrap_or(term)
}

/// Build a `sh:NodeShape IRI -> gmeow:enforcesFailureClass IRI` index from the raw
/// (repository-wide, unscoped) shapes Turtle text. The derive pipeline projects
/// `gmeow:enforcesFailureClass` directly onto the generated shape node itself — the
/// SAME IRI a SHACL engine reports as a violation's `sh:sourceShape` — for both
/// `logic:Constraint`-derived procedural shapes and plain OWL-restriction-derived
/// validation shapes alike, so one generic query over either generated surface reaches
/// every annotated shape regardless of which derive path produced it. Read unscoped
/// (not through `scope_shapes_to_slice`'s slice-owned filter) because a finding's
/// reported source shape may legitimately belong to a co-foundational grounding module
/// (`logic:`/`lang:`/`math:`) the cell's own slice does not own.
fn shape_failure_class_index(shapes_ttl: &str) -> Result<BTreeMap<String, String>> {
    let dataset = native_query::dataset_from_turtle(shapes_ttl)?;
    let solutions = native_query::select(
        &dataset,
        "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>\n\
         SELECT ?shape ?class WHERE { ?shape gmeow:enforcesFailureClass ?class }",
    )?;
    let shape_idx = solutions.variables.iter().position(|v| v == "shape");
    let class_idx = solutions.variables.iter().position(|v| v == "class");
    let mut index = BTreeMap::new();
    if let (Some(si), Some(ci)) = (shape_idx, class_idx) {
        for row in &solutions.rows {
            if let (Some(Some(TermValue::Iri(shape))), Some(Some(TermValue::Iri(class)))) =
                (row.get(si), row.get(ci))
            {
                index.insert(shape.clone(), class.clone());
            }
        }
    }
    Ok(index)
}

/// The MODULE's own contribution to the two whole-dataset native channels, computed
/// ONCE per spec file and subtracted from every cell's reading of them.
///
/// Both the native structural lint and the reasoner-derived dimension gate are run over
/// the validated graph — `module ∪ example` — because that is the graph the other two
/// channels see and the graph a native check needs in order to resolve a fixture node's
/// peer-owned types. But the module is in that union for EVERY cell, so whatever those
/// channels say about the module alone is a constant, identical for all 286 cells, and
/// says nothing whatever about the fixture under test. (The `math:` module's own
/// structural lint reports eight dangling-subsumption-target diagnostics about
/// `gmeow:`-owned superclasses it does not itself declare; counted per cell, they would
/// red every isolation claim in the slice while naming a defect no fixture caused.)
///
/// Subtracting a measured baseline is the exact way to attribute the remainder to the
/// fixture: it is not a token filter, a severity carve-out, or an allow-list — it drops
/// EXACTLY the findings the module produces with no example loaded at all, so a fixture
/// that genuinely trips one of those same checks still shows up (its finding names the
/// fixture's own node, so it is a different message and survives the difference).
struct NativeChannelBaseline {
    /// Error-severity structural-lint messages the module alone produces.
    lint_errors: BTreeSet<String>,
    /// `(subject, failure-class)` dimension-gate markers the module alone produces.
    dimension_markers: BTreeSet<(String, String)>,
}

impl NativeChannelBaseline {
    /// Measure the module-only reading of both native channels.
    ///
    /// # Errors
    ///
    /// Propagates a dimension-gate chase failure, which its own contract defines as a
    /// genuine internal-invariant violation (non-stratifiable rules, or a declined native
    /// forward chase) rather than a missing-data condition — never silently swallowed,
    /// because an empty baseline would over-report every cell instead of under-reporting.
    fn measure(module: &Arc<RdfDataset>) -> Result<Self> {
        Ok(Self {
            lint_errors: structural_lint_dataset(module, &conformance_lint_config())
                .errors()
                .into_iter()
                .collect(),
            dimension_markers: dimension_gate_markers(module, &[])
                .map_err(|e| {
                    Diag::of_kind(ShapeValidation {
                        detail: format!(
                            "the reasoner-derived math: dimension gate failed over the slice \
                             module alone: {e}"
                        ),
                    })
                })?
                .into_iter()
                .collect(),
        })
    }
}

/// The lint configuration the conformance channel runs under: the GMEOW vocabulary
/// namespace and ontology IRI, and no selector-token / core-slice grading.
///
/// The checks this channel exists to reach — the `math:` probability, distribution,
/// dependency-model, projection and ingest invariants — are decided from the data's own
/// asserted types and values, so they are namespace-grading-independent and a bare config
/// exercises them fully. It is the same shape `gmeow_validate`'s own integration tests
/// and the pipeline execution-discharge harnesses build.
fn conformance_lint_config() -> LintConfig {
    LintConfig {
        namespace: gmeow_ns::GMEOW_NS.to_owned(),
        ontology_iri: "https://blackcatinformatics.ca/gmeow".to_owned(),
        selector_tokens: BTreeSet::new(),
        core_slice_iris: std::collections::HashSet::new(),
        annotation_predicates: std::collections::HashSet::new(),
    }
}

/// Every `<prefix>:<ClassName>:` class-token embedded in a native Rust finding's
/// message, resolved to full IRIs against the namespace prefixes the native
/// validators (`crates/logic/src/math_expression.rs`, `crates/validate/src/lint.rs`)
/// use in their `<prefix>:<LocalName>: ` message-token convention documented on
/// `crate::math_expression::failure_class_local_name` — the ONLY place a native
/// (non-SHACL) finding names the semantic failure class(es) it decided, since its
/// `Finding::code` is one stable string per gate FUNCTION, shared by every class that
/// function can raise (e.g. `math:StructuralKeyOnRejectedExpression`'s code is shared
/// with whichever `MathLoweringError` variant rejected the underlying expression, so a
/// finding routed through that gate embeds BOTH its own class token and the inner
/// rejection's class token).
fn message_class_tokens(message: &str) -> BTreeSet<String> {
    const PREFIXES: &[(&str, &str)] = &[
        ("math:", "https://blackcatinformatics.ca/math/"),
        ("logic:", "https://blackcatinformatics.ca/logic/"),
        ("lang:", "https://blackcatinformatics.ca/lang/"),
        ("gmeow:", "https://blackcatinformatics.ca/gmeow/"),
    ];
    let mut tokens = BTreeSet::new();
    for (prefix, ns) in PREFIXES {
        let mut rest = message;
        while let Some(idx) = rest.find(prefix) {
            let after = &rest[idx + prefix.len()..];
            let end = after
                .find(|c: char| !c.is_ascii_alphanumeric())
                .unwrap_or(after.len());
            let name = &after[..end];
            let starts_upper = name.chars().next().is_some_and(|c| c.is_ascii_uppercase());
            if starts_upper && after[end..].starts_with(':') {
                tokens.insert(format!("{ns}{name}"));
            }
            rest = if end < after.len() {
                &after[end + 1..]
            } else {
                ""
            };
            if rest.is_empty() {
                break;
            }
        }
    }
    tokens
}

/// `gmeow:expectedFailureClass` isolation check: EVERY finding produced across ALL FOUR
/// executed channels must name `expected_class`. This is the ONLY conformance-cell
/// mechanism that reaches native (non-SHACL) failure classes, and it closes two gaps at
/// once: a SPARQL-derived gate's shared generic component code letting an unrelated
/// cross-fire finding hide beside the intended one, and a purely native class (no SHACL
/// derivation exists at all, e.g. `math:StructuralKeyDrift`) having no
/// conformance-cell mechanism to assert against whatsoever.
///
/// The four channels, and why each is here:
///
/// 1. **native SHACL** Violation-severity results, resolved to a class through the
///    generated shape's own `gmeow:enforcesFailureClass` (`failure_class_index`).
/// 2. **the native `math:` expression-identity gate**
///    ([`check_math_expression_findings`]), resolved via [`message_class_tokens`] — the
///    only reader of `math:structuralKey` / normal-form identity, which carries no SHACL
///    derivation at all.
/// 3. **the native structural lint** (`gmeow_validate::lint::structural_lint_dataset`),
///    resolved the same way. Without it a whole tier of the slice's authored rules —
///    every obligation decided by arithmetic or by a cross-node join with no SHACL target
///    shape: probability-magnitude bounds, distribution-parameter positivity and
///    dimension, dependency-model completeness, exact-preservation mass, the projection
///    loss ledger, ingest liftability — was invisible to the isolation authority. Their
///    counter-examples could therefore not be celled AT ALL: no SHACL finding exists to
///    pin, so a cell naming their class would have failed with "no finding matched it"
///    while the rule was in fact firing, one channel over.
/// 4. **the reasoner-derived measure-and-dimension gate**
///    (`gmeow_logic::reason::math_gate::dimension_gate_markers`), which decides
///    `math:DimensionalInhomogeneity` by exact ℚ⁷ exponent arithmetic. Its projected
///    SHACL shape targets `math:homogeneousOperandRel`, a reified relation that exists
///    only in a reasoned closure, so over asserted fixture data it can never fire and the
///    class was likewise uncellable.
///
/// Channels 3 and 4 read the validated `module ∪ example` graph, with the module's own
/// constant contribution subtracted ([`NativeChannelBaseline`]); channel 2 reads the
/// asserted graph as both substrates, because conformance cells pin the fixture AS
/// AUTHORED and a cell expecting a derived surface leak would be pinning an entailment
/// rather than a fixture. Channel 4 is likewise given no derived edges for the same
/// reason: the marker it credits must be one the fixture's own asserted structure
/// entails.
fn check_failure_class_isolation(
    ec: &ExampleConformance,
    report: &purrdf::shapes::report::ValidationReport,
    data: &Arc<RdfDataset>,
    expected_class: &str,
    failure_class_index: &BTreeMap<String, String>,
    baseline: &NativeChannelBaseline,
) -> Result<()> {
    // One (description, matches-expected) pair per finding across every channel.
    let mut findings: Vec<(String, bool)> = Vec::new();

    for r in report
        .results
        .iter()
        .filter(|r| matches!(r.severity, purrdf::shapes::report::Severity::Violation))
    {
        let finding = finding_from_shacl(r);
        let shape = strip_angle(&r.source_shape.to_string()).to_owned();
        let class = failure_class_index.get(&shape).cloned();
        let matches = class.as_deref() == Some(expected_class);
        findings.push((
            format!(
                "shacl {} at {} (shape {shape}, class {})",
                finding.code,
                r.focus_node,
                class.as_deref().unwrap_or("<unmapped>")
            ),
            matches,
        ));
    }

    // Conformance cells validate the fixture as AUTHORED, so the asserted graph is both
    // substrates here: the grammar half is what these cells pin, and a cell that expected a
    // derived surface leak would be pinning an entailment, not a fixture.
    for finding in check_math_expression_findings(data, data) {
        if finding.severity != Severity::Error {
            continue;
        }
        let tokens = message_class_tokens(&finding.message);
        let matches = tokens.contains(expected_class);
        let token_list = if tokens.is_empty() {
            "<no class token>".to_owned()
        } else {
            tokens.into_iter().collect::<Vec<_>>().join(", ")
        };
        findings.push((
            format!(
                "native {} ({token_list}): {}",
                finding.code, finding.message
            ),
            matches,
        ));
    }

    // The native structural lint: the slice's arithmetic / cross-node-join tier. Only the
    // messages the EXAMPLE adds over the module's own constant reading are the fixture's.
    for message in structural_lint_dataset(data, &conformance_lint_config()).errors() {
        if baseline.lint_errors.contains(&message) {
            continue;
        }
        let tokens = message_class_tokens(&message);
        let matches = tokens.contains(expected_class);
        let token_list = if tokens.is_empty() {
            "<no class token>".to_owned()
        } else {
            tokens.into_iter().collect::<Vec<_>>().join(", ")
        };
        findings.push((format!("lint ({token_list}): {message}"), matches));
    }

    // The reasoner-derived measure-and-dimension gate. Each marker IS a failure-class
    // IRI, so it needs no message-token convention to resolve.
    let markers = dimension_gate_markers(data, &[]).map_err(|e| {
        Diag::of_kind(ShapeValidation {
            detail: format!(
                "cell {}: the reasoner-derived math: dimension gate failed — its own contract \
                 makes this a genuine internal-invariant violation (non-stratifiable rules or a \
                 declined native forward chase), never a missing-fixture condition, so it is a \
                 hard failure rather than an empty marker set the isolation check would read as \
                 \"the gate found nothing\": {e}",
                ec.iri
            ),
        })
    })?;
    for (subject, class) in markers {
        if baseline
            .dimension_markers
            .contains(&(subject.clone(), class.clone()))
        {
            continue;
        }
        let matches = class == expected_class;
        findings.push((format!("dimension-gate {subject} (class {class})"), matches));
    }

    let matched = findings.iter().filter(|(_, m)| *m).count();
    let unmatched: Vec<&str> = findings
        .iter()
        .filter(|(_, m)| !*m)
        .map(|(d, _)| d.as_str())
        .collect();

    if matched == 0 {
        return Err(Diag::of_kind(ConformanceCell {
            detail: format!(
                "cell {} expected failure class {expected_class}, but no finding (SHACL, native \
                 expression gate, structural lint, or dimension gate) matched it; findings \
                 observed: [{}]",
                ec.iri,
                findings
                    .iter()
                    .map(|(d, _)| d.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        }));
    }
    if !unmatched.is_empty() {
        return Err(Diag::of_kind(ConformanceCell {
            detail: format!(
                "cell {} expected ONLY failure class {expected_class}, but {} other finding(s) \
                 also fired: [{}]",
                ec.iri,
                unmatched.len(),
                unmatched.join("; ")
            ),
        }));
    }
    Ok(())
}

/// Restrict the repository-wide generated shape union to the authority owned by
/// one slice, plus that slice's residual authored shapes.
///
/// Generated validation files are deliberately repository-wide.  A partially
/// migrated slice must load them so migrated constraints remain live alongside
/// its residual `shapes.ttl`, but applying every other slice's shapes to this
/// slice's module would violate the documented slice-scoped conformance contract.
/// The grounding kernel may expose all three peer modules as validation data, but
/// that does not broaden shape ownership: a lang test still enforces lang shapes,
/// never every math or logic shape.
/// Ownership is recovered without filename heuristics from the canonical graph:
/// module terms name their ontology through `rdfs:isDefinedBy`, generated shapes
/// either derive from one of those terms (`*-shape` / `*-domain-shape`) or carry
/// `logic:formalizes` / `gmeow:enforcesFailureClass` back to one, and every local
/// residual node shape is owned by construction.
///
/// # Errors
///
/// Returns a typed validation diagnostic when the module has no recoverable
/// ontology authority, or when the generated/local shape metadata cannot be parsed.
pub fn scope_shapes_to_slice(
    mut shapes: Shapes,
    shapes_ttl: &str,
    module: &RdfDataset,
    local_shapes_path: Option<&Path>,
) -> Result<Shapes> {
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
    const OWL_ONTOLOGY: &str = "http://www.w3.org/2002/07/owl#Ontology";
    const SH_NODE_SHAPE: &str = "http://www.w3.org/ns/shacl#NodeShape";
    const LOGIC_FORMALIZES: &str = "https://blackcatinformatics.ca/logic/formalizes";
    const ENFORCES_FAILURE_CLASS: &str =
        "https://blackcatinformatics.ca/gmeow/enforcesFailureClass";

    let module_quads: Vec<_> = module.owned_quads().collect();
    let ontology_iris: BTreeSet<String> = module_quads
        .iter()
        .filter(|q| q.predicate.as_str() == RDF_TYPE)
        .filter_map(|q| match (&q.subject, &q.object) {
            (RdfTerm::Iri(subject), RdfTerm::Iri(object)) if object == OWL_ONTOLOGY => {
                Some(subject.clone())
            }
            _ => None,
        })
        .collect();
    if ontology_iris.is_empty() {
        return Err(Diag::of_kind(ShapeValidation {
            detail: "slice module declares no owl:Ontology authority".to_owned(),
        }));
    }

    let owned_terms: BTreeSet<String> = module_quads
        .iter()
        .filter(|q| q.predicate.as_str() == RDFS_IS_DEFINED_BY)
        .filter_map(|q| match (&q.subject, &q.object) {
            (RdfTerm::Iri(subject), RdfTerm::Iri(owner)) if ontology_iris.contains(owner) => {
                Some(subject.clone())
            }
            _ => None,
        })
        .collect();
    if owned_terms.is_empty() {
        return Err(Diag::of_kind(ShapeValidation {
            detail: "slice module authority owns no rdfs:isDefinedBy terms".to_owned(),
        }));
    }

    // A canonical logic:Constraint is owned through rdfs:isDefinedBy, while its
    // projected shape authority is the object of logic:formalizes and need not be
    // declared as a standalone ontology term. Include that one-hop projection
    // identity in the ownership set so the generated shape is not discarded.
    let mut owned_authorities = owned_terms.clone();
    owned_authorities.extend(module_quads.iter().filter_map(|q| {
        if q.predicate.as_str() != LOGIC_FORMALIZES {
            return None;
        }
        match (&q.subject, &q.object) {
            (RdfTerm::Iri(source), RdfTerm::Iri(authority)) if owned_terms.contains(source) => {
                Some(authority.clone())
            }
            _ => None,
        }
    }));

    let shape_graph = native_query::dataset_from_turtle(shapes_ttl)?;
    let metadata_owned_shapes: BTreeSet<String> = shape_graph
        .owned_quads()
        .filter(|q| {
            matches!(
                q.predicate.as_str(),
                LOGIC_FORMALIZES | ENFORCES_FAILURE_CLASS
            )
        })
        .filter_map(|q| match (q.subject, q.object) {
            (RdfTerm::Iri(shape), RdfTerm::Iri(authority))
                if owned_authorities.contains(&authority) =>
            {
                Some(shape)
            }
            _ => None,
        })
        .collect();

    let local_shape_ids = match local_shapes_path {
        None => BTreeSet::new(),
        Some(path) => native_query::dataset_from_file(path)?
            .owned_quads()
            .filter(|q| q.predicate.as_str() == RDF_TYPE)
            .filter_map(|q| match (q.subject, q.object) {
                (RdfTerm::Iri(shape), RdfTerm::Iri(kind)) if kind == SH_NODE_SHAPE => Some(shape),
                _ => None,
            })
            .collect(),
    };

    shapes.node_shapes.retain(|shape| {
        let rendered = shape.id.to_string();
        let Some(shape_iri) = rendered
            .strip_prefix('<')
            .and_then(|value| value.strip_suffix('>'))
        else {
            return false;
        };
        if local_shape_ids.contains(shape_iri) || metadata_owned_shapes.contains(shape_iri) {
            return true;
        }
        generated_shape_source(shape_iri).is_some_and(|source| owned_terms.contains(source))
    });

    Ok(shapes)
}

fn generated_shape_source(shape_iri: &str) -> Option<&str> {
    shape_iri
        .strip_suffix("-domain-shape")
        .or_else(|| shape_iri.strip_suffix("-shape"))
}

fn join_codes(codes: &BTreeSet<String>) -> String {
    if codes.is_empty() {
        "<none>".to_owned()
    } else {
        codes.iter().cloned().collect::<Vec<_>>().join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{CompetencyQuestion, ExpectedCell, ExpectedRow};
    use gmeow_logic_compile::result_shape::{
        ColumnKind, ResultColumn, ResultShape, RowCardinality,
    };

    /// Materialize inline Turtle into a native dataset via the canonical codec.
    fn store_from_turtle(ttl: &str) -> Arc<RdfDataset> {
        native_query::dataset_from_turtle(ttl).expect("valid turtle")
    }

    /// A minimal SELECT competency question over an inline query.
    fn cq_with(query: &str) -> CompetencyQuestion {
        CompetencyQuestion {
            iri: "https://example.org/cqShape".to_owned(),
            query_inline: Some(query.to_owned()),
            query_file: None,
            project_query_file: None,
            expect_ask: None,
            expect_row_count: None,
            exact_rows: false,
            expected_rows: Vec::new(),
            reasoning: ReasoningProfile::None,
            data_file: None,
            result_shape: None,
            input_shape: None,
            consumes: None,
            rationale: None,
        }
    }

    const Q_X: &str = "PREFIX ex: <https://example.org/> \
        SELECT ?x WHERE { ?x a ex:Thing }";

    fn one_thing_store() -> Arc<RdfDataset> {
        store_from_turtle("@prefix ex: <https://example.org/> .\nex:a a ex:Thing .\n")
    }

    #[test]
    fn generated_shape_union_is_scoped_back_to_the_slice_authority() {
        let module = store_from_turtle(
            r#"
            @prefix ex: <https://example.org/> .
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

            ex:Slice a owl:Ontology ; rdfs:isDefinedBy ex:Slice .
            ex:Owned a owl:Class ; rdfs:isDefinedBy ex:Slice .
            ex:OwnedConstraint rdfs:isDefinedBy ex:Slice ;
                logic:formalizes ex:OwnedShapeAuthority .
            "#,
        );
        let shapes_ttl = r#"
            @prefix ex: <https://example.org/> .
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            @prefix sh: <http://www.w3.org/ns/shacl#> .

            ex:Owned-shape a sh:NodeShape ; sh:targetClass ex:Owned .
            ex:OwnedProceduralShape a sh:NodeShape ;
                logic:formalizes ex:OwnedShapeAuthority ;
                sh:targetClass ex:Owned .
            ex:Foreign-shape a sh:NodeShape ; sh:targetClass ex:Foreign .
        "#;
        let parsed = parse_shapes(shapes_ttl).expect("shape union parses");
        let scoped = scope_shapes_to_slice(parsed, shapes_ttl, &module, None)
            .expect("shape union scopes to module authority");
        let ids: BTreeSet<String> = scoped
            .node_shapes
            .iter()
            .map(|shape| shape.id.to_string())
            .collect();

        assert_eq!(
            ids,
            BTreeSet::from([
                "<https://example.org/Owned-shape>".to_owned(),
                "<https://example.org/OwnedProceduralShape>".to_owned(),
            ])
        );
    }

    #[test]
    fn lang_gmn_nonlexical_guard_rejects_word_form_subclasses() {
        let spec_path = paths::repo_root().join("slices/grounding/lang/tests/structural.ttl");
        let spec = dsl::load_spec(&spec_path).expect("lang structural assertions parse");
        let pattern = spec
            .structural
            .iter()
            .find(|assertion| assertion.iri.ends_with("saGmnSignsAreNonLexicalForms"))
            .and_then(|assertion| assertion.pattern.as_deref())
            .expect("the GMN non-lexical structural ASK is present");
        let store = store_from_turtle(
            r#"
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
            @prefix lang: <https://blackcatinformatics.ca/lang/> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix ex: <https://example.org/> .

            lang:SyntacticWord rdfs:subClassOf lang:WordForm .
            ex:form a lang:Form, lang:SyntacticWord .
            ex:denotation a lang:Denotation ;
                gmeow:gmnDenotationGrapheme ex:glyph ;
                lang:denotedForm ex:form .
            "#,
        );

        assert!(
            !run_ask(&store, pattern).expect("the GMN non-lexical ASK executes"),
            "a directly situated GMN form must still be rejected when its specific type is a WordForm subclass"
        );
    }

    /// A `gmeow:saFailWitness` must actually TRIP the ban: over module ∪ fixture the
    /// assertion's pattern is required to be violated. A fixture that supplies the banned
    /// triple passes the teeth check; a fixture that does NOT supply it hard-fails — proving
    /// the teeth check is not itself vacuous (a `scopeModule` ban whose ASK is a typo or is
    /// dead would otherwise pass forever, since the real module never carries the banned
    /// pattern). This is the teeth of the teeth check.
    #[test]
    fn structural_fail_witness_requires_the_ban_to_trip() {
        let tmp = tempfile::tempdir().expect("temp slice dir");
        let dir = tmp.path();
        // The real module never carries the banned triple.
        std::fs::write(
            dir.join("module.ttl"),
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
        )
        .expect("write module");
        // A witness that DOES supply the banned pattern.
        std::fs::write(
            dir.join("witness-trips.ttl"),
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:preferredRank a owl:ObjectProperty .\n",
        )
        .expect("write tripping witness");
        // A witness that does NOT supply it (an unrelated triple).
        std::fs::write(
            dir.join("witness-inert.ttl"),
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:somethingElse a owl:ObjectProperty .\n",
        )
        .expect("write inert witness");

        let pattern = "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/> \
                       PREFIX owl: <http://www.w3.org/2002/07/owl#> \
                       ASK { gmeow:preferredRank a owl:ObjectProperty }";
        let tripping = StructuralAssertion {
            iri: "https://example.org/saBanned".to_owned(),
            polarity: Polarity::MustNot,
            pattern: Some(pattern.to_owned()),
            shape: None,
            scope: Scope::Module,
            fail_witness: Some("witness-trips.ttl".to_owned()),
            rationale: None,
        };
        // A tripping witness: normal check passes (module clean) AND the teeth check passes.
        let (mut mo, mut mae) = (None, None);
        run_structural_cell(&tripping, dir, &mut mo, &mut mae)
            .expect("a witness that supplies the banned pattern trips the mustNot ban");

        // An inert witness: the teeth check must hard-fail.
        let inert = StructuralAssertion {
            fail_witness: Some("witness-inert.ttl".to_owned()),
            ..tripping.clone()
        };
        let (mut mo2, mut mae2) = (None, None);
        let err = run_structural_cell(&inert, dir, &mut mo2, &mut mae2)
            .expect_err("a witness that fails to supply the banned pattern must hard-fail");
        assert!(
            err.message().contains("did NOT trip") && err.message().contains("vacuous"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn result_shape_conforming_bindings_pass() {
        let store = one_thing_store();
        let mut cq = cq_with(Q_X);
        // ?x is an IRI, required, exactly one row — matches the data.
        cq.result_shape = Some(ResultShape::new(
            vec![ResultColumn::required("x", ColumnKind::Iri)],
            RowCardinality::Count(1),
        ));
        cq.expect_row_count = Some(1); // satisfy the row-comparison tier too
        execute_competency_query(&store, &cq, Q_X).expect("conforming shape passes");
    }

    #[test]
    fn result_shape_term_kind_mismatch_hard_fails() {
        let store = one_thing_store();
        let mut cq = cq_with(Q_X);
        // ?x declared a literal, but the data binds an IRI.
        cq.result_shape = Some(ResultShape::new(
            vec![ResultColumn::required(
                "x",
                ColumnKind::Literal { datatype: None },
            )],
            RowCardinality::Contains,
        ));
        let err = execute_competency_query(&store, &cq, Q_X)
            .expect_err("term-kind mismatch must hard-fail");
        assert!(
            err.message().contains("result-shape contract") && err.message().contains("term-kind"),
            "unexpected error: {err}"
        );
    }

    /// The composition pre-check (`is_satisfiable_by`) surfaces a mismatch when
    /// the producer LACKS a column the consumer requires.  The input shape
    /// declares two required columns {x:IRI, y:IRI}; the producer only provides
    /// {x:IRI} — so `input.is_satisfiable_by(&producer)` must be `Err` with a
    /// `MissingColumn` variant naming "y".
    #[test]
    fn is_satisfiable_by_surfaces_missing_required_column() {
        use gmeow_logic_compile::result_shape::Mismatch;

        let input = ResultShape::new(
            vec![
                ResultColumn::required("x", ColumnKind::Iri),
                ResultColumn::required("y", ColumnKind::Iri),
            ],
            RowCardinality::Contains,
        );
        let producer = ResultShape::new(
            vec![ResultColumn::required("x", ColumnKind::Iri)],
            RowCardinality::Contains,
        );
        let err = input
            .is_satisfiable_by(&producer)
            .expect_err("producer missing required column must be Err");
        assert!(
            matches!(err, Mismatch::MissingColumn { ref var } if var == "y"),
            "expected MissingColumn {{ var: y }}, got: {err:?}"
        );
    }

    /// A `gmeow:cqDataFile` overlay must (a) make the fixture's instances visible to
    /// the query, (b) never leak into the shared base dataset (the frozen IR is
    /// immutable — the overlay is a UNION into a fresh dataset, so the base is
    /// untouched by construction), and (c) be rejected outright in the RDFS lane.
    #[test]
    fn cq_data_file_overlay_applies_and_is_removed() {
        let tmp = tempfile::tempdir().expect("temp slice dir");
        let dir = tmp.path();
        let fixture = "@prefix ex: <https://example.org/test/> .\n\
                       @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
                       ex:event1 a gmeow:Event .\n";
        std::fs::write(dir.join("data.ttl"), fixture).expect("write fixture");

        // Empty shared base: the only way the SELECT matches is via the overlay.
        let store = store_from_turtle("@prefix ex: <https://example.org/> .\n");
        let cq = CompetencyQuestion {
            iri: "https://example.org/test/cqOverlay".to_owned(),
            query_inline: Some(
                "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/> \
                 SELECT ?e WHERE { ?e a gmeow:Event }"
                    .to_owned(),
            ),
            query_file: None,
            project_query_file: None,
            expect_ask: None,
            expect_row_count: None,
            exact_rows: false,
            expected_rows: vec![ExpectedRow {
                cells: vec![ExpectedCell {
                    var: "e".to_owned(),
                    value: TermValue::Iri("https://example.org/test/event1".to_owned()),
                }],
            }],
            reasoning: ReasoningProfile::None,
            data_file: Some("data.ttl".to_owned()),
            result_shape: None,
            input_shape: None,
            consumes: None,
            rationale: None,
        };

        run_competency_cell(&store, &cq, dir).expect("overlay cell must pass");
        assert_eq!(
            store.quad_count(),
            0,
            "the overlay must not leak into the shared base dataset (the union builds a fresh one)"
        );

        // Same cell in the RDFS lane: hard-fail, never silently under-answer.
        let mut rdfs_cq = cq.clone();
        rdfs_cq.reasoning = ReasoningProfile::Rdfs;
        let err = run_competency_cell(&store, &rdfs_cq, dir)
            .expect_err("cqDataFile + reasoningRdfs must be rejected");
        assert!(
            err.message().contains("reasoningNone"),
            "unexpected error: {err}"
        );
    }

    /// `gmeow:expectedSoleFinding` can FAIL, on the production conformance surface.
    ///
    /// The red half drives the shipped `compensation-typed-as-its-forward-receipt.ttl`
    /// counter-example — the one fixture in the enactment corpus whose single authored
    /// defect is MEASURED to cascade into three laws — through the real
    /// `run_conformance_cell`, against the real generated shape surface, with the
    /// sole-finding flag set. Without the flag the cell passes, because every check
    /// before it is an EXISTENCE check: some finding carries the code, and some finding
    /// carrying it came from the pinned shape. That is precisely the gap that made
    /// "and NO other finding" unfalsifiable everywhere it is written, so the green half
    /// is the SAME cell with the flag unbound, and the difference between them is the
    /// whole of what the new field buys.
    #[test]
    fn the_sole_finding_flag_rejects_a_fixture_that_trips_a_second_law() {
        let slice_dir = paths::repo_root().join("slices/grounding/logic");
        let shape_paths = paths::shapes_files(&slice_dir);
        let shapes_ttl = shape_paths
            .iter()
            .map(|path| std::fs::read_to_string(path).expect("generated shape surface is readable"))
            .collect::<Vec<_>>()
            .join("\n");
        let shapes = parse_shapes(&shapes_ttl).expect("generated shape surface parses");
        let owned_module = native_query::dataset_from_file(&paths::module_file(&slice_dir))
            .expect("the logic module parses");
        let module = native_query::dataset_from_files(&paths::conformance_module_files(&slice_dir))
            .expect("the grounding kernel modules parse");
        let local_shapes = slice_dir.join("shapes.ttl");
        let shapes = scope_shapes_to_slice(
            shapes,
            &shapes_ttl,
            &owned_module,
            local_shapes.is_file().then_some(local_shapes.as_path()),
        )
        .expect("shape ownership scopes to the logic slice");

        let cell = |sole: Option<bool>| ExampleConformance {
            iri: "https://example.org/ecSoleFindingProbe".to_owned(),
            file: "tests/counter-examples/compensation-typed-as-its-forward-receipt.ttl".to_owned(),
            outcome: Outcome::Violates,
            violation_code: Some("shacl.SPARQLConstraintComponent".to_owned()),
            expected_source_shape: Some(
                "https://blackcatinformatics.ca/logic/\
                 CompensationNotInverseConstraintProceduralConstraintShape"
                    .to_owned(),
            ),
            expected_sole_finding: sole,
            expected_failure_class: None,
            rationale: None,
        };

        let baseline =
            NativeChannelBaseline::measure(&module).expect("the logic module measures cleanly");

        run_conformance_cell(
            &cell(None),
            &slice_dir,
            &module,
            &shapes,
            &BTreeMap::new(),
            &baseline,
        )
        .expect(
            "without the flag the cell passes on the existence checks alone — which is the \
             behaviour every unmigrated cell keeps",
        );

        let err = run_conformance_cell(
            &cell(Some(true)),
            &slice_dir,
            &module,
            &shapes,
            &BTreeMap::new(),
            &baseline,
        )
        .expect_err("the cascade must be caught once the cell claims sole-ness");
        let message = err.message();
        assert!(
            message.contains("expectedSoleFinding") && message.contains("also raised"),
            "the failure must name the flag and enumerate the intruding findings, so an \
             author can see WHICH other law fired; got: {message}"
        );
        assert!(
            message.contains("ReceiptRequiresAttemptConstraint")
                || message.contains("CompensationBindsExactForwardEffectConstraint"),
            "the intruder list must name the cascading laws by shape; got: {message}"
        );
    }

    /// An UNPINNED `gmeow:expectedSoleFinding` is a hard failure, and the fixture that
    /// proves why is one the old fallback could not fail on.
    ///
    /// `translation-unanalyzed-overclaim.ttl` trips TWO distinct lang laws —
    /// `lang:UnmarkedSourceOverclaimConstraintProceduralConstraintShape` and
    /// `lang:UnmarkedTargetOverclaimConstraintProceduralConstraintShape` — and BOTH raise
    /// `shacl.SPARQLConstraintComponent`. The first cut's unpinned reading asked only
    /// whether some OTHER shape raised a finding carrying the expected code, so on this
    /// fixture every violating shape answered "yes, I am one of them" and the intruder set
    /// came back empty: the cell claimed soleness, tripped two laws, and went green. That
    /// is the vacuity, and it is why the pin is now REQUIRED rather than a fallback.
    ///
    /// The two halves are the same cell differing only in the pin: unpinned is rejected as
    /// a cell-configuration failure that names the missing property, and pinned is rejected
    /// for the real reason, naming the SECOND law by shape.
    #[test]
    fn an_unpinned_sole_finding_claim_is_a_hard_failure() {
        let slice_dir = paths::repo_root().join("slices/grounding/lang");
        let shape_paths = paths::shapes_files(&slice_dir);
        let shapes_ttl = shape_paths
            .iter()
            .map(|path| std::fs::read_to_string(path).expect("generated shape surface is readable"))
            .collect::<Vec<_>>()
            .join("\n");
        let shapes = parse_shapes(&shapes_ttl).expect("generated shape surface parses");
        let owned_module = native_query::dataset_from_file(&paths::module_file(&slice_dir))
            .expect("the lang module parses");
        let module = native_query::dataset_from_files(&paths::conformance_module_files(&slice_dir))
            .expect("the grounding kernel modules parse");
        let local_shapes = slice_dir.join("shapes.ttl");
        let shapes = scope_shapes_to_slice(
            shapes,
            &shapes_ttl,
            &owned_module,
            local_shapes.is_file().then_some(local_shapes.as_path()),
        )
        .expect("shape ownership scopes to the lang slice");

        let cell = |pin: Option<&str>| ExampleConformance {
            iri: "https://example.org/ecUnpinnedSoleProbe".to_owned(),
            file: "tests/counter-examples/translation-unanalyzed-overclaim.ttl".to_owned(),
            outcome: Outcome::Violates,
            violation_code: Some("shacl.SPARQLConstraintComponent".to_owned()),
            expected_source_shape: pin.map(ToOwned::to_owned),
            expected_sole_finding: Some(true),
            expected_failure_class: None,
            rationale: None,
        };

        let baseline =
            NativeChannelBaseline::measure(&module).expect("the lang module measures cleanly");

        let err = run_conformance_cell(
            &cell(None),
            &slice_dir,
            &module,
            &shapes,
            &BTreeMap::new(),
            &baseline,
        )
        .expect_err(
            "a soleness claim with no named law must be rejected outright — under the old \
             fallback this very cell passed while the fixture tripped two laws",
        );
        let message = err.message();
        assert!(
            message.contains("expectedSoleFinding")
                && message.contains("without gmeow:expectedSourceShape"),
            "the failure must name both properties so an author knows what to bind; got: {message}"
        );

        let err = run_conformance_cell(
            &cell(Some(
                "https://blackcatinformatics.ca/lang/\
                 UnmarkedSourceOverclaimConstraintProceduralConstraintShape",
            )),
            &slice_dir,
            &module,
            &shapes,
            &BTreeMap::new(),
            &baseline,
        )
        .expect_err("once the law is named, the SECOND law raising the same code is an intruder");
        let message = err.message();
        assert!(
            message.contains("also raised")
                && message.contains("UnmarkedTargetOverclaimConstraintProceduralConstraintShape"),
            "the intruder list must name the second law by shape, not merely by component code \
             (both laws raise shacl.SPARQLConstraintComponent); got: {message}"
        );
    }

    /// The DECLARATIVE half of the same requirement has teeth.
    ///
    /// `shapes/test-dsl-shapes.ttl` states the pin requirement as SHACL so a cell is
    /// rejected at DSL-lint time, not only when the harness reaches it. That file is on
    /// the `EXCLUDED` list of every shape union in the repository (it lints the test DSL,
    /// never the data graph), and `dev_validate` does not yet populate
    /// `test_dsl_shapes_ttl`, so nothing else executes it — which is exactly the shape a
    /// rule takes when it is decorative. This runs the SHIPPED file against a synthetic
    /// `gmeow:ExampleConformance` cell and requires it to red, so the rule cannot rot into
    /// prose. The green half is the same cell with the pin bound.
    #[test]
    fn the_test_dsl_shapes_reject_an_unpinned_sole_finding_declaration() {
        let shapes_ttl =
            std::fs::read_to_string(paths::repo_root().join("shapes/test-dsl-shapes.ttl"))
                .expect("the test-DSL shape file is readable");
        let shapes = parse_shapes(&shapes_ttl).expect("the test-DSL shape file parses");

        let cell = |pin: &str| {
            format!(
                r#"
                @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
                @prefix ex: <https://example.org/> .
                ex:ec a gmeow:ExampleConformance ;
                    gmeow:exampleFile "tests/counter-examples/x.ttl" ;
                    gmeow:expectedOutcome gmeow:violates ;
                    gmeow:expectedViolationCode "shacl.MinCountConstraintComponent" ;
                    {pin}
                    gmeow:expectedSoleFinding true .
                "#
            )
        };
        let report = validate_dataset(&store_from_turtle(&cell("")), &shapes)
            .expect("validating the DSL cell succeeds");
        let unpinned: Vec<String> = report
            .results
            .iter()
            .filter(|r| matches!(r.severity, purrdf::shapes::report::Severity::Violation))
            .filter_map(|r| r.message.clone())
            .collect();
        assert!(
            unpinned
                .iter()
                .any(|m| m.contains("must also bind gmeow:expectedSourceShape")),
            "the shape rule must reject a soleness declaration with no pinned law; got: \
             {unpinned:?}"
        );

        let pinned = validate_dataset(
            &store_from_turtle(&cell("gmeow:expectedSourceShape ex:SomeConstraintShape ;")),
            &shapes,
        )
        .expect("validating the pinned DSL cell succeeds");
        let remaining: Vec<String> = pinned
            .results
            .iter()
            .filter(|r| matches!(r.severity, purrdf::shapes::report::Severity::Violation))
            .filter_map(|r| r.message.clone())
            .collect();
        assert!(
            remaining.is_empty(),
            "a pinned soleness declaration is well-formed and must raise nothing; got: \
             {remaining:?}"
        );
    }
}
