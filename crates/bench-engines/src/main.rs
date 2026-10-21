// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `bench-engines` — the engine/reference benchmark harness.
//!
//! It drives every committed mini bench case (`conformance/logic/cases/bench/`, or an
//! explicit `--corpus-dir`) through the NATIVE engine and, per fragment, the
//! applicable live or captured reference, and emits two strictly-separated outputs.
//!
//! Two cheap native-only gate verbs run without constructing an external engine:
//! `--check-golden` (a single-run native-vs-published agreement gate) and `--soak N` (the N-run soak
//! window — the same deterministic check re-run N times, asserting gap-zero AND a
//! byte-identical finding-graph digest across every run; see [`run_soak`]).
//!
//! The two strictly-separated outputs are:
//!
//! * **(2a) a DETERMINISTIC structured artifact** (`--emit-cost <path>`, else stdout):
//!   per `(corpus, case, engine)` the sorted `CostVector` `(rule, predicate, stratum,
//!   count)` tuples, `consumed_steps`, the derived-fact / answer counts, the three native
//!   allocation scalars (`alloc_bytes`, `alloc_count`, `peak_live_bytes`), and the
//!   verdict-agreement booleans+tokens. Every value is an INTEGER or a stable fingerprint
//!   and every map is serialized with sorted keys (the native sorted free-form
//!   `Value`), so the bytes are a pure function of `(engine version, corpus)`. This is the
//!   drift-gate-eligible signal. It carries NO wall-clock and NO peak-RSS.
//!
//! * **(2b) a REPORT-ONLY advisory table** (always to stderr, clearly labeled
//!   "report-only, non-gating"): rows `(corpus, fragment, engine, wall_ns, peak_rss,
//!   verdict-agreement)`. Wall-clock and peak-RSS are NON-deterministic (clock jitter,
//!   allocator high-water, page reuse), so they live HERE and NEVER enter the committed
//!   artifact.
//!
//! The primary forward/backward allocation sample is a **warm-plan execution**: one
//! semantically-checked call primes the process cache outside the measured region, then the
//! recorded call executes through that immutable plan. Cold planning is not hidden; every
//! forward case separately records a paired cold/warm complete-materialization probe with plan
//! builds, planning units, closure/cost parity, and both allocation-win verdicts.
//!
//! # How each allocation scalar gates
//!
//! `main` pins the process-GLOBAL Rayon pool to a SINGLE thread once, adding the calling
//! thread as its only worker (`num_threads(1).use_current_thread()`) so every parallel
//! section the native engine issues executes inline on the measuring thread — good
//! measurement hygiene. Under that pool `consumed_steps`, the cost vector, `derived_count`,
//! every verdict-agreement token, AND `peak_live_bytes` are byte-identical across runs;
//! they gate by EXACT drift-match through the committed baseline (the `cost_descriptor`
//! divergence-ledger comparison).
//!
//! `peak_live_bytes` (the [`gmeow_cost_measure`] net-live high-water) is deterministic
//! because it nets each transient scratch allocation — freed within the region — back to
//! zero, so it enters the exact descriptor. The two TOTAL-allocation scalars (`alloc_bytes`
//! / `alloc_count`) are NOT byte-reproducible: they occupy discrete 14-allocation states
//! deep in the native core that survive a process-global total, the inline single-thread
//! pool, AND a fully rayon-free sequential engine. After flat-slot kernels cut small-query
//! allocation totals roughly in half, a 12-process soak exposed the same quantum across a
//! 42-allocation span on `ancestor-query`; that absolute floor now dominates a percentage
//! for small totals. So bytes gate through a 1% one-sided band, while counts use the greater
//! of 1% and the measured 42-allocation floor (see [`ALLOC_COUNT_JITTER_FLOOR`]), folded
//! through the SAME divergence ledger: a within-band
//! run is a non-blocking `Agree`, a breach a blocking `CorpusOnly` cost-regression finding.
//! They are therefore stripped from the exact `cost_descriptor` (an exact match would
//! flake) but remain in the artifact (2a) as committed, drift-gated integer columns — no
//! longer advisory. See `LOGIC-PERFORMANCE.md §Measurement doctrine`.
//!
//! # Verdict-agreement (deterministic set equality)
//!
//! For each `(case, world)` the harness compares, as deterministic set equality:
//!
//! * **native ↔ golden** — the native derived-fact / answer COUNT against the
//!   hand-derived golden's committed `rows` count (the golden carries only a count, so
//!   the token is `derived=<n>`);
//! * **native ↔ reference** — forward/existential cases use their hand-derived
//!   counts, while backward cases additionally compare the fully-sorted answer
//!   fingerprint to the captured SLD digest.
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
//! Cases run in-process with a fresh EDB per case; no subprocess or secondary
//! reasoning runtime is constructed.
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

use purrdf_lex::json::record::ToJson;
use purrdf_lex::json::{self, Object, Value};

use gmeow_conformance::bench_corpus::{
    BenchCase, Fragment, load_bench_corpora, load_bench_corpora_from,
};
use gmeow_conformance::divergence::{agreement_tally, emit_divergence_nq};
use gmeow_conformance::error::{Cli, Io, RunFailed, Serialize};

use gmeow_cost_measure::{CountingAllocator, measure};

use gmeow_errors::Diag;

use gmeow_logic::cost::{
    ForwardRows, IncrementalForwardSession, IncrementalGroundingCostSession, RepeatForwardSession,
    SignedForwardRow, run_native_forward, run_rule_parallel_evidence,
};
use gmeow_logic::dispatch::dispatch_query;
use gmeow_logic::materialize::{MaterializationLimits, materialize_existential_rules};
use gmeow_logic::provenance::{ASSERT_RULE_IRI, term_display};
use gmeow_logic::query_ir::{AnswerSet, Budget, parse_query_program};
use gmeow_logic::reason::{
    ExternalComparison, build_ledger, compare_external_corpus, divergence_findings,
};
use gmeow_logic::result::EngineId;
use gmeow_logic::seam::WorldFactSnapshot;
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
/// Version of the captured SLD answer-set goldens used by the backward lane.
const BACKWARD_REFERENCE: &str = "captured-sld-goldens/v1";

fn main() -> gmeow_errors::Result<()> {
    // Pool-quiesce: force the process-GLOBAL Rayon pool to a single thread — the calling
    // thread itself (`use_current_thread`) — BEFORE any engine call, so every parallel
    // section the native engine issues executes INLINE on the one measuring thread (no
    // cross-thread work-stealing handoff, no per-call `install()` wrapping the backward
    // path's `!Send` `WorldStore` could never satisfy). This keeps `peak_live_bytes` (a
    // per-thread net-live high-water) exact and confines all allocation to one thread;
    // the process-global `gmeow-cost-measure` totals then attribute solely to the
    // sequential measured region. This is a measurement tool, not a speed tool, so a
    // single-thread global pool is exactly right. `build_global` can be called only once
    // per process and hard-errors if the pool was already built; here it is the first
    // statement, and a failure is a hard fail (no-optionality).
    rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .use_current_thread()
        .build_global()
        .expect("bench-engines: single-thread global rayon pool must initialize");

    let mut corpus_dir: Option<PathBuf> = None;
    let mut emit_cost: Option<PathBuf> = None;
    let mut check_cost: Option<PathBuf> = None;
    let mut compare_baseline: Option<String> = None;
    let mut check_golden = false;
    let mut soak: Option<usize> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--check-golden" => {
                check_golden = true;
            }
            "--soak" => {
                let raw = args.next().ok_or_else(|| {
                    Diag::of_kind(Cli {
                        detail: "--soak requires a window size N (an integer >= 2)".to_string(),
                    })
                })?;
                let n: usize = raw.parse().map_err(|_| {
                    Diag::of_kind(Cli {
                        detail: format!(
                            "--soak window must be a non-negative integer, got `{raw}`"
                        ),
                    })
                })?;
                soak = Some(n);
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
            "--compare-baseline" => {
                compare_baseline = Some(args.next().ok_or_else(|| {
                    Diag::of_kind(Cli {
                        detail: "--compare-baseline requires a git ref value (e.g. origin/main)"
                            .to_string(),
                    })
                })?);
            }
            other => {
                return Err(Diag::of_kind(Cli {
                    detail: format!("unknown argument: {other}"),
                }));
            }
        }
    }

    // `--compare-baseline <ref>` is a PURE artifact diff (no engine run, no corpus load):
    // it proves the shared-arrangement store's deterministic cost WIN by comparing the
    // REGENERATED working-tree `bench/cost-baseline.json` against a prior committed one at
    // `<ref>`. Returns before any corpus load / engine run.
    if let Some(git_ref) = &compare_baseline {
        return run_compare_baseline(git_ref);
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
    // (`expected/result.json`). It deliberately drives no external engine: golden
    // agreement is native-vs-published only, so the check stays cheap enough to wire into
    // `make check`. It returns before any full cost run / artifact emission below.
    if check_golden {
        return run_golden_gate(&cases);
    }

    // ── (1b) The SOAK-WINDOW gap-zero gate ───────────────────────────────────────
    // `--soak N` runs the SAME deterministic native-vs-golden agreement check N times
    // over the committed corpora and asserts, for EVERY run, gap-zero (`dl_gap == 0 &&
    // corpus_only == 0`) AND that the finding-graph blake3 digest is byte-identical across
    // all N runs (a drifting fingerprint is itself a divergence finding — reproducibility
    // is the soak invariant). Native-only, so it stays cheap enough to wire on-gate. It
    // returns before any full cost run / artifact emission below.
    if let Some(window) = soak {
        return run_soak(&cases, window);
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
        let ledger = build_ledger(Vec::new(), Vec::new(), rows);
        let findings = divergence_findings(&ledger);
        let tally = agreement_tally(corpus, comps);
        let graph = emit_divergence_nq(corpus, comps);
        let graph_digest =
            purrdf_hash::hex::encode(purrdf_hash::blake3::hash(graph.as_bytes()).as_bytes());

        // Surface each finding's ledger identity (finding_iri + code + anchor) so the
        // deterministic artifact demonstrates the join keys, sorted by IRI.
        let mut ids: Vec<Value> = findings
            .iter()
            .map(|f| finding_record(f).map(Value::from))
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        ids.sort_by(|a, b| a["finding_iri"].as_str().cmp(&b["finding_iri"].as_str()));

        corpus_ledgers.insert(
            corpus.clone(),
            Value::from(
                Object::new()
                    .with("cases", (tally.cases).to_json())
                    .with("agree", (tally.agree).to_json())
                    .with("corpus_only", (tally.corpus_only).to_json())
                    .with("dl_gap", (tally.dl_gap).to_json())
                    .with("finding_count", (findings.len()).to_json())
                    .with("finding_graph_blake3", (graph_digest).to_json())
                    .with("findings", (ids).to_json()),
            ),
        );
    }

    // A real four-worker run over the permanent balanced fixture. This is deliberately
    // outside every allocation-measured case: its gate is the scheduler-independent
    // rule-task work vector, exact merge-buffer row bound, full output/provenance parity,
    // and budget-sweep parity — never wall time on this shared host.
    let parallel = run_rule_parallel_evidence().map_err(|error| {
        Diag::of_kind(RunFailed {
            detail: format!("four-worker rule-parallel evidence failed: {error}"),
        })
    })?;
    let rule_parallelism = Value::from(
        Object::new()
            .with("fixture", ("balanced-six-rule-v1").to_json())
            .with("worker_count", (parallel.worker_count).to_json())
            .with("rule_count", (parallel.rule_count).to_json())
            .with("seed_rows", (parallel.seed_rows).to_json())
            .with("derived_rows", (parallel.derived_rows).to_json())
            .with("consumed_steps", (parallel.consumed_steps).to_json())
            .with("parallel_rounds", (parallel.parallel_rounds).to_json())
            .with("rule_tasks", (parallel.rule_tasks).to_json())
            .with(
                "serial_candidate_rows",
                (parallel.serial_candidate_rows).to_json(),
            )
            .with(
                "critical_path_candidate_rows",
                (parallel.critical_path_candidate_rows).to_json(),
            )
            .with(
                "critical_path_rows_saved",
                (parallel.serial_candidate_rows - parallel.critical_path_candidate_rows).to_json(),
            )
            .with(
                "max_buffered_candidate_rows",
                (parallel.max_buffered_candidate_rows).to_json(),
            )
            .with(
                "max_task_candidate_rows",
                (parallel.max_task_candidate_rows).to_json(),
            )
            .with("budget_cases", (parallel.budget_cases).to_json())
            .with("output_parity", (parallel.output_parity).to_json())
            .with("budget_parity", (parallel.budget_parity).to_json())
            .with(
                "parallel_path_entered",
                (parallel.parallel_path_entered).to_json(),
            )
            .with(
                "critical_path_strictly_lower",
                (parallel.critical_path_strictly_lower).to_json(),
            )
            .with(
                "closure_blake3",
                (purrdf_hash::hex::Digest32::new(parallel.closure_hash).to_hex()).to_json(),
            ),
    );

    // ── Cost-regression check (L3): compare THIS fresh run's deterministic cost +
    //    verdict-agreement against the committed baseline; ANY divergence is a
    //    cost-regression gmeow:Finding routed through the SHARED divergence ledger
    //    (content-addressed identity), and hard-fails the run. This is the richer
    //    honesty surface behind the primary on-gate strict-sync cost-ledger
    //    drift gate. Run BEFORE the artifact is assembled (it borrows the records). ──
    if let Some(baseline_path) = &check_cost {
        run_cost_regression_check(baseline_path, &case_records)?;
        run_parallelism_regression_check(baseline_path, &rule_parallelism)?;
    }

    // ── (2a) The DETERMINISTIC structured artifact ──────────────────────────────
    let artifact = Value::from(
        Object::new()
            .with("schema", ("gmeow.bench-engines.cost/1").to_json())
            .with(
                "engine_pins",
                Value::from(
                    Object::new()
                        .with("native", (EngineId::native().version).to_json())
                        .with("backward_reference", (BACKWARD_REFERENCE).to_json()),
                ),
            )
            .with("case_count", (case_records.len()).to_json())
            .with("cases", (case_records).to_json())
            .with("rule_parallelism", (rule_parallelism).to_json())
            .with("ledgers", (corpus_ledgers).to_json()),
    );
    let json = pretty_json(&artifact) + "\n";

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

/// One report-only advisory row (stderr only, non-gating). It carries ONLY the
/// NON-deterministic wall-clock and peak-RSS — the allocation scalars are no longer
/// advisory (they gate through the committed artifact: `peak_live_bytes` by exact
/// drift-match, the total-allocation scalars through the one-sided tolerance band).
struct AdvisoryRow {
    corpus: String,
    fragment: &'static str,
    engine: &'static str,
    wall_ns: u128,
    peak_rss_kib: u64,
    agreement: bool,
}

/// Drive one case through the native engine and the applicable native scratch or
/// committed reference. `main` pinned the
/// global Rayon pool to the single calling thread, so all engine allocation lands on the
/// measuring thread: `consumed_steps` / cost-vector / `peak_live_bytes` are exactly
/// reproducible, and the total-allocation scalars are captured for the tolerance-band gate.
fn run_case(case: &BenchCase) -> gmeow_errors::Result<CaseOutcome> {
    match case.fragment {
        Fragment::Forward => run_forward(case),
        Fragment::Existential => run_existential(case),
        Fragment::Backward => run_backward(case),
        Fragment::Incremental => run_incremental(case),
        Fragment::IncrementalGrounding => run_incremental_grounding(case),
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

/// FORWARD fragment: native `run_native_forward` with the allocation sample
/// plugged into its cost vector and checked against the hand-derived golden.
fn run_forward(case: &BenchCase) -> gmeow_errors::Result<CaseOutcome> {
    let (world, golden_rows) = sole_world(case)?;
    let program = case
        .canonical_program()
        .map_err(|e| run_err(case, format!("canonical program parse failed: {e}")))?;

    let edb = purrdf::parse_dataset(case.edb_nq.as_bytes(), "application/n-quads", None)
        .map_err(|e| run_err(case, format!("EDB parse error: {e}")))?;

    // Deterministic repeat-evaluation evidence for compile-don't-interpret. Parsing,
    // EDB loading, and stratum certification are outside both measured regions. Each
    // region performs a complete materialization; only the physical plan may persist.
    let plan_contract = format!("bench-repeat-plan-v1:{}/{}", case.corpus, case.name);
    let mut repeat = RepeatForwardSession::prepare(edb.as_ref(), &program, plan_contract)
        .map_err(|e| run_err(case, format!("repeat-forward prepare failed: {e}")))?;
    let (cold_res, cold_sample) = measure(|| repeat.evaluate());
    let cold = cold_res.map_err(|e| run_err(case, format!("cold plan evaluation failed: {e}")))?;
    let (warm_res, warm_sample) = measure(|| repeat.evaluate());
    let warm = warm_res.map_err(|e| run_err(case, format!("warm plan evaluation failed: {e}")))?;
    let (record_res, record_sample) = measure(|| repeat.evaluate_record_provenance());
    let record_probe =
        record_res.map_err(|e| run_err(case, format!("Record evaluation failed: {e}")))?;
    let (skip_res, skip_sample) = measure(|| repeat.evaluate_skip());
    let skip = skip_res.map_err(|e| run_err(case, format!("Skip evaluation failed: {e}")))?;
    let repeat_parity = cold.closure_hash == warm.closure_hash
        && cold.consumed_steps == warm.consumed_steps
        && cold.cost == warm.cost
        && cold.rule_hash == warm.rule_hash
        && cold.solver_version == warm.solver_version;
    let plan_reused = !cold.cache_hit
        && cold.plan_builds == 1
        && cold.planning_units > 0
        && warm.cache_hit
        && warm.plan_builds == 0
        && warm.planning_units == 0
        && warm.same_executable_as_first;
    let alloc_count_win = warm_sample.count < cold_sample.count;
    let peak_live_win = warm_sample.peak_live < cold_sample.peak_live;
    let provenance_closure_parity = record_probe.fact_closure_hash == skip.fact_closure_hash;
    let provenance_step_parity = record_probe.consumed_steps == skip.consumed_steps;
    let annotation_complete = record_probe.annotation_count == skip.fact_count;
    if !(repeat_parity
        && plan_reused
        && alloc_count_win
        && peak_live_win
        && provenance_closure_parity
        && provenance_step_parity
        && annotation_complete)
    {
        return Err(run_err(
            case,
            format!(
                "repeat-forward contract failed: parity={repeat_parity} plan_reused={plan_reused} \
                 cold_alloc_count={} warm_alloc_count={} cold_peak_live={} warm_peak_live={} \
                 provenance_closure_parity={provenance_closure_parity} \
                 provenance_step_parity={provenance_step_parity} \
                 annotation_complete={annotation_complete}",
                cold_sample.count, warm_sample.count, cold_sample.peak_live, warm_sample.peak_live
            ),
        ));
    }
    let record_peak_overhead_bytes =
        i128::from(record_sample.peak_live) - i128::from(skip_sample.peak_live);
    let record_alloc_count_overhead =
        i128::from(record_sample.count) - i128::from(skip_sample.count);

    // Prime the production process-wide plan cache outside the primary allocation sample.
    // Cold planning remains measured explicitly by the paired local repeat probe above;
    // this sample is the steady-state execution cost, not a cold-start/cache mixture.
    run_native_forward(edb.as_ref(), &program)
        .map_err(|e| run_err(case, format!("native forward plan prime failed: {e}")))?;

    // Native (measured). The global Rayon pool is the single calling thread (set in
    // `main`), so the engine's parallel work runs inline on the measuring thread; the
    // process-global totals and per-thread peak-live capture it completely — pool-quiesce.
    let native_start = Instant::now();
    let (native_res, sample) = measure(|| run_native_forward(edb.as_ref(), &program));
    let native_wall = native_start.elapsed().as_nanos();
    let mut native =
        native_res.map_err(|e| run_err(case, format!("native forward failed: {e}")))?;
    native
        .cost
        .set_allocation(sample.bytes, sample.count, sample.peak_live);
    let native_derived = native.cost.total_derivations();
    let native_fp = fingerprint_rows(&native.rows);

    // Native closure against the hand-derived golden count.
    let native_golden_tok = count_token(native_derived);
    let golden_tok = count_token(golden_rows);
    let agree_golden = native_golden_tok == golden_tok;

    let comparisons = vec![comp(
        case,
        &world,
        "native-vs-golden",
        &native_golden_tok,
        &golden_tok,
    )];

    let record = Value::from(
        Object::new()
            .with("corpus", (case.corpus).to_json())
            .with("case", (case.name).to_json())
            .with("fragment", ("forward").to_json())
            .with("world", (world).to_json())
            .with("golden_rows", (golden_rows).to_json())
            .with(
                "native",
                Value::from(
                    Object::new()
                        .with("engine", (native.engine.version).to_json())
                        .with("consumed_steps", (native.consumed_steps).to_json())
                        .with("derived_count", (native_derived).to_json())
                        .with("alloc_bytes", (native.cost.alloc_bytes()).to_json())
                        .with("alloc_count", (native.cost.alloc_count()).to_json())
                        .with("peak_live_bytes", (native.cost.peak_live_bytes()).to_json())
                        .with("rows_fingerprint", (native_fp).to_json())
                        .with(
                            "cost_vector",
                            (cost_tuples(&native.cost.to_sorted_tuples())).to_json(),
                        )
                        .with(
                            "plan_cache",
                            Value::from(
                                Object::new()
                                    .with("solver_version", (cold.solver_version).to_json())
                                    .with(
                                        "rule_hash",
                                        (purrdf_hash::hex::Digest32::new(cold.rule_hash).to_hex())
                                            .to_json(),
                                    )
                                    .with(
                                        "cold",
                                        Value::from(
                                            Object::new()
                                                .with("cache_hit", (cold.cache_hit).to_json())
                                                .with("plan_builds", (cold.plan_builds).to_json())
                                                .with(
                                                    "planning_units",
                                                    (cold.planning_units).to_json(),
                                                )
                                                .with(
                                                    "consumed_steps",
                                                    (cold.consumed_steps).to_json(),
                                                )
                                                .with(
                                                    "peak_live_bytes",
                                                    (cold_sample.peak_live).to_json(),
                                                )
                                                .with(
                                                    "closure_blake3",
                                                    (purrdf_hash::hex::Digest32::new(
                                                        cold.closure_hash,
                                                    )
                                                    .to_hex())
                                                    .to_json(),
                                                )
                                                .with(
                                                    "cost_vector",
                                                    (cost_tuples(&cold.cost.to_sorted_tuples()))
                                                        .to_json(),
                                                ),
                                        ),
                                    )
                                    .with(
                                        "warm",
                                        Value::from(
                                            Object::new()
                                                .with("cache_hit", (warm.cache_hit).to_json())
                                                .with("plan_builds", (warm.plan_builds).to_json())
                                                .with(
                                                    "planning_units",
                                                    (warm.planning_units).to_json(),
                                                )
                                                .with(
                                                    "consumed_steps",
                                                    (warm.consumed_steps).to_json(),
                                                )
                                                .with(
                                                    "peak_live_bytes",
                                                    (warm_sample.peak_live).to_json(),
                                                )
                                                .with(
                                                    "closure_blake3",
                                                    (purrdf_hash::hex::Digest32::new(
                                                        warm.closure_hash,
                                                    )
                                                    .to_hex())
                                                    .to_json(),
                                                )
                                                .with(
                                                    "cost_vector",
                                                    (cost_tuples(&warm.cost.to_sorted_tuples()))
                                                        .to_json(),
                                                ),
                                        ),
                                    )
                                    .with(
                                        "same_executable",
                                        (warm.same_executable_as_first).to_json(),
                                    )
                                    .with("repeat_parity", (repeat_parity).to_json())
                                    .with(
                                        "warm_alloc_count_strictly_lower",
                                        (alloc_count_win).to_json(),
                                    )
                                    .with(
                                        "warm_peak_live_strictly_lower",
                                        (peak_live_win).to_json(),
                                    ),
                            ),
                        )
                        .with(
                            "provenance",
                            Value::from(
                                Object::new()
                                    .with(
                                        "record",
                                        Value::from(
                                            Object::new()
                                                .with(
                                                    "annotation_count",
                                                    (record_probe.annotation_count).to_json(),
                                                )
                                                .with(
                                                    "max_proof_height",
                                                    (record_probe.max_proof_height).to_json(),
                                                )
                                                .with(
                                                    "consumed_steps",
                                                    (record_probe.consumed_steps).to_json(),
                                                )
                                                .with(
                                                    "fact_count",
                                                    (record_probe.annotation_count).to_json(),
                                                )
                                                .with(
                                                    "fact_closure_blake3",
                                                    (purrdf_hash::hex::Digest32::new(
                                                        record_probe.fact_closure_hash,
                                                    )
                                                    .to_hex())
                                                    .to_json(),
                                                )
                                                .with(
                                                    "alloc_count",
                                                    (record_sample.count).to_json(),
                                                )
                                                .with(
                                                    "peak_live_bytes",
                                                    (record_sample.peak_live).to_json(),
                                                ),
                                        ),
                                    )
                                    .with(
                                        "skip",
                                        Value::from(
                                            Object::new()
                                                .with("annotation_count", (0).to_json())
                                                .with(
                                                    "consumed_steps",
                                                    (skip.consumed_steps).to_json(),
                                                )
                                                .with("fact_count", (skip.fact_count).to_json())
                                                .with(
                                                    "fact_closure_blake3",
                                                    (purrdf_hash::hex::Digest32::new(
                                                        skip.fact_closure_hash,
                                                    )
                                                    .to_hex())
                                                    .to_json(),
                                                )
                                                .with("alloc_count", (skip_sample.count).to_json())
                                                .with(
                                                    "peak_live_bytes",
                                                    (skip_sample.peak_live).to_json(),
                                                ),
                                        ),
                                    )
                                    .with("closure_parity", (provenance_closure_parity).to_json())
                                    .with("step_parity", (provenance_step_parity).to_json())
                                    .with("annotation_complete", (annotation_complete).to_json())
                                    .with(
                                        "record_peak_overhead_bytes",
                                        (record_peak_overhead_bytes).to_json(),
                                    )
                                    .with(
                                        "record_alloc_count_overhead",
                                        (record_alloc_count_overhead).to_json(),
                                    ),
                            ),
                        ),
                ),
            )
            .with(
                "agreement",
                Value::from(
                    Object::new()
                        .with("native_vs_golden", (agree_golden).to_json())
                        .with("native_golden_token", (native_golden_tok).to_json())
                        .with("golden_token", (golden_tok).to_json()),
                ),
            ),
    );

    let peak = peak_rss_kib();
    Ok(CaseOutcome {
        record,
        advisory: vec![AdvisoryRow {
            corpus: case.corpus.clone(),
            fragment: "forward",
            engine: "native",
            wall_ns: native_wall,
            peak_rss_kib: peak,
            agreement: agree_golden,
        }],
        comparisons,
    })
}

/// INCREMENTAL fragment: prepare one fixed-rule session from `input.nq` outside
/// measurement, apply `delta.nq` as a signed insertion, compare the full closure
/// against a clean native rebuild, then retract the same batch and compare with the
/// original base closure. The scratch native path is the semantic reference; no live
/// secondary engine is constructed.
fn run_incremental(case: &BenchCase) -> gmeow_errors::Result<CaseOutcome> {
    let (world, golden_rows) = sole_world(case)?;
    let program = case
        .canonical_program()
        .map_err(|e| run_err(case, format!("canonical program parse failed: {e}")))?;
    let base = purrdf::parse_dataset(case.edb_nq.as_bytes(), "application/n-quads", None)
        .map_err(|e| run_err(case, format!("base EDB parse error: {e}")))?;
    let delta = purrdf::parse_dataset(case.delta_nq.as_bytes(), "application/n-quads", None)
        .map_err(|e| run_err(case, format!("delta EDB parse error: {e}")))?;
    let updated = purrdf::RdfDataset::union(&[base.as_ref(), delta.as_ref()])?;

    // Bootstrap and base scratch evaluation are intentionally outside the measured
    // transaction. The optimized consumer is a loop over one stable base session.
    let mut session = IncrementalForwardSession::prepare(
        base.as_ref(),
        &program,
        &gmeow_logic::annotation::AnnotationContract::exact(),
    )
    .map_err(|e| run_err(case, format!("incremental prepare failed: {e}")))?;
    let base_scratch = run_native_forward(base.as_ref(), &program)
        .map_err(|e| run_err(case, format!("base scratch rebuild failed: {e}")))?;
    let base_fp = fingerprint_rows(&base_scratch.rows);

    let incremental_start = Instant::now();
    let (incremental_res, incremental_sample) = measure(|| session.insert(delta.as_ref(), None));
    let incremental_wall = incremental_start.elapsed().as_nanos();
    let mut incremental =
        incremental_res.map_err(|e| run_err(case, format!("incremental insertion failed: {e}")))?;
    incremental.cost.set_allocation(
        incremental_sample.bytes,
        incremental_sample.count,
        incremental_sample.peak_live,
    );
    let incremental_fp = fingerprint_rows(&incremental.rows);
    let incremental_changes_fp = fingerprint_signed_rows(&incremental.changes);

    let scratch_start = Instant::now();
    let (scratch_res, scratch_sample) = measure(|| run_native_forward(&updated, &program));
    let scratch_wall = scratch_start.elapsed().as_nanos();
    let mut scratch =
        scratch_res.map_err(|e| run_err(case, format!("updated scratch rebuild failed: {e}")))?;
    scratch.cost.set_allocation(
        scratch_sample.bytes,
        scratch_sample.count,
        scratch_sample.peak_live,
    );
    let scratch_fp = fingerprint_rows(&scratch.rows);
    let scratch_derived = scratch.cost.total_derivations();

    // Retraction parity is part of every incremental corpus observation, not just a
    // unit test. This is unbounded by design: bounded deletion has no partial frontier.
    let retracted = session
        .retract(delta.as_ref())
        .map_err(|e| run_err(case, format!("incremental retraction failed: {e}")))?;
    let retracted_fp = fingerprint_rows(&retracted.rows);
    let retracted_changes_fp = fingerprint_signed_rows(&retracted.changes);

    let native_golden_tok = count_token(incremental.derived_count);
    let golden_tok = count_token(golden_rows);
    let agree_golden = native_golden_tok == golden_tok;
    let agree_insert = incremental_fp == scratch_fp;
    let agree_retract = retracted_fp == base_fp;
    let parity_token = if agree_insert && agree_retract {
        "insert-and-retract-match-scratch"
    } else {
        "incremental-scratch-parity-mismatch"
    };
    let step_win = incremental.consumed_steps < scratch.consumed_steps;
    let step_token = if step_win {
        "incremental-steps-strictly-lower".to_owned()
    } else {
        format!(
            "incremental-steps={} scratch-steps={}",
            incremental.consumed_steps, scratch.consumed_steps
        )
    };

    let comparisons = vec![
        comp(
            case,
            &world,
            "native-vs-golden",
            &native_golden_tok,
            &golden_tok,
        ),
        comp(
            case,
            &world,
            "incremental-insert-vs-scratch",
            &incremental_fp,
            &scratch_fp,
        ),
        comp(
            case,
            &world,
            "incremental-retract-vs-scratch",
            &retracted_fp,
            &base_fp,
        ),
        comp(
            case,
            &world,
            "incremental-step-win",
            &step_token,
            "incremental-steps-strictly-lower",
        ),
    ];

    let record = Value::from(
        Object::new()
            .with("corpus", (case.corpus).to_json())
            .with("case", (case.name).to_json())
            .with("fragment", ("incremental").to_json())
            .with("world", (world).to_json())
            .with("golden_rows", (golden_rows).to_json())
            .with(
                "native",
                Value::from(
                    Object::new()
                        .with("engine", (incremental.engine.version).to_json())
                        .with("consumed_steps", (incremental.consumed_steps).to_json())
                        .with("derived_count", (incremental.derived_count).to_json())
                        .with("joined_rows", (incremental.joined_rows).to_json())
                        .with("inner_iterations", (incremental.inner_iterations).to_json())
                        .with("signed_change_count", (incremental.changes.len()).to_json())
                        .with("signed_changes_blake3", (incremental_changes_fp).to_json())
                        .with("rows_fingerprint", (incremental_fp).to_json())
                        .with("alloc_bytes", (incremental.cost.alloc_bytes()).to_json())
                        .with("alloc_count", (incremental.cost.alloc_count()).to_json())
                        .with(
                            "peak_live_bytes",
                            (incremental.cost.peak_live_bytes()).to_json(),
                        )
                        .with(
                            "cost_vector",
                            (cost_tuples(&incremental.cost.to_sorted_tuples())).to_json(),
                        )
                        .with(
                            "retraction",
                            Value::from(
                                Object::new()
                                    .with("derived_count", (retracted.derived_count).to_json())
                                    .with("joined_rows", (retracted.joined_rows).to_json())
                                    .with(
                                        "inner_iterations",
                                        (retracted.inner_iterations).to_json(),
                                    )
                                    .with(
                                        "signed_change_count",
                                        (retracted.changes.len()).to_json(),
                                    )
                                    .with("signed_changes_blake3", (retracted_changes_fp).to_json())
                                    .with("rows_fingerprint", (retracted_fp).to_json()),
                            ),
                        )
                        .with(
                            "scratch",
                            Value::from(
                                Object::new()
                                    .with("engine", (scratch.engine.version).to_json())
                                    .with("consumed_steps", (scratch.consumed_steps).to_json())
                                    .with("derived_count", (scratch_derived).to_json())
                                    .with(
                                        "peak_live_bytes",
                                        (scratch.cost.peak_live_bytes()).to_json(),
                                    )
                                    .with("rows_fingerprint", (scratch_fp).to_json())
                                    .with(
                                        "cost_vector",
                                        (cost_tuples(&scratch.cost.to_sorted_tuples())).to_json(),
                                    ),
                            ),
                        ),
                ),
            )
            .with(
                "reference",
                Value::from(
                    Object::new()
                        .with("engine", ("native-scratch").to_json())
                        .with("derived_count", (scratch_derived).to_json())
                        .with("rows_fingerprint", (scratch_fp).to_json())
                        .with("base_rows_fingerprint", (base_fp).to_json()),
                ),
            )
            .with(
                "agreement",
                Value::from(
                    Object::new()
                        .with("native_vs_golden", (agree_golden).to_json())
                        .with(
                            "native_vs_reference",
                            (agree_insert && agree_retract).to_json(),
                        )
                        .with("incremental_insert_vs_scratch", (agree_insert).to_json())
                        .with("incremental_retract_vs_scratch", (agree_retract).to_json())
                        .with("incremental_step_win", (step_win).to_json())
                        .with("native_golden_token", (native_golden_tok).to_json())
                        .with("golden_token", (golden_tok).to_json())
                        .with("native_reference_descriptor", (parity_token).to_json())
                        .with(
                            "reference_descriptor",
                            ("insert-and-retract-match-scratch").to_json(),
                        )
                        .with("step_token", (step_token).to_json()),
                ),
            ),
    );

    let peak = peak_rss_kib();
    Ok(CaseOutcome {
        record,
        advisory: vec![
            AdvisoryRow {
                corpus: case.corpus.clone(),
                fragment: "incremental",
                engine: "native-incremental",
                wall_ns: incremental_wall,
                peak_rss_kib: peak,
                agreement: agree_golden && agree_insert && agree_retract && step_win,
            },
            AdvisoryRow {
                corpus: case.corpus.clone(),
                fragment: "incremental",
                engine: "native-scratch",
                wall_ns: scratch_wall,
                peak_rss_kib: peak,
                agreement: agree_insert,
            },
        ],
        comparisons,
    })
}

/// INCREMENTAL-GROUNDING fragment: maintain the exact ground WFS solver slice under
/// one insertion, compare it with a clean ground+solve rebuild, then retract the
/// same batch and prove recovery of the base result.  The solver itself reruns from
/// scratch on both changed shots and is reported as such; the measured optimization
/// is grounding only.
fn run_incremental_grounding(case: &BenchCase) -> gmeow_errors::Result<CaseOutcome> {
    let (world, golden_rows) = sole_world(case)?;
    let program = case
        .canonical_program()
        .map_err(|e| run_err(case, format!("canonical program parse failed: {e}")))?;
    let base = purrdf::parse_dataset(case.edb_nq.as_bytes(), "application/n-quads", None)
        .map_err(|e| run_err(case, format!("base EDB parse error: {e}")))?;
    let delta = purrdf::parse_dataset(case.delta_nq.as_bytes(), "application/n-quads", None)
        .map_err(|e| run_err(case, format!("delta EDB parse error: {e}")))?;
    let updated = purrdf::RdfDataset::union(&[base.as_ref(), delta.as_ref()])?;
    let updated_edb_count = updated.quads().count() as u64;

    let mut session = IncrementalGroundingCostSession::prepare(base.as_ref(), &program)
        .map_err(|e| run_err(case, format!("incremental grounding prepare failed: {e}")))?;
    let base_fp = session.current_rows_fingerprint();

    let incremental_start = Instant::now();
    let (incremental_res, incremental_sample) = measure(|| session.insert(delta.as_ref()));
    let incremental_wall = incremental_start.elapsed().as_nanos();
    let incremental = incremental_res
        .map_err(|e| run_err(case, format!("incremental grounding insertion failed: {e}")))?;
    session.check_grounding_scratch_parity().map_err(|e| {
        run_err(
            case,
            format!("maintained ground-program parity failed: {e}"),
        )
    })?;

    let scratch_start = Instant::now();
    let (scratch_res, scratch_sample) = measure(|| session.scratch_rebuild());
    let scratch_wall = scratch_start.elapsed().as_nanos();
    let scratch =
        scratch_res.map_err(|e| run_err(case, format!("grounding scratch rebuild failed: {e}")))?;

    let retracted = session.retract(delta.as_ref()).map_err(|e| {
        run_err(
            case,
            format!("incremental grounding retraction failed: {e}"),
        )
    })?;
    session
        .check_grounding_scratch_parity()
        .map_err(|e| run_err(case, format!("retracted ground-program parity failed: {e}")))?;

    let incremental_derived = incremental
        .row_count
        .checked_sub(updated_edb_count)
        .ok_or_else(|| {
            run_err(
                case,
                "incremental WFS row count is below updated EDB count".to_owned(),
            )
        })?;
    let scratch_derived = scratch
        .row_count
        .checked_sub(updated_edb_count)
        .ok_or_else(|| {
            run_err(
                case,
                "scratch WFS row count is below updated EDB count".to_owned(),
            )
        })?;
    let incremental_fp = purrdf_hash::hex::Digest32::new(incremental.rows_fingerprint).to_hex();
    let scratch_fp = purrdf_hash::hex::Digest32::new(scratch.rows_fingerprint).to_hex();
    let retracted_fp = purrdf_hash::hex::Digest32::new(retracted.rows_fingerprint).to_hex();
    let base_fp = purrdf_hash::hex::Digest32::new(base_fp).to_hex();

    let agree_golden = incremental_derived == golden_rows;
    let agree_insert = incremental_fp == scratch_fp && incremental_derived == scratch_derived;
    let agree_retract = retracted_fp == base_fp;
    let committed_groundings = incremental.ground_rule_changes as u64;
    let scratch_groundings = scratch.active_ground_rules as u64;
    let step_win = committed_groundings < scratch_groundings;
    let probe_win = incremental.ground_rule_probe_rows < scratch.ground_rule_probe_rows;
    if !changed_solver_shots_are_flagged(
        incremental.solver_reran,
        incremental.solver_status,
        retracted.solver_reran,
        retracted.solver_status,
    ) {
        return Err(run_err(
            case,
            "changed ground slices must explicitly report a flagged from-scratch solver run"
                .to_owned(),
        ));
    }

    let native_golden_tok = count_token(incremental_derived);
    let golden_tok = count_token(golden_rows);
    let parity_token = if agree_insert && agree_retract {
        "insert-and-retract-ground-slice-match-scratch"
    } else {
        "incremental-grounding-scratch-parity-mismatch"
    };
    let step_token = if step_win {
        "incremental-ground-commits-strictly-lower".to_owned()
    } else {
        format!(
            "incremental-ground-commits={committed_groundings} scratch-ground-commits={scratch_groundings}"
        )
    };
    let probe_token = if probe_win {
        "incremental-ground-probes-strictly-lower".to_owned()
    } else {
        format!(
            "incremental-ground-probes={} scratch-ground-probes={}",
            incremental.ground_rule_probe_rows, scratch.ground_rule_probe_rows
        )
    };

    let comparisons = vec![
        comp(
            case,
            &world,
            "native-vs-golden",
            &native_golden_tok,
            &golden_tok,
        ),
        comp(
            case,
            &world,
            "incremental-grounding-insert-vs-scratch",
            &incremental_fp,
            &scratch_fp,
        ),
        comp(
            case,
            &world,
            "incremental-grounding-retract-vs-scratch",
            &retracted_fp,
            &base_fp,
        ),
        comp(
            case,
            &world,
            "incremental-grounding-commit-win",
            &step_token,
            "incremental-ground-commits-strictly-lower",
        ),
        comp(
            case,
            &world,
            "incremental-grounding-probe-win",
            &probe_token,
            "incremental-ground-probes-strictly-lower",
        ),
    ];

    let record = Value::from(
        Object::new()
            .with("corpus", (case.corpus).to_json())
            .with("case", (case.name).to_json())
            .with("fragment", ("incremental-grounding").to_json())
            .with("world", (world).to_json())
            .with("golden_rows", (golden_rows).to_json())
            .with(
                "native",
                Value::from(
                    Object::new()
                        .with("engine", (EngineId::native().version).to_json())
                        .with("consumed_steps", (committed_groundings).to_json())
                        .with("derived_count", (incremental_derived).to_json())
                        .with("joined_rows", (incremental.joined_rows).to_json())
                        .with("alloc_bytes", (incremental_sample.bytes).to_json())
                        .with("alloc_count", (incremental_sample.count).to_json())
                        .with("peak_live_bytes", (incremental_sample.peak_live).to_json())
                        .with("cost_vector", (Value::Array(Vec::new())).to_json())
                        .with("rows_fingerprint", (incremental_fp).to_json())
                        .with(
                            "grounding",
                            Value::from(
                                Object::new()
                                    .with("edb_changes", (incremental.edb_changes).to_json())
                                    .with(
                                        "ground_rule_changes",
                                        (incremental.ground_rule_changes).to_json(),
                                    )
                                    .with(
                                        "universe_changes",
                                        (incremental.universe_changes).to_json(),
                                    )
                                    .with(
                                        "universe_joined_rows",
                                        (incremental.universe_joined_rows).to_json(),
                                    )
                                    .with(
                                        "ground_rule_joined_rows",
                                        (incremental.ground_rule_joined_rows).to_json(),
                                    )
                                    .with(
                                        "ground_rule_probe_rows",
                                        (incremental.ground_rule_probe_rows).to_json(),
                                    )
                                    .with(
                                        "active_ground_rules",
                                        (incremental.active_ground_rules).to_json(),
                                    )
                                    .with("solver", (incremental.solver).to_json())
                                    .with("solver_status", (incremental.solver_status).to_json())
                                    .with("solver_reran", (incremental.solver_reran).to_json()),
                            ),
                        )
                        .with(
                            "retraction",
                            Value::from(
                                Object::new()
                                    .with("rows_fingerprint", (retracted_fp).to_json())
                                    .with("edb_changes", (retracted.edb_changes).to_json())
                                    .with(
                                        "ground_rule_changes",
                                        (retracted.ground_rule_changes).to_json(),
                                    )
                                    .with(
                                        "universe_changes",
                                        (retracted.universe_changes).to_json(),
                                    )
                                    .with("joined_rows", (retracted.joined_rows).to_json())
                                    .with(
                                        "ground_rule_probe_rows",
                                        (retracted.ground_rule_probe_rows).to_json(),
                                    )
                                    .with("solver_status", (retracted.solver_status).to_json())
                                    .with("solver_reran", (retracted.solver_reran).to_json()),
                            ),
                        )
                        .with(
                            "scratch",
                            Value::from(
                                Object::new()
                                    .with("engine", (EngineId::native().version).to_json())
                                    .with("consumed_steps", (scratch_groundings).to_json())
                                    .with("derived_count", (scratch_derived).to_json())
                                    .with("peak_live_bytes", (scratch_sample.peak_live).to_json())
                                    .with("rows_fingerprint", (scratch_fp).to_json())
                                    .with("cost_vector", (Value::Array(Vec::new())).to_json())
                                    .with(
                                        "ground_rule_probe_rows",
                                        (scratch.ground_rule_probe_rows).to_json(),
                                    )
                                    .with(
                                        "active_ground_rules",
                                        (scratch.active_ground_rules).to_json(),
                                    ),
                            ),
                        ),
                ),
            )
            .with(
                "reference",
                Value::from(
                    Object::new()
                        .with("engine", ("native-grounding-scratch").to_json())
                        .with("derived_count", (scratch_derived).to_json())
                        .with("rows_fingerprint", (scratch_fp).to_json())
                        .with("base_rows_fingerprint", (base_fp).to_json()),
                ),
            )
            .with(
                "agreement",
                Value::from(
                    Object::new()
                        .with("native_vs_golden", (agree_golden).to_json())
                        .with(
                            "native_vs_reference",
                            (grounding_semantic_parity(agree_insert, agree_retract)).to_json(),
                        )
                        .with("incremental_insert_vs_scratch", (agree_insert).to_json())
                        .with("incremental_retract_vs_scratch", (agree_retract).to_json())
                        .with("incremental_step_win", (step_win).to_json())
                        .with("incremental_probe_win", (probe_win).to_json())
                        .with("native_golden_token", (native_golden_tok).to_json())
                        .with("golden_token", (golden_tok).to_json())
                        .with("native_reference_descriptor", (parity_token).to_json())
                        .with(
                            "reference_descriptor",
                            ("insert-and-retract-ground-slice-match-scratch").to_json(),
                        )
                        .with("step_token", (step_token).to_json())
                        .with("probe_token", (probe_token).to_json()),
                ),
            ),
    );

    let peak = peak_rss_kib();
    Ok(CaseOutcome {
        record,
        advisory: vec![
            AdvisoryRow {
                corpus: case.corpus.clone(),
                fragment: "incremental-grounding",
                engine: "native-incremental-grounding",
                wall_ns: incremental_wall,
                peak_rss_kib: peak,
                agreement: agree_golden && agree_insert && agree_retract && step_win && probe_win,
            },
            AdvisoryRow {
                corpus: case.corpus.clone(),
                fragment: "incremental-grounding",
                engine: "native-grounding-scratch",
                wall_ns: scratch_wall,
                peak_rss_kib: peak,
                agreement: agree_insert,
            },
        ],
        comparisons,
    })
}

/// Semantic reference agreement is answer parity only. Deterministic work wins are
/// recorded in their dedicated fields and must never redefine correctness.
fn grounding_semantic_parity(insert_matches: bool, retract_matches: bool) -> bool {
    insert_matches && retract_matches
}

/// Every changed non-monotone solver shot must disclose that solving reran from
/// scratch; insertion and retraction are symmetric parts of the evidence contract.
fn changed_solver_shots_are_flagged(
    insert_reran: bool,
    insert_status: &str,
    retract_reran: bool,
    retract_status: &str,
) -> bool {
    insert_reran
        && insert_status == "flagged-non-incremental"
        && retract_reran
        && retract_status == "flagged-non-incremental"
}

/// EXISTENTIAL fragment: native value-inventing chase against the hand-derived count.
fn run_existential(case: &BenchCase) -> gmeow_errors::Result<CaseOutcome> {
    let (world, golden_rows) = sole_world(case)?;
    let rules = case
        .existential_rules()
        .map_err(|error| run_err(case, format!("typed existential parse failed: {error}")))?;
    let edb = purrdf::parse_dataset(case.edb_nq.as_bytes(), "application/n-quads", None)
        .map_err(|error| run_err(case, format!("EDB parse error: {error}")))?;
    let started = Instant::now();
    let (native, sample) = measure(|| {
        materialize_existential_rules(edb.as_ref(), &rules, MaterializationLimits::default())
    });
    let wall_ns = started.elapsed().as_nanos();
    let native = native.map_err(|error| run_err(case, format!("native chase failed: {error}")))?;
    let native_derived = native
        .quads
        .iter()
        .filter(|quad| quad.rule_iri != ASSERT_RULE_IRI)
        .count() as u64;
    let native_token = count_token(native_derived);
    let golden_token = count_token(golden_rows);
    let agreement = native_token == golden_token;
    let comparisons = vec![comp(
        case,
        &world,
        "native-vs-golden",
        &native_token,
        &golden_token,
    )];
    let record = Value::from(
        Object::new()
            .with("corpus", (case.corpus).to_json())
            .with("case", (case.name).to_json())
            .with("fragment", ("existential").to_json())
            .with("world", (world).to_json())
            .with("golden_rows", (golden_rows).to_json())
            .with(
                "native",
                Value::from(
                    Object::new()
                        .with("engine", (EngineId::native().version).to_json())
                        .with("consumed_steps", (native.frontier.consumed_steps).to_json())
                        .with("derived_count", (native_derived).to_json())
                        .with("alloc_bytes", (sample.bytes).to_json())
                        .with("alloc_count", (sample.count).to_json())
                        .with("peak_live_bytes", (sample.peak_live).to_json())
                        .with("cost_vector", (Value::Array(Vec::new())).to_json()),
                ),
            )
            .with(
                "agreement",
                Value::from(
                    Object::new()
                        .with("native_vs_golden", (agreement).to_json())
                        .with("native_golden_token", (native_token).to_json())
                        .with("golden_token", (golden_token).to_json()),
                ),
            ),
    );
    Ok(CaseOutcome {
        record,
        advisory: vec![AdvisoryRow {
            corpus: case.corpus.clone(),
            fragment: "existential",
            engine: "native",
            wall_ns,
            peak_rss_kib: peak_rss_kib(),
            agreement,
        }],
        comparisons,
    })
}

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
    let published_fp = golden.digest.as_deref().ok_or_else(|| {
        Diag::of_kind(RunFailed {
            detail: format!(
                "{}/{}: backward golden must carry the captured answer-set digest",
                case.corpus, case.name
            ),
        })
    })?;

    let store = WorldStore::new()?;
    store
        .load_nquads(&case.edb_nq)
        .map_err(|e| run_err(case, format!("EDB load failed: {e}")))?;
    let program = parse_query_program(&case.rules)
        .map_err(|e| run_err(case, format!("query parse failed: {e}")))?;
    let foreign = WorldFactSnapshot::from_world(&store, world, HORN_PROFILE_IRI)
        .map_err(|e| run_err(case, format!("foreign snapshot failed: {e}")))?;
    let budget = Budget::default();
    // Prime the process-wide demand-plan cache outside the steady-state sample. The
    // answer is checked because a failed prime is a real engine failure, never ignored.
    dispatch_query(&foreign, world, &program, HORN_PROFILE_IRI, &budget)
        .map_err(|e| run_err(case, format!("native backward plan prime failed: {e}")))?;
    // Native backward (measured; global Rayon pool is the single calling thread — pool-quiesce).
    let native_start = Instant::now();
    let (native_res, sample) =
        measure(|| dispatch_query(&foreign, world, &program, HORN_PROFILE_IRI, &budget));
    let native_wall = native_start.elapsed().as_nanos();
    let native = native_res.map_err(|e| run_err(case, format!("native backward failed: {e}")))?;
    let native_count = native.bindings.len() as u64;
    let native_fp = fingerprint_answers(&native);
    let consumed_steps = native.frontier.consumed_steps;

    let native_golden_tok = count_token(native_count);
    let golden_tok = count_token(golden_rows);
    let agree_golden = native_golden_tok == golden_tok;
    let agree_published = native_fp == published_fp;

    let comparisons = vec![
        comp(
            case,
            world,
            "native-vs-golden",
            &native_golden_tok,
            &golden_tok,
        ),
        comp(
            case,
            world,
            "native-vs-captured-sld",
            &native_fp,
            published_fp,
        ),
    ];

    let record = Value::from(
        Object::new()
            .with("corpus", (case.corpus).to_json())
            .with("case", (case.name).to_json())
            .with("fragment", ("backward").to_json())
            .with("world", (world).to_json())
            .with("golden_rows", (golden_rows).to_json())
            .with(
                "native",
                Value::from(
                    Object::new()
                        .with("engine", (EngineId::native().version).to_json())
                        .with("consumed_steps", (consumed_steps).to_json())
                        .with("answer_count", (native_count).to_json())
                        .with("alloc_bytes", (sample.bytes).to_json())
                        .with("alloc_count", (sample.count).to_json())
                        .with("peak_live_bytes", (sample.peak_live).to_json())
                        .with("answers_fingerprint", (native_fp).to_json())
                        .with("cost_vector", (Value::Array(Vec::new())).to_json()),
                ),
            )
            .with(
                "reference",
                Value::from(
                    Object::new()
                        .with("engine", (BACKWARD_REFERENCE).to_json())
                        .with("answer_count", (golden_rows).to_json())
                        .with("answers_fingerprint", (published_fp).to_json()),
                ),
            )
            .with(
                "agreement",
                Value::from(
                    Object::new()
                        .with("native_vs_golden", (agree_golden).to_json())
                        .with("native_vs_reference", (agree_published).to_json())
                        .with("native_golden_token", (native_golden_tok).to_json())
                        .with("golden_token", (golden_tok).to_json())
                        .with("native_reference_descriptor", (native_fp).to_json())
                        .with("reference_descriptor", (published_fp).to_json()),
                ),
            ),
    );

    let peak = peak_rss_kib();
    Ok(CaseOutcome {
        record,
        advisory: vec![AdvisoryRow {
            corpus: case.corpus.clone(),
            fragment: "backward",
            engine: "native",
            wall_ns: native_wall,
            peak_rss_kib: peak,
            agreement: agree_golden && agree_published,
        }],
        comparisons,
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────────────

/// Compact and pretty wire boundaries share sorted, last-wins free-form objects.
fn compact_json(value: &Value) -> String {
    let mut value = value.clone();
    json::record::sorted_last_wins(&mut value);
    json::write_compact(&value)
}

/// Sort free-form names recursively and render two-space JSON without a trailing newline.
fn pretty_json(value: &Value) -> String {
    let mut value = value.clone();
    json::record::sorted_last_wins(&mut value);
    json::write_pretty(&value)
}

/// A report is a projection of an existing ledger witness, never a new identity.
fn finding_record(finding: &gmeow_errors::Finding) -> gmeow_errors::Result<Object> {
    let required = |name, value: &Option<String>| {
        value.as_deref().filter(|value| !value.trim().is_empty()).map(ToJson::to_json).ok_or_else(|| {
            Diag::of_kind(RunFailed {
                detail: format!("divergence finding {} has no nonempty {name}; expected a DiagLedger witness", finding.code),
            })
        })
    };
    Ok(Object::new()
        .with("code", finding.code.to_json())
        .with(
            "finding_iri",
            required("finding_iri", &finding.finding_iri)?,
        )
        .with("anchor_iri", required("anchor_iri", &finding.anchor_iri)?)
        .with("antecedents", finding.antecedents.to_json()))
}

/// Refuse malformed selected benchmark evidence through the existing run-failure diagnostic.
fn invalid_case(detail: impl Into<String>) -> Diag {
    Diag::of_kind(RunFailed {
        detail: detail.into(),
    })
}

/// Read a required identity string; missing, non-string and blank identities are errors.
fn nonempty_member<'a>(
    value: &'a Value,
    field: &str,
    context: &str,
) -> gmeow_errors::Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| invalid_case(format!("{context}: {field} must be a nonempty string")))
}

/// The actual five wire fragments have different result counts and identities.
fn case_key(rec: &Value) -> gmeow_errors::Result<(String, String)> {
    if rec.as_object().is_none() {
        return Err(invalid_case("cost case must be an object"));
    }
    let corpus = nonempty_member(rec, "corpus", "cost case")?;
    let case = nonempty_member(rec, "case", corpus)?;
    Ok((corpus.to_owned(), case.to_owned()))
}

/// Admit common counters and the selected fragment's actual output identities before comparison.
fn admit_case(rec: &Value) -> gmeow_errors::Result<(String, String)> {
    let key = case_key(rec)?;
    let context = format!("{}/{}", key.0, key.1);
    let fragment = nonempty_member(rec, "fragment", &context)?;
    if !matches!(
        fragment,
        "forward" | "existential" | "backward" | "incremental" | "incremental-grounding"
    ) {
        return Err(invalid_case(format!(
            "{context}: unknown fragment {fragment}"
        )));
    }
    nonempty_member(rec, "world", &context)?;
    if rec.get("golden_rows").and_then(Value::as_u64).is_none() {
        return Err(invalid_case(format!(
            "{context}: golden_rows must be an unsigned u64 integer"
        )));
    }
    if rec.get("agreement").and_then(Value::as_object).is_none() {
        return Err(invalid_case(format!(
            "{context}: agreement must be an object"
        )));
    }
    let native = rec
        .get("native")
        .filter(|value| value.as_object().is_some())
        .ok_or_else(|| invalid_case(format!("{context}: native must be an object")))?;
    nonempty_member(native, "engine", &context)?;
    for field in [
        "consumed_steps",
        "peak_live_bytes",
        if fragment == "backward" {
            "answer_count"
        } else {
            "derived_count"
        },
    ] {
        if native.get(field).and_then(Value::as_u64).is_none() {
            return Err(invalid_case(format!(
                "{context}: native.{field} must be an unsigned u64 integer"
            )));
        }
    }
    if native
        .get("cost_vector")
        .and_then(Value::as_array)
        .is_none()
    {
        return Err(invalid_case(format!(
            "{context}: native.cost_vector must be an array"
        )));
    }
    if fragment == "backward" {
        nonempty_member(native, "answers_fingerprint", &context)?;
    } else if matches!(
        fragment,
        "forward" | "incremental" | "incremental-grounding"
    ) {
        nonempty_member(native, "rows_fingerprint", &context)?;
    }
    if matches!(fragment, "incremental" | "incremental-grounding") {
        let retraction = native
            .get("retraction")
            .filter(|value| value.as_object().is_some())
            .ok_or_else(|| {
                invalid_case(format!("{context}: native.retraction must be an object"))
            })?;
        nonempty_member(
            retraction,
            "rows_fingerprint",
            &format!("{context}: native.retraction"),
        )?;
        if fragment == "incremental" {
            for (label, value) in [("native", native), ("native.retraction", retraction)] {
                nonempty_member(
                    value,
                    "signed_changes_blake3",
                    &format!("{context}: {label}"),
                )?;
                for field in ["signed_change_count", "derived_count"] {
                    if value.get(field).and_then(Value::as_u64).is_none() {
                        return Err(invalid_case(format!(
                            "{context}: {label}.{field} must be an unsigned u64 integer"
                        )));
                    }
                }
            }
        }
    }
    Ok(key)
}

/// Index already-selected cases with the shared admission boundary and refuse duplicate keys.
fn index_case_records<'a>(
    cases: &'a [Value],
    context: &str,
) -> gmeow_errors::Result<BTreeMap<(String, String), &'a Value>> {
    let mut out = BTreeMap::new();
    for case in cases {
        let key = admit_case(case)?;
        if out.insert(key.clone(), case).is_some() {
            return Err(invalid_case(format!(
                "{context}: duplicate cost case {}/{}",
                key.0, key.1
            )));
        }
    }
    Ok(out)
}

/// Require a document object and its cases array; absence is never an empty case collection.
fn document_cases<'a>(doc: &'a Value, context: &str) -> gmeow_errors::Result<&'a [Value]> {
    if doc.as_object().is_none() {
        return Err(invalid_case(format!(
            "{context}: expected an object with a cases array"
        )));
    }
    doc.get("cases")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| invalid_case(format!("{context}: cases must be an array")))
}

/// Evaluation/output identity excludes measurements that the improvement gate permits to fall.
fn evaluation_descriptor(rec: &Value) -> String {
    let mut fields = Object::new();
    for name in ["fragment", "world", "golden_rows", "reference", "agreement"] {
        if let Some(value) = rec.get(name) {
            fields.insert(name, value.clone());
        }
    }
    let mut native = Object::new();
    for name in [
        "engine",
        "consumed_steps",
        "derived_count",
        "answer_count",
        "cost_vector",
        "rows_fingerprint",
        "answers_fingerprint",
        "signed_change_count",
        "signed_changes_blake3",
    ] {
        if let Some(value) = rec["native"].get(name) {
            native.insert(name, value.clone());
        }
    }
    if let Some(retraction) = rec["native"].get("retraction") {
        let mut output = Object::new();
        for name in [
            "rows_fingerprint",
            "derived_count",
            "signed_change_count",
            "signed_changes_blake3",
        ] {
            if let Some(value) = retraction.get(name) {
                output.insert(name, value.clone());
            }
        }
        native.insert("retraction", Value::from(output));
    }
    fields.insert("native", Value::from(native));
    compact_json(&Value::from(fields))
}

/// Percentage component of the one-sided allocation regression band.
///
/// Allocation bytes use this 1% ceiling directly. Allocation counts use the greater of
/// this ceiling and [`ALLOC_COUNT_JITTER_FLOOR`]: flat-slot kernels reduced the small
/// backward cases enough that the allocator's existing absolute quantization became
/// larger than 1% even though its absolute span did not grow.
const ALLOC_BAND_NUM: u64 = 1;
/// Denominator of the allocation-band tolerance (see [`ALLOC_BAND_NUM`]).
const ALLOC_BAND_DEN: u64 = 100;
/// Absolute allocation-count jitter admitted in addition to the 1% relative band.
///
/// A 12-fresh-process soak of `ancestor-query` observed exactly 1793, 1821, and 1835
/// allocations: the established 14-allocation quantum across a maximum span of 42. This
/// remains one-sided: reductions always pass, while a 43-allocation increase at this
/// scale fails.
const ALLOC_COUNT_JITTER_FLOOR: u64 = 42;

/// A COMPLETE deterministic descriptor of one case's native cost + verdict-agreement
/// sub-records, EXCLUDING the two non-reproducible total-allocation scalars (`alloc_bytes`
/// / `alloc_count`), which gate through the separate one-sided [`alloc_band_comp`] band
/// (see [`ALLOC_BAND_NUM`]). `peak_live_bytes` — the deterministic net-live high-water —
/// STAYS in the descriptor. The native writer boundary recursively sorts object keys, so this is byte-stable; ANY change to a deterministic count / fingerprint /
/// verdict changes the descriptor, and thus surfaces as a cost-regression divergence.
fn cost_descriptor(rec: &Value) -> String {
    let mut native = rec.get("native").cloned().unwrap_or(Value::Null);
    // Strip the non-reproducible total-allocation scalars from the EXACT descriptor;
    // they gate via the tolerance band instead (an exact match on them would flake).
    if let Some(obj) = native.as_object_mut() {
        obj.remove("alloc_bytes");
        obj.remove("alloc_count");
        // Record-vs-Skip total allocation counts are the same advisory transient as
        // the top-level total. Keep them in the committed evidence table, but exclude
        // them (and their derived delta) from exact drift identity. The exact
        // per-mode peak-live values remain in the descriptor.
        if let Some(provenance) = obj.get_mut("provenance").and_then(Value::as_object_mut) {
            provenance.remove("record_alloc_count_overhead");
            for mode in ["record", "skip"] {
                if let Some(mode_obj) = provenance.get_mut(mode).and_then(Value::as_object_mut) {
                    mode_obj.remove("alloc_count");
                }
            }
        }
    }
    let agreement = rec.get("agreement").cloned().unwrap_or(Value::Null);
    // Both sub-objects are now integer/boolean/fingerprint-valued; a compact canonical
    // JSON of the pair is the comparable descriptor.
    format!(
        "native={} agreement={}",
        compact_json(&native),
        compact_json(&agreement),
    )
}

/// The native `(alloc_bytes, alloc_count)` total-allocation scalars of a per-case record.
/// Yields `None` if the record carries no `native` object or the object lacks the alloc
/// fields (a malformed / stale baseline), which the caller turns into a hard error —
/// never a silent `0`.
fn native_alloc(rec: &Value) -> Option<(u64, u64)> {
    let native = rec.get("native")?;
    Some((
        native.get("alloc_bytes")?.as_u64()?,
        native.get("alloc_count")?.as_u64()?,
    ))
}

/// Whether `fresh` is under the greater of the relative 1% ceiling and an optional
/// measured absolute jitter floor.
fn within_alloc_band(fresh: u64, baseline: u64, absolute_floor: u64) -> bool {
    fresh <= alloc_band_ceiling(baseline, absolute_floor)
}

/// The inclusive integer ceiling of the allocation band for `baseline` — the largest
/// `fresh` value that still passes [`within_alloc_band`].
fn alloc_band_ceiling(baseline: u64, absolute_floor: u64) -> u64 {
    let relative = ((baseline as u128) * ((ALLOC_BAND_DEN + ALLOC_BAND_NUM) as u128)
        / (ALLOC_BAND_DEN as u128)) as u64;
    relative.max(baseline.saturating_add(absolute_floor))
}

/// Build one allocation-band divergence comparison for the shared ledger: within-band ⇒
/// equal tokens (a non-blocking `Agree` corroboration), a breach ⇒ divergent tokens (a
/// blocking `CorpusOnly` cost-regression finding). `kind` distinguishes the bytes vs count
/// band lanes so their structural focus keys never collide with each other or with the
/// exact `::cost` comparison.
fn alloc_band_comp(
    corpus: &str,
    case: &str,
    world: &str,
    kind: &str,
    fresh: u64,
    baseline: u64,
    absolute_floor: u64,
) -> ExternalComparison {
    let (native, published) = if within_alloc_band(fresh, baseline, absolute_floor) {
        (
            "within-alloc-band".to_string(),
            "within-alloc-band".to_string(),
        )
    } else {
        (
            format!("alloc={fresh}"),
            format!(
                "alloc<=ceiling={}",
                alloc_band_ceiling(baseline, absolute_floor)
            ),
        )
    };
    ExternalComparison {
        case: format!("{corpus}/{case}::{kind}"),
        world: world.to_owned(),
        native,
        published,
    }
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
    let baseline: Value = json::record::read_document(&bytes).map_err(|e| {
        Diag::of_kind(Serialize {
            detail: format!("cost baseline {} parse: {e}", baseline_path.display()),
        })
    })?;
    let base_cases = document_cases(&baseline, "cost baseline")?;
    let fresh_idx = index_case_records(fresh, "fresh cost records")?;
    let base_idx = index_case_records(base_cases, "cost baseline")?;

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
            .ok_or_else(|| invalid_case(format!("{corpus}/{case}: union key has no record")))?;
        let world = nonempty_member(world, "world", &format!("{corpus}/{case}"))?.to_owned();
        let corpus_comps = comps_by_corpus.entry(corpus.clone()).or_default();
        corpus_comps.push(ExternalComparison {
            case: format!("{corpus}/{case}::cost"),
            world: world.clone(),
            native,
            published,
        });

        // Allocation-band gate (the one-sided `fresh ≤ baseline·(1+ε)` regression check
        // for the non-reproducible total-allocation scalars, stripped from the exact
        // descriptor above). Only compared when BOTH sides carry the case — a case
        // present on only one side is already a blocking `::cost` divergence, so a
        // missing-side alloc comparison would be redundant noise. A present record whose
        // `native` object lacks the alloc fields is a stale/malformed baseline: hard-fail
        // with a regenerate hint, never silently skip.
        if let (Some(fresh_rec), Some(base_rec)) = (fresh_rec, base_rec) {
            corpus_comps.push(ExternalComparison {
                case: format!("{corpus}/{case}::output-identity"),
                world: world.clone(),
                native: evaluation_descriptor(fresh_rec),
                published: evaluation_descriptor(base_rec),
            });
            let (fresh_bytes, fresh_count) = native_alloc(fresh_rec).ok_or_else(|| {
                Diag::of_kind(RunFailed {
                    detail: format!(
                        "fresh cost record {corpus}/{case} carries no native alloc_bytes/alloc_count"
                    ),
                })
            })?;
            let (base_bytes, base_count) = native_alloc(base_rec).ok_or_else(|| {
                Diag::of_kind(RunFailed {
                    detail: format!(
                        "cost baseline case {corpus}/{case} carries no native alloc_bytes/alloc_count \
                         — regenerate the baseline via `make maint-bench-cost-baseline`"
                    ),
                })
            })?;
            corpus_comps.push(alloc_band_comp(
                corpus,
                case,
                &world,
                "alloc-bytes-band",
                fresh_bytes,
                base_bytes,
                0,
            ));
            corpus_comps.push(alloc_band_comp(
                corpus,
                case,
                &world,
                "alloc-count-band",
                fresh_count,
                base_count,
                ALLOC_COUNT_JITTER_FLOOR,
            ));
        }
    }

    // Fold each corpus's comparisons through the shared divergence ledger: an equal
    // descriptor is `Agree` (a non-blocking corroboration finding), a divergent one is
    // `CorpusOnly` (a blocking cost-regression finding with content-addressed identity).
    let mut regressions = 0usize;
    let mut regression_findings: Vec<Value> = Vec::new();
    for (corpus, comps) in &comps_by_corpus {
        let rows = compare_external_corpus(corpus, comps);
        let ledger = build_ledger(Vec::new(), Vec::new(), rows);
        regressions += ledger.corpus_only;
        for f in divergence_findings(&ledger) {
            if f.code == "reason.divergence.corpus-only" {
                regression_findings.push(Value::from(
                    finding_record(&f)?.with("message", f.message.to_json()),
                ));
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
    let report = Value::from(
        Object::new()
            .with(
                "schema",
                ("gmeow.bench-engines.cost-regression/1").to_json(),
            )
            .with("regression_count", (regressions).to_json())
            .with("baseline", (baseline_path.display().to_string()).to_json())
            .with("findings", (regression_findings).to_json()),
    );
    let json = pretty_json(&report);
    println!("{json}");
    Err(Diag::of_kind(RunFailed {
        detail: format!(
            "{regressions} cost-regression finding(s): the fresh engine cost/agreement run diverged \
             from the committed baseline {} — regenerate + review before refreshing the baseline",
            baseline_path.display()
        ),
    }))
}

/// Exact drift gate for the dedicated multi-worker structural evidence record.
///
/// Kept separate from per-case allocation bands because it contains no allocation
/// sample: every field is a deterministic integer, boolean, or closure fingerprint.
/// Divergence still folds through the shared reasoning ledger, so a mismatch is a
/// content-addressed cost-regression finding rather than an ad hoc JSON diff.
fn run_parallelism_regression_check(
    baseline_path: &Path,
    fresh: &Value,
) -> gmeow_errors::Result<()> {
    let bytes = std::fs::read(baseline_path).map_err(|error| {
        Diag::of_kind(Io {
            detail: format!("reading cost baseline {}: {error}", baseline_path.display()),
        })
    })?;
    let baseline: Value = json::record::read_document(&bytes).map_err(|error| {
        Diag::of_kind(Serialize {
            detail: format!("cost baseline {} parse: {error}", baseline_path.display()),
        })
    })?;
    let published = baseline.get("rule_parallelism");
    let native_token = compact_json(fresh);
    let published_token = match published {
        Some(value) => compact_json(value),
        None => "absent-from-baseline".to_owned(),
    };
    let comparison = ExternalComparison {
        case: "relational-core-mini/rule-parallel-critical-path::cost".to_owned(),
        world: "https://example.org/parallel/world".to_owned(),
        native: native_token,
        published: published_token,
    };
    let rows = compare_external_corpus("relational-core-mini", &[comparison]);
    let ledger = build_ledger(Vec::new(), Vec::new(), rows);
    if ledger.corpus_only == 0 {
        eprintln!(
            "✓ rule-parallel cost-regression check: four-worker structural evidence matches {}.",
            baseline_path.display()
        );
        return Ok(());
    }

    let findings = divergence_findings(&ledger)
        .into_iter()
        .filter(|finding| finding.code == "reason.divergence.corpus-only")
        .map(|finding| {
            finding_record(&finding)
                .map(|record| Value::from(record.with("message", finding.message.to_json())))
        })
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    let report = Value::from(
        Object::new()
            .with(
                "schema",
                ("gmeow.bench-engines.rule-parallel-regression/1").to_json(),
            )
            .with("baseline", (baseline_path.display().to_string()).to_json())
            .with("findings", (findings).to_json()),
    );
    println!("{}", pretty_json(&report));
    Err(Diag::of_kind(RunFailed {
        detail: format!(
            "four-worker rule-parallel evidence diverged from the committed baseline {}",
            baseline_path.display()
        ),
    }))
}

// ── The committed cost-WIN gate: `--compare-baseline <ref>` ──────────────────────
//
// `--check-cost` gates by EXACT drift-match against a committed baseline, so it proves
// REPRODUCIBILITY, never IMPROVEMENT. This verb proves the DIRECTION of the change: it
// diffs the regenerated working-tree `bench/cost-baseline.json` against a prior committed
// one at a git ref, and hard-fails unless the shared-arrangement store's deterministic
// cost win holds. The case manifests below are COMMITTED constants, so the acceptance is
// a permanent, re-runnable gate — never a one-shot script fitted to results.

/// The cases whose `peak_live_bytes` MUST STRICTLY DROP against the prior baseline — the
/// **authoritative deterministic cost win**.  `peak_live_bytes` (the instantaneous
/// allocation high-water mark) is EXACT and reproducible run-to-run, so it is the metric
/// that gates: the shared-arrangement store sheds the resident `keys` HashSet, the two
/// eager `by_subject` / `by_object` posting maps, the per-round `RowArena` staging, and the
/// per-round `DenseBitset`, and on every relation large enough for those structures to
/// clear the fixed non-store floor the high-water mark falls.  These are the cases where
/// that drop is measurable at the committed mini-corpus scale.
///
/// (Total `alloc_bytes` is ADVISORY, never a gate — it is transient scratch that carries a
/// sub-ε non-deterministic run-to-run jitter, so gating on it would be gating on noise.
/// It is reported as corroboration only; see [`COST_WIN_CASES`].)
const COST_PEAK_WIN_CASES: &[(&str, &str)] = &[("relational-core-mini", "same-generation")];

/// The `relational-core-mini` forward-recursive cases whose churn-shedding `alloc_bytes`
/// drop is REPORTED as advisory corroboration of the win — the deterministic-cost analogues
/// of the `el_closure` / `materialize_core` benches.  `alloc_bytes` registers the per-round
/// arena / bitset / posting churn the store no longer allocates, but it is non-deterministic
/// transient scratch, so it never gates (the authoritative gate is `peak_live_bytes` strictly
/// dropping on [`COST_PEAK_WIN_CASES`]).  At this mini corpus scale `peak_live_bytes` on these
/// forward cases is dominated by fixed non-store overhead, so their high-water mark stays flat
/// — they are gated by NON-REGRESSION (below), and the peak drop is proven on the cases above.
const COST_WIN_CASES: &[(&str, &str)] = &[
    ("relational-core-mini", "transitive-closure"),
    ("relational-core-mini", "non-linear-transitive-closure"),
    ("relational-core-mini", "points-to"),
    ("relational-core-mini", "reachability"),
    ("relational-core-mini", "same-generation"),
    ("relational-core-mini", "scc"),
    ("relational-core-mini", "mutual-recursion"),
];

/// The small / tail-only cases that must NOT regress `peak_live_bytes` — the
/// allocation-light regime the shared-arrangement store must never tax (the
/// deterministic analogues of the `foundation_evaluate` small-relation guard).
const COST_NOREG_CASES: &[(&str, &str)] = &[
    ("chasebench-mini", "deep-linear"),
    ("chasebench-mini", "doctors-like"),
    ("chasebench-mini", "lubm-like"),
];

/// Admit both documents through the same case boundary as exact replay.
fn index_native_cases<'a>(
    doc: &'a Value,
    context: &str,
) -> gmeow_errors::Result<BTreeMap<(String, String), &'a Value>> {
    index_case_records(document_cases(doc, context)?, context)
}

/// Compare a REGENERATED cost baseline (`new`) against a prior committed one (`old`),
/// returning `(report_lines, violations)`.  The win/no-regression contract:
///   * each `peak_win` case's `peak_live_bytes` is STRICTLY lower — the AUTHORITATIVE
///     deterministic cost win.  `peak_live_bytes` is exact and reproducible run-to-run, so
///     a per-case strict-drop is a firm (never-flaking) verdict: on every relation large
///     enough to clear the fixed non-store floor, shedding the resident HashSet / posting
///     maps / per-round bitset lowers the high-water mark.
///   * every case present in BOTH baselines: `peak_live_bytes` NON-INCREASED (exact, no
///     flake), with evaluation counters/vectors and fragment-specific output identities
///     UNCHANGED as sorted compact tokens (exact byte-identical evaluation).
///   * the `win` corpus's `alloc_bytes` (aggregate + per-case) is REPORTED as advisory
///     corroboration ONLY — it is non-deterministic transient scratch (a sub-ε ~±14-alloc
///     run-to-run jitter), so it never contributes a violation; gating on it would gate on
///     noise.  The deterministic `peak_live_bytes` drop above carries the win.
///
/// Pure over the two JSON docs (the manifests are parameters), so the gate is
/// unit-testable and re-runnable — never a throwaway script.  A non-empty `violations`
/// vector is a HARD FAIL.
fn compare_baselines(
    old: &Value,
    new: &Value,
    peak_win: &[(&str, &str)],
    win: &[(&str, &str)],
    noreg: &[(&str, &str)],
) -> (Vec<String>, Vec<String>) {
    let (o, n) = match (
        index_native_cases(old, "prior baseline"),
        index_native_cases(new, "fresh baseline"),
    ) {
        (Ok(old), Ok(new)) => (old, new),
        (old, new) => {
            let violations = [old.err(), new.err()]
                .into_iter()
                .flatten()
                .map(|error| error.to_string())
                .collect();
            return (Vec::new(), violations);
        }
    };
    let u = |v: &Value, k: &str| v["native"].get(k).and_then(Value::as_u64);
    let mut report: Vec<String> = Vec::new();
    let mut viol: Vec<String> = Vec::new();

    // Determinism + peak-live non-regression over every case present in BOTH baselines.
    for (key, ov) in &o {
        let Some(nv) = n.get(key) else {
            viol.push(format!(
                "{}/{}: dropped from the regenerated baseline",
                key.0, key.1
            ));
            continue;
        };
        if evaluation_descriptor(ov) != evaluation_descriptor(nv) {
            viol.push(format!("{}/{}: evaluation/output identity changed (consumed_steps changed or result count/fingerprint, cost_vector or profile moved — not byte-identical)", key.0, key.1));
        }
        if let (Some(op), Some(np)) = (u(ov, "peak_live_bytes"), u(nv, "peak_live_bytes"))
            && np > op
        {
            viol.push(format!(
                "{}/{}: peak_live_bytes REGRESSED {op} -> {np}",
                key.0, key.1
            ));
        }
    }

    // PEAK-WIN cases: `peak_live_bytes` STRICTLY lower — the authoritative deterministic
    // win (exact, never flakes).  A flat or risen high-water mark on a named win case is a
    // HARD FAIL: the deterministic cost win the shared-arrangement store promises did not
    // materialize where it must.
    for &(corpus, case) in peak_win {
        let k = (corpus.to_owned(), case.to_owned());
        let (Some(ov), Some(nv)) = (o.get(&k), n.get(&k)) else {
            viol.push(format!(
                "PEAK-WIN {corpus}/{case}: absent from a baseline (cannot prove the win)"
            ));
            continue;
        };
        let (Some(op), Some(np)) = (u(ov, "peak_live_bytes"), u(nv, "peak_live_bytes")) else {
            viol.push(format!(
                "PEAK-WIN {corpus}/{case}: missing peak_live_bytes field"
            ));
            continue;
        };
        if np >= op {
            viol.push(format!(
                "PEAK-WIN {corpus}/{case}: peak_live_bytes did not strictly drop {op} -> {np}"
            ));
        }
        report.push(format!(
            "PEAK-WIN {corpus}/{case:<28} peak {op}->{np} ({:+})",
            i128::from(np) - i128::from(op)
        ));
    }

    // WIN cases: `alloc_bytes` reported as ADVISORY corroboration only — NOT a gate.  It is
    // non-deterministic transient scratch (a sub-ε run-to-run jitter), so it contributes no
    // violation; the deterministic `peak_live_bytes` drop above carries the win.
    let (mut win_old_sum, mut win_new_sum) = (0u128, 0u128);
    let mut complete_advisory = true;
    for &(corpus, case) in win {
        let k = (corpus.to_owned(), case.to_owned());
        let (Some(ov), Some(nv)) = (o.get(&k), n.get(&k)) else {
            complete_advisory = false;
            report.push(format!(
                "adv   {corpus}/{case}: alloc unavailable (case absent)"
            ));
            continue;
        };
        let (Some(oa), Some(na)) = (u(ov, "alloc_bytes"), u(nv, "alloc_bytes")) else {
            complete_advisory = false;
            report.push(format!(
                "adv   {corpus}/{case}: alloc unavailable (missing unsigned alloc_bytes)"
            ));
            continue;
        };
        win_old_sum += u128::from(oa);
        win_new_sum += u128::from(na);
        let peak = match (u(ov, "peak_live_bytes"), u(nv, "peak_live_bytes")) {
            (Some(op), Some(np)) => format!("peak {op}->{np}"),
            _ => "peak ?".to_string(),
        };
        report.push(format!(
            "adv   {corpus}/{case:<30} alloc {oa}->{na} ({:+})  {peak}",
            i128::from(na) - i128::from(oa)
        ));
    }
    if !win.is_empty() && complete_advisory {
        report.push(format!(
            "adv   aggregate alloc {win_old_sum}->{win_new_sum} ({:+}) [advisory, not gated]",
            win_new_sum as i128 - win_old_sum as i128
        ));
    }

    if !win.is_empty() && !complete_advisory {
        report.push("adv   aggregate alloc unavailable [advisory, not gated]".to_owned());
    }

    // NOREG cases: named explicitly so the small-relation guard is a documented part of
    // the manifest (peak non-regression is enforced by the global loop above).
    for &(corpus, case) in noreg {
        let k = (corpus.to_owned(), case.to_owned());
        if let (Some(ov), Some(nv)) = (o.get(&k), n.get(&k)) {
            if let (Some(op), Some(np)) = (u(ov, "peak_live_bytes"), u(nv, "peak_live_bytes")) {
                report.push(format!(
                    "NOREG {corpus}/{case:<30} peak {op}->{np} ({:+})",
                    i128::from(np) - i128::from(op)
                ));
            }
        } else {
            viol.push(format!(
                "NOREG {corpus}/{case}: absent from a baseline (cannot prove non-regression)"
            ));
        }
    }

    (report, viol)
}

/// Run the `--compare-baseline <ref>` gate: read the prior committed baseline from the git
/// ref (a local object read, never a network fetch), the regenerated one from the working
/// tree, compare, print the before→after table, and HARD-FAIL on any violation.
fn run_compare_baseline(git_ref: &str) -> gmeow_errors::Result<()> {
    let spec = format!("{git_ref}:bench/cost-baseline.json");
    let out = std::process::Command::new("git")
        .args(["show", &spec])
        .output()
        .map_err(|e| {
            Diag::of_kind(Io {
                detail: format!("running `git show {spec}`: {e}"),
            })
        })?;
    if !out.status.success() {
        return Err(Diag::of_kind(RunFailed {
            detail: format!(
                "`git show {spec}` failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        }));
    }
    let old: Value = json::record::read_document(&out.stdout).map_err(|e| {
        Diag::of_kind(Serialize {
            detail: format!("parsing prior baseline at {spec}: {e}"),
        })
    })?;
    let new_bytes = std::fs::read("bench/cost-baseline.json").map_err(|e| {
        Diag::of_kind(Io {
            detail: format!("reading working-tree bench/cost-baseline.json: {e}"),
        })
    })?;
    let new: Value = json::record::read_document(&new_bytes).map_err(|e| {
        Diag::of_kind(Serialize {
            detail: format!("parsing working-tree bench/cost-baseline.json: {e}"),
        })
    })?;

    let (report, violations) = compare_baselines(
        &old,
        &new,
        COST_PEAK_WIN_CASES,
        COST_WIN_CASES,
        COST_NOREG_CASES,
    );
    for line in &report {
        println!("{line}");
    }
    if violations.is_empty() {
        println!(
            "compare-baseline vs {git_ref}: PASS — peak_live_bytes STRICTLY DROPPED on all {} \
             deterministic peak-win cases (the authoritative win), non-increased on every \
             case, evaluation byte-identical; advisory allocation availability reported above.",
            COST_PEAK_WIN_CASES.len()
        );
        Ok(())
    } else {
        for v in &violations {
            eprintln!("VIOLATION: {v}");
        }
        Err(Diag::of_kind(RunFailed {
            detail: format!(
                "compare-baseline vs {git_ref}: {} violation(s) — the cost win / no-regression \
                 contract failed against the prior committed baseline",
                violations.len()
            ),
        }))
    }
}

/// Drive one case through the native engine and return its world, count comparison,
/// and optional full-result fingerprint comparison. Backward cases must compare the
/// native answer-set fingerprint with the captured SLD digest; other fragments retain
/// their mathematically sound count invariant. No external engine is constructed.
struct GoldenObservation {
    world: String,
    native_count: u64,
    golden_count: u64,
    fingerprint: Option<(&'static str, String, String)>,
}

fn native_golden_pair(case: &BenchCase) -> gmeow_errors::Result<GoldenObservation> {
    match case.fragment {
        Fragment::Forward => {
            let (world, golden_rows) = sole_world(case)?;
            let program = case
                .canonical_program()
                .map_err(|e| run_err(case, format!("canonical program parse failed: {e}")))?;
            let edb = purrdf::parse_dataset(case.edb_nq.as_bytes(), "application/n-quads", None)
                .map_err(|e| run_err(case, format!("EDB parse error: {e}")))?;
            let native = run_native_forward(edb.as_ref(), &program)
                .map_err(|e| run_err(case, format!("native forward failed: {e}")))?;
            Ok(GoldenObservation {
                world,
                native_count: native.cost.total_derivations(),
                golden_count: golden_rows,
                fingerprint: None,
            })
        }
        Fragment::Existential => {
            let (world, golden_rows) = sole_world(case)?;
            let rules = case
                .existential_rules()
                .map_err(|e| run_err(case, format!("typed existential parse failed: {e}")))?;
            let edb = purrdf::parse_dataset(case.edb_nq.as_bytes(), "application/n-quads", None)
                .map_err(|e| run_err(case, format!("EDB parse error: {e}")))?;
            let native = materialize_existential_rules(
                edb.as_ref(),
                &rules,
                MaterializationLimits::default(),
            )
            .map_err(|e| run_err(case, format!("native chase failed: {e}")))?;
            let native_derived = native
                .quads
                .iter()
                .filter(|q| q.rule_iri != ASSERT_RULE_IRI)
                .count() as u64;
            Ok(GoldenObservation {
                world,
                native_count: native_derived,
                golden_count: golden_rows,
                fingerprint: None,
            })
        }
        Fragment::Incremental => {
            let (world, golden_rows) = sole_world(case)?;
            let program = case
                .canonical_program()
                .map_err(|e| run_err(case, format!("canonical program parse failed: {e}")))?;
            let base = purrdf::parse_dataset(case.edb_nq.as_bytes(), "application/n-quads", None)
                .map_err(|e| run_err(case, format!("base EDB parse error: {e}")))?;
            let delta =
                purrdf::parse_dataset(case.delta_nq.as_bytes(), "application/n-quads", None)
                    .map_err(|e| run_err(case, format!("delta EDB parse error: {e}")))?;
            let updated = purrdf::RdfDataset::union(&[base.as_ref(), delta.as_ref()])?;
            let base_scratch = run_native_forward(base.as_ref(), &program)
                .map_err(|e| run_err(case, format!("base scratch rebuild failed: {e}")))?;
            let updated_scratch = run_native_forward(&updated, &program)
                .map_err(|e| run_err(case, format!("updated scratch rebuild failed: {e}")))?;
            let mut session = IncrementalForwardSession::prepare(
                base.as_ref(),
                &program,
                &gmeow_logic::annotation::AnnotationContract::exact(),
            )
            .map_err(|e| run_err(case, format!("incremental prepare failed: {e}")))?;
            let inserted = session
                .insert(delta.as_ref(), None)
                .map_err(|e| run_err(case, format!("incremental insertion failed: {e}")))?;
            let retracted = session
                .retract(delta.as_ref())
                .map_err(|e| run_err(case, format!("incremental retraction failed: {e}")))?;
            let incremental_fp = format!(
                "insert={};retract={}",
                fingerprint_rows(&inserted.rows),
                fingerprint_rows(&retracted.rows)
            );
            let scratch_fp = format!(
                "insert={};retract={}",
                fingerprint_rows(&updated_scratch.rows),
                fingerprint_rows(&base_scratch.rows)
            );
            Ok(GoldenObservation {
                world,
                native_count: inserted.derived_count,
                golden_count: golden_rows,
                fingerprint: Some(("incremental-vs-scratch", incremental_fp, scratch_fp)),
            })
        }
        Fragment::IncrementalGrounding => {
            let (world, golden_rows) = sole_world(case)?;
            let program = case
                .canonical_program()
                .map_err(|e| run_err(case, format!("canonical program parse failed: {e}")))?;
            let base = purrdf::parse_dataset(case.edb_nq.as_bytes(), "application/n-quads", None)
                .map_err(|e| run_err(case, format!("base EDB parse error: {e}")))?;
            let delta =
                purrdf::parse_dataset(case.delta_nq.as_bytes(), "application/n-quads", None)
                    .map_err(|e| run_err(case, format!("delta EDB parse error: {e}")))?;
            let updated = purrdf::RdfDataset::union(&[base.as_ref(), delta.as_ref()])?;
            let updated_edb_count = updated.quads().count() as u64;
            let mut session = IncrementalGroundingCostSession::prepare(base.as_ref(), &program)
                .map_err(|e| run_err(case, format!("incremental grounding prepare failed: {e}")))?;
            let base_fp =
                purrdf_hash::hex::Digest32::new(session.current_rows_fingerprint()).to_hex();
            let inserted = session.insert(delta.as_ref()).map_err(|e| {
                run_err(case, format!("incremental grounding insertion failed: {e}"))
            })?;
            let scratch = session
                .scratch_rebuild()
                .map_err(|e| run_err(case, format!("grounding scratch rebuild failed: {e}")))?;
            let retracted = session.retract(delta.as_ref()).map_err(|e| {
                run_err(
                    case,
                    format!("incremental grounding retraction failed: {e}"),
                )
            })?;
            let inserted_fp = purrdf_hash::hex::Digest32::new(inserted.rows_fingerprint).to_hex();
            let retracted_fp = purrdf_hash::hex::Digest32::new(retracted.rows_fingerprint).to_hex();
            let scratch_fp = purrdf_hash::hex::Digest32::new(scratch.rows_fingerprint).to_hex();
            let incremental_fp = format!("insert={inserted_fp};retract={retracted_fp}");
            let scratch_fp = format!("insert={scratch_fp};retract={base_fp}");
            let native_count = inserted
                .row_count
                .checked_sub(updated_edb_count)
                .ok_or_else(|| {
                    run_err(
                        case,
                        "incremental WFS row count is below updated EDB count".to_owned(),
                    )
                })?;
            Ok(GoldenObservation {
                world,
                native_count,
                golden_count: golden_rows,
                fingerprint: Some((
                    "incremental-grounding-vs-scratch",
                    incremental_fp,
                    scratch_fp,
                )),
            })
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
            let published_fp = golden.digest.clone().ok_or_else(|| {
                Diag::of_kind(RunFailed {
                    detail: format!(
                        "{}/{}: backward golden must carry the captured answer-set digest",
                        case.corpus, case.name
                    ),
                })
            })?;
            let store = WorldStore::new()?;
            store
                .load_nquads(&case.edb_nq)
                .map_err(|e| run_err(case, format!("EDB load failed: {e}")))?;
            let program = parse_query_program(&case.rules)
                .map_err(|e| run_err(case, format!("query parse failed: {e}")))?;
            let foreign = WorldFactSnapshot::from_world(&store, world, HORN_PROFILE_IRI)
                .map_err(|e| run_err(case, format!("foreign snapshot failed: {e}")))?;
            let budget = Budget::default();
            let native = dispatch_query(&foreign, world, &program, HORN_PROFILE_IRI, &budget)
                .map_err(|e| run_err(case, format!("native backward failed: {e}")))?;
            let native_fp = fingerprint_answers(&native);
            Ok(GoldenObservation {
                world: world.clone(),
                native_count: native.bindings.len() as u64,
                golden_count: golden_rows,
                fingerprint: Some(("native-vs-captured-sld", native_fp, published_fp)),
            })
        }
    }
}

/// (Deliverable 1) The ON-GATE native-vs-golden agreement gate. Runs the NATIVE engine
/// ONLY over every committed mini bench case and compares its deterministic derived /
/// answer count against the committed golden (`expected/result.json`). Backward cases
/// additionally compare the complete canonical answer-set fingerprint with the captured
/// SLD digest. Each comparison is folded through the shared divergence ledger, so every disagreement becomes a named
/// `gmeow:Finding` carrying content-addressed ledger identity (`finding_iri` + anchor +
/// antecedents), NOT a bare diff. ANY disagreement HARD-FAILS (no-optionality). No external
/// is driven, so this is cheap enough for `make check`.
/// Drive every committed mini bench case through the NATIVE engine ONLY and group the
/// resulting native-vs-golden comparisons by corpus (deterministic BTree order; the
/// loader yields cases sorted by `(corpus, case)`). This is the shared native-only path
/// behind BOTH the single-run golden gate ([`run_golden_gate`]) and the N-run soak window
/// ([`run_soak`]) — no external engine is ever constructed, so both stay cheap on-gate.
fn golden_comps_by_corpus(
    cases: &[BenchCase],
) -> gmeow_errors::Result<BTreeMap<String, Vec<ExternalComparison>>> {
    let mut comps_by_corpus: BTreeMap<String, Vec<ExternalComparison>> = BTreeMap::new();
    for case in cases {
        let observation = native_golden_pair(case)?;
        let comps = comps_by_corpus.entry(case.corpus.clone()).or_default();
        comps.push(comp(
            case,
            &observation.world,
            "native-vs-golden",
            &count_token(observation.native_count),
            &count_token(observation.golden_count),
        ));
        if let Some((kind, native_fp, published_fp)) = observation.fingerprint {
            comps.push(comp(
                case,
                &observation.world,
                kind,
                &native_fp,
                &published_fp,
            ));
        }
    }
    Ok(comps_by_corpus)
}

fn run_golden_gate(cases: &[BenchCase]) -> gmeow_errors::Result<()> {
    let comps_by_corpus = golden_comps_by_corpus(cases)?;

    let mut disagreements = 0usize;
    let mut findings: Vec<Value> = Vec::new();
    for (corpus, comps) in &comps_by_corpus {
        let rows = compare_external_corpus(corpus, comps);
        let ledger = build_ledger(Vec::new(), Vec::new(), rows);
        disagreements += ledger.corpus_only;
        for f in divergence_findings(&ledger) {
            if f.code == "reason.divergence.corpus-only" {
                findings.push(Value::from(
                    finding_record(&f)?.with("message", f.message.to_json()),
                ));
            }
        }
    }

    if disagreements == 0 {
        eprintln!(
            "✓ golden gate: all {} native mini-corpus case(s) agree with their committed golden \
             (count invariants plus captured backward answer digests; no external engine run).",
            cases.len()
        );
        return Ok(());
    }

    findings.sort_by(|a, b| a["finding_iri"].as_str().cmp(&b["finding_iri"].as_str()));
    let report = Value::from(
        Object::new()
            .with("schema", ("gmeow.bench-engines.golden-gate/1").to_json())
            .with("disagreement_count", (disagreements).to_json())
            .with("findings", (findings).to_json()),
    );
    let json = pretty_json(&report);
    println!("{json}");
    Err(Diag::of_kind(RunFailed {
        detail: format!(
            "{disagreements} golden-gate disagreement(s): the native engine's result diverged from \
             the committed golden on the mini bench corpora — the native core changed a result set, \
             or a committed golden is stale; review before landing"
        ),
    }))
}

/// (Deliverable 1) The SOAK-WINDOW gap-zero gate. Runs the DETERMINISTIC native-vs-golden
/// agreement check ([`golden_comps_by_corpus`]) `window` times over the committed corpora
/// and asserts, for EVERY run:
///
/// * **(a) gap-zero** — the folded divergence ledger has `dl_gap == 0 && corpus_only == 0`
///   across all corpora (no native↔published disagreement, no coverage gap);
/// * **(b) fingerprint reproducibility** — the run's finding-graph digest (a blake3 over
///   the per-corpus `emit_divergence_nq` finding graphs, in sorted corpus order) is
///   byte-identical to every other run's. A run whose fingerprint drifts is ITSELF a
///   divergence finding: reproducibility is the soak invariant a one-shot tally cannot show.
///
/// ANY run breaking gap-zero, or ANY digest drift across the window, HARD-FAILS with a
/// divergence finding (no-optionality). `window` must be `>= 2` (a window of one is a
/// single-run tally, not soak evidence). Reuses the shared golden/agreement ledger path —
/// no ledger is re-implemented here.
fn run_soak(cases: &[BenchCase], window: usize) -> gmeow_errors::Result<()> {
    if window < 2 {
        return Err(Diag::of_kind(Cli {
            detail: format!(
                "--soak window must be >= 2 (a window of {window} is a single-run tally, not soak \
                 evidence — a soak window is an N-run longitudinal reproducibility check)"
            ),
        }));
    }

    // Each run's finding-graph digest; asserted byte-identical across the whole window.
    let mut run_digests: Vec<String> = Vec::with_capacity(window);
    let mut corpora_count = 0usize;
    let case_count = cases.len();

    for run_ix in 1..=window {
        let comps_by_corpus = golden_comps_by_corpus(cases)?;
        corpora_count = comps_by_corpus.len();

        // Fold each corpus through the SHARED divergence ledger: tally the gap kinds and
        // fold the per-corpus finding graph into this run's combined digest (sorted corpus
        // order, so the digest is a pure function of the run's comparisons).
        let mut total_dl_gap = 0usize;
        let mut total_corpus_only = 0usize;
        let mut hasher = purrdf_hash::blake3::Hasher::new();
        let mut findings: Vec<Value> = Vec::new();
        for (corpus, comps) in &comps_by_corpus {
            let tally = agreement_tally(corpus, comps);
            total_dl_gap += tally.dl_gap;
            total_corpus_only += tally.corpus_only;

            let graph = emit_divergence_nq(corpus, comps);
            hasher.update(corpus.as_bytes());
            hasher.update(b"\x1f");
            hasher.update(graph.as_bytes());
            hasher.update(b"\n");

            // Surface the blocking divergence findings (corpus-only / dl-gap) for the
            // hard-fail report, each carrying its content-addressed ledger identity.
            if tally.corpus_only > 0 || tally.dl_gap > 0 {
                let rows = compare_external_corpus(corpus, comps);
                let ledger = build_ledger(Vec::new(), Vec::new(), rows);
                for f in divergence_findings(&ledger) {
                    if f.code == "reason.divergence.corpus-only"
                        || f.code == "reason.divergence.dl-gap"
                    {
                        findings.push(Value::from(
                            finding_record(&f)?.with("message", f.message.to_json()),
                        ));
                    }
                }
            }
        }
        let digest = purrdf_hash::hex::encode(hasher.finalize().as_bytes());

        // (a) gap-zero: ANY corpus-only disagreement or dl-gap breaks the soak window.
        if total_dl_gap != 0 || total_corpus_only != 0 {
            findings.sort_by(|a, b| a["finding_iri"].as_str().cmp(&b["finding_iri"].as_str()));
            let report = Value::from(
                Object::new()
                    .with("schema", ("gmeow.bench-engines.soak/1").to_json())
                    .with("run", (run_ix).to_json())
                    .with("window", (window).to_json())
                    .with("corpus_only", (total_corpus_only).to_json())
                    .with("dl_gap", (total_dl_gap).to_json())
                    .with("findings", (findings).to_json()),
            );
            let json = pretty_json(&report);
            println!("{json}");
            return Err(Diag::of_kind(RunFailed {
                detail: format!(
                    "soak run {run_ix}/{window} broke gap-zero: {total_corpus_only} corpus-only + \
                     {total_dl_gap} dl-gap divergence finding(s) over the committed corpora — the \
                     native core changed a result set, or a committed golden is stale"
                ),
            }));
        }

        eprintln!(
            "✓ soak run {run_ix}/{window}: gap-zero (0 corpus-only, 0 dl-gap); \
             finding-graph digest {digest}"
        );
        run_digests.push(digest);
    }

    // (b) reproducibility: every run's finding-graph digest must be byte-identical. A
    // drifting fingerprint is itself a divergence finding.
    let first = run_digests[0].clone();
    for (ix, d) in run_digests.iter().enumerate() {
        if *d != first {
            let report = Value::from(
                Object::new()
                    .with("schema", ("gmeow.bench-engines.soak/1").to_json())
                    .with("window", (window).to_json())
                    .with("reproducibility", (false).to_json())
                    .with("run", (ix + 1).to_json())
                    .with("expected_finding_graph_blake3", (first).to_json())
                    .with("actual_finding_graph_blake3", (d).to_json()),
            );
            let json = pretty_json(&report);
            println!("{json}");
            return Err(Diag::of_kind(RunFailed {
                detail: format!(
                    "soak reproducibility broke: run {}/{window} finding-graph digest {d} != run 1 \
                     digest {first} — a drifting fingerprint is itself a divergence finding \
                     (non-deterministic native output over a fixed corpus)",
                    ix + 1
                ),
            }));
        }
    }

    eprintln!(
        "✓ soak: {window}/{window} runs gap-zero with byte-identical finding-graph digest \
         {first} over {corpora_count} corpora ({case_count} case(s))."
    );
    Ok(())
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
/// (`native-vs-golden` / `native-vs-reference`) so the ledger's structural focus keys
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
    let mut hasher = purrdf_hash::blake3::Hasher::new();
    for row in &rows.rows {
        hasher.update(row.predicate.as_bytes());
        hasher.update(b"\x1f");
        for arg in &row.args {
            hasher.update(term_display(arg).as_bytes());
            hasher.update(b"\x1f");
        }
        hasher.update(b"\n");
    }
    purrdf_hash::hex::encode(hasher.finalize().as_bytes())
}

/// A stable fingerprint of a lexical signed closure-change batch.
fn fingerprint_signed_rows(rows: &[SignedForwardRow]) -> String {
    let mut hasher = purrdf_hash::blake3::Hasher::new();
    for change in rows {
        hasher.update(change.weight.to_string().as_bytes());
        hasher.update(b"\x1f");
        hasher.update(change.row.predicate.as_bytes());
        hasher.update(b"\x1f");
        for arg in &change.row.args {
            hasher.update(term_display(arg).as_bytes());
            hasher.update(b"\x1f");
        }
        hasher.update(b"\n");
    }
    purrdf_hash::hex::encode(hasher.finalize().as_bytes())
}

/// A `blake3` fingerprint of a canonically-sorted answer set. [`AnswerSet`] bindings
/// are sorted (each binding is a `BTreeMap`, and the vector is canonicalized), so the
/// fingerprint is a pure function of the answer set.
fn fingerprint_answers(answers: &AnswerSet) -> String {
    let mut hasher = purrdf_hash::blake3::Hasher::new();
    for binding in &answers.bindings {
        for (var, val) in binding {
            hasher.update(var.as_bytes());
            hasher.update(b"=");
            hasher.update(val.as_bytes());
            hasher.update(b"\x1f");
        }
        hasher.update(b"\n");
    }
    purrdf_hash::hex::encode(hasher.finalize().as_bytes())
}

/// Project the sorted cost-vector tuples into a JSON array of `[rule, predicate,
/// stratum, count]` — integer-valued, in `CostKey` order, so the array is
/// byte-deterministic.
fn cost_tuples(tuples: &[(String, String, u32, u64)]) -> Value {
    Value::Array(
        tuples
            .iter()
            .map(|(rule, pred, stratum, count)| {
                Value::Array(vec![
                    (rule).to_json(),
                    (pred).to_json(),
                    (stratum).to_json(),
                    (count).to_json(),
                ])
            })
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
        "   page reuse), so they are ADVISORY-ONLY — NEVER in the committed artifact. The three"
    );
    eprintln!(
        "   allocation scalars now GATE via the committed artifact (peak_live_bytes by exact"
    );
    eprintln!(
        "   drift-match; alloc_bytes/alloc_count via the one-sided tolerance band), so they are"
    );
    eprintln!("   no longer here. Verdict-agreement is the deterministic column.");
    eprintln!(
        "{:<22} {:<12} {:<7} {:>13} {:>9}  agree",
        "corpus", "fragment", "engine", "wall_ns", "rss_kib"
    );
    for r in rows {
        eprintln!(
            "{:<22} {:<12} {:<7} {:>13} {:>9}  {}",
            r.corpus, r.fragment, r.engine, r.wall_ns, r.peak_rss_kib, r.agreement
        );
    }
    eprintln!(
        "────────────────────────────────────────────────────────────────────────────────────"
    );
}

#[cfg(test)]
#[path = "json_tests.rs"]
mod tests;
