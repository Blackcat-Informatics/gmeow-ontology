// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Differential SPARQL parity: the native `gmeow-sparql-eval` engine (purrdf S6,
//! #912) vs the oxigraph baseline on the SAME data and queries.
//!
//! This test is the acceptance evidence for #912's two load-bearing lines:
//! - **byte-identical CONSTRUCT** output (compared at the RDFC-1.0 canonical
//!   N-Quads layer — `freeze` sorts/dedups and canonicalization relabels blanks),
//!   and
//! - SELECT/ASK results that match oxigraph as a multiset.
//!
//! It lives in `crates/rdf` (which already has oxigraph) as a dev-only diff, so the
//! native engine's own crate stays oxigraph-free (gated by `make rdf-core-hygiene`).
//! Both engines are driven from one IR dataset: oxigraph via a materialized `Store`,
//! the native engine over the `RdfDataset` directly.

#![cfg(feature = "oxigraph")]

use gmeow_rdf::oxigraph::{store_from_dataset, GraphPolicy};
use gmeow_rdf::{
    canonicalize, dataset_from_bytes, NativeRdfFormat, OxigraphBackend, SparqlRequest, SparqlResult,
};
use gmeow_sparql_eval::NativeSparqlEngine;
use oxigraph::store::Store;
use std::sync::Arc;

use gmeow_rdf::RdfDataset;
use gmeow_rdf_core::SparqlEngine;

/// A small but varied dataset exercising IRIs, typed/plain/lang literals, multiple
/// predicates, and a node that is the object of two `:knows` edges.
const DATA: &str = r#"
<http://ex/alice> <http://ex/knows> <http://ex/bob> .
<http://ex/alice> <http://ex/knows> <http://ex/carol> .
<http://ex/bob>   <http://ex/knows> <http://ex/carol> .
<http://ex/alice> <http://ex/name> "Alice" .
<http://ex/alice> <http://ex/age>  "30"^^<http://www.w3.org/2001/XMLSchema#integer> .
<http://ex/bob>   <http://ex/age>  "17"^^<http://www.w3.org/2001/XMLSchema#integer> .
<http://ex/carol> <http://ex/member> <http://ex/club> .
"#;

fn fixtures() -> (Arc<RdfDataset>, Store) {
    let dataset = dataset_from_bytes(DATA.as_bytes(), NativeRdfFormat::NTriples).expect("parse IR");
    let store = store_from_dataset(&dataset, GraphPolicy::PreserveNamedGraphs).expect("store");
    (dataset, store)
}

fn run_both(dataset: &Arc<RdfDataset>, store: &Store, query: &str) -> (SparqlResult, SparqlResult) {
    let request = SparqlRequest {
        query,
        base_iri: None,
    };
    let ox = OxigraphBackend
        .query(store, request)
        .unwrap_or_else(|e| panic!("oxigraph query failed for {query:?}: {e:?}"));
    let native = NativeSparqlEngine::new()
        .query(dataset, request)
        .unwrap_or_else(|e| panic!("native query failed for {query:?}: {e:?}"));
    (ox, native)
}

/// A stable, order-insensitive key for a solution row.
fn row_key(row: &[Option<gmeow_rdf::TermValue>]) -> String {
    format!("{row:?}")
}

/// Assert the two results agree. SELECT solutions are compared as a multiset
/// (sorted), CONSTRUCT graphs at the canonical N-Quads layer, ASK by value.
fn assert_parity(query: &str, ox: &SparqlResult, native: &SparqlResult) {
    match (ox, native) {
        (
            SparqlResult::Solutions {
                variables: ox_vars,
                rows: ox_rows,
            },
            SparqlResult::Solutions {
                variables: nat_vars,
                rows: nat_rows,
            },
        ) => {
            assert_eq!(ox_vars, nat_vars, "{query}: variable list differs");
            let mut ox_sorted: Vec<String> = ox_rows.iter().map(|r| row_key(r)).collect();
            let mut nat_sorted: Vec<String> = nat_rows.iter().map(|r| row_key(r)).collect();
            ox_sorted.sort();
            nat_sorted.sort();
            assert_eq!(ox_sorted, nat_sorted, "{query}: solution multiset differs");
        }
        (SparqlResult::Graph(ox_g), SparqlResult::Graph(nat_g)) => {
            assert_eq!(
                canonicalize(ox_g).nquads,
                canonicalize(nat_g).nquads,
                "{query}: CONSTRUCT canonical N-Quads differ"
            );
        }
        (SparqlResult::Boolean(ox_b), SparqlResult::Boolean(nat_b)) => {
            assert_eq!(ox_b, nat_b, "{query}: ASK boolean differs");
        }
        _ => panic!("{query}: result shape mismatch ({ox:?} vs {native:?})"),
    }
}

/// The representative corpus-shaped query set: BGP joins, FILTER (incl. NOT
/// EXISTS), OPTIONAL, UNION, MINUS, DISTINCT, typed-literal comparison, ASK, and
/// CONSTRUCT (the byte-parity line).
fn parity_queries() -> Vec<&'static str> {
    vec![
        // BGP — single and joined.
        "SELECT ?o WHERE { <http://ex/alice> <http://ex/knows> ?o }",
        "SELECT ?a ?b ?c WHERE { ?a <http://ex/knows> ?b . ?b <http://ex/knows> ?c }",
        // FILTER over a typed literal (value-space comparison).
        "SELECT ?s WHERE { ?s <http://ex/age> ?n FILTER(?n >= 18) }",
        // FILTER NOT EXISTS (the corpus-critical anti-join idiom).
        "SELECT ?s WHERE { ?s <http://ex/knows> ?o FILTER NOT EXISTS { ?s <http://ex/member> ?c } }",
        // OPTIONAL.
        "SELECT ?s ?m WHERE { ?s <http://ex/knows> ?o OPTIONAL { ?s <http://ex/member> ?m } }",
        // UNION.
        "SELECT ?x WHERE { { ?x <http://ex/knows> <http://ex/carol> } UNION { ?x <http://ex/member> <http://ex/club> } }",
        // MINUS.
        "SELECT ?s WHERE { ?s <http://ex/knows> ?o MINUS { ?s <http://ex/member> ?c } }",
        // DISTINCT over a projected variable.
        "SELECT DISTINCT ?o WHERE { ?s <http://ex/knows> ?o }",
        // String built-ins + BIND.
        "SELECT ?u WHERE { <http://ex/alice> <http://ex/name> ?nm BIND(UCASE(?nm) AS ?u) }",
        // ASK — true and false.
        "ASK { <http://ex/alice> <http://ex/knows> <http://ex/bob> }",
        "ASK { <http://ex/alice> <http://ex/knows> <http://ex/nobody> }",
        // CONSTRUCT — the byte-identical-output acceptance line.
        "CONSTRUCT { ?s <http://ex/related> ?o } WHERE { ?s <http://ex/knows> ?o }",
        "CONSTRUCT { ?o <http://ex/knownBy> ?s } WHERE { ?s <http://ex/knows> ?o }",
    ]
}

#[test]
fn native_matches_oxigraph_on_representative_queries() {
    let (dataset, store) = fixtures();
    for query in parity_queries() {
        let (ox, native) = run_both(&dataset, &store, query);
        assert_parity(query, &ox, &native);
    }
}

#[test]
fn order_by_matches_oxigraph_in_sequence() {
    // ORDER BY is sequence-sensitive: compare rows in order, not as a multiset.
    let (dataset, store) = fixtures();
    let query = "SELECT ?s ?n WHERE { ?s <http://ex/age> ?n } ORDER BY ?n";
    let (ox, native) = run_both(&dataset, &store, query);
    match (&ox, &native) {
        (
            SparqlResult::Solutions { rows: ox_rows, .. },
            SparqlResult::Solutions { rows: nat_rows, .. },
        ) => {
            let ox_seq: Vec<String> = ox_rows.iter().map(|r| row_key(r)).collect();
            let nat_seq: Vec<String> = nat_rows.iter().map(|r| row_key(r)).collect();
            assert_eq!(ox_seq, nat_seq, "ORDER BY sequence differs");
        }
        _ => panic!("expected solutions"),
    }
}

// ---------------------------------------------------------------------------
// Corpus-driven differential parity harness (Gap 5 / #912 acceptance evidence)
// ---------------------------------------------------------------------------
//
// Loads gmeow.gts (the real merged ontology) as the shared dataset, enumerates
// every *.rq under queries/ and generated/queries/, classifies each query, and
// asserts parity between the native and oxigraph engines.
//
// Classification:
//   NONDETERMINISTIC — query text contains NOW(, RAND(, UUID(, or STRUUID(:
//       run native only; assert well-formed result (no panic/hard error).
//   IN-SCOPE (deterministic native Ok) — run oxigraph too; assert_parity.
//   DEFERRED — native Err whose message matches a known-deferred construct
//       (property path, service, lateral, describe): record, do not fail.
//   UNEXPECTED — native Err not matching a known-deferred construct: HARD FAIL;
//       collect all such cases and report them at the end.
//
// Regression guards (pinned as const with explanatory comments):
//   CORPUS_MIN_TOTAL — corpus must not shrink below 141.
//   CORPUS_MIN_GREEN — green (matched) count must not shrink below this floor.
//       Set to the real count observed on the first run minus a small margin.
//       Raising scope (fixing a DEFERRED construct) MUST raise this floor.

/// Nondeterministic SPARQL builtins: results vary per-call, so parity against
/// oxigraph is not meaningful. We run native only and assert well-formed output.
fn is_nondeterministic(query_text: &str) -> bool {
    let lower = query_text.to_lowercase();
    lower.contains("now(")
        || lower.contains("rand(")
        || lower.contains("uuid(")
        || lower.contains("struuid(")
}

/// Returns true if the error message matches a known-deferred SPARQL construct
/// (property paths, SERVICE federation, LATERAL, DESCRIBE, RDF-1.2 triple terms in
/// patterns). These are in-scope for later S8 (#914) / S6b (#928) / SPARQL-1.2 work;
/// an Err here is expected, not a gap.
fn is_deferred_construct(err_msg: &str) -> bool {
    let lower = err_msg.to_lowercase();
    lower.contains("property path")
        || lower.contains("path expression")
        || lower.contains("service")
        || lower.contains("lateral")
        || lower.contains("describe")
        // The algebra type names the parser surfaces for path operators:
        || lower.contains("pathexpr")
        || lower.contains("unsupported path")
        || lower.contains("path operator")
        // Catch-all: any "not supported" / "not implemented" mentioning path
        || (lower.contains("not support") && lower.contains("path"))
        || (lower.contains("not implement") && lower.contains("path"))
        // The algebra uses ZeroOrMore / OneOrMore / ZeroOrOne for * + ? paths
        || lower.contains("zeroormore")
        || lower.contains("oneormore")
        || lower.contains("zeroorone")
        || lower.contains("alternative path")
        || lower.contains("inverse path")
        || lower.contains("sequence path")
        || lower.contains("negated path")
        // RDF-1.2 / SPARQL 1.2: variable inside a quoted triple term (triple term
        // pattern matching with unbound variables). The native engine explicitly scopes
        // this out of S6 — "S6 scope" in the error. Deferred to SPARQL-1.2 work.
        || lower.contains("variable inside a quoted triple")
        || lower.contains("quoted triple term")
        || (lower.contains("s6 scope") && lower.contains("quoted"))
}

/// Returns true if the query text is a multi-query file (contains more than one
/// top-level SPARQL query statement). SPARQL allows only one query per invocation;
/// some corpus files contain multiple queries separated by comments (e.g. for
/// documentation purposes). Such files cannot be run by a single engine invocation
/// and are skipped with a log note. Counted separately in the tally.
///
/// Detection: count top-level SELECT/CONSTRUCT/ASK/DESCRIBE keywords that appear
/// at the start of a non-comment line (after stripping leading whitespace).
fn is_multi_query_file(query_text: &str) -> bool {
    let mut count = 0usize;
    for line in query_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        let upper = trimmed.to_uppercase();
        if upper.starts_with("SELECT ")
            || upper.starts_with("SELECT\t")
            || upper.starts_with("CONSTRUCT ")
            || upper.starts_with("CONSTRUCT\t")
            || upper.starts_with("CONSTRUCT{")
            || upper.starts_with("ASK ")
            || upper.starts_with("ASK\t")
            || upper.starts_with("ASK{")
            || upper.starts_with("DESCRIBE ")
            || upper.starts_with("DESCRIBE\t")
        {
            count += 1;
            if count > 1 {
                return true;
            }
        }
    }
    false
}

/// Collect every *.rq file under the two corpus roots, sorted for determinism.
fn collect_corpus_files() -> Vec<std::path::PathBuf> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest.join("..").join("..");
    let roots = [
        repo_root.join("queries"),
        repo_root.join("generated").join("queries"),
    ];
    let mut files = Vec::new();
    for root in &roots {
        if !root.exists() {
            continue;
        }
        collect_rq_recursive(root, &mut files);
    }
    files.sort();
    files
}

fn collect_rq_recursive(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rq_recursive(&path, out);
        } else if path.extension().is_some_and(|e| e == "rq") {
            out.push(path);
        }
    }
}

/// Corpus must not shrink below 141 queries (the count at Gap 5 authoring time).
const CORPUS_MIN_TOTAL: usize = 141;

/// Minimum number of queries that must match (green) after classification.
///
/// = corpus(141)
///   − property-path-deferred(23, S8 #914) − rdf12-triple-term-deferred(1, SPARQL-1.2)
///   − nondeterministic(1, NOW()) − multi-query-skipped(1)
///   − margin(2) for corpus drift
///
/// Observed: 115 green on first run (2026-06-26). Set floor to 113 (115 − 2 margin).
/// Raising scope (fixing a DEFERRED construct) MUST raise this floor and update this
/// comment with the new observed count.
const CORPUS_MIN_GREEN: usize = 113;

#[test]
#[cfg(all(feature = "oxigraph", feature = "gts"))]
fn corpus_parity_against_real_ontology() {
    // -----------------------------------------------------------------------
    // 1. Load the real merged ontology graph — identical source for both engines.
    // -----------------------------------------------------------------------
    let gts_path = {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest
            .join("..")
            .join("..")
            .join("generated")
            .join("dist")
            .join("gmeow.gts")
    };
    let gts_bytes = std::fs::read(&gts_path)
        .unwrap_or_else(|e| panic!("read gmeow.gts at {}: {e}", gts_path.display()));
    let store = gmeow_rdf::gts::flattened_oxigraph_store_from_bytes(&gts_bytes)
        .expect("oxigraph store from gts");
    let dataset =
        gmeow_rdf::oxigraph::dataset_from_store(&store).expect("native dataset from store");

    // -----------------------------------------------------------------------
    // 2. Enumerate the corpus.
    // -----------------------------------------------------------------------
    let corpus = collect_corpus_files();
    let total = corpus.len();
    assert!(
        total >= CORPUS_MIN_TOTAL,
        "corpus shrank: {total} < {CORPUS_MIN_TOTAL} — a query directory may be missing"
    );

    // -----------------------------------------------------------------------
    // 3. Per-query classification.
    // -----------------------------------------------------------------------
    let engine = NativeSparqlEngine::new();
    let ox = OxigraphBackend;

    let mut matched: usize = 0;
    let mut deferred: usize = 0;
    let mut nondet: usize = 0;
    let mut multi_query_skipped: usize = 0;
    let mut unexpected_failures: Vec<(String, String)> = Vec::new();

    for path in &corpus {
        let query_text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let label = path.display().to_string();

        // Multi-query files: some corpus files contain multiple separate SPARQL
        // queries (e.g. documentation examples). SPARQL engines accept exactly one
        // query per invocation; such files cannot be run as-is and are skipped.
        // This is a corpus format issue, not an engine gap.
        if is_multi_query_file(&query_text) {
            multi_query_skipped += 1;
            eprintln!("  [multi-query-skip] {label}");
            continue;
        }

        let request = SparqlRequest {
            query: &query_text,
            base_iri: None,
        };

        // --- NONDETERMINISTIC class ---
        if is_nondeterministic(&query_text) {
            nondet += 1;
            // Run native only; assert well-formed (no panic, no hard Err from a
            // truly unsupported construct).
            match engine.query(&dataset, request) {
                Ok(SparqlResult::Solutions { .. })
                | Ok(SparqlResult::Graph(_))
                | Ok(SparqlResult::Boolean(_)) => {
                    // Well-formed — expected.
                }
                Err(e) => {
                    // If the nondeterministic query also hits a deferred construct,
                    // count it as deferred rather than unexpected.
                    let msg = e.to_string();
                    if is_deferred_construct(&msg) {
                        deferred += 1;
                        nondet -= 1; // reclassify
                    } else {
                        unexpected_failures.push((
                            label.clone(),
                            format!("nondeterministic query errored unexpectedly: {msg}"),
                        ));
                    }
                }
            }
            continue;
        }

        // --- Run native engine ---
        match engine.query(&dataset, request) {
            Ok(native_result) => {
                // IN-SCOPE class — compare to oxigraph.
                let request_ox = SparqlRequest {
                    query: &query_text,
                    base_iri: None,
                };
                let ox_result = ox.query(&store, request_ox).unwrap_or_else(|e| {
                    panic!("oxigraph failed on in-scope query {label}: {e}\nQuery:\n{query_text}")
                });
                // assert_parity panics with a descriptive message on mismatch.
                // We catch it so we can collect ALL mismatches before failing.
                let parity_ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    assert_parity(&label, &ox_result, &native_result);
                }));
                match parity_ok {
                    Ok(()) => matched += 1,
                    Err(payload) => {
                        let msg = if let Some(s) = payload.downcast_ref::<String>() {
                            s.clone()
                        } else if let Some(s) = payload.downcast_ref::<&str>() {
                            (*s).to_owned()
                        } else {
                            "parity assertion panicked (unknown payload)".to_owned()
                        };
                        unexpected_failures
                            .push((label.clone(), format!("parity mismatch: {msg}")));
                    }
                }
            }
            Err(e) => {
                let msg = e.to_string();
                if is_deferred_construct(&msg) {
                    // DEFERRED — expected gap (property paths → S8 #914 / federation → S6b #928).
                    deferred += 1;
                } else {
                    // UNEXPECTED — a real remaining gap or parse gap. Collect; report all at end.
                    unexpected_failures.push((label.clone(), msg));
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // 4. Honest tally line — always printed (--nocapture or CI log).
    // -----------------------------------------------------------------------
    eprintln!(
        "corpus parity: {matched} matched, {deferred} deferred(paths/service/rdf12-triple-terms), \
         {nondet} nondeterministic, {multi_query_skipped} multi-query-skipped, \
         {} unexpected, total {total}",
        unexpected_failures.len()
    );

    // -----------------------------------------------------------------------
    // 5. Regression guards.
    // -----------------------------------------------------------------------

    // 5a. Hard fail on any unexpected error or parity mismatch — list ALL of them.
    assert!(
        unexpected_failures.is_empty(),
        "in-scope corpus queries errored or mismatched unexpectedly ({} failures):\n{}",
        unexpected_failures.len(),
        unexpected_failures
            .iter()
            .map(|(f, e)| format!("  {f}\n    → {e}"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // 5b. Green floor — must not regress below the observed baseline.
    assert!(
        matched >= CORPUS_MIN_GREEN,
        "green corpus shrank: {matched} matched < {CORPUS_MIN_GREEN} CORPUS_MIN_GREEN \
         — a previously passing query now fails; investigate before merging"
    );
}
