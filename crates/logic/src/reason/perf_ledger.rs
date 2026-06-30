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
//! * [`PerfStatus::FlaggedNonIncremental`] — the three canonical "hard parts": a
//!   native fallback EXISTS for each; they are simply not yet incremental, and
//!   stay heavy-path fallbacks longest. NOT a missing capability.
//! * [`PerfStatus::DeclaredP1`] — the three advanced levers intentionally out of
//!   the P0 scope. NOT defects, NOT yet built: a declared, bounded later stage.
//!
//! The wording mirrors the canonical lever prose in
//! `slices/core/logic/design/LOGIC-RUNTIME.md`. Process flow (which ticket, which
//! PR) lives only in the issue tracker, never in this code or its emitted Turtle.

use crate::reason::artifacts::{escape_literal, gmeow, GMEOW_NS, RDFS_COMMENT, RDF_TYPE};
use gmeow_rdf::turtle::emit_resource;

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
    /// The six canonical rows in their fixed order.
    pub rows: Vec<PerfRow>,
}

/// Build the canonical performance ledger.
///
/// Six rows, fixed order: the three `flagged-non-incremental` hard parts followed
/// by the three `declared-p1` advanced levers. This is the single source of the
/// ledger content — both the Turtle emitter and any structured consumer fold from
/// it, so they can never disagree.
pub fn perf_ledger() -> PerfLedger {
    PerfLedger {
        rows: vec![
            // ── The three canonical hard parts (a native fallback EXISTS; not yet
            //    incremental, so they stay heavy-path fallbacks longest). ──
            PerfRow {
                construct: "incremental well-founded / stable-model semantics",
                status: PerfStatus::FlaggedNonIncremental,
                note: "well-founded / stable-model semantics incrementally stays a \
                       heavy-path fallback longest; a native fallback exists but is not \
                       yet incremental",
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
            // ── The three advanced levers, intentionally out of P0 scope (NOT
            //    defects, NOT yet built — a declared later stage). ──
            PerfRow {
                construct: "worst-case-optimal joins",
                status: PerfStatus::DeclaredP1,
                note: "worst-case-optimal joins for cyclic graph patterns — an advanced \
                       lever declared out of the current scope, not yet built",
            },
            PerfRow {
                construct: "provenance semirings",
                status: PerfStatus::DeclaredP1,
                note: "provenance semirings computing world/standpoint and the quantitative \
                       axes in one pass, where the semiring is the axis algebra — an advanced \
                       lever declared out of the current scope, not yet built",
            },
            PerfRow {
                construct:
                    "compile-don't-interpret (specialize per content-addressed contract hash)",
                status: PerfStatus::DeclaredP1,
                note: "compile-don't-interpret, specialized per content-addressed contract \
                       hash — an advanced lever declared out of the current scope, not yet \
                       built",
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
# semi-naive + stratified negation + index selection — and magic-sets) are NOT
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
                (RDF_TYPE.to_owned(), format!("<{}>", gmeow("PerfLedger"))),
                (gmeow("entryCount"), self.rows.len().to_string()),
                (
                    RDFS_COMMENT.to_owned(),
                    "\"the deferred / non-incremental parts of the native physical engine's \
                     seven-lever stack; the built P0 levers are not rows here\"@en"
                        .to_owned(),
                ),
            ],
        ));

        out.push_str("\n# --- deferred / non-incremental entries ---\n");
        for (index, row) in self.rows.iter().enumerate() {
            out.push_str(&emit_resource(
                &gmeow(&format!("perf-entry-{index}")),
                &[
                    (
                        RDF_TYPE.to_owned(),
                        format!("<{}>", gmeow("PerfLedgerEntry")),
                    ),
                    (
                        gmeow("construct"),
                        format!("\"{}\"@en", escape_literal(row.construct)),
                    ),
                    (
                        gmeow("perfStatus"),
                        format!("<{}{}>", GMEOW_NS, row.status.iri_local()),
                    ),
                    (
                        gmeow("note"),
                        format!("\"{}\"@en", escape_literal(row.note)),
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

    /// The ledger carries exactly the six canonical rows, three of each status,
    /// in the fixed `flagged-non-incremental`-then-`declared-p1` order.
    #[test]
    fn ledger_has_six_rows_three_of_each_status() {
        let ledger = perf_ledger();
        assert_eq!(ledger.rows.len(), 6, "exactly six deferred rows");
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
        assert_eq!(flagged, 3, "three flagged-non-incremental hard parts");
        assert_eq!(p1, 3, "three declared-p1 advanced levers");
        // Fixed order: the three hard parts precede the three levers.
        assert!(
            ledger.rows[..3]
                .iter()
                .all(|r| r.status == PerfStatus::FlaggedNonIncremental),
            "the first three rows are the flagged-non-incremental hard parts"
        );
        assert!(
            ledger.rows[3..]
                .iter()
                .all(|r| r.status == PerfStatus::DeclaredP1),
            "the last three rows are the declared-p1 levers"
        );
    }

    /// The emitted Turtle pins the exact canonical content: the banner, the two
    /// status individuals, all three hard parts, and all three P1 levers. This is
    /// the in-crate golden.
    #[test]
    fn turtle_golden_pins_the_six_rows_and_two_statuses() {
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
        assert!(ttl.contains("gmeow/entryCount> 6"));

        // The three FlaggedNonIncremental hard parts (canon wording).
        for construct in [
            "incremental well-founded / stable-model semantics",
            "existential-rule chase with termination and incrementality together",
            "paraconsistent / modal facets",
        ] {
            assert!(
                ttl.contains(construct),
                "the flagged-non-incremental hard part must appear verbatim: {construct}"
            );
        }
        // The three DeclaredP1 advanced levers (canon wording).
        for lever in [
            "worst-case-optimal joins",
            "provenance semirings",
            "compile-don't-interpret (specialize per content-addressed contract hash)",
        ] {
            assert!(
                ttl.contains(lever),
                "the declared-p1 advanced lever must appear verbatim: {lever}"
            );
        }

        // Both status individuals are emitted as objects.
        assert!(ttl.contains(
            "gmeow/perfStatus> <https://blackcatinformatics.ca/gmeow/FlaggedNonIncremental>"
        ));
        assert!(ttl.contains("gmeow/perfStatus> <https://blackcatinformatics.ca/gmeow/DeclaredP1>"));

        // Six entries of type gmeow:PerfLedgerEntry.
        assert_eq!(
            ttl.matches("#type> <https://blackcatinformatics.ca/gmeow/PerfLedgerEntry>")
                .count(),
            6,
            "exactly six PerfLedgerEntry rows are emitted"
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
}
