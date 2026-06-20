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
//!   module + shapes via the native SHACL engine and compare finding codes.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

use gmeow_shacl::engine::{parse_shapes, validate};
use gmeow_validate::findings::finding_from_shacl;
use gmeow_validate::store::{build_store, parse_file};
use gmeow_validate::validate_all::{scoped_overlay_insert, scoped_overlay_remove};

use crate::dsl::{
    self, CompetencyQuestion, ExampleConformance, ExpectedRow, Outcome, Polarity, ReasoningProfile,
    Scope, StructuralAssertion,
};
use crate::paths;
use crate::stores::{merged_store, rdfs_closed_store};

/// A canonical (variable-name, term-N-Triples) binding set for one result row,
/// sorted so row identity is independent of projection/iteration order.
type CanonRow = Vec<(String, String)>;

// ── Per-file aggregators (the datatest-stable entry points) ─────────────────────

/// Run every competency question in a `competency.ttl` spec file.
///
/// # Errors
///
/// Returns `Err(String)` aggregating each failing cell's diagnostic.
pub fn run_competency_file(path: &Path) -> Result<(), String> {
    let spec = dsl::load_spec(path)?;
    // The asserted merged graph is the default lane and is always built once.
    // The RDFS-closed graph is built lazily — only if some question opts into it
    // via gmeow:cqReasoning gmeow:reasoningRdfs — and then reused across cells.
    let merged = merged_store()?;
    let mut rdfs: Option<Store> = None;

    let mut results: Vec<(&str, Result<(), String>)> = Vec::with_capacity(spec.competency.len());
    for cq in &spec.competency {
        let store: &Store = match cq.reasoning {
            ReasoningProfile::None => &merged,
            ReasoningProfile::Rdfs => {
                if rdfs.is_none() {
                    rdfs = Some(rdfs_closed_store()?);
                }
                rdfs.as_ref().expect("rdfs store just built")
            }
        };
        results.push((cq.iri.as_str(), run_competency_cell(store, cq)));
    }
    aggregate(path, "competency", results.into_iter())
}

/// Run every structural assertion in a `structural.ttl` spec file.
///
/// # Errors
///
/// Returns `Err(String)` aggregating each failing cell's diagnostic.
pub fn run_structural_file(path: &Path) -> Result<(), String> {
    let spec = dsl::load_spec(path)?;
    let slice_dir = paths::slice_dir(path);
    aggregate(
        path,
        "structural",
        spec.structural
            .iter()
            .map(|sa| (sa.iri.as_str(), run_structural_cell(sa, &slice_dir))),
    )
}

/// Run every example-conformance fixture in an `example-conformance.ttl` file.
///
/// # Errors
///
/// Returns `Err(String)` aggregating each failing cell's diagnostic.
pub fn run_conformance_file(path: &Path) -> Result<(), String> {
    let spec = dsl::load_spec(path)?;
    let slice_dir = paths::slice_dir(path);
    aggregate(
        path,
        "example-conformance",
        spec.conformance
            .iter()
            .map(|ec| (ec.iri.as_str(), run_conformance_cell(ec, &slice_dir))),
    )
}

/// Collect per-cell results, returning one aggregated error if any failed.
fn aggregate<'a>(
    path: &Path,
    kind: &str,
    cells: impl Iterator<Item = (&'a str, Result<(), String>)>,
) -> Result<(), String> {
    let mut count = 0usize;
    let failures: Vec<String> = cells
        .inspect(|_| count += 1)
        .filter_map(|(iri, result)| result.err().map(|e| format!("  • [{iri}] {e}")))
        .collect();
    if failures.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} of {count} {kind} cell(s) failed in {}:\n{}",
        failures.len(),
        path.display(),
        failures.join("\n")
    ))
}

// ── Competency ──────────────────────────────────────────────────────────────────

fn run_competency_cell(store: &Store, cq: &CompetencyQuestion) -> Result<(), String> {
    let query = load_query(cq)?;
    let results = SparqlEvaluator::new()
        .parse_query(&query)
        .map_err(|e| format!("query parse error: {e}"))?
        .on_store(store)
        .execute()
        .map_err(|e| format!("query evaluation error: {e}"))?;

    match results {
        QueryResults::Boolean(actual) => {
            let expected = cq
                .expect_ask
                .ok_or("ASK query but no gmeow:cqExpectAsk on the question")?;
            if actual != expected {
                return Err(format!("ASK expected {expected}, got {actual}"));
            }
            Ok(())
        }
        QueryResults::Solutions(solutions) => {
            let vars: Vec<String> = solutions
                .variables()
                .iter()
                .map(|v| v.as_str().to_owned())
                .collect();
            let mut actual: Vec<CanonRow> = Vec::new();
            for sol in solutions {
                let sol = sol.map_err(|e| format!("solution error: {e}"))?;
                let mut row: CanonRow = vars
                    .iter()
                    .filter_map(|v| sol.get(v.as_str()).map(|t| (v.clone(), t.to_string())))
                    .collect();
                row.sort();
                actual.push(row);
            }
            check_select(cq, &actual)
        }
        QueryResults::Graph(_) => {
            Err("competency query must be ASK or SELECT, got CONSTRUCT/DESCRIBE".to_owned())
        }
    }
}

/// Compare a SELECT competency question's actual rows against its expectation.
fn check_select(cq: &CompetencyQuestion, actual: &[CanonRow]) -> Result<(), String> {
    let actual_set: BTreeSet<CanonRow> = actual.iter().cloned().collect();
    let expected_set: BTreeSet<CanonRow> =
        cq.expected_rows.iter().map(canon_expected_row).collect();

    if let Some(want) = cq.expect_row_count {
        // Escape-hatch tier: pin the count, and any enumerated sample rows must
        // be a subset of the actual result.
        let got = actual.len() as u64;
        if got != want {
            return Err(format!("expected {want} rows, got {got}"));
        }
        return missing_rows(&expected_set, &actual_set)
            .map_err(|m| format!("sample row(s) absent from result: {m}"));
    }

    // Enumerated tier: the question must carry expected rows.
    if cq.expected_rows.is_empty() {
        return Err(
            "SELECT competency question has neither gmeow:cqExpectRowCount nor gmeow:cqExpectRow"
                .to_owned(),
        );
    }
    if cq.exact_rows {
        if actual_set != expected_set {
            let missing = set_diff(&expected_set, &actual_set);
            let extra = set_diff(&actual_set, &expected_set);
            return Err(format!(
                "exact-row mismatch: {} expected-but-absent, {} unexpected (missing={missing}; extra={extra})",
                expected_set.difference(&actual_set).count(),
                actual_set.difference(&expected_set).count(),
            ));
        }
        Ok(())
    } else {
        missing_rows(&expected_set, &actual_set)
            .map_err(|m| format!("expected row(s) absent from result: {m}"))
    }
}

fn missing_rows(expected: &BTreeSet<CanonRow>, actual: &BTreeSet<CanonRow>) -> Result<(), String> {
    let missing = set_diff(expected, actual);
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
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

fn canon_expected_row(row: &ExpectedRow) -> CanonRow {
    let mut cells: CanonRow = row
        .cells
        .iter()
        .map(|c| (c.var.clone(), c.value.to_string()))
        .collect();
    cells.sort();
    cells
}

fn load_query(cq: &CompetencyQuestion) -> Result<String, String> {
    match (&cq.query_inline, &cq.query_file) {
        (Some(q), None) => Ok(q.clone()),
        (None, Some(rel)) => {
            let p = paths::query_file(rel);
            std::fs::read_to_string(&p)
                .map_err(|e| format!("cannot read cqQueryFile {}: {e}", p.display()))
        }
        (Some(_), Some(_)) => {
            Err("competency question sets both cqQuery and cqQueryFile".to_owned())
        }
        (None, None) => Err("competency question sets neither cqQuery nor cqQueryFile".to_owned()),
    }
}

// ── Structural ──────────────────────────────────────────────────────────────────

fn run_structural_cell(sa: &StructuralAssertion, slice_dir: &Path) -> Result<(), String> {
    let pattern = match (&sa.pattern, &sa.shape) {
        (Some(p), None) => p,
        (None, Some(shape)) => {
            // No T2 exemplar exercises saShape; fail loudly rather than silently
            // pass (the no-optionality / hard-fail doctrine).
            return Err(format!(
                "saShape execution is not yet implemented (shape {shape}); refusing to silently pass"
            ));
        }
        (Some(_), Some(_)) => return Err("assertion sets both saPattern and saShape".to_owned()),
        (None, None) => return Err("assertion sets neither saPattern nor saShape".to_owned()),
    };

    let mut sources = vec![paths::module_file(slice_dir)];
    if sa.scope == Scope::ModuleAndExamples {
        sources.extend(example_ttls(&paths::examples_dir(slice_dir)));
    }
    let store = build_store(&sources)
        .map_err(|e| format!("building scoped store for structural assertion: {e}"))?;
    let holds = run_ask(&store, pattern)?;

    match (sa.polarity, holds) {
        (Polarity::Must, false) => {
            Err("polarity 'must' but the ASK pattern did NOT hold".to_owned())
        }
        (Polarity::MustNot, true) => Err("polarity 'mustNot' but the ASK pattern HELD".to_owned()),
        _ => Ok(()),
    }
}

fn run_ask(store: &Store, query: &str) -> Result<bool, String> {
    let results = SparqlEvaluator::new()
        .parse_query(query)
        .map_err(|e| format!("saPattern parse error: {e}"))?
        .on_store(store)
        .execute()
        .map_err(|e| format!("saPattern evaluation error: {e}"))?;
    match results {
        QueryResults::Boolean(b) => Ok(b),
        QueryResults::Solutions(_) | QueryResults::Graph(_) => {
            Err("saPattern must be a SPARQL ASK query".to_owned())
        }
    }
}

/// Every `*.ttl` directly under a slice's `examples/` dir (sorted; empty if the
/// directory is absent).
fn example_ttls(examples_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(examples_dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "ttl"))
        .collect();
    files.sort();
    files
}

// ── Example conformance ─────────────────────────────────────────────────────────

fn run_conformance_cell(ec: &ExampleConformance, slice_dir: &Path) -> Result<(), String> {
    let data_store = build_store(&[paths::module_file(slice_dir)])
        .map_err(|e| format!("building module store: {e}"))?;
    let shapes_path = paths::shapes_file(slice_dir);
    let shapes_ttl = std::fs::read_to_string(&shapes_path)
        .map_err(|e| format!("cannot read {}: {e}", shapes_path.display()))?;
    let shapes = parse_shapes(&shapes_ttl).map_err(|e| format!("parsing slice shapes: {e}"))?;

    let example_path = paths::example_file(slice_dir, &ec.file);
    let example_quads = parse_file(&example_path)
        .map_err(|e| format!("parsing example {}: {e}", example_path.display()))?;

    // Scoped overlay: validate (module + example) against the slice shapes, then
    // restore the module store — exactly the validation-path example idiom.
    let inserted = scoped_overlay_insert(&data_store, example_quads.iter());
    let report = validate(&data_store, &shapes);
    scoped_overlay_remove(&data_store, &inserted);

    let codes: BTreeSet<String> = report
        .results
        .iter()
        .map(|r| finding_from_shacl(r).code)
        .collect();

    match ec.outcome {
        Outcome::Conforms => {
            if codes.is_empty() {
                Ok(())
            } else {
                Err(format!(
                    "expected conformance, got finding(s): {}",
                    join_codes(&codes)
                ))
            }
        }
        Outcome::Violates => {
            let expected = ec
                .violation_code
                .as_deref()
                .ok_or("outcome 'violates' but no gmeow:expectedViolationCode")?;
            if codes.contains(expected) {
                Ok(())
            } else {
                Err(format!(
                    "expected violation {expected}, got finding(s): {}",
                    join_codes(&codes)
                ))
            }
        }
    }
}

fn join_codes(codes: &BTreeSet<String>) -> String {
    if codes.is_empty() {
        "<none>".to_owned()
    } else {
        codes.iter().cloned().collect::<Vec<_>>().join(", ")
    }
}
