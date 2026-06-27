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

use oxigraph::model::Term;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;

use gmeow_logic_compile::result_shape::{ObservedBinding, ObservedTerm};
use gmeow_shacl::engine::{parse_shapes, validate};
use gmeow_validate::findings::finding_from_shacl;
use gmeow_validate::store::{build_store, parse_file};
use gmeow_validate::validate_all::OverlayGuard;

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
    let slice_dir = paths::slice_dir(path);
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
        results.push((cq.iri.as_str(), run_competency_cell(store, cq, &slice_dir)));
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
    Err(format!(
        "{} of {count} {kind} cell(s) failed in {}:\n{}",
        failures.len(),
        path.display(),
        failures.join("\n")
    ))
}

// ── Competency ──────────────────────────────────────────────────────────────────

fn run_competency_cell(
    store: &Store,
    cq: &CompetencyQuestion,
    slice_dir: &Path,
) -> Result<(), String> {
    let query = load_query(cq)?;
    let Some(rel) = &cq.data_file else {
        // No overlay: run the query directly over the (asserted or RDFS) store.
        return execute_competency_query(store, cq, &query);
    };

    // Overlay lane: a slice-relative ABox fixture is inserted onto the asserted
    // merged graph for this one query, then removed. The merged store is shared
    // across cells in this file, so a leak would contaminate later questions —
    // scoped_overlay_insert returns only the quads it actually inserted (skipping
    // any already present). The OverlayGuard removes exactly that set
    // unconditionally on scope exit, including panic unwind.
    if cq.reasoning != ReasoningProfile::None {
        // The RDFS closure is computed BEFORE the overlay, so an overlaid fixture's
        // entailments would be invisible. Refuse rather than silently under-answer.
        return Err(format!(
            "{}: gmeow:cqDataFile is only honoured in the asserted (reasoningNone) lane, \
             not gmeow:reasoningRdfs",
            cq.iri
        ));
    }
    let fixture_path = paths::example_file(slice_dir, rel);
    let quads = parse_file(&fixture_path)
        .map_err(|e| format!("parsing cqDataFile {}: {e}", fixture_path.display()))?;
    let _overlay = OverlayGuard::insert(store, quads.iter());
    execute_competency_query(store, cq, &query) // _overlay drops here, removing the overlay
}

/// Execute a competency question's (already-resolved) query over `store` and
/// check the result against its expectation.
fn execute_competency_query(
    store: &Store,
    cq: &CompetencyQuestion,
    query: &str,
) -> Result<(), String> {
    // Input→output contract, checked BEFORE execution (hard-fail, no silent pass):
    // a declared output shape's required columns must be covered by the declared
    // input shape — a query that cannot type-check never runs.
    if let (Some(out), Some(inp)) = (&cq.result_shape, &cq.input_shape) {
        out.is_satisfiable_by(inp)
            .map_err(|e| format!("input-shape contract (checked before execution): {e}"))?;
    }

    let results = SparqlEvaluator::new()
        .parse_query(query)
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
            // The observed (var, term-kind) bindings, kept alongside the canonical
            // rows so the declared output shape can type-check them.
            let mut observed: Vec<Vec<ObservedBinding>> = Vec::new();
            for sol in solutions {
                let sol = sol.map_err(|e| format!("solution error: {e}"))?;
                let mut row: CanonRow = Vec::new();
                let mut obs: Vec<ObservedBinding> = Vec::new();
                for v in &vars {
                    if let Some(t) = sol.get(v.as_str()) {
                        row.push((v.clone(), t.to_string()));
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
                shape
                    .validate_bindings(&observed)
                    .map_err(|e| format!("result-shape contract: {e}"))?;
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

/// Project one oxigraph result binding into the pure-data [`ObservedTerm`] the
/// result-shape contract checks. An RDF-star triple term cannot be typed by a
/// column kind, so it hard-fails rather than being silently misclassified.
fn observed_term(cq_iri: &str, var: &str, term: &Term) -> Result<ObservedTerm, String> {
    Ok(match term {
        Term::NamedNode(_) => ObservedTerm::Iri,
        Term::BlankNode(_) => ObservedTerm::BlankNode,
        Term::Literal(l) => ObservedTerm::Literal {
            datatype: l.datatype().as_str().to_owned(),
        },
        Term::Triple(_) => {
            return Err(format!(
                "{cq_iri}: result binding ?{var} is an RDF-star triple term, which a logic:ResultShape does not type"
            ));
        }
    })
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
        sources.extend(example_ttls(&paths::examples_dir(slice_dir))?);
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
fn example_ttls(examples_dir: &Path) -> Result<Vec<PathBuf>, String> {
    // An absent examples/ dir is normal (→ no examples). Any OTHER read error
    // (permissions, I/O) must propagate, not masquerade as "no examples": that
    // would silently run a scopeModuleAndExamples assertion against module-only
    // data and could pass spuriously.
    let entries = match std::fs::read_dir(examples_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("read_dir {}: {e}", examples_dir.display())),
    };
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|e| format!("read_dir entry under {}: {e}", examples_dir.display()))?;
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|x| x == "ttl") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
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
    // OverlayGuard removes the inserted quads unconditionally on scope exit,
    // including panic unwind.
    let _overlay = OverlayGuard::insert(&data_store, example_quads.iter());
    let report = validate(&data_store, &shapes);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::{CompetencyQuestion, ExpectedCell, ExpectedRow};
    use gmeow_logic_compile::result_shape::{
        ColumnKind, ResultColumn, ResultShape, RowCardinality,
    };
    use oxigraph::model::{NamedNode, Term};

    /// Materialize inline Turtle into a store via the native codec (#909).
    fn store_from_turtle(ttl: &str) -> Store {
        use gmeow_rdf::oxigraph::{store_from_dataset, GraphPolicy};
        use gmeow_rdf::parse_dataset;
        let dataset = parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("valid turtle");
        store_from_dataset(dataset.as_ref(), GraphPolicy::PreserveNamedGraphs).expect("store")
    }

    /// A minimal SELECT competency question over an inline query.
    fn cq_with(query: &str) -> CompetencyQuestion {
        CompetencyQuestion {
            iri: "https://example.org/cqShape".to_owned(),
            query_inline: Some(query.to_owned()),
            query_file: None,
            expect_ask: None,
            expect_row_count: None,
            exact_rows: false,
            expected_rows: Vec::new(),
            reasoning: ReasoningProfile::None,
            data_file: None,
            result_shape: None,
            input_shape: None,
            rationale: None,
        }
    }

    const Q_X: &str = "PREFIX ex: <https://example.org/> \
        SELECT ?x WHERE { ?x a ex:Thing }";

    fn one_thing_store() -> Store {
        store_from_turtle("@prefix ex: <https://example.org/> .\nex:a a ex:Thing .\n")
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
            err.contains("result-shape contract") && err.contains("term-kind"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn input_shape_incompatibility_hard_fails_before_execution() {
        let store = one_thing_store();
        let mut cq = cq_with(Q_X);
        // Output requires a column ?y that the input shape never provides → the
        // pre-execution input→output check fails before the query runs.
        cq.result_shape = Some(ResultShape::new(
            vec![ResultColumn::required("y", ColumnKind::Iri)],
            RowCardinality::Contains,
        ));
        cq.input_shape = Some(ResultShape::new(
            vec![ResultColumn::required("x", ColumnKind::Iri)],
            RowCardinality::Contains,
        ));
        let err = execute_competency_query(&store, &cq, Q_X)
            .expect_err("incompatible input shape must hard-fail");
        assert!(
            err.contains("input-shape contract") && err.contains("before execution"),
            "unexpected error: {err}"
        );
    }

    /// A `gmeow:cqDataFile` overlay must (a) make the fixture's instances visible to
    /// the query, (b) be removed afterwards so it never leaks into the shared store,
    /// and (c) be rejected outright in the RDFS lane.
    #[test]
    fn cq_data_file_overlay_applies_and_is_removed() {
        let dir = std::env::temp_dir().join(format!("slicetest-overlay-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp slice dir");
        let fixture = "@prefix ex: <https://example.org/test/> .\n\
                       @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
                       ex:event1 a gmeow:Event .\n";
        std::fs::write(dir.join("data.ttl"), fixture).expect("write fixture");

        // Empty shared store: the only way the SELECT matches is via the overlay.
        let store = Store::new().expect("store");
        let cq = CompetencyQuestion {
            iri: "https://example.org/test/cqOverlay".to_owned(),
            query_inline: Some(
                "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/> \
                 SELECT ?e WHERE { ?e a gmeow:Event }"
                    .to_owned(),
            ),
            query_file: None,
            expect_ask: None,
            expect_row_count: None,
            exact_rows: false,
            expected_rows: vec![ExpectedRow {
                cells: vec![ExpectedCell {
                    var: "e".to_owned(),
                    value: Term::NamedNode(
                        NamedNode::new("https://example.org/test/event1").unwrap(),
                    ),
                }],
            }],
            reasoning: ReasoningProfile::None,
            data_file: Some("data.ttl".to_owned()),
            result_shape: None,
            input_shape: None,
            rationale: None,
        };

        run_competency_cell(&store, &cq, &dir).expect("overlay cell must pass");
        assert_eq!(
            store.len().expect("len"),
            0,
            "the overlay must be removed — it must not leak into the shared store"
        );

        // Same cell in the RDFS lane: hard-fail, never silently under-answer.
        let mut rdfs_cq = cq.clone();
        rdfs_cq.reasoning = ReasoningProfile::Rdfs;
        let err = run_competency_cell(&store, &rdfs_cq, &dir)
            .expect_err("cqDataFile + reasoningRdfs must be rejected");
        assert!(err.contains("reasoningNone"), "unexpected error: {err}");

        std::fs::remove_dir_all(&dir).ok();
    }
}
