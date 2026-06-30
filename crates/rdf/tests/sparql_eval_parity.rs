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
                ..
            },
            SparqlResult::Solutions {
                variables: nat_vars,
                rows: nat_rows,
                ..
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

// ---------------------------------------------------------------------------
// Sharding (#1045)
// ---------------------------------------------------------------------------
//
// The full sweep is eval-bound: ~1.5 s shared load (gts decode + oxigraph store +
// native dataset) then evaluation over the real ontology — over the 25 s always-on
// per-test budget when run whole. The dominant native outlier is
// `generated/queries/ontolex.rq` (~107 s, see OFF_GATE_HEAVY); carving the heavy
// projections out drops the remaining corpus to a few seconds. We then split that
// remainder across `NUM_SHARDS` independent `#[test]` fns; nextest runs each in its own
// process, in parallel, so per-shard wall time is ~1.5 s load + a small eval tail —
// well under budget with CI headroom and room for corpus growth.
//
// The original whole-corpus green guard was `CORPUS_MIN_GREEN = 113` (observed 115).
// It is now expressed as per-shard floors (the gated subset) plus
// [`OFF_GATE_HEAVY_MIN_GREEN`]: `39 + 32 + 29 + 29` (= 129) `+ 6` = a **135**
// whole-corpus floor (observed total 143). Same one-below-each drift margin the per-shard
// floors use; expressed shard-locally.

/// How many independent shard tests the gated corpus parity sweep is split across.
const NUM_SHARDS: usize = 4;

/// Per-shard green floors for the GATED subset (corpus minus [`OFF_GATE_HEAVY`]).
/// Sharding is by a STABLE hash of each query's repo-relative path (not its position
/// in the sorted corpus), so a given file stays in the same shard as the corpus
/// grows — which keeps these per-shard minimums valid when queries are added.
/// Observed gated greens `[42, 33, 30, 32]` (sum 137); floors set one below each (with extra
/// margin where the corpus has since grown) for drift. Shard 1 observed dropped 34 → 33 when
/// the `axis-not-disjoint` anti-join was carved into [`OFF_GATE_HEAVY`] (its ~9 s native eval
/// tipped the shard past budget as the ontology grew); the floor 32 still holds. With
/// [`OFF_GATE_HEAVY_MIN_GREEN`] (6) the whole-corpus green floor is 135 (observed total 143).
/// Raising scope (fixing a DEFERRED construct) MUST raise the affected shard's floor and this
/// comment; moving a query into [`OFF_GATE_HEAVY`] lowers the affected shard's observed green
/// (re-measure and reset).
const CORPUS_MIN_GREEN_PER_SHARD: [usize; NUM_SHARDS] = [39, 32, 29, 29];

/// Stable FNV-1a hash of a query's repo-relative path → shard id. Stable across
/// machines/worktrees (the key is repo-relative, not absolute) and across corpus
/// growth (per-file, not positional), unlike a `sorted_index % N` scheme which
/// would reshuffle every file's shard whenever a query is added.
///
/// `rel_path` **must** use `'/'` separators (Windows `'\\'` must be normalized
/// before calling); the FNV hash is over raw bytes, so any backslash would change
/// the shard assignment and invalidate the hardcoded per-shard floors.
fn shard_of(rel_path: &str) -> usize {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in rel_path.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (h % NUM_SHARDS as u64) as usize
}

/// Repo root as the corpus enumerator builds it (`crates/rdf/../..`), used to derive
/// the stable repo-relative shard key.
fn corpus_repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

#[test]
#[cfg(all(feature = "oxigraph", feature = "gts"))]
fn corpus_parity_shard_0() {
    run_corpus_shard(0);
}

#[test]
#[cfg(all(feature = "oxigraph", feature = "gts"))]
fn corpus_parity_shard_1() {
    run_corpus_shard(1);
}

#[test]
#[cfg(all(feature = "oxigraph", feature = "gts"))]
fn corpus_parity_shard_2() {
    run_corpus_shard(2);
}

#[test]
#[cfg(all(feature = "oxigraph", feature = "gts"))]
fn corpus_parity_shard_3() {
    run_corpus_shard(3);
}

/// OFF-GATE: the full native-vs-oxigraph parity sweep over the [`OFF_GATE_HEAVY`]
/// queries, which are too slow on the native engine to fit the 25 s always-on budget.
/// Kept always-runnable (NOT `#[ignore]`d) and exercised on the `maint-rust-heavy`
/// nextest profile / `make maint-rust-heavy`; the per-commit gate excludes it via the
/// default profile's `default-filter` (see `.config/nextest.toml`). This preserves
/// #912 parity coverage for these queries off the critical path.
#[test]
#[cfg(all(feature = "oxigraph", feature = "gts"))]
fn corpus_parity_heavy_offgate() {
    let tally = run_corpus_subset(&|rel| is_off_gate_heavy(rel));
    assert_corpus_tally("off-gate-heavy", &tally, OFF_GATE_HEAVY_MIN_GREEN);
}

/// Cheap whole-corpus tripwire (no load, no eval — milliseconds): the corpus must
/// not shrink below [`CORPUS_MIN_TOTAL`]. Guards against a query directory going
/// missing, which the per-shard tests (each seeing only their slice) cannot detect.
#[test]
fn corpus_inventory_floor() {
    let total = collect_corpus_files().len();
    assert!(
        total >= CORPUS_MIN_TOTAL,
        "corpus shrank: {total} < {CORPUS_MIN_TOTAL} — a query directory may be missing"
    );
}

/// Queries whose NATIVE evaluation is heavy enough on the real ontology that they
/// cannot meet the 25 s always-on per-test budget (#1045) — either a single query that
/// alone blows the budget, or a heavy generated-projection CONSTRUCT that, aggregated
/// onto its hash-assigned shard, tips that shard over budget under CI contention. They
/// keep their full native-vs-oxigraph parity coverage in the OFF-GATE
/// [`corpus_parity_heavy_offgate`] test (maint lane), not on the per-commit gate.
///
/// Entries:
///   - `generated/queries/ontolex.rq` — GMEOW→ontolex projection CONSTRUCT.
///   - `queries/qc/missing-definitions.rq`.
///   - `generated/queries/schema-org.rq` — a 1250-line projection CONSTRUCT.
///   - `generated/queries/vcard.rq`.
///   - `generated/queries/foaf.rq`.
///
/// These are heavy generated CONSTRUCT projections; `schema-org`/`vcard`/`foaf` all hash
/// into the same shard (shard 1), whose aggregate × CI ~5× contention blew the 25 s budget,
/// and `ontolex` once pushed its shard past the 120 s terminate cliff. They are tracked as
/// native-engine performance gaps; remove from this list as the engine speeds up (projection
/// planning) and they rejoin the gated shards automatically.
///
/// Anti-join indexing already moved one query out: the `FILTER NOT EXISTS` over every
/// `owl:Class` was here at ~44 s native, but building the inner pattern's probe index once
/// per site and reusing it across outer rows made the anti-join roughly linear, so it now
/// runs on the gate (shard 1). The same indexing also sharply cut these projections' native
/// time (they use `FILTER NOT EXISTS` too): the whole off-gate-heavy subset now runs in
/// ~7 s total on the dev box (2026-06-27, down from >110 s pre-fix). They are kept off-gate
/// conservatively — the CONSTRUCT projections previously tripped CI under contention — until
/// their gated CI behavior is confirmed.
///
/// `queries/verify/axis-not-disjoint.rq` joined the carve-out as the ontology grew: it is a
/// `FILTER NOT EXISTS` disjointness check over the (now larger) axis/class population, and
/// its native eval climbed to ~9 s solo — the dominant term in shard 1's aggregate, which
/// tipped that shard past the zero-headroom 25 s budget under CI ~3-5× contention even though
/// it ran ~24 s locally. Like the others it keeps full parity coverage on the off-gate-heavy
/// maint lane; remove it once native anti-join planning scales it back under budget.
///
/// Paths are repo-relative (the same key as [`shard_of`]).
const OFF_GATE_HEAVY: &[&str] = &[
    "generated/queries/ontolex.rq",
    "queries/qc/missing-definitions.rq",
    "generated/queries/schema-org.rq",
    "generated/queries/vcard.rq",
    "generated/queries/foaf.rq",
    "queries/verify/axis-not-disjoint.rq",
];

/// Green floor for the off-gate-heavy subset. Equal to the number of in-scope
/// (parity-matched) entries in [`OFF_GATE_HEAVY`]; observed 6 — the four CONSTRUCT
/// projections, `missing-definitions`, and the `axis-not-disjoint` anti-join carved out as
/// the ontology grew (the `class-without-stereotype` anti-join was re-gated once its inner
/// index became reusable, so it left this carve-out).
const OFF_GATE_HEAVY_MIN_GREEN: usize = 6;

/// True if `rel_path` (repo-relative, using `/` separators) is an off-gate-heavy query.
fn is_off_gate_heavy(rel_path: &str) -> bool {
    let norm = rel_path.replace('\\', "/");
    OFF_GATE_HEAVY.contains(&norm.as_str())
}

/// The classification result for one subset of the corpus.
#[cfg(all(feature = "oxigraph", feature = "gts"))]
struct CorpusTally {
    matched: usize,
    deferred: usize,
    nondet: usize,
    multi_query_skipped: usize,
    unexpected_failures: Vec<(String, String)>,
}

/// Load the real merged ontology once and run the differential parity sweep over
/// the corpus files for which `include(repo_relative_path)` is true. Returns the
/// tally; the caller prints it and applies the regression guards. `include` is how
/// shards (`shard_of == n`) and the off-gate-heavy carve-out partition the corpus.
#[cfg(all(feature = "oxigraph", feature = "gts"))]
fn run_corpus_subset(include: &dyn Fn(&str) -> bool) -> CorpusTally {
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
    let repo_root = corpus_repo_root();

    // -----------------------------------------------------------------------
    // 3. Per-query classification — included files only.
    // -----------------------------------------------------------------------
    let engine = NativeSparqlEngine::new();
    let ox = OxigraphBackend;

    let mut matched: usize = 0;
    let mut deferred: usize = 0;
    let mut nondet: usize = 0;
    let mut multi_query_skipped: usize = 0;
    let mut unexpected_failures: Vec<(String, String)> = Vec::new();

    for path in &corpus {
        // Stable, repo-relative key — skip files this subset does not own.
        // Always '/'-normalized so shard_of() hashes the same bytes on all platforms
        // (Windows paths would otherwise introduce '\\' separators that change the
        // FNV hash and invalidate the hardcoded per-shard floors).
        let rel = path
            .strip_prefix(&repo_root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if !include(&rel) {
            continue;
        }
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

    CorpusTally {
        matched,
        deferred,
        nondet,
        multi_query_skipped,
        unexpected_failures,
    }
}

/// Print the honest tally line and apply the shared regression guards (hard-fail on
/// ANY unexpected error/mismatch; assert the green floor). `scope` labels the subset.
#[cfg(all(feature = "oxigraph", feature = "gts"))]
fn assert_corpus_tally(scope: &str, tally: &CorpusTally, green_floor: usize) {
    eprintln!(
        "corpus parity [{scope}]: {} matched, {} deferred(paths/service/rdf12-triple-terms), \
         {} nondeterministic, {} multi-query-skipped, {} unexpected",
        tally.matched,
        tally.deferred,
        tally.nondet,
        tally.multi_query_skipped,
        tally.unexpected_failures.len()
    );
    // Hard fail on any unexpected error or parity mismatch — list ALL of them.
    assert!(
        tally.unexpected_failures.is_empty(),
        "[{scope}] in-scope corpus queries errored or mismatched unexpectedly ({} failures):\n{}",
        tally.unexpected_failures.len(),
        tally
            .unexpected_failures
            .iter()
            .map(|(f, e)| format!("  {f}\n    → {e}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    // Green floor — must not regress below the observed baseline.
    assert!(
        tally.matched >= green_floor,
        "[{scope}] green corpus shrank: {} matched < {green_floor} floor — a previously \
         passing query now fails; investigate before merging",
        tally.matched
    );
}

/// One gated shard: the corpus files assigned to `shard` by [`shard_of`], minus the
/// off-gate-heavy carve-out. Each shard reloads the ontology (~1.5 s) and runs its
/// slice; nextest parallelises the shards so wall time stays well under the 25 s budget.
#[cfg(all(feature = "oxigraph", feature = "gts"))]
fn run_corpus_shard(shard: usize) {
    let tally = run_corpus_subset(&|rel| !is_off_gate_heavy(rel) && shard_of(rel) == shard);
    assert_corpus_tally(
        &format!("shard {shard}/{NUM_SHARDS}"),
        &tally,
        CORPUS_MIN_GREEN_PER_SHARD[shard],
    );
}
