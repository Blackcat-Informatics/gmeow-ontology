// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The native physical engine's **performance ledger** — a first-class reasoning
//! artifact that FLAGS, honestly and machine-readably, the parts of the
//! seven-lever execution stack that are deliberately not yet incremental and the
//! advanced levers intentionally out of the current scope.
//!
//! This is a *flag, don't build* deliverable. The P0 levers that ARE built — the
//! one relational core (semi-naive + stratified negation + index selection) and
//! magic-sets / demand transformation — are NOT ledger rows: the ledger records
//! ONLY the deferred / non-incremental items, so a row is never misread as a
//! shipped feature.
//!
//! Two honest statuses keep a row from being misread as a defect, a TODO, or a
//! knob (Principle 17, no overclaim; maximal information flow):
//!
//! * [`PerfStatus::FlaggedNonIncremental`] — an explicit incremental boundary: a
//!   native fallback EXISTS; the construct is simply not yet incremental. NOT a
//!   missing capability.
//! * [`PerfStatus::DeclaredP1`] — the advanced levers intentionally out of
//!   the P0 scope. NOT defects, NOT yet built: a declared, bounded later stage.
//!
//! The wording mirrors the canonical lever prose in
//! `slices/grounding/logic/design/LOGIC-RUNTIME.md`. Process flow (which ticket, which
//! PR) lives only in the issue tracker, never in this code or its emitted Turtle.

use crate::reason::artifacts::{GMEOW_NS, RDF_TYPE, RDFS_COMMENT, gmeow};
use purrdf::turtle::emit_resource;
use purrdf::{RdfLiteral, RdfTerm};

/// `xsd:integer` — the datatype of the ledger's `gmeow:entryCount` object.
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

/// The two honest statuses a perf-ledger row can carry.
///
/// The variants are distinct *kinds* of "not done", never collapsed: a
/// non-incremental hard part has a working native fallback today; a declared-P1
/// lever has no implementation yet and is not meant to in this scope. Conflating
/// them would overclaim (a P1 lever read as "exists, just slow") or underclaim (a
/// hard part read as "absent").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PerfStatus {
    /// A canonical hard part: a native fallback exists, it is simply not yet
    /// incremental and stays a heavy-path fallback longest.
    FlaggedNonIncremental,
    /// An advanced lever intentionally out of P0 scope — not a defect, not yet
    /// built, a declared later stage.
    DeclaredP1,
}

impl PerfStatus {
    /// The `gmeow:` status individual IRI local name for this status.
    fn iri_local(self) -> &'static str {
        match self {
            PerfStatus::FlaggedNonIncremental => "FlaggedNonIncremental",
            PerfStatus::DeclaredP1 => "DeclaredP1",
        }
    }
}

/// One row of the performance ledger: a deferred / non-incremental construct, its
/// honest status, and a one-line note in the canonical lever wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerfRow {
    /// The deferred construct or advanced lever this row flags.
    pub construct: &'static str,
    /// The honest status keeping the row from being misread.
    pub status: PerfStatus,
    /// A one-line note, in the canonical lever prose, explaining the flag.
    pub note: &'static str,
}

/// The performance ledger: the fixed, deterministically-ordered set of deferred /
/// non-incremental rows. The order is canonical (the `flagged-non-incremental`
/// hard parts first, then the `declared-p1` levers), so the emitted Turtle is
/// content-stable run to run.
#[derive(Debug, Clone)]
pub struct PerfLedger {
    /// The canonical deferred rows in their fixed order.
    pub rows: Vec<PerfRow>,
}

/// The non-monotone solver selected for one maintained-ground-program shot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonmonotoneSolver {
    /// Van Gelder alternating-fixpoint evaluation.
    WellFounded,
    /// Exhaustive stable-model enumeration followed by cautious intersection.
    StableModel,
}

impl NonmonotoneSolver {
    /// Stable machine-readable solver name for a per-shot ledger row.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WellFounded => "well-founded alternating fixpoint",
            Self::StableModel => "stable-model cautious enumeration",
        }
    }
}

/// What happened at the explicitly non-incremental solver boundary for one shot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveDisposition {
    /// The asserted EDB and active ground rules were unchanged, so the previous
    /// solution was reused without entering the solver.
    ReusedUnchangedGroundSlice,
    /// The maintained solver slice changed and the existing solver reran from
    /// scratch.  This is the honest open-research boundary.
    ReranFromScratch,
}

/// Machine-readable performance-ledger row for one incremental-grounding shot.
///
/// The static [`PerfLedger`] says that non-monotone **solving** remains a flagged
/// boundary. This row makes each run equally explicit without contaminating the
/// deterministic repository-wide Turtle with process history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonmonotoneSolveRun {
    /// Solver whose boundary this row records.
    pub solver: NonmonotoneSolver,
    /// Same honest status as the canonical static ledger row.
    pub status: PerfStatus,
    /// Reuse versus explicit from-scratch solving for this shot.
    pub disposition: SolveDisposition,
    /// Consolidated asserted-fact changes in the solver slice.
    pub edb_changes: usize,
    /// Active ground-rule zero-crossings in the solver slice.
    pub ground_rule_changes: usize,
}

impl NonmonotoneSolveRun {
    /// Whether this shot entered the deliberately non-incremental solver.
    pub fn solver_reran(self) -> bool {
        self.disposition == SolveDisposition::ReranFromScratch
    }
}

/// Construct the per-shot ledger row at the maintained-ground-program boundary.
pub(crate) fn nonmonotone_solve_run(
    solver: NonmonotoneSolver,
    slice_changed: bool,
    edb_changes: usize,
    ground_rule_changes: usize,
) -> NonmonotoneSolveRun {
    NonmonotoneSolveRun {
        solver,
        status: PerfStatus::FlaggedNonIncremental,
        disposition: if slice_changed {
            SolveDisposition::ReranFromScratch
        } else {
            SolveDisposition::ReusedUnchangedGroundSlice
        },
        edb_changes,
        ground_rule_changes,
    }
}

/// Build the canonical performance ledger.
///
/// Five rows, fixed order: the remaining `flagged-non-incremental` boundaries.
/// Selective WCOJ, compile-don't-interpret, and bounded provenance annotations are
/// built and therefore absent. This is the single source of the
/// ledger content — both the Turtle emitter and any structured consumer fold from
/// it, so they can never disagree.
pub fn perf_ledger() -> PerfLedger {
    PerfLedger {
        rows: vec![
            // ── The three canonical hard parts (a native fallback EXISTS; not yet
            //    incremental, so they stay heavy-path fallbacks longest). ──
            PerfRow {
                construct: "incremental well-founded / stable-model solving",
                status: PerfStatus::FlaggedNonIncremental,
                note: "the ground program is maintained incrementally, while well-founded / \
                       stable-model solving reruns from scratch when that slice changes and \
                       remains explicitly non-incremental",
            },
            PerfRow {
                construct: "existential-rule chase with termination and incrementality together",
                status: PerfStatus::FlaggedNonIncremental,
                note: "existential-rule chase with termination AND incrementality together \
                       stays a heavy-path fallback longest; a native fallback exists but is \
                       not yet incremental",
            },
            PerfRow {
                construct: "paraconsistent / modal facets",
                status: PerfStatus::FlaggedNonIncremental,
                note: "the paraconsistent / modal facets stay heavy-path fallbacks longest; \
                       a native fallback exists but is not yet incremental",
            },
            PerfRow {
                construct: "rule-program-changing conjecture candidates",
                status: PerfStatus::FlaggedNonIncremental,
                note: "ground fact candidates use the signed fixed-contract session; a \
                       candidate that changes the rule program has a native fallback but is \
                       not yet incremental",
            },
            PerfRow {
                construct: "bounded retractions and non-positive counterfactual programs",
                status: PerfStatus::FlaggedNonIncremental,
                note: "unbounded positive counterfactual revisions use signed incremental \
                       maintenance; bounded retractions and programs with negation, \
                       builtins, or rule facts retain a native fallback but are not yet \
                       incremental",
            },
        ],
    }
}

/// The banner + prefix block prepended to the performance-ledger Turtle. It
/// explains the two statuses so a reader of the bare artifact never misreads a
/// row as a defect, a TODO, or a knob.
const PERF_HEADER: &str = "\
# GMEOW native physical engine performance ledger.
# A first-class reasoning artifact flagging the deferred / non-incremental parts
# of the seven-lever execution stack. The built P0 levers (the relational core —
# semi-naive + stratified negation + index selection, selective worst-case-optimal
# joins, magic-sets, positive-Datalog signed incrementality, cached compiled plans,
# bounded min-height / Z-weight provenance, and non-monotone incremental grounding)
# are NOT
# rows here; this ledger records ONLY the deferred items, so a row is never
# misread as a shipped feature. Two honest statuses:
#   gmeow:FlaggedNonIncremental — a canonical hard part: a native fallback EXISTS,
#     it is simply not yet incremental and stays a heavy-path fallback longest.
#   gmeow:DeclaredP1 — an advanced lever intentionally out of the current scope:
#     NOT a defect, NOT yet built, a declared later stage.
# DO NOT EDIT.
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
";

impl PerfLedger {
    /// Render the performance ledger as deterministic RDF 1.2 Turtle in the
    /// `gmeow:` vocabulary.
    ///
    /// Emits the banner, the ledger header individual, then one
    /// `gmeow:PerfLedgerEntry` per row (in the fixed canonical order) carrying
    /// `gmeow:construct`, `gmeow:perfStatus` (the `gmeow:FlaggedNonIncremental` /
    /// `gmeow:DeclaredP1` individual), and `gmeow:note`. The output is a pure
    /// function of [`perf_ledger`], so it is byte-stable run to run. No process
    /// tokens (no issue / PR numbers) ever appear.
    pub fn to_turtle(&self) -> String {
        let mut out = String::from(PERF_HEADER);

        out.push_str("\n# --- ledger header (deferred / non-incremental rows only) ---\n");
        out.push_str(&emit_resource(
            &gmeow("perf-ledger"),
            &[
                (RDF_TYPE.to_owned(), RdfTerm::iri(gmeow("PerfLedger"))),
                (
                    gmeow("entryCount"),
                    RdfTerm::literal(RdfLiteral::typed(self.rows.len().to_string(), XSD_INTEGER)),
                ),
                (
                    RDFS_COMMENT.to_owned(),
                    RdfTerm::literal(RdfLiteral::language_tagged(
                        "the deferred / non-incremental parts of the native physical engine's \
                         seven-lever stack; the built P0 levers are not rows here",
                        "en",
                    )),
                ),
            ],
        ));

        out.push_str("\n# --- deferred / non-incremental entries ---\n");
        for (index, row) in self.rows.iter().enumerate() {
            out.push_str(&emit_resource(
                &gmeow(&format!("perf-entry-{index}")),
                &[
                    (RDF_TYPE.to_owned(), RdfTerm::iri(gmeow("PerfLedgerEntry"))),
                    (
                        gmeow("construct"),
                        RdfTerm::literal(RdfLiteral::language_tagged(row.construct, "en")),
                    ),
                    (
                        gmeow("perfStatus"),
                        RdfTerm::iri(format!("{}{}", GMEOW_NS, row.status.iri_local())),
                    ),
                    (
                        gmeow("note"),
                        RdfTerm::literal(RdfLiteral::language_tagged(row.note, "en")),
                    ),
                ],
            ));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ledger carries exactly the five still-live incremental boundaries. Built
    /// optimization levers are removed rather than left behind as stale declarations.
    #[test]
    fn ledger_has_five_live_flagged_rows_and_no_stale_declared_levers() {
        let ledger = perf_ledger();
        assert_eq!(ledger.rows.len(), 5, "exactly five deferred rows remain");
        let flagged = ledger
            .rows
            .iter()
            .filter(|r| r.status == PerfStatus::FlaggedNonIncremental)
            .count();
        let p1 = ledger
            .rows
            .iter()
            .filter(|r| r.status == PerfStatus::DeclaredP1)
            .count();
        assert_eq!(flagged, 5, "five flagged-non-incremental boundaries");
        assert_eq!(p1, 0, "no already-built lever remains declared-p1");
        assert!(
            ledger
                .rows
                .iter()
                .all(|r| r.status == PerfStatus::FlaggedNonIncremental),
            "all remaining rows are real non-incremental boundaries"
        );
    }

    /// The emitted Turtle pins the exact canonical content and proves completed
    /// levers are absent from the deferred ledger.
    #[test]
    fn turtle_golden_pins_five_rows_and_excludes_built_levers() {
        let ttl = perf_ledger().to_turtle();

        // Banner + the two status individuals are explained.
        assert!(
            ttl.contains("native physical engine performance ledger"),
            "the banner names the artifact"
        );
        assert!(
            ttl.contains("gmeow:FlaggedNonIncremental — a canonical hard part"),
            "the banner explains the FlaggedNonIncremental status"
        );
        assert!(
            ttl.contains(
                "gmeow:DeclaredP1 — an advanced lever intentionally out of the current scope"
            ),
            "the banner explains the DeclaredP1 status"
        );

        // The ledger header individual + entry count.
        assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/PerfLedger>"));
        assert!(ttl.contains(&format!("gmeow/entryCount> \"5\"^^<{XSD_INTEGER}>")));

        // The three FlaggedNonIncremental hard parts (canon wording).
        for construct in [
            "incremental well-founded / stable-model solving",
            "existential-rule chase with termination and incrementality together",
            "paraconsistent / modal facets",
            "rule-program-changing conjecture candidates",
            "bounded retractions and non-positive counterfactual programs",
        ] {
            assert!(
                ttl.contains(construct),
                "the flagged-non-incremental hard part must appear verbatim: {construct}"
            );
        }
        // These three levers are built, so none may be misrepresented as deferred.
        for lever in [
            "provenance semirings",
            "compile-don't-interpret (specialize per content-addressed contract hash)",
            "worst-case-optimal joins",
        ] {
            assert!(
                !ttl.contains(&format!("construct> \"{lever}")),
                "a built lever must not remain a deferred construct: {lever}"
            );
        }

        // Only the status used by a live row is emitted as an object.
        assert!(ttl.contains(
            "gmeow/perfStatus> <https://blackcatinformatics.ca/gmeow/FlaggedNonIncremental>"
        ));
        assert!(
            !ttl.contains("gmeow/perfStatus> <https://blackcatinformatics.ca/gmeow/DeclaredP1>")
        );

        // Five entries of type gmeow:PerfLedgerEntry.
        assert_eq!(
            ttl.matches("#type> <https://blackcatinformatics.ca/gmeow/PerfLedgerEntry>")
                .count(),
            5,
            "exactly five PerfLedgerEntry rows are emitted"
        );

        // NO process tokens: no `#NNNN` issue/PR references, no `F3`/`T6` ticket
        // tokens (process flow lives only in the issue tracker, never here).
        for ch in ttl.chars().zip(ttl.chars().skip(1)) {
            assert!(
                !(ch.0 == '#' && ch.1.is_ascii_digit()),
                "no `#NNNN` issue/PR token may appear in the perf-ledger Turtle"
            );
        }
        assert!(
            !ttl.contains("F3") && !ttl.contains("T6"),
            "no F3/T6 ticket tokens may appear in the perf-ledger Turtle"
        );

        // Determinism: the emitter is a pure function of the canonical ledger.
        assert_eq!(
            ttl,
            perf_ledger().to_turtle(),
            "the perf-ledger Turtle must be byte-deterministic"
        );
    }

    #[test]
    fn per_shot_row_never_mislabels_from_scratch_solving_as_incremental() {
        let rerun = nonmonotone_solve_run(NonmonotoneSolver::WellFounded, true, 1, 3);
        assert_eq!(rerun.status, PerfStatus::FlaggedNonIncremental);
        assert_eq!(rerun.solver.as_str(), "well-founded alternating fixpoint");
        assert!(rerun.solver_reran());
        assert_eq!(rerun.edb_changes, 1);
        assert_eq!(rerun.ground_rule_changes, 3);

        let reused = nonmonotone_solve_run(NonmonotoneSolver::StableModel, false, 0, 0);
        assert_eq!(
            reused.disposition,
            SolveDisposition::ReusedUnchangedGroundSlice
        );
        assert!(!reused.solver_reran());
        assert_eq!(reused.solver.as_str(), "stable-model cautious enumeration");
    }
}
