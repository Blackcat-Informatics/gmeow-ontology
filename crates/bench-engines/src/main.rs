// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `bench-engines` — the engine-vs-engine benchmark harness.
//!
//! It drives every committed mini bench case (`conformance/logic/cases/bench/`, or an
//! explicit `--corpus-dir`) through the NATIVE engine and, per fragment, the
//! applicable ORACLE, and emits TWO strictly-separated outputs:
//!
//! * **(2a) a DETERMINISTIC structured artifact** (`--emit-cost <path>`, else stdout):
//!   per `(corpus, case, engine)` the sorted `CostVector` `(rule, predicate, stratum,
//!   count)` tuples, `consumed_steps`, the derived-fact / answer counts, the native
//!   `peak_live_bytes` allocation scalar, and the verdict-agreement booleans+tokens.
//!   Every value is an INTEGER or a stable fingerprint and every map is serialized with
//!   sorted keys (serde_json's default `BTreeMap`-backed `Value`), so the bytes are a
//!   pure function of `(engine version, corpus)`. This is the drift-gate-eligible
//!   signal. It carries NO wall-clock, NO peak-RSS, and NO total-allocation scalars.
//!
//! * **(2b) a REPORT-ONLY advisory table** (always to stderr, clearly labeled
//!   "report-only, non-gating"): rows `(corpus, fragment, engine, wall_ns, peak_rss,
//!   alloc_bytes, alloc_count, verdict-agreement)`. Wall-clock and peak-RSS are
//!   NON-deterministic (clock jitter, allocator high-water, page reuse), and the
//!   total-allocation scalars carry a small non-deterministic transient (see below), so
//!   all of them live HERE and NEVER enter the committed artifact.
//!
//! # Which allocation scalar gates (measured, R1 pool-quiesce)
//!
//! `main` pins the process-GLOBAL Rayon pool to a SINGLE thread once
//! (`rayon::ThreadPoolBuilder::new().num_threads(1).build_global()`) before any engine
//! call — good measurement hygiene that makes `peak_live_bytes` rock-solid. Measured
//! across repeated runs (and repeated in-process calls), `consumed_steps`, the cost
//! vector, `derived_count`, every verdict-agreement token, AND `peak_live_bytes` are
//! byte-identical.
//!
//! The TOTAL-allocation scalars (`alloc_bytes` / `alloc_count`) are NOT: even under the
//! 1-thread pool they still vary ~0.008% on the most-recursive case (`same-generation`),
//! and — proven by three back-to-back IN-PROCESS measurements differing from each other
//! — the residue is irreducible transient scratch (Rayon block-coordination / allocator
//! bookkeeping the measuring thread performs while the engine runs), not a per-process
//! hash seed a thread cap could remove. `peak_live_bytes` — the high-water of NET
//! simultaneously-live bytes — nets that transient scratch to zero (each scratch
//! allocation is freed within the region, so it never raises the net-live high-water),
//! so it stays deterministic. Therefore ONLY `peak_live_bytes` enters the deterministic
//! artifact (2a); `alloc_bytes` / `alloc_count` are surfaced advisory-only (2b). This
//! keeps the Task 8 drift gate byte-stable while still tracking the total-allocation
//! signal (the "fewer clones / fewer owned-key allocations" proxy) as advisory data.
//!
//! # Verdict-agreement (deterministic set equality)
//!
//! For each `(case, world)` the harness compares, as deterministic set equality:
//!
//! * **native ↔ golden** — the native derived-fact / answer COUNT against the
//!   hand-derived golden's committed `rows` count (the golden carries only a count, so
//!   the token is `derived=<n>`);
//! * **native ↔ oracle** — for the FORWARD and BACKWARD fragments a `blake3`
//!   fingerprint of the fully-sorted result rows / bindings (a true set equality over
//!   ground terms), and for the EXISTENTIAL fragment the derived COUNT (the chase
//!   invents fresh labeled nulls whose identifiers legitimately differ per engine, so
//!   only the count is a sound cross-engine invariant).
//!
//! Both comparisons are folded through the SHARED divergence ledger
//! ([`gmeow_logic::reason::compare_external_corpus`] → [`build_ledger`] →
//! [`divergence_findings`]), so every agreement/divergence row is a restricted
//! `gmeow:Finding` carrying content-addressed ledger identity (`finding_iri` +
//! anchor + antecedents) — the harness reuses that machinery rather than reinventing
//! a parallel agreement vocabulary. The aggregate per-corpus tally
//! ([`agreement_tally`]) and the emitted `gmeow:Finding` N-Quads graph
//! ([`emit_divergence_nq`]) are folded in the same pass.
//!
//! # Oracle isolation (in-process, fresh engine per call)
//!
//! Cases run IN-PROCESS with a fresh EDB per case; no subprocess is needed. This is
//! sound because each oracle already rebuilds a FRESH engine per call: Nemo's
//! `nemo_engine::load_string(rls)` constructs a new engine inside every
//! `run_chase_typed`, serialized by `CHASE_LOCK`, and Scryer's `run_scryer` builds a
//! fresh `Machine` under `SCRYER_LOCK`. The production `reason::crosscheck_native_vs_nemo`
//! lane already dual-runs many cases in one process on exactly this design, so the
//! same in-process, lock-serialized pattern is the established precedent here.
//!
//! # Allocator confinement
//!
//! This binary — and ONLY this binary — installs the counting `#[global_allocator]`
//! from `gmeow-cost-measure`. A `#[global_allocator]` is process-global, so the blast
//! radius must be exactly this maintenance binary. The harness therefore lives in its
//! own DEDICATED LEAF crate (`gmeow-bench-engines`) precisely so `gmeow-cost-measure`
//! stays out of the shipped `gmeow` CLI's dependency graph: the CLI reaches
//! `gmeow-conformance` transitively (`gmeow-cli` → `gmeow-pipeline` →
//! `gmeow-conformance`, all normal edges — the conformance pipeline stage), so putting
//! this bin (and its `gmeow-cost-measure` dependency) INSIDE `gmeow-conformance` would
//! pull the counting allocator into the CLI's graph. As a leaf that nothing depends
//! on, this crate keeps that edge absent — verified by `cargo tree -p gmeow-cli -i
//! gmeow-cost-measure` returning nothing. `gmeow-conformance` itself gains no
//! `gmeow-cost-measure` edge; this crate depends on both as siblings.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::{Value, json};

use gmeow_conformance::bench_corpus::{
    BenchCase, Fragment, load_bench_corpora, load_bench_corpora_from,
};
use gmeow_conformance::divergence::{agreement_tally, emit_divergence_nq};
use gmeow_conformance::error::{Cli, Io, RunFailed, Serialize};

use gmeow_cost_measure::{CountingAllocator, measure};

use gmeow_errors::Diag;

use gmeow_logic::cost::{
    ForwardRows, run_native_forward, run_nemo_forward, run_nemo_forward_facts_only,
};
use gmeow_logic::dispatch::{cyclic_predicates, dispatch_query};
use gmeow_logic::materialize::materialize_routed;
use gmeow_logic::nary::{
    nary_canonical_fingerprint, run_native_nary_forward_run, run_nemo_nary_forward,
};
use gmeow_logic::nary_rls::parse_nary_rls_program;
use gmeow_logic::provenance::{ASSERT_RULE_IRI, term_display};
use gmeow_logic::query_ir::{AnswerSet, Budget, parse_query_program};
use gmeow_logic::reason::{
    ExternalComparison, build_ledger, compare_external_corpus, divergence_findings,
};
use gmeow_logic::result::EngineId;
use gmeow_logic::scryer_engine::run_scryer;
use gmeow_logic::seam::WorldStoreForeign;
use gmeow_logic::store::WorldStore;

/// Install the counting allocator on THIS binary (and only this binary) so
/// [`measure`] accounts the native engine's allocations. See the module docs.
#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::new();

/// The positive-Horn decidability profile the backward dispatch / existential chase
/// runs under (no cut, no negation — the mini backward/existential cases are pure
/// Horn). The dispatch profile is the full IRI the profile gate keys on; the
/// materialize router keys on the bare token.
const HORN_PROFILE_IRI: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
const HORN_PROFILE_TOKEN: &str = "PositiveHornProfile";

/// The pinned Nemo engine revision (mirrors `crates/logic/Cargo.toml`'s `nemo` git
/// `rev`). Surfaced as an engine-version pin in the deterministic artifact so a
/// cost/agreement baseline is attributable to the exact oracle build that produced it.
const NEMO_REV: &str = "4415bc2e180adf33a7a4b98ddc41be9914b7584e";
/// The pinned Scryer-Prolog branch (mirrors `crates/logic/Cargo.toml`'s
/// `scryer-prolog` git `branch`). Surfaced as an engine-version pin.
const SCRYER_BRANCH: &str = "master";

fn main() -> gmeow_errors::Result<()> {
    // R1 pool-quiesce: force the process-GLOBAL Rayon pool to a single thread BEFORE
    // any engine call, so every parallel operation the native engine issues runs on
    // the one measuring thread and the `gmeow-cost-measure` thread-local counters
    // capture bytes/count/peak-live COMPLETELY and deterministically — no stolen-closure
    // pollution, and no per-call `install()` wrapping (which the backward path's
    // `!Send` `WorldStore` could never satisfy). This is a measurement tool, not a
    // speed tool, so a single-thread global pool is exactly right. `build_global` can
    // be called only once per process and hard-errors if the pool was already built;
    // here it is the first statement, and a failure is a hard fail (no-optionality).
    rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build_global()
        .expect("bench-engines: single-thread global rayon pool must initialize");

    let mut corpus_dir: Option<PathBuf> = None;
    let mut emit_cost: Option<PathBuf> = None;
    let mut check_cost: Option<PathBuf> = None;
    let mut check_golden = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--check-golden" => {
                check_golden = true;
            }
            "--corpus-dir" => {
                corpus_dir = Some(PathBuf::from(args.next().ok_or_else(|| {
                    Diag::of_kind(Cli {
                        detail: "--corpus-dir requires a path value".to_string(),
                    })
                })?));
            }
            "--emit-cost" => {
                emit_cost = Some(PathBuf::from(args.next().ok_or_else(|| {
                    Diag::of_kind(Cli {
                        detail: "--emit-cost requires a path value".to_string(),
                    })
                })?));
            }
            "--check-cost" => {
                check_cost = Some(PathBuf::from(args.next().ok_or_else(|| {
                    Diag::of_kind(Cli {
                        detail: "--check-cost requires a path value".to_string(),
                    })
                })?));
            }
            other => {
                return Err(Diag::of_kind(Cli {
                    detail: format!("unknown argument: {other}"),
                }));
            }
        }
    }

    // Default is the committed mini corpus; `--corpus-dir` is the seam a later fetch
    // lane points at a full fetched-distribution root (the same hard-fail loader and
    // license audit govern both).
    let cases = match &corpus_dir {
        Some(root) => load_bench_corpora_from(root)?,
        None => load_bench_corpora()?,
    };
    if cases.is_empty() {
        return Err(Diag::of_kind(RunFailed {
            detail: "the bench corpus loaded zero cases; expected at least one".to_string(),
        }));
    }

    // ── (1) The ON-GATE native-vs-golden agreement gate ─────────────────────────
    // `--check-golden` runs the NATIVE engine ONLY over the committed mini corpora and
    // HARD-FAILS if any case's native result disagrees with its committed golden
    // (`expected/result.json`). It deliberately drives NO oracle (Nemo/Scryer): golden
    // agreement is native-vs-published only, so the check stays cheap enough to wire into
    // `make check`. It returns before any oracle run / artifact emission below.
    if check_golden {
        return run_golden_gate(&cases);
    }

    // Per-case deterministic records, and the report-only advisory rows.
    let mut case_records: Vec<Value> = Vec::new();
    let mut advisory: Vec<AdvisoryRow> = Vec::new();
    // Divergence comparisons, grouped by corpus (deterministic order preserved: the
    // corpus vector is sorted by (corpus, case) coming out of the loader).
    let mut comps_by_corpus: BTreeMap<String, Vec<ExternalComparison>> = BTreeMap::new();

    for case in &cases {
        let outcome = run_case(case)?;
        case_records.push(outcome.record);
        advisory.extend(outcome.advisory);
        comps_by_corpus
            .entry(case.corpus.clone())
            .or_default()
            .extend(outcome.comparisons);
    }

    // Fold each corpus's comparisons through the shared divergence ledger so the
    // agreement/divergence rows carry content-addressed ledger identity, and surface
    // the aggregate per-corpus tally + emitted finding-graph digest.
    let mut corpus_ledgers: BTreeMap<String, Value> = BTreeMap::new();
    for (corpus, comps) in &comps_by_corpus {
        let rows = compare_external_corpus(corpus, comps);
        let ledger = build_ledger(Vec::new(), Vec::new(), Vec::new(), rows);
        let findings = divergence_findings(&ledger);
        let tally = agreement_tally(corpus, comps);
        let graph = emit_divergence_nq(corpus, comps);
        let graph_digest = blake3::hash(graph.as_bytes()).to_hex().to_string();

        // Surface each finding's ledger identity (finding_iri + code + anchor) so the
        // deterministic artifact demonstrates the join keys, sorted by IRI.
        let mut ids: Vec<Value> = findings
            .iter()
            .map(|f| {
                json!({
                    "code": f.code,
                    "finding_iri": f.finding_iri.clone().unwrap_or_default(),
                    "anchor_iri": f.anchor_iri.clone().unwrap_or_default(),
                    "antecedents": f.antecedents.clone(),
                })
            })
            .collect();
        ids.sort_by(|a, b| a["finding_iri"].as_str().cmp(&b["finding_iri"].as_str()));

        corpus_ledgers.insert(
            corpus.clone(),
            json!({
                "cases": tally.cases,
                "agree": tally.agree,
                "corpus_only": tally.corpus_only,
                "dl_gap": tally.dl_gap,
                "finding_count": findings.len(),
                "finding_graph_blake3": graph_digest,
                "findings": ids,
            }),
        );
    }

    // ── Cost-regression check (L3): compare THIS fresh run's deterministic cost +
    //    verdict-agreement against the committed baseline; ANY divergence is a
    //    cost-regression gmeow:Finding routed through the SHARED divergence ledger
    //    (content-addressed identity), and hard-fails the run. This is the richer
    //    honesty surface behind the primary on-gate `check-generated` cost-ledger
    //    drift gate. Run BEFORE the artifact is assembled (it borrows the records). ──
    if let Some(baseline_path) = &check_cost {
        run_cost_regression_check(baseline_path, &case_records)?;
    }

    // ── (2a) The DETERMINISTIC structured artifact ──────────────────────────────
    let artifact = json!({
        "schema": "gmeow.bench-engines.cost/1",
        "engine_pins": {
            "native": EngineId::native().version,
            "nemo_rev": NEMO_REV,
            "scryer_branch": SCRYER_BRANCH,
        },
        "case_count": case_records.len(),
        "cases": case_records,
        "ledgers": corpus_ledgers,
    });
    let json = serde_json::to_string_pretty(&artifact).map_err(|e| {
        Diag::of_kind(Serialize {
            detail: e.to_string(),
        })
    })? + "\n";

    match &emit_cost {
        Some(path) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    Diag::of_kind(Io {
                        detail: format!("creating {}: {e}", parent.display()),
                    })
                })?;
            }
            std::fs::write(path, &json).map_err(|e| {
                Diag::of_kind(Io {
                    detail: format!("writing {}: {e}", path.display()),
                })
            })?;
        }
        None => {
            print!("{json}");
        }
    }

    // ── (2b) The REPORT-ONLY advisory table (never gating) ──────────────────────
    print_advisory_table(&advisory);

    Ok(())
}

/// The per-case result: the deterministic JSON record, the report-only advisory
/// rows, and the divergence comparisons to fold into the ledger.
struct CaseOutcome {
    record: Value,
    advisory: Vec<AdvisoryRow>,
    comparisons: Vec<ExternalComparison>,
}

/// One report-only advisory row (stderr only, non-gating). It carries the
/// NON-deterministic wall-clock / peak-RSS plus the total-allocation scalars
/// (`alloc_bytes` / `alloc_count`), which carry a small irreducible transient (Rayon /
/// allocator scratch) and so are advisory-only — never in the committed artifact.
/// `alloc_bytes`/`alloc_count` are `0` for the oracle rows (only the native run is
/// allocation-measured).
struct AdvisoryRow {
    corpus: String,
    fragment: &'static str,
    engine: &'static str,
    wall_ns: u128,
    peak_rss_kib: u64,
    alloc_bytes: u64,
    alloc_count: u64,
    agreement: bool,
}

/// Drive one case through the native engine and the applicable oracle. The measured
/// native run is deterministic because `main` pinned the global Rayon pool to a single
/// thread, so all engine allocation lands on the measuring thread.
fn run_case(case: &BenchCase) -> gmeow_errors::Result<CaseOutcome> {
    match case.fragment {
        Fragment::Forward => run_forward(case),
        Fragment::Existential => run_existential(case),
        Fragment::Backward => run_backward(case),
        Fragment::NaryExistential => run_nary(case),
    }
}

/// The single world every mini bench case is scoped to (its golden is keyed by world
/// IRI). The forward/existential seams materialize over the whole dataset; the mini
/// corpora are single-world, so the aggregate derived count equals the sole world's
/// golden. A multi-world case is a hard error (the aggregate would be ambiguous),
/// never a silent mis-attribution.
fn sole_world(case: &BenchCase) -> gmeow_errors::Result<(String, u64)> {
    if case.golden.len() != 1 {
        return Err(Diag::of_kind(RunFailed {
            detail: format!(
                "{}/{}: the forward/existential aggregate comparison requires exactly one \
                 golden world, found {} — a multi-world bench case needs a per-world seam",
                case.corpus,
                case.name,
                case.golden.len()
            ),
        }));
    }
    let (world, golden) = case.golden.iter().next().expect("checked len == 1");
    Ok((world.clone(), golden.rows))
}

/// FORWARD fragment: native `run_native_forward` (with the allocation sample plugged
/// into its cost vector) vs the Nemo oracle `run_nemo_forward`.
fn run_forward(case: &BenchCase) -> gmeow_errors::Result<CaseOutcome> {
    let (world, golden_rows) = sole_world(case)?;

    let edb = purrdf::parse_dataset(case.edb_nq.as_bytes(), "application/n-quads", None)
        .map_err(|e| run_err(case, format!("EDB parse error: {e}")))?;

    // Native (measured). The global Rayon pool is single-threaded (set in `main`), so
    // the engine's parallel work runs on the measuring thread and the thread-local
    // allocation counters capture it completely and deterministically — R1 pool-quiesce.
    let native_start = Instant::now();
    let (native_res, sample) = measure(|| run_native_forward(edb.as_ref(), &case.rules));
    let native_wall = native_start.elapsed().as_nanos();
    let mut native =
        native_res.map_err(|e| run_err(case, format!("native forward failed: {e}")))?;
    native
        .cost
        .set_allocation(sample.bytes, sample.count, sample.peak_live);
    let native_derived = native.cost.total_derivations();
    let native_fp = fingerprint_rows(&native.rows);

    // Nemo oracle.
    let nemo_start = Instant::now();
    let nemo = run_nemo_forward(edb.as_ref(), &case.rules)
        .map_err(|e| run_err(case, format!("nemo forward failed: {e}")))?;
    let nemo_wall = nemo_start.elapsed().as_nanos();
    let nemo_fp = fingerprint_rows(&nemo);

    // native ↔ golden by derived count; native ↔ nemo by full-row fingerprint.
    let native_golden_tok = count_token(native_derived);
    let golden_tok = count_token(golden_rows);
    let agree_golden = native_golden_tok == golden_tok;
    let agree_nemo = native_fp == nemo_fp;

    let comparisons = vec![
        comp(
            case,
            &world,
            "native-vs-golden",
            &native_golden_tok,
            &golden_tok,
        ),
        comp(case, &world, "native-vs-nemo", &native_fp, &nemo_fp),
    ];

    let record = json!({
        "corpus": case.corpus,
        "case": case.name,
        "fragment": "forward",
        "world": world,
        "golden_rows": golden_rows,
        "native": {
            "engine": native.engine.version,
            "consumed_steps": native.consumed_steps,
            "derived_count": native_derived,
            // Total allocation bytes/count carry small non-deterministic transient
            // scratch (rayon/allocator) in the current native core, so they are
            // advisory-only (2b); peak simultaneously-live bytes nets transient scratch
            // to zero and is the deterministic, gate-eligible allocation metric.
            "peak_live_bytes": native.cost.peak_live_bytes(),
            "rows_fingerprint": native_fp,
            "cost_vector": cost_tuples(&native.cost.to_sorted_tuples()),
        },
        "oracle": {
            "engine": "nemo",
            "rows_fingerprint": nemo_fp,
            "row_count": nemo.len(),
        },
        "agreement": {
            "native_vs_golden": agree_golden,
            "native_vs_oracle": agree_nemo,
            "native_golden_token": native_golden_tok,
            "golden_token": golden_tok,
            "native_oracle_token": native_fp,
            "oracle_token": nemo_fp,
        },
    });

    let peak = peak_rss_kib();
    Ok(CaseOutcome {
        record,
        advisory: vec![
            AdvisoryRow {
                corpus: case.corpus.clone(),
                fragment: "forward",
                engine: "native",
                wall_ns: native_wall,
                peak_rss_kib: peak,
                alloc_bytes: native.cost.alloc_bytes(),
                alloc_count: native.cost.alloc_count(),
                agreement: agree_golden,
            },
            AdvisoryRow {
                corpus: case.corpus.clone(),
                fragment: "forward",
                engine: "nemo",
                wall_ns: nemo_wall,
                peak_rss_kib: peak,
                alloc_bytes: 0,
                alloc_count: 0,
                agreement: agree_nemo,
            },
        ],
        comparisons,
    })
}

/// EXISTENTIAL fragment: native value-inventing chase (`materialize_routed`) vs the
/// Nemo oracle. The chase invents fresh labeled nulls whose identifiers legitimately
/// differ per engine, so the native↔nemo comparison is by DERIVED COUNT (the sound
/// cross-engine invariant), not a row fingerprint.
fn run_existential(case: &BenchCase) -> gmeow_errors::Result<CaseOutcome> {
    let (world, golden_rows) = sole_world(case)?;

    // Native chase (measured; global Rayon pool is single-threaded — R1 pool-quiesce).
    let native_start = Instant::now();
    let (native_res, sample) = measure(|| {
        materialize_routed(
            &case.rules,
            &case.edb_nq,
            None,
            None,
            None,
            Some(HORN_PROFILE_TOKEN),
        )
    });
    let native_wall = native_start.elapsed().as_nanos();
    let native = native_res.map_err(|e| run_err(case, format!("native chase failed: {e}")))?;
    // Derived = every quad NOT attributed to the EDB-echo assert rule.
    let native_derived = native
        .quads
        .iter()
        .filter(|q| q.rule_iri != ASSERT_RULE_IRI)
        .count() as u64;
    let consumed_steps = native.frontier.consumed_steps;

    // Nemo oracle: derived = rows whose predicate is not an EDB predicate.
    let edb = purrdf::parse_dataset(case.edb_nq.as_bytes(), "application/n-quads", None)
        .map_err(|e| run_err(case, format!("EDB parse error: {e}")))?;
    let edb_preds = edb_predicates(&case.edb_nq)?;
    // The provenance-carrying Nemo forward oracle rejects invented nulls, so the
    // existential fragment drives the facts-only Nemo chase (the SAME path the
    // materialize router demotes uncertified existentials to).
    let nemo_start = Instant::now();
    let nemo = run_nemo_forward_facts_only(edb.as_ref(), &case.rules)
        .map_err(|e| run_err(case, format!("nemo facts-only chase failed: {e}")))?;
    let nemo_wall = nemo_start.elapsed().as_nanos();
    let nemo_derived = nemo
        .rows
        .iter()
        .filter(|r| !edb_preds.contains(&r.predicate))
        .count() as u64;

    let native_golden_tok = count_token(native_derived);
    let golden_tok = count_token(golden_rows);
    let native_nemo_tok = count_token(native_derived);
    let nemo_tok = count_token(nemo_derived);
    let agree_golden = native_golden_tok == golden_tok;
    let agree_nemo = native_nemo_tok == nemo_tok;

    let comparisons = vec![
        comp(
            case,
            &world,
            "native-vs-golden",
            &native_golden_tok,
            &golden_tok,
        ),
        comp(case, &world, "native-vs-nemo", &native_nemo_tok, &nemo_tok),
    ];

    let record = json!({
        "corpus": case.corpus,
        "case": case.name,
        "fragment": "existential",
        "world": world,
        "golden_rows": golden_rows,
        "native": {
            "engine": EngineId::native().version,
            "consumed_steps": consumed_steps,
            "derived_count": native_derived,
            // peak-live is the deterministic, gate-eligible allocation metric; the
            // total bytes/count carry small transient scratch and are advisory-only (2b).
            "peak_live_bytes": sample.peak_live,
            // The chase seam exposes no decomposable (rule,predicate,stratum) vector,
            // so none is fabricated (the no-optionality doctrine: an absent measure is
            // absent, never a zeroed lie).
            "cost_vector": Value::Array(Vec::new()),
        },
        "oracle": {
            "engine": "nemo",
            "derived_count": nemo_derived,
        },
        "agreement": {
            "native_vs_golden": agree_golden,
            "native_vs_oracle": agree_nemo,
            "native_golden_token": native_golden_tok,
            "golden_token": golden_tok,
            "native_oracle_token": native_nemo_tok,
            "oracle_token": nemo_tok,
        },
    });

    let peak = peak_rss_kib();
    Ok(CaseOutcome {
        record,
        advisory: vec![
            AdvisoryRow {
                corpus: case.corpus.clone(),
                fragment: "existential",
                engine: "native",
                wall_ns: native_wall,
                peak_rss_kib: peak,
                alloc_bytes: sample.bytes,
                alloc_count: sample.count,
                agreement: agree_golden,
            },
            AdvisoryRow {
                corpus: case.corpus.clone(),
                fragment: "existential",
                engine: "nemo",
                wall_ns: nemo_wall,
                peak_rss_kib: peak,
                alloc_bytes: 0,
                alloc_count: 0,
                agreement: agree_nemo,
            },
        ],
        comparisons,
    })
}

/// NARY-EXISTENTIAL fragment: the native reified-binary n-ary chase
/// (`run_native_nary_forward_run`) vs the Nemo n-ary oracle (`run_nemo_nary_forward`) over
/// the SAME n-ary `.rls` program + delimited (`data/<rel>.csv`) EDB. Both engines invent
/// fresh nulls whose identifiers legitimately differ per engine, so the native↔nemo verdict
/// is a NULL-BLIND canonical fingerprint (`nary_canonical_fingerprint`, colour refinement),
/// not a raw row fingerprint — the sound cross-engine invariant for a value-inventing
/// n-ary chase. The native closure's tuple COUNT drives the native↔golden comparison.
fn run_nary(case: &BenchCase) -> gmeow_errors::Result<CaseOutcome> {
    let (world, golden_rows) = sole_world(case)?;

    // Parse the n-ary `.rls` ONCE; the SAME program drives native (via the reified lowering)
    // and Nemo (verbatim). A genuinely-unsupported construct hard-fails here (named).
    let rules = parse_nary_rls_program(&case.rules)
        .map_err(|e| run_err(case, format!("n-ary .rls parse failed: {e}")))?;

    // Native (measured; global Rayon pool is single-threaded — R1 pool-quiesce).
    let native_start = Instant::now();
    let (native_res, sample) = measure(|| run_native_nary_forward_run(&case.nary_edb, &rules));
    let native_wall = native_start.elapsed().as_nanos();
    let native =
        native_res.map_err(|e| run_err(case, format!("native n-ary chase failed: {e}")))?;
    let native_derived = native.tuples.len() as u64;
    let consumed_steps = native.consumed_steps;
    let native_fp = nary_canonical_fingerprint(&native.tuples);

    // Nemo n-ary oracle over the SAME EDB + program (facts-only typed chase at full arity).
    let nemo_start = Instant::now();
    let nemo = run_nemo_nary_forward(&case.nary_edb, &case.rules)
        .map_err(|e| run_err(case, format!("nemo n-ary chase failed: {e}")))?;
    let nemo_wall = nemo_start.elapsed().as_nanos();
    let nemo_fp = nary_canonical_fingerprint(&nemo);

    // native ↔ golden by de-reified closure count; native ↔ nemo by null-blind fingerprint.
    let native_golden_tok = count_token(native_derived);
    let golden_tok = count_token(golden_rows);
    let agree_golden = native_golden_tok == golden_tok;
    let agree_nemo = native_fp == nemo_fp;

    let comparisons = vec![
        comp(
            case,
            &world,
            "native-vs-golden",
            &native_golden_tok,
            &golden_tok,
        ),
        comp(case, &world, "native-vs-nemo", &native_fp, &nemo_fp),
    ];

    let record = json!({
        "corpus": case.corpus,
        "case": case.name,
        "fragment": "nary-existential",
        "world": world,
        "golden_rows": golden_rows,
        "native": {
            "engine": EngineId::native().version,
            "consumed_steps": consumed_steps,
            "derived_count": native_derived,
            // peak-live is the deterministic, gate-eligible allocation metric; the total
            // bytes/count carry small transient scratch and are advisory-only (2b).
            "peak_live_bytes": sample.peak_live,
            // The reified n-ary chase seam exposes no decomposable (rule,predicate,stratum)
            // vector, so none is fabricated (no-optionality: an absent measure is absent).
            "cost_vector": Value::Array(Vec::new()),
            "closure_fingerprint": native_fp,
        },
        "oracle": {
            "engine": "nemo",
            "closure_count": nemo.len(),
            "closure_fingerprint": nemo_fp,
        },
        "agreement": {
            "native_vs_golden": agree_golden,
            "native_vs_oracle": agree_nemo,
            "native_golden_token": native_golden_tok,
            "golden_token": golden_tok,
            "native_oracle_token": native_fp,
            "oracle_token": nemo_fp,
        },
    });

    let peak = peak_rss_kib();
    Ok(CaseOutcome {
        record,
        advisory: vec![
            AdvisoryRow {
                corpus: case.corpus.clone(),
                fragment: "nary-existential",
                engine: "native",
                wall_ns: native_wall,
                peak_rss_kib: peak,
                alloc_bytes: sample.bytes,
                alloc_count: sample.count,
                agreement: agree_golden,
            },
            AdvisoryRow {
                corpus: case.corpus.clone(),
                fragment: "nary-existential",
                engine: "nemo",
                wall_ns: nemo_wall,
                peak_rss_kib: peak,
                alloc_bytes: 0,
                alloc_count: 0,
                agreement: agree_nemo,
            },
        ],
        comparisons,
    })
}

/// BACKWARD fragment: native goal-directed `dispatch_query` vs the Scryer oracle
/// `run_scryer`. The EDB loads into a `WorldStore`; the `.logic` query text parses to
/// a `QProgram`; both engines answer the same goal against the same world snapshot.
fn run_backward(case: &BenchCase) -> gmeow_errors::Result<CaseOutcome> {
    if case.golden.len() != 1 {
        return Err(Diag::of_kind(RunFailed {
            detail: format!(
                "{}/{}: backward bench case must carry exactly one golden world, found {}",
                case.corpus,
                case.name,
                case.golden.len()
            ),
        }));
    }
    let (world, golden) = case.golden.iter().next().expect("checked len == 1");
    let golden_rows = golden.rows;

    let store = WorldStore::new();
    store
        .load_nquads(&case.edb_nq)
        .map_err(|e| run_err(case, format!("EDB load failed: {e}")))?;
    let program = parse_query_program(&case.rules)
        .map_err(|e| run_err(case, format!("query parse failed: {e}")))?;
    let foreign = WorldStoreForeign::from_world(&store, world, HORN_PROFILE_IRI)
        .map_err(|e| run_err(case, format!("foreign snapshot failed: {e}")))?;
    let budget = Budget::default();
    let table_preds = cyclic_predicates(&program);

    // Native backward (measured; global Rayon pool is single-threaded — R1 pool-quiesce).
    let native_start = Instant::now();
    let (native_res, sample) =
        measure(|| dispatch_query(&foreign, &store, world, &program, HORN_PROFILE_IRI, &budget));
    let native_wall = native_start.elapsed().as_nanos();
    let native = native_res.map_err(|e| run_err(case, format!("native backward failed: {e}")))?;
    let native_count = native.bindings.len() as u64;
    let native_fp = fingerprint_answers(&native);
    let consumed_steps = native.frontier.consumed_steps;

    // Scryer oracle.
    let scryer_start = Instant::now();
    let scryer = run_scryer(&foreign, world, &program, &table_preds, &budget)
        .map_err(|e| run_err(case, format!("scryer backward failed: {e}")))?;
    let scryer_wall = scryer_start.elapsed().as_nanos();
    let scryer_fp = fingerprint_answers(&scryer);

    let native_golden_tok = count_token(native_count);
    let golden_tok = count_token(golden_rows);
    let agree_golden = native_golden_tok == golden_tok;
    let agree_scryer = native_fp == scryer_fp;

    let comparisons = vec![
        comp(
            case,
            world,
            "native-vs-golden",
            &native_golden_tok,
            &golden_tok,
        ),
        comp(case, world, "native-vs-scryer", &native_fp, &scryer_fp),
    ];

    let record = json!({
        "corpus": case.corpus,
        "case": case.name,
        "fragment": "backward",
        "world": world,
        "golden_rows": golden_rows,
        "native": {
            "engine": EngineId::native().version,
            "consumed_steps": consumed_steps,
            "answer_count": native_count,
            // peak-live is the deterministic, gate-eligible allocation metric; the
            // total bytes/count carry small transient scratch and are advisory-only (2b).
            "peak_live_bytes": sample.peak_live,
            "answers_fingerprint": native_fp,
            // No decomposable cost vector at the backward dispatch seam.
            "cost_vector": Value::Array(Vec::new()),
        },
        "oracle": {
            "engine": "scryer",
            "answer_count": scryer.bindings.len(),
            "answers_fingerprint": scryer_fp,
        },
        "agreement": {
            "native_vs_golden": agree_golden,
            "native_vs_oracle": agree_scryer,
            "native_golden_token": native_golden_tok,
            "golden_token": golden_tok,
            "native_oracle_token": native_fp,
            "oracle_token": scryer_fp,
        },
    });

    let peak = peak_rss_kib();
    Ok(CaseOutcome {
        record,
        advisory: vec![
            AdvisoryRow {
                corpus: case.corpus.clone(),
                fragment: "backward",
                engine: "native",
                wall_ns: native_wall,
                peak_rss_kib: peak,
                alloc_bytes: sample.bytes,
                alloc_count: sample.count,
                agreement: agree_golden,
            },
            AdvisoryRow {
                corpus: case.corpus.clone(),
                fragment: "backward",
                engine: "scryer",
                wall_ns: scryer_wall,
                peak_rss_kib: peak,
                alloc_bytes: 0,
                alloc_count: 0,
                agreement: agree_scryer,
            },
        ],
        comparisons,
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────────────

/// The `(corpus, case)` identity key of a per-case deterministic record. Hard-fails if
/// either field is missing (a malformed record is an error, never silently keyed empty).
fn case_key(rec: &Value) -> gmeow_errors::Result<(String, String)> {
    let corpus = rec.get("corpus").and_then(Value::as_str).ok_or_else(|| {
        Diag::of_kind(RunFailed {
            detail: "cost record missing `corpus`".to_string(),
        })
    })?;
    let case = rec.get("case").and_then(Value::as_str).ok_or_else(|| {
        Diag::of_kind(RunFailed {
            detail: format!("cost record for corpus {corpus} missing `case`"),
        })
    })?;
    Ok((corpus.to_owned(), case.to_owned()))
}

/// A COMPLETE deterministic descriptor of one case's native cost + verdict-agreement
/// sub-records. serde_json serializes object keys sorted (no `preserve_order` feature),
/// so this is byte-stable; ANY change to a deterministic count / fingerprint / verdict
/// changes the descriptor, and thus surfaces as a cost-regression divergence.
fn cost_descriptor(rec: &Value) -> String {
    let native = rec.get("native").cloned().unwrap_or(Value::Null);
    let agreement = rec.get("agreement").cloned().unwrap_or(Value::Null);
    // Both sub-objects are already integer/boolean/fingerprint-valued; a compact
    // canonical JSON of the pair is the comparable descriptor.
    format!(
        "native={} agreement={}",
        serde_json::to_string(&native).unwrap_or_default(),
        serde_json::to_string(&agreement).unwrap_or_default(),
    )
}

/// Compare THIS fresh run's per-case deterministic cost + verdict-agreement against the
/// committed baseline (`bench/cost-baseline.json`). Each `(corpus, case)` divergence —
/// a changed count/fingerprint/verdict, a case dropped from the run, or a case absent
/// from the baseline — is folded through the SHARED divergence ledger as a `CorpusOnly`
/// row, so every cost regression becomes a `gmeow:Finding` carrying content-addressed
/// ledger identity (`finding_iri` + anchor + antecedents), NOT a bare diff. Emits the
/// regression findings and HARD-FAILS on any divergence (no-optionality).
fn run_cost_regression_check(baseline_path: &Path, fresh: &[Value]) -> gmeow_errors::Result<()> {
    let bytes = std::fs::read(baseline_path).map_err(|e| {
        Diag::of_kind(Io {
            detail: format!("reading cost baseline {}: {e}", baseline_path.display()),
        })
    })?;
    let baseline: Value = serde_json::from_slice(&bytes).map_err(|e| {
        Diag::of_kind(Serialize {
            detail: format!("cost baseline {} parse: {e}", baseline_path.display()),
        })
    })?;
    let base_cases = baseline
        .get("cases")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            Diag::of_kind(RunFailed {
                detail: format!(
                    "cost baseline {} has no `cases` array",
                    baseline_path.display()
                ),
            })
        })?;

    // Index both sides by (corpus, case).
    let mut fresh_idx: BTreeMap<(String, String), &Value> = BTreeMap::new();
    for r in fresh {
        fresh_idx.insert(case_key(r)?, r);
    }
    let mut base_idx: BTreeMap<(String, String), &Value> = BTreeMap::new();
    for r in base_cases {
        base_idx.insert(case_key(r)?, r);
    }

    // The union of keys → per-corpus comparisons (deterministic BTree order).
    let mut keys: BTreeSet<(String, String)> = BTreeSet::new();
    keys.extend(fresh_idx.keys().cloned());
    keys.extend(base_idx.keys().cloned());

    let mut comps_by_corpus: BTreeMap<String, Vec<ExternalComparison>> = BTreeMap::new();
    for key in &keys {
        let (corpus, case) = key;
        let fresh_rec = fresh_idx.get(key);
        let base_rec = base_idx.get(key);
        let native = fresh_rec
            .map(|r| cost_descriptor(r))
            .unwrap_or_else(|| "absent-from-fresh-run".to_string());
        let published = base_rec
            .map(|r| cost_descriptor(r))
            .unwrap_or_else(|| "absent-from-baseline".to_string());
        let world = fresh_rec
            .or(base_rec)
            .and_then(|r| r.get("world").and_then(Value::as_str))
            .unwrap_or("")
            .to_owned();
        comps_by_corpus
            .entry(corpus.clone())
            .or_default()
            .push(ExternalComparison {
                case: format!("{corpus}/{case}::cost"),
                world,
                native,
                published,
            });
    }

    // Fold each corpus's comparisons through the shared divergence ledger: an equal
    // descriptor is `Agree` (a non-blocking corroboration finding), a divergent one is
    // `CorpusOnly` (a blocking cost-regression finding with content-addressed identity).
    let mut regressions = 0usize;
    let mut regression_findings: Vec<Value> = Vec::new();
    for (corpus, comps) in &comps_by_corpus {
        let rows = compare_external_corpus(corpus, comps);
        let ledger = build_ledger(Vec::new(), Vec::new(), Vec::new(), rows);
        regressions += ledger.corpus_only;
        for f in divergence_findings(&ledger) {
            if f.code == "reason.divergence.corpus-only" {
                regression_findings.push(json!({
                    "code": f.code,
                    "finding_iri": f.finding_iri.clone().unwrap_or_default(),
                    "anchor_iri": f.anchor_iri.clone().unwrap_or_default(),
                    "antecedents": f.antecedents.clone(),
                    "message": f.message,
                }));
            }
        }
    }

    if regressions == 0 {
        eprintln!(
            "✓ cost-regression check: {} case(s) match the committed baseline {} (no deterministic-count divergence).",
            keys.len(),
            baseline_path.display()
        );
        return Ok(());
    }

    // Emit the cost-regression findings (each carrying its ledger identity), sorted by
    // finding IRI for determinism, then HARD-FAIL.
    regression_findings.sort_by(|a, b| a["finding_iri"].as_str().cmp(&b["finding_iri"].as_str()));
    let report = json!({
        "schema": "gmeow.bench-engines.cost-regression/1",
        "regression_count": regressions,
        "baseline": baseline_path.display().to_string(),
        "findings": regression_findings,
    });
    let json = serde_json::to_string_pretty(&report).map_err(|e| {
        Diag::of_kind(Serialize {
            detail: e.to_string(),
        })
    })?;
    println!("{json}");
    Err(Diag::of_kind(RunFailed {
        detail: format!(
            "{regressions} cost-regression finding(s): the fresh engine cost/agreement run diverged \
             from the committed baseline {} — regenerate + review before refreshing the baseline",
            baseline_path.display()
        ),
    }))
}

/// Drive one case through the NATIVE engine ONLY and return `(world, native_count,
/// golden_rows)` — the native derived-fact / answer COUNT and the committed golden's
/// count for its sole world. This is the exact native invocation each `run_*` fragment
/// runner performs, minus the allocation `measure` wrapper and WITHOUT touching any
/// oracle (Nemo/Scryer are never constructed), so the golden gate stays cheap on-gate.
fn native_golden_pair(case: &BenchCase) -> gmeow_errors::Result<(String, u64, u64)> {
    match case.fragment {
        Fragment::Forward => {
            let (world, golden_rows) = sole_world(case)?;
            let edb = purrdf::parse_dataset(case.edb_nq.as_bytes(), "application/n-quads", None)
                .map_err(|e| run_err(case, format!("EDB parse error: {e}")))?;
            let native = run_native_forward(edb.as_ref(), &case.rules)
                .map_err(|e| run_err(case, format!("native forward failed: {e}")))?;
            Ok((world, native.cost.total_derivations(), golden_rows))
        }
        Fragment::Existential => {
            let (world, golden_rows) = sole_world(case)?;
            let native = materialize_routed(
                &case.rules,
                &case.edb_nq,
                None,
                None,
                None,
                Some(HORN_PROFILE_TOKEN),
            )
            .map_err(|e| run_err(case, format!("native chase failed: {e}")))?;
            let native_derived = native
                .quads
                .iter()
                .filter(|q| q.rule_iri != ASSERT_RULE_IRI)
                .count() as u64;
            Ok((world, native_derived, golden_rows))
        }
        Fragment::NaryExistential => {
            let (world, golden_rows) = sole_world(case)?;
            let rules = parse_nary_rls_program(&case.rules)
                .map_err(|e| run_err(case, format!("n-ary .rls parse failed: {e}")))?;
            let native = run_native_nary_forward_run(&case.nary_edb, &rules)
                .map_err(|e| run_err(case, format!("native n-ary chase failed: {e}")))?;
            Ok((world, native.tuples.len() as u64, golden_rows))
        }
        Fragment::Backward => {
            if case.golden.len() != 1 {
                return Err(Diag::of_kind(RunFailed {
                    detail: format!(
                        "{}/{}: backward bench case must carry exactly one golden world, found {}",
                        case.corpus,
                        case.name,
                        case.golden.len()
                    ),
                }));
            }
            let (world, golden) = case.golden.iter().next().expect("checked len == 1");
            let golden_rows = golden.rows;
            let store = WorldStore::new();
            store
                .load_nquads(&case.edb_nq)
                .map_err(|e| run_err(case, format!("EDB load failed: {e}")))?;
            let program = parse_query_program(&case.rules)
                .map_err(|e| run_err(case, format!("query parse failed: {e}")))?;
            let foreign = WorldStoreForeign::from_world(&store, world, HORN_PROFILE_IRI)
                .map_err(|e| run_err(case, format!("foreign snapshot failed: {e}")))?;
            let budget = Budget::default();
            let native =
                dispatch_query(&foreign, &store, world, &program, HORN_PROFILE_IRI, &budget)
                    .map_err(|e| run_err(case, format!("native backward failed: {e}")))?;
            Ok((world.clone(), native.bindings.len() as u64, golden_rows))
        }
    }
}

/// (Deliverable 1) The ON-GATE native-vs-golden agreement gate. Runs the NATIVE engine
/// ONLY over every committed mini bench case and compares its deterministic derived /
/// answer COUNT against the committed golden (`expected/result.json`). Each `(corpus,
/// case)` comparison is folded through the SHARED divergence ledger — an equal count is
/// `Agree`, a divergent one is `CorpusOnly` — so every disagreement becomes a named
/// `gmeow:Finding` carrying content-addressed ledger identity (`finding_iri` + anchor +
/// antecedents), NOT a bare diff. ANY disagreement HARD-FAILS (no-optionality). No oracle
/// is driven, so this is cheap enough for `make check`.
fn run_golden_gate(cases: &[BenchCase]) -> gmeow_errors::Result<()> {
    let mut comps_by_corpus: BTreeMap<String, Vec<ExternalComparison>> = BTreeMap::new();
    for case in cases {
        let (world, native_count, golden_rows) = native_golden_pair(case)?;
        comps_by_corpus
            .entry(case.corpus.clone())
            .or_default()
            .push(comp(
                case,
                &world,
                "native-vs-golden",
                &count_token(native_count),
                &count_token(golden_rows),
            ));
    }

    let mut disagreements = 0usize;
    let mut findings: Vec<Value> = Vec::new();
    for (corpus, comps) in &comps_by_corpus {
        let rows = compare_external_corpus(corpus, comps);
        let ledger = build_ledger(Vec::new(), Vec::new(), Vec::new(), rows);
        disagreements += ledger.corpus_only;
        for f in divergence_findings(&ledger) {
            if f.code == "reason.divergence.corpus-only" {
                findings.push(json!({
                    "code": f.code,
                    "finding_iri": f.finding_iri.clone().unwrap_or_default(),
                    "anchor_iri": f.anchor_iri.clone().unwrap_or_default(),
                    "antecedents": f.antecedents.clone(),
                    "message": f.message,
                }));
            }
        }
    }

    if disagreements == 0 {
        eprintln!(
            "✓ golden gate: all {} native mini-corpus case(s) agree with their committed golden \
             (native-vs-golden count equality; no oracle run).",
            cases.len()
        );
        return Ok(());
    }

    findings.sort_by(|a, b| a["finding_iri"].as_str().cmp(&b["finding_iri"].as_str()));
    let report = json!({
        "schema": "gmeow.bench-engines.golden-gate/1",
        "disagreement_count": disagreements,
        "findings": findings,
    });
    let json = serde_json::to_string_pretty(&report).map_err(|e| {
        Diag::of_kind(Serialize {
            detail: e.to_string(),
        })
    })?;
    println!("{json}");
    Err(Diag::of_kind(RunFailed {
        detail: format!(
            "{disagreements} golden-gate disagreement(s): the native engine's result diverged from \
             the committed golden on the mini bench corpora — the native core changed a result set, \
             or a committed golden is stale; review before landing"
        ),
    }))
}

/// Wrap a case-scoped failure as a typed `RunFailed` diagnostic (hard-fail, never a
/// silent skip: a listed engine that cannot be driven is an error).
fn run_err(case: &BenchCase, detail: String) -> Diag {
    Diag::of_kind(RunFailed {
        detail: format!("{}/{}: {detail}", case.corpus, case.name),
    })
}

/// A deterministic count-valued verdict token.
fn count_token(n: u64) -> String {
    format!("derived={n}")
}

/// Build one divergence comparison. `kind` distinguishes the two comparison lanes
/// (`native-vs-golden` / `native-vs-oracle`) so the ledger's structural focus keys
/// never collide. Never yields the `"incomplete"` token, so a comparison is always
/// classified Agree / CorpusOnly (never mis-read as a native coverage gap).
fn comp(
    case: &BenchCase,
    world: &str,
    kind: &str,
    native: &str,
    published: &str,
) -> ExternalComparison {
    ExternalComparison {
        case: format!("{}/{}::{kind}", case.corpus, case.name),
        world: world.to_owned(),
        native: native.to_owned(),
        published: published.to_owned(),
    }
}

/// A `blake3` fingerprint of a canonically-sorted forward row set. [`ForwardRows`] is
/// already in the engine-independent `(predicate, term-display of args)` order, so the
/// fingerprint is a pure function of the materialized set.
fn fingerprint_rows(rows: &ForwardRows) -> String {
    let mut hasher = blake3::Hasher::new();
    for row in &rows.rows {
        hasher.update(row.predicate.as_bytes());
        hasher.update(b"\x1f");
        for arg in &row.args {
            hasher.update(term_display(arg).as_bytes());
            hasher.update(b"\x1f");
        }
        hasher.update(b"\n");
    }
    hasher.finalize().to_hex().to_string()
}

/// A `blake3` fingerprint of a canonically-sorted answer set. [`AnswerSet`] bindings
/// are sorted (each binding is a `BTreeMap`, and the vector is canonicalized), so the
/// fingerprint is a pure function of the answer set.
fn fingerprint_answers(answers: &AnswerSet) -> String {
    let mut hasher = blake3::Hasher::new();
    for binding in &answers.bindings {
        for (var, val) in binding {
            hasher.update(var.as_bytes());
            hasher.update(b"=");
            hasher.update(val.as_bytes());
            hasher.update(b"\x1f");
        }
        hasher.update(b"\n");
    }
    hasher.finalize().to_hex().to_string()
}

/// The set of EDB predicate IRIs in a world-scoped N-Quads EDB (sorted for
/// determinism). Used to separate derived rows from EDB-echo rows on the oracle's
/// row set, which — unlike the native seams — carries no provenance flag.
fn edb_predicates(edb_nq: &str) -> gmeow_errors::Result<std::collections::BTreeSet<String>> {
    let store = WorldStore::new();
    store.load_nquads(edb_nq).map_err(|e| {
        Diag::of_kind(RunFailed {
            detail: format!("EDB predicate scan: {e}"),
        })
    })?;
    let mut preds = std::collections::BTreeSet::new();
    for world in store.worlds() {
        for quad in store.quads_for_pattern_in_world(&world, None, None, None) {
            if let Some(p) = quad.p.as_iri() {
                preds.insert(p.to_owned());
            }
        }
    }
    Ok(preds)
}

/// Project the sorted cost-vector tuples into a JSON array of `[rule, predicate,
/// stratum, count]` — integer-valued, in `CostKey` order, so the array is
/// byte-deterministic.
fn cost_tuples(tuples: &[(String, String, u32, u64)]) -> Value {
    Value::Array(
        tuples
            .iter()
            .map(|(rule, pred, stratum, count)| json!([rule, pred, stratum, count]))
            .collect(),
    )
}

/// The process peak resident-set size in KiB (Linux `VmHWM`), for the REPORT-ONLY
/// advisory table. Non-deterministic (allocator high-water, page reuse, background
/// threads) — read here and NEVER folded into the committed artifact.
fn peak_rss_kib() -> u64 {
    let status = match std::fs::read_to_string("/proc/self/status") {
        Ok(s) => s,
        Err(_) => return 0,
    };
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            for tok in rest.split_whitespace() {
                if let Ok(v) = tok.parse::<u64>() {
                    return v;
                }
            }
        }
    }
    0
}

/// Print the REPORT-ONLY advisory table to stderr, clearly labeled as non-gating.
/// Wall-clock and peak-RSS are non-deterministic and appear ONLY here.
fn print_advisory_table(rows: &[AdvisoryRow]) {
    eprintln!(
        "── bench-engines advisory table (REPORT-ONLY, non-gating) ──────────────────────────"
    );
    eprintln!(
        "   wall_ns and peak_rss_kib are NON-DETERMINISTIC (clock jitter, allocator high-water,"
    );
    eprintln!(
        "   page reuse). alloc_bytes / alloc_count carry a small irreducible transient (rayon /"
    );
    eprintln!(
        "   allocator scratch) even under the R1 single-thread pool, so they are ADVISORY-ONLY"
    );
    eprintln!(
        "   too — NEVER in the committed artifact (there, peak_live_bytes is the deterministic,"
    );
    eprintln!(
        "   gate-eligible allocation metric). Verdict-agreement is the deterministic column."
    );
    eprintln!(
        "{:<22} {:<12} {:<7} {:>13} {:>9} {:>12} {:>11}  agree",
        "corpus", "fragment", "engine", "wall_ns", "rss_kib", "alloc_bytes", "alloc_count"
    );
    for r in rows {
        eprintln!(
            "{:<22} {:<12} {:<7} {:>13} {:>9} {:>12} {:>11}  {}",
            r.corpus,
            r.fragment,
            r.engine,
            r.wall_ns,
            r.peak_rss_kib,
            r.alloc_bytes,
            r.alloc_count,
            r.agreement
        );
    }
    eprintln!(
        "────────────────────────────────────────────────────────────────────────────────────"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal per-case deterministic record with the fields the cost-regression
    /// check compares.
    fn rec(corpus: &str, case: &str, steps: u64) -> Value {
        json!({
            "corpus": corpus,
            "case": case,
            "world": "urn:gmeow:test:world",
            "native": {
                "consumed_steps": steps,
                "derived_count": 3,
                "peak_live_bytes": 100,
                "cost_vector": [],
            },
            "agreement": { "native_vs_golden": true, "native_vs_oracle": true },
        })
    }

    /// Write a minimal cost baseline artifact to a unique temp path.
    fn write_baseline(cases: Vec<Value>) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut p = std::env::temp_dir();
        p.push(format!(
            "gmeow-cost-baseline-test-{}-{nanos}.json",
            std::process::id()
        ));
        std::fs::write(
            &p,
            serde_json::to_string(&json!({ "cases": cases })).unwrap(),
        )
        .unwrap();
        p
    }

    #[test]
    fn cost_descriptor_is_stable_and_count_sensitive() {
        let a = rec("c", "x", 3);
        let b = rec("c", "x", 3);
        assert_eq!(
            cost_descriptor(&a),
            cost_descriptor(&b),
            "identical records must yield identical descriptors"
        );
        let changed = rec("c", "x", 4);
        assert_ne!(
            cost_descriptor(&a),
            cost_descriptor(&changed),
            "a changed consumed_steps count must change the descriptor"
        );
    }

    #[test]
    fn regression_check_passes_on_match() {
        let fresh = vec![rec("c", "x", 3)];
        let base = write_baseline(vec![rec("c", "x", 3)]);
        let out = run_cost_regression_check(&base, &fresh);
        std::fs::remove_file(&base).ok();
        assert!(
            out.is_ok(),
            "an identical fresh run must not regress: {out:?}"
        );
    }

    #[test]
    fn regression_check_hard_fails_on_count_divergence() {
        // Fresh run has consumed_steps=3; the committed baseline recorded 4 → a
        // deterministic-count divergence is a cost regression (hard fail).
        let fresh = vec![rec("c", "x", 3)];
        let base = write_baseline(vec![rec("c", "x", 4)]);
        let out = run_cost_regression_check(&base, &fresh);
        std::fs::remove_file(&base).ok();
        assert!(
            out.is_err(),
            "a diverged deterministic count must be a cost regression"
        );
    }

    #[test]
    fn regression_check_hard_fails_on_dropped_case() {
        // The baseline has a case the fresh run does not produce → divergence.
        let fresh = vec![rec("c", "x", 3)];
        let base = write_baseline(vec![rec("c", "x", 3), rec("c", "y", 5)]);
        let out = run_cost_regression_check(&base, &fresh);
        std::fs::remove_file(&base).ok();
        assert!(
            out.is_err(),
            "a case present in the baseline but absent from the fresh run must regress"
        );
    }
}
