// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Emit native↔external-corpus reasoning divergences as a GTS-foldable
//! `gmeow:Finding` graph.
//!
//! When the native reasoner is graded against a published external corpus (the
//! W3C OWL 2 suite / ORE), every native verdict that disagrees with — or cannot
//! reach — the corpus's published expected result is retained as a coverage signal
//! rather than collapsed away. The divergence ledger classifies each comparison
//! ([`gmeow_logic::reason::compare_external_corpus`]) and projects the non-agreeing
//! rows into restricted [`gmeow_diagnostics::Finding`]s
//! ([`gmeow_logic::reason::divergence_findings`]) — the diagnostics doctrine holds
//! that divergence-ledger entries ARE `gmeow:Finding`s, so this reuses that model
//! rather than minting a parallel vocabulary.
//!
//! The findings are emitted as N-Quads in a dedicated named graph
//! ([`CONFORMANCE_GRAPH`]) so they can be folded into a `gmeow.gts` evidence bundle
//! alongside, but distinct from, the `graph/diagnostics` validation findings. The
//! emitter dogfoods the single diagnostics projection
//! ([`gmeow_diagnostics::render::to_gmeow_rdf_in_graph`]): content-addressed finding
//! IRIs, no blank nodes, deterministic order — fold-stable through GTS.

use gmeow_diagnostics::render::to_gmeow_rdf_in_graph;
use gmeow_diagnostics::Report;
use gmeow_logic::reason::{
    build_ledger, compare_external_corpus, divergence_findings, ExternalComparison,
};

/// The named graph the conformance divergence findings ride in — a sibling of
/// `graph/diagnostics`, separating reasoner-correctness evidence (the native-superset
/// coverage-gate signal) from validation/lint findings.
pub const CONFORMANCE_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/conformance";

/// Grade native verdicts against an external corpus's published expected verdicts
/// and emit the divergences as a `gmeow:Finding` N-Quads graph.
///
/// Returns the empty string when every comparison agrees (the Lane-A invariant);
/// each disagreement (`CorpusOnly`) or undecidable case (`DlGap`) becomes one
/// `gmeow:Finding` in [`CONFORMANCE_GRAPH`], carrying the raw published expected
/// verdict as provenance. The output is deterministic and GTS-fold-stable.
pub fn emit_divergence_nq(corpus: &str, comparisons: &[ExternalComparison]) -> String {
    let rows = compare_external_corpus(corpus, comparisons);
    let ledger = build_ledger(Vec::new(), Vec::new(), Vec::new(), rows);
    let findings = divergence_findings(&ledger);

    let mut report = Report::new("conformance");
    for finding in findings {
        report.add_finding(finding);
    }
    to_gmeow_rdf_in_graph(&report, CONFORMANCE_GRAPH)
}

/// One corpus's aggregate native↔published agreement tally: the per-kind counts the
/// divergence ledger records, plus the case total.
///
/// This is the aggregate sibling of [`emit_divergence_nq`]: where that emitter drops
/// agreements and keeps only the divergent rows as findings, a tally KEEPS the agree
/// count — an all-agree corpus still produces a full tally (agree == cases). It is
/// the raw input the benchmark dashboard projects into a per-corpus pass rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgreementTally {
    /// The corpus name (the `cases/external/<corpus>/` directory).
    pub corpus: String,
    /// Total graded native↔published comparisons in the corpus.
    pub cases: usize,
    /// Comparisons where the native verdict matched the published expected.
    pub agree: usize,
    /// Comparisons the native path DECIDED but disagreed with the published expected.
    pub corpus_only: usize,
    /// Comparisons the native path could not decide (an honest coverage gap).
    pub dl_gap: usize,
}

/// Classify one corpus's graded comparisons into an [`AgreementTally`].
///
/// A pure classification over the already-grouped comparisons (no disk walk): it runs
/// the same [`compare_external_corpus`] ledger the divergence emitter uses, so the two
/// projections agree by construction. Frozen external grading yields only Agree /
/// CorpusOnly / DlGap rows, so `agree + corpus_only + dl_gap == cases`.
pub fn agreement_tally(corpus: &str, comparisons: &[ExternalComparison]) -> AgreementTally {
    let rows = compare_external_corpus(corpus, comparisons);
    let ledger = build_ledger(Vec::new(), Vec::new(), Vec::new(), rows);
    AgreementTally {
        corpus: corpus.to_string(),
        cases: comparisons.len(),
        agree: ledger.agree,
        corpus_only: ledger.corpus_only,
        dl_gap: ledger.dl_gap,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmp(case: &str, world: &str, native: &str, published: &str) -> ExternalComparison {
        ExternalComparison {
            case: case.to_owned(),
            world: world.to_owned(),
            native: native.to_owned(),
            published: published.to_owned(),
        }
    }

    #[test]
    fn all_agree_emits_nothing() {
        let nq = emit_divergence_nq(
            "w3c-owl2-el",
            &[cmp("consistency/open", "w", "consistent", "consistent")],
        );
        assert!(
            nq.is_empty(),
            "an all-agree run has no divergence graph: {nq:?}"
        );
    }

    #[test]
    fn tally_counts_agree_corpus_only_and_dl_gap() {
        // The aggregate tally keeps ALL three kinds (unlike the findings emitter,
        // which drops agrees): one Agree, one CorpusOnly (decided-but-wrong), one
        // DlGap (undecidable). cases == agree + corpus_only + dl_gap.
        let tally = agreement_tally(
            "w3c-owl2-el",
            &[
                cmp("consistency/open", "w", "consistent", "consistent"), // Agree
                cmp("inconsistency/clash", "w", "consistent", "inconsistent"), // CorpusOnly
                cmp("beyond-el/cardinality", "w", "incomplete", "consistent"), // DlGap
            ],
        );
        assert_eq!(tally.corpus, "w3c-owl2-el");
        assert_eq!(tally.cases, 3);
        assert_eq!(tally.agree, 1);
        assert_eq!(tally.corpus_only, 1);
        assert_eq!(tally.dl_gap, 1);
    }

    #[test]
    fn all_agree_corpus_still_yields_a_full_tally() {
        // An all-agree corpus emits no divergence graph but MUST still tally (agree ==
        // cases) — else the dashboard would silently drop a 100%-agreeing corpus.
        let tally = agreement_tally(
            "tptp-mini",
            &[
                cmp("theorem-a", "w", "inconsistent", "inconsistent"),
                cmp("theorem-b", "w", "consistent", "consistent"),
            ],
        );
        assert_eq!(tally.cases, 2);
        assert_eq!(tally.agree, 2);
        assert_eq!(tally.corpus_only, 0);
        assert_eq!(tally.dl_gap, 0);
    }

    #[test]
    fn wrong_answer_and_undecidable_emit_two_findings() {
        let nq = emit_divergence_nq(
            "w3c-owl2-el",
            &[
                cmp("consistency/open", "w", "consistent", "consistent"), // Agree → no row
                cmp("inconsistency/clash", "w", "consistent", "inconsistent"), // CorpusOnly
                cmp("beyond-el/cardinality", "w", "incomplete", "consistent"), // DlGap
            ],
        );

        // Every emitted quad lands in the conformance graph, never diagnostics.
        let lines: Vec<&str> = nq.lines().collect();
        assert!(!lines.is_empty(), "divergences must emit");
        for line in &lines {
            assert!(
                line.ends_with(&format!("<{CONFORMANCE_GRAPH}> .")),
                "line not in the conformance graph: {line}"
            );
        }

        // Exactly two findings (one CorpusOnly + one DlGap), typed gmeow:Finding.
        let finding_types = lines.iter().filter(|l| l.contains("/Finding>")).count();
        assert_eq!(finding_types, 2, "two findings (CorpusOnly + DlGap)");

        // The structured divergence kinds the native⊇external coverage gate keys on are present.
        assert!(nq.contains("reason.divergence.corpus-only"));
        assert!(nq.contains("reason.divergence.dl-gap"));
        // The raw published expected verdict rides verbatim as provenance.
        assert!(
            nq.contains("published expected is inconsistent"),
            "corpus-only finding carries the published expected: {nq}"
        );

        // Deterministic.
        assert_eq!(
            nq,
            emit_divergence_nq(
                "w3c-owl2-el",
                &[
                    cmp("consistency/open", "w", "consistent", "consistent"),
                    cmp("inconsistency/clash", "w", "consistent", "inconsistent"),
                    cmp("beyond-el/cardinality", "w", "incomplete", "consistent"),
                ],
            )
        );
    }
}
