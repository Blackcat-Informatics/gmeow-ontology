// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The off-gate independent OWL-Direct consistency differential.
//!
//! This is the sibling of [`crate::entail_crosscheck`] for CONSISTENCY. It drives
//! gmeow's OWN native structured DL verdict ([`crate::reason::dl::DlVerdict`] via
//! [`crate::reason::dl_consistency`]) against the independent, conformance-tested
//! OWL-Direct ALCOIQ tableau ([`crate::entail_oracle::consistency`], a
//! `purrdf::entail::materialize_dl` engine) and folds the comparison into the same
//! structured [`crate::reason::ledger::DivergenceLedger`]. No Java, no Docker, no
//! network — but it is NP-hard, so it runs OFF-gate as
//! `gmeow-dev reason-consistency-crosscheck` / `make maint-reason-consistency-crosscheck`,
//! never in `make check`.
//!
//! # Feasibility verdict (why per-world, not per-corpus)
//!
//! A **per-corpus** check — merging every named graph into one giant tableau
//! instance — is INFEASIBLE and rejected on the merits, not as a scope cut. It is
//! one enormous NP-hard OWL-Direct instance AND it is *semantically wrong* for
//! gmeow's world-scoped chase: collapsing the worlds invents cross-world clashes
//! neither engine derives in isolation (the exact artifact
//! [`crate::entail_crosscheck`]'s per-world design avoids). The feasible design is
//! the **per-world isolated** differential implemented here: both engines reason
//! over the SAME single-world projection
//! ([`purrdf::RdfDataset::project_named_graph`]) — faithful to the native chase,
//! which itself treats each named graph in isolation.
//!
//! A per-world native verdict is NOT derivable from one whole-bundle native run:
//! the native `gaps`/coverage and per-world verdict do not decompose after the
//! fact, so each world is re-reasoned natively here (unlike the subsumption arm,
//! which reads its native side off one caller-supplied closure). This is why the
//! arm cannot reuse a single caller-side chase.
//!
//! # Value verdict (what this differential is, and is not)
//!
//! The load-bearing fact: native is, by the `incomplete-never-wrong` doctrine
//! ([`crate::reason::dl`]), a sound **SUBSET** on coverage. It deliberately
//! WITHHOLDS out-of-fragment constructs (`owl:oneOf`, `owl:cardinality`,
//! `owl:InverseFunctionalProperty`, `owl:disjointUnionOf`) rather than guess. So
//! this is **NOT** a `native ⊇ oracle` gate like the subsumption arm.
//!
//! * A world where native withholds a construct the oracle decides is a
//!   NON-failing `oracle-supplement`: informational coverage enrichment, never a
//!   failure. Native's by-design incompleteness is correct, not a defect.
//! * The tripwire fires on exactly ONE condition: native DECIDED a world/class
//!   consistent while the sound OWL-Direct oracle proves it inconsistent/empty
//!   (`OracleOnly`) — a native SOUNDNESS miss.
//! * Native deciding inconsistent where the oracle's fragment is consistent is
//!   native's richer calculus (`NativeOnly`) — recorded, non-failing.
//!
//! Because only genuine soundness misses map to `OracleOnly`, the correct,
//! consistent committed bundle yields ZERO `OracleOnly` rows, so the lane exits 0
//! (exactly as the subsumption arm's `OracleOnly` count is 0 on a correct bundle);
//! it reddens only on a genuine native soundness regression. Comparing the
//! STRUCTURED verdicts — never a boolean fold — is what preserves the signal.
//!
//! # Robustness (the watchdog)
//!
//! [`materialize_dl`](purrdf::entail::materialize_dl) has NO cancellation path, so
//! each world's oracle tableau runs on a worker thread read with `recv_timeout`. A
//! world exceeding [`ORACLE_BUDGET`] emits a NON-failing `oracle-undecided` row
//! (recorded and loud, never a silent skip) and the run moves on — but the
//! budget-exceeded worker is uninterruptible and keeps running in the background,
//! unjoined. Worlds are dispatched SEQUENTIALLY (not via rayon), but sequential
//! *dispatch* does NOT bound concurrent *leaked* workers: because a timed-out
//! worker is never joined, finished-but-unjoined and still-running workers can
//! ACCUMULATE across consecutive timeouts and run CONCURRENTLY, which can saturate
//! CPU on exactly the hard bundle where timeouts cluster. A process-global live
//! worker counter ([`LIVE_ORACLE_WORKERS`]) makes this accumulation observable: a
//! loud `stderr` warning fires whenever more than one oracle worker is live at
//! once.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::thread;
use std::time::Duration;

use purrdf::RdfDataset;

use crate::entail_oracle;
use crate::reason::dl::DlVerdict;
use crate::reason::dl_consistency;
use crate::reason::ledger::{
    DivergenceKind, DivergenceLedger, LedgerRow, LedgerVerdict, build_ledger, compare_subsumption,
    enforce,
};

/// Build a reasoning-driver hard-fail diagnostic (mirrors `reason::mod::reason_err`).
fn crosscheck_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

/// The ledger category every row this arm emits carries.
const CONSISTENCY: &str = "consistency";
/// `owl:Nothing` — the bottom class an unsatisfiable class is proven equal to; the
/// object of every per-class unsatisfiability tuple compared here.
const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";
/// Detail prefix marking a NON-failing oracle-supplement row (native withheld an
/// out-of-fragment construct; the oracle's verdict is recorded as enrichment).
const ORACLE_SUPPLEMENT: &str = "oracle-supplement";
/// Detail prefix marking a NON-failing oracle-undecided row (the oracle exceeded
/// the per-world budget; no verdict, so no divergence is possible for that world).
const ORACLE_UNDECIDED: &str = "oracle-undecided";

/// The per-world oracle-tableau time budget: a maintainer-lane give-up-and-record
/// threshold, NOT a correctness parameter. `materialize_dl` has no cancellation
/// path, so a world that exceeds this is recorded as `oracle-undecided`
/// (non-failing) and the leaked worker is left to die. Sized for the off-gate
/// maintainer lane, where a per-world OWL-Direct tableau may be arbitrarily hard.
pub const ORACLE_BUDGET: Duration = Duration::from_secs(120);

/// Process-global count of oracle-tableau worker threads currently live
/// (spawned but not yet finished computing). Since [`oracle_within_budget`]
/// never joins a timed-out worker, this counter is the ONLY honest observable
/// for the leaked-worker accumulation the watchdog cannot prevent: it is
/// incremented when a worker starts and decremented when it finishes, whether
/// or not anyone is still listening for its result.
static LIVE_ORACLE_WORKERS: AtomicUsize = AtomicUsize::new(0);

/// The full outcome of one consistency cross-check run: the classified
/// [`DivergenceLedger`], the strict [`LedgerVerdict`] over it, and calibration
/// counts. `agree` counts pure agreements only (global + per-class);
/// `oracle_supplement` and `oracle_undecided` are counted separately even though
/// both ride the non-failing [`DivergenceKind::Agree`] kind.
#[derive(Debug, Clone)]
pub struct ConsistencyCrosscheckOutcome {
    /// The classified divergence ledger (consistency rows only).
    pub ledger: DivergenceLedger,
    /// The strict verdict: `passed` is false iff any `OracleOnly` row is present.
    pub verdict: LedgerVerdict,
    /// Distinct named-graph worlds present in the source bundle.
    pub source_worlds: usize,
    /// Pure agreement rows (both engines decided and agree), excluding the
    /// non-failing supplement/undecided rows that also ride the `Agree` kind.
    pub agree: usize,
    /// Native-richer rows (native decided inconsistent where the oracle's fragment
    /// is consistent) — recorded, non-failing.
    pub native_only: usize,
    /// Native soundness misses (native decided consistent, oracle proved
    /// inconsistent/empty) — the sole FAILING kind.
    pub oracle_only: usize,
    /// Worlds where native withheld an out-of-fragment construct and the oracle
    /// supplied the verdict — informational, non-failing.
    pub oracle_supplement: usize,
    /// Worlds where the oracle exceeded [`ORACLE_BUDGET`] — recorded, non-failing.
    pub oracle_undecided: usize,
}

/// The result of running a per-world oracle computation under the watchdog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchdogOutcome {
    /// The oracle finished within budget with this structured `(flag, unsat)` verdict.
    Decided((bool, Vec<String>)),
    /// The oracle exceeded the budget; no verdict is available for this world.
    Undecided,
}

/// Build the NON-failing `oracle-undecided` row for a world whose oracle tableau
/// exceeded `budget`. Loud and recorded (never a silent skip); classified
/// [`DivergenceKind::Agree`] so [`enforce`] never fails on it — a budget miss is
/// not a native defect.
pub fn oracle_undecided_row(world: &str, budget: Duration) -> LedgerRow {
    LedgerRow {
        kind: DivergenceKind::Agree,
        category: CONSISTENCY.to_owned(),
        subject: String::new(),
        object: String::new(),
        world: world.to_owned(),
        detail: format!(
            "{ORACLE_UNDECIDED}({world}, {budget:?}): the OWL-Direct oracle exceeded its \
             per-world budget; no verdict — no divergence possible for this world"
        ),
    }
}

/// Run `compute` (a per-world oracle computation) on a worker thread and take its
/// verdict iff it finishes within `budget`.
///
/// This is the production watchdog seam. `materialize_dl` has no cancellation
/// path, so on a budget miss we return [`WatchdogOutcome::Undecided`] and leave the
/// worker thread to run to completion and die on its own (its result is discarded
/// when the receiver is dropped). That worker is uninterruptible and is never
/// joined, so it keeps running in the background after we give up on it; because
/// dispatch across worlds is sequential but leaked workers are not, unjoined
/// workers from consecutive timeouts can ACCUMULATE and run CONCURRENTLY,
/// saturating CPU on exactly the hard bundle where timeouts cluster. This is
/// tracked via [`LIVE_ORACLE_WORKERS`]: a budget miss with more than one worker
/// still live emits a loud `stderr` warning naming the accumulation.
///
/// # Errors
///
/// Returns `Err` if the worker thread panicked (the channel disconnected without a
/// value) — an oracle panic is a HARD FAIL, never silently downgraded to
/// "undecided".
pub fn oracle_within_budget<F>(
    compute: F,
    budget: Duration,
) -> gmeow_errors::Result<WatchdogOutcome>
where
    F: FnOnce() -> (bool, Vec<String>) + Send + 'static,
{
    let (tx, rx) = channel();
    thread::spawn(move || {
        LIVE_ORACLE_WORKERS.fetch_add(1, Ordering::SeqCst);
        let result = compute();
        LIVE_ORACLE_WORKERS.fetch_sub(1, Ordering::SeqCst);
        // A send error means the receiver already gave up (budget elapsed); the
        // computed verdict is simply discarded.
        let _ = tx.send(result);
    });
    match rx.recv_timeout(budget) {
        Ok(verdict) => Ok(WatchdogOutcome::Decided(verdict)),
        Err(RecvTimeoutError::Timeout) => {
            let live = LIVE_ORACLE_WORKERS.load(Ordering::SeqCst);
            if live > 1 {
                eprintln!(
                    "WARNING: oracle budget exceeded with {live} oracle-tableau workers now \
                     live (leaked uninterruptible workers are ACCUMULATING and running \
                     CONCURRENTLY — this can saturate CPU; see the consistency_crosscheck \
                     module docs)"
                );
            }
            Ok(WatchdogOutcome::Undecided)
        }
        Err(RecvTimeoutError::Disconnected) => Err(crosscheck_err(
            "the OWL-Direct consistency oracle worker panicked (a hard fail): the tableau \
             materialization did not return a verdict"
                .to_owned(),
        )),
    }
}

/// Classify ONE world's native↔oracle consistency verdicts into ledger rows.
///
/// Both `native` and `oracle` reasoned over the SAME `project_named_graph(world)`
/// projection. This is the production classification seam
/// [`run_consistency_crosscheck`] calls per world; the `OracleOnly` fail branch is
/// unreachable on any sound native engine, so it is exercised directly by feeding a
/// crafted `(native-decided-consistent, oracle-inconsistent)` pair (the
/// anti-regression tripwire it guards).
///
/// * **native has a gap** (`!native.gaps.is_empty()`) → a single NON-failing
///   `oracle-supplement` row ([`DivergenceKind::Agree`]); the global and per-class
///   dimensions are skipped because native did not decide this world.
/// * **global dimension** (native decided): the oracle is globally consistent iff
///   its flag is `true` OR its unsat list is non-empty (a `(false, [X…])` result is
///   a consistent ontology with empty classes, whose empty classes flow through the
///   per-class dimension). `(true,true)`/`(false,false)` → [`DivergenceKind::Agree`];
///   native inconsistent + oracle consistent → [`DivergenceKind::NativeOnly`];
///   native consistent + oracle globally inconsistent (`(false, [])`, an ABox
///   clash) → [`DivergenceKind::OracleOnly`] (the sole FAIL).
/// * **per-class dimension** (native decided, MAXIMAL INFORMATION): native
///   `unsatisfiable_classes` and the oracle's unsat classes are mapped to
///   `(class, owl:Nothing, world)` tuples and compared through the existing
///   [`compare_subsumption`]; each row's category is reset to `"consistency"` and
///   its `world` is appended to the detail so the world IRI travels with every row.
///   An `OracleOnly` per-class row (oracle proved a class empty native decided
///   satisfiable) FAILS; a `NativeOnly` per-class row is non-failing richness.
pub fn classify_consistency(
    native: &DlVerdict,
    oracle: &(bool, Vec<String>),
    world: &str,
) -> Vec<LedgerRow> {
    // Native withheld an out-of-fragment construct: it did not decide this world.
    // Record the oracle's verdict as non-failing enrichment and stop — the global
    // and per-class dimensions require a native decision.
    if !native.gaps.is_empty() {
        let oracle_verdict = if oracle.0 {
            "consistent"
        } else {
            "inconsistent"
        };
        return vec![LedgerRow {
            kind: DivergenceKind::Agree,
            category: CONSISTENCY.to_owned(),
            subject: String::new(),
            object: String::new(),
            world: world.to_owned(),
            detail: format!(
                "{ORACLE_SUPPLEMENT}({world}): native withheld out-of-fragment construct; \
                 oracle verdict={oracle_verdict}"
            ),
        }];
    }

    let mut rows: Vec<LedgerRow> = Vec::new();

    // ── Global consistency dimension ────────────────────────────────────────────
    let native_consistent = native.consistent;
    // The oracle is globally consistent unless it reported an ABox clash `(false,
    // [])`. `(false, [X…])` (unsatisfiable-but-unpopulated classes) is a consistent
    // ontology globally; those empty classes are handled per-class below. This
    // defers to `entail_oracle::global_consistent_verdict` as the single contract
    // authority: `consistency` guarantees a global inconsistency yields an empty
    // unsat list, so a populated list is never a global inconsistency.
    let oracle_global_consistent = entail_oracle::global_consistent_verdict(oracle.0, &oracle.1);
    let global = match (native_consistent, oracle_global_consistent) {
        (true, true) => LedgerRow {
            kind: DivergenceKind::Agree,
            category: CONSISTENCY.to_owned(),
            subject: String::new(),
            object: String::new(),
            world: world.to_owned(),
            detail: format!("native and the oracle agree consistent (world {world})"),
        },
        (false, false) => LedgerRow {
            kind: DivergenceKind::Agree,
            category: CONSISTENCY.to_owned(),
            subject: String::new(),
            object: String::new(),
            world: world.to_owned(),
            detail: format!("native and the oracle agree inconsistent (ABox clash, world {world})"),
        },
        (false, true) => LedgerRow {
            kind: DivergenceKind::NativeOnly,
            category: CONSISTENCY.to_owned(),
            subject: String::new(),
            object: String::new(),
            world: world.to_owned(),
            detail: format!(
                "native decided inconsistent where the oracle's fragment is consistent \
                 (native's richer calculus, world {world})"
            ),
        },
        (true, false) => LedgerRow {
            kind: DivergenceKind::OracleOnly,
            category: CONSISTENCY.to_owned(),
            subject: String::new(),
            object: String::new(),
            world: world.to_owned(),
            detail: format!(
                "native decided CONSISTENT but the sound OWL-Direct oracle proved a GLOBAL \
                 inconsistency (native soundness miss, world {world})"
            ),
        },
    };
    rows.push(global);

    // ── Per-class unsatisfiability dimension (do not discard the lists) ──────────
    let native_unsat: Vec<(String, String, String)> = native
        .unsatisfiable_classes
        .iter()
        .map(|u| (u.class.clone(), OWL_NOTHING.to_owned(), world.to_owned()))
        .collect();
    let oracle_unsat: Vec<(String, String, String)> = oracle
        .1
        .iter()
        .map(|c| (c.clone(), OWL_NOTHING.to_owned(), world.to_owned()))
        .collect();
    let mut class_rows = compare_subsumption(&native_unsat, &oracle_unsat);
    for row in &mut class_rows {
        // The rows arrive stamped "subsumption"; reset to "consistency" so the
        // ledger stays coherent, and append the world so the IRI rides in the
        // detail (the world field already carries it; this keeps it greppable).
        row.category = CONSISTENCY.to_owned();
        row.detail = format!(
            "per-class unsatisfiability: {} (world {})",
            row.detail, row.world
        );
    }
    rows.extend(class_rows);

    rows
}

/// Run the native ↔ OWL-Direct-oracle consistency differential over `bundle`,
/// per world, with the default [`ORACLE_BUDGET`].
///
/// Both engines reason over the SAME single-world projection. Worlds are processed
/// sequentially (the oracle is uninterruptible; see the module docs). The result
/// folds every per-world classification into a [`DivergenceLedger`], carries the
/// strict [`enforce`] verdict (fails ONLY on `OracleOnly`), and reports calibration
/// counts. Every row grounds as a `gmeow:Finding` via
/// [`crate::reason::divergence_findings`].
///
/// # Errors
///
/// Returns `Err` if the world enumeration cannot be built, a native per-world
/// consistency run fails, or an oracle worker panics (a hard fail).
pub fn run_consistency_crosscheck(
    bundle: &RdfDataset,
) -> gmeow_errors::Result<ConsistencyCrosscheckOutcome> {
    let worlds = crate::entail_crosscheck::source_worlds(bundle)?;
    let source_worlds = worlds.len();

    let mut rows: Vec<LedgerRow> = Vec::new();
    for world in &worlds {
        // Project ONCE; native reads the borrow, then the owned projection moves
        // into the oracle worker thread (both reason over the identical world).
        let world_ds = bundle.project_named_graph(world);
        let native = dl_consistency(&world_ds)?;

        let oracle_ds = world_ds;
        let outcome = oracle_within_budget(
            move || entail_oracle::consistency(&oracle_ds),
            ORACLE_BUDGET,
        )?;
        match outcome {
            WatchdogOutcome::Decided(oracle) => {
                rows.extend(classify_consistency(&native, &oracle, world));
            }
            WatchdogOutcome::Undecided => {
                rows.push(oracle_undecided_row(world, ORACLE_BUDGET));
            }
        }
    }

    let ledger = build_ledger(Vec::new(), rows, Vec::new(), Vec::new());
    let verdict = enforce(&ledger);

    // Derive the calibration counts. Supplement and undecided rows both ride the
    // non-failing `Agree` kind, so peel them off the raw `Agree` tally to leave the
    // pure agreements.
    let oracle_supplement = ledger
        .rows
        .iter()
        .filter(|r| r.detail.starts_with(ORACLE_SUPPLEMENT))
        .count();
    let oracle_undecided = ledger
        .rows
        .iter()
        .filter(|r| r.detail.starts_with(ORACLE_UNDECIDED))
        .count();
    let agree = ledger
        .agree
        .saturating_sub(oracle_supplement)
        .saturating_sub(oracle_undecided);
    let native_only = ledger.native_only;
    let oracle_only = ledger.oracle_only;

    Ok(ConsistencyCrosscheckOutcome {
        ledger,
        verdict,
        source_worlds,
        agree,
        native_only,
        oracle_only,
        oracle_supplement,
        oracle_undecided,
    })
}
