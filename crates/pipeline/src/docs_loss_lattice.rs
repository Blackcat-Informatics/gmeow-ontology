// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The A3 documentation-loss-lattice gate.
//!
//! Asserts, over the SINGLE honest per-format capability source
//! ([`gmeow_docs::formats`] — the same table the print PDF's loss appendix and
//! external-projection loss ledger both read), two falsifiable invariants:
//!
//!   1. **Totality** — every [`Capability`] is, for every [`DocFormat`], in exactly one
//!      of the format's `representable` / `dropped` partitions (representable XOR
//!      dropped). A capability that is neither, or both, reds the gate.
//!   2. **Monotonicity along the projection DAG** — the dropped-capability sets grow
//!      monotonically along the DAG's covering edges
//!      ([`gmeow_docs::formats::PROJECTION_DAG_EDGES`]): for each `src → tgt`,
//!      `dropped(src) ⊆ dropped(tgt)`. The DAG is `canonical → body-set → {site →
//!      snippets, mdbook, pdf}`, NOT a linear chain — mdbook and pdf are incomparable
//!      siblings off the body-set (mdbook packs the live engines the pdf never
//!      carries), so no `site → mdbook` / `mdbook → pdf` edge exists to check. Nothing
//!      a format drops is regained by the strictly-poorer format that refines it.
//!
//! This gate lives in the pipeline crate (not `crates/validate`) because it reads
//! `gmeow_docs::formats`, and `gmeow-docs` already depends on `gmeow-validate`; a
//! `validate → docs` edge would cycle the acyclic crate DAG the crate-layering gate
//! enforces. It is wired into the `crate-check` gate surface (`make check`) alongside
//! the crate-layering / repo-static static gates.

use std::collections::BTreeSet;

use gmeow_docs::formats::{Capability, DocFormat, PROJECTION_DAG_EDGES, format_capabilities};
use gmeow_errors::{
    Diag, DiagLedger, FindingCategory, GateVerdict, Grade, Location, Severity, StageId, Standpoint,
    register_code,
};

/// The original graded witnesses of the documentation capability checks.
#[derive(Debug, Clone, Default)]
pub struct DocsLossLatticeReport {
    ledger: DiagLedger,
}

impl DocsLossLatticeReport {
    /// The gate passes iff no invariant was violated.
    pub fn ok(&self) -> bool {
        self.ledger.verdict() == GateVerdict::Collected
    }

    pub fn ledger(&self) -> &DiagLedger {
        &self.ledger
    }

    #[track_caller]
    fn violation(&mut self, identity: &[&str], message: String) {
        self.ledger.attach(
            Diag::new(
                register_code("docs-loss-lattice-violation"),
                Grade::new(
                    Severity::Error,
                    FindingCategory::ModelingDisciplineViolation,
                    Standpoint::Binding,
                ),
                message,
            )
            .with_location(Location::new(
                Some("crates/docs-model/src/formats.rs".to_owned()),
                None,
                None,
                None,
            ))
            .with_focus(serde_json::to_string(identity).expect("capability identities serialize")),
            StageId::new("docs.loss-lattice"),
        );
    }
}

/// Run the docs-loss-lattice gate over [`gmeow_docs::formats`].
pub fn check_docs_loss_lattice() -> DocsLossLatticeReport {
    check_capabilities(format_capabilities)
}

fn check_capabilities(
    capabilities: impl Fn(DocFormat) -> gmeow_docs::formats::SurfaceCapabilities,
) -> DocsLossLatticeReport {
    let mut report = DocsLossLatticeReport::default();

    // Invariant 1: total, disjoint partition per format.
    for fmt in DocFormat::ALL {
        let caps = capabilities(fmt);
        let representable: BTreeSet<Capability> = caps.representable.iter().copied().collect();
        let dropped: BTreeSet<Capability> = caps.dropped.iter().copied().collect();
        for cap in Capability::ALL {
            let in_repr = representable.contains(&cap);
            let in_drop = dropped.contains(&cap);
            if in_repr && in_drop {
                report.violation(
                    &["partition-overlap", fmt.slug(), cap.slug()],
                    format!(
                        "docs format '{}' lists capability '{}' as BOTH representable and dropped",
                        fmt.slug(),
                        cap.slug(),
                    ),
                );
            } else if !in_repr && !in_drop {
                report.violation(&["partition-missing", fmt.slug(), cap.slug()], format!(
                    "docs format '{}' lists capability '{}' as NEITHER representable nor dropped",
                    fmt.slug(),
                    cap.slug(),
                ));
            }
        }
        if caps.representable.len() + caps.dropped.len() != Capability::ALL.len() {
            report.violation(
                &["partition-size", fmt.slug()],
                format!(
                    "docs format '{}' partition size {} + {} != {} capabilities",
                    fmt.slug(),
                    caps.representable.len(),
                    caps.dropped.len(),
                    Capability::ALL.len(),
                ),
            );
        }
    }

    // Invariant 2: monotone dropped sets along the projection DAG's covering edges
    // (NOT a linear `DocFormat::ALL` chain — mdbook and pdf are incomparable siblings).
    // For each declared `src → tgt` refinement edge, the target must drop a superset.
    let dropped_of = |fmt: DocFormat| -> BTreeSet<Capability> {
        capabilities(fmt).dropped.into_iter().collect()
    };
    for &(src, tgt) in PROJECTION_DAG_EDGES {
        let src_dropped = dropped_of(src);
        let tgt_dropped = dropped_of(tgt);
        if !src_dropped.is_subset(&tgt_dropped) {
            let regained: Vec<&str> = src_dropped
                .difference(&tgt_dropped)
                .map(|c| c.slug())
                .collect();
            report.violation(&["monotonicity", src.slug(), tgt.slug()], format!(
                "docs loss DAG is not monotone: refinement target '{}' regains capabilit(y/ies) \
                 [{}] that its source '{}' drops",
                tgt.slug(),
                regained.join(", "),
                src.slug(),
            ));
        }
    }

    report
}

#[path = "docs_loss_lattice.tests.rs"]
#[cfg(test)]
mod tests;
