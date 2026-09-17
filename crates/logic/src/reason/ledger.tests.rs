// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn dl_gap_rows_and_tally() {
    let gaps = vec![DlGap::new("reason.dl-gap.complementOf", "msg")];
    let rows = dl_gap_rows(&gaps);
    assert_eq!(rows.len(), 1, "one DlGap row per gap");
    assert_eq!(rows[0].kind, DivergenceKind::DlGap);
    assert!(
        rows[0].detail.contains("complementOf"),
        "detail must mention complementOf: {:?}",
        rows[0].detail
    );

    let ledger = build_ledger(Vec::new(), rows, Vec::new());
    assert_eq!(ledger.dl_gap, 1, "build_ledger tallies dl_gap == 1");
}

#[test]
fn existential_gap_rows_are_counted_dlgaps_scoped_out_of_crosscheck() {
    // A refused existential program's weak-acyclicity violation becomes a counted
    // DlGap row carrying the violation evidence verbatim.
    let violations = vec![
        "weak-acyclicity: existential edge p[O|<http://ex/D>] -> p[O|<http://ex/D>] \
             lies in a cycle (the restricted chase may not terminate)"
            .to_owned(),
    ];
    let rows = existential_gap_rows(&violations);
    assert_eq!(rows.len(), 1, "one DlGap row per violation");
    assert_eq!(rows[0].kind, DivergenceKind::DlGap);
    assert_eq!(rows[0].category, EXISTENTIAL_CHASE_CATEGORY);
    assert!(
        rows[0].detail.contains("lies in a cycle"),
        "the violation evidence rides verbatim in detail: {:?}",
        rows[0].detail
    );

    // Routed into a ledger they ARE counted as gaps (the counted divergence ledger),
    // so `enforce` fails on them — the capability-gap is enforced, not dropped.
    let ledger = build_ledger(Vec::new(), rows, Vec::new());
    assert_eq!(ledger.dl_gap, 1, "build_ledger tallies the existential gap");
    assert!(
        !enforce(&ledger).passed,
        "a counted existential capability-gap must fail the strict verdict"
    );

    // …yet the category is DISJOINT from every DL/EL crosscheck category, so the
    // committed crosscheck corpus (which sources its gaps from unsupported constructs)
    // never counts these rows against its gapCount==0 gate.
    assert_ne!(EXISTENTIAL_CHASE_CATEGORY, "consistency");
    assert_ne!(EXISTENTIAL_CHASE_CATEGORY, "subsumption");
    assert_ne!(EXISTENTIAL_CHASE_CATEGORY, "external-corpus");
}

#[test]
fn certified_program_has_no_existential_gap_rows() {
    // No violations ⇒ no gap rows: a certified (WeaklyAcyclic) program is not a gap.
    assert!(existential_gap_rows(&[]).is_empty());
}

// ── enforce (the strict native⊇reference decision criterion 3) ──────────

#[test]
fn enforce_fails_on_dl_gap_alone() {
    // A DlGap is a native coverage defect and fails the strict verdict.
    let gaps = dl_gap_rows(&[DlGap::new("reason.dl-gap.complementOf", "beyond EL")]);
    let ledger = build_ledger(Vec::new(), gaps, Vec::new());
    assert_eq!(ledger.dl_gap, 1);
    let verdict = enforce(&ledger);
    assert!(!verdict.passed, "a DlGap alone must fail");
    assert!(
        verdict.reasons.iter().any(|r| r.contains("coverage gap")),
        "reason names the native DL coverage gap: {verdict:?}"
    );
}

// ── external-corpus grading (CorpusOnly vs DlGap disjointness) ──────────────

fn cmp(case: &str, world: &str, native: &str, published: &str) -> ExternalComparison {
    ExternalComparison {
        case: case.to_owned(),
        world: world.to_owned(),
        native: native.to_owned(),
        published: published.to_owned(),
    }
}

#[test]
fn external_agreement_is_agree_not_corpus_only() {
    let rows = compare_external_corpus(
        "w3c-owl2-el",
        &[cmp("consistency/open", "w", "consistent", "consistent")],
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, DivergenceKind::Agree);
    let ledger = build_ledger(Vec::new(), Vec::new(), rows);
    assert_eq!(ledger.corpus_only, 0);
    assert!(enforce(&ledger).passed, "pure external agreement passes");
}

#[test]
fn external_wrong_decided_answer_is_corpus_only() {
    // native DECIDED consistent, but the corpus published inconsistent.
    let rows = compare_external_corpus(
        "w3c-owl2-el",
        &[cmp(
            "inconsistency/clash",
            "w",
            "consistent",
            "inconsistent",
        )],
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, DivergenceKind::CorpusOnly);
    // The raw published expected is retained verbatim as provenance.
    assert_eq!(rows[0].object, "inconsistent");
    assert!(rows[0].detail.contains("published expected"));
    let ledger = build_ledger(Vec::new(), Vec::new(), rows);
    assert_eq!(ledger.corpus_only, 1);
    let verdict = enforce(&ledger);
    assert!(!verdict.passed, "a CorpusOnly row must fail the gate");
    assert!(
        verdict.reasons.iter().any(|r| r.contains("corpus-only")),
        "reason names the corpus-only divergence: {verdict:?}"
    );
}

#[test]
fn external_undecidable_is_dl_gap_never_corpus_only() {
    // native COULD NOT decide (incomplete): a coverage gap, never a corpus
    // disagreement — even though native ≠ published.
    let rows = compare_external_corpus(
        "ore-large",
        &[cmp(
            "beyond-el/cardinality",
            "w",
            "incomplete",
            "consistent",
        )],
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].kind,
        DivergenceKind::DlGap,
        "an undecidable case is a DlGap, not a CorpusOnly"
    );
    let ledger = build_ledger(Vec::new(), Vec::new(), rows);
    assert_eq!(
        ledger.corpus_only, 0,
        "no corpus-only row for an undecidable case"
    );
    assert_eq!(ledger.dl_gap, 1);
}

// ── divergence_findings projection (divergence rows ARE gmeow:Findings) ─────

#[test]
fn divergence_findings_fold_agree_as_corroboration_and_carry_kind_and_provenance() {
    use gmeow_errors::FindingCategory;
    let external = compare_external_corpus(
        "w3c-owl2-el",
        &[
            cmp("consistency/open", "w", "consistent", "consistent"), // Agree → corroboration
            cmp("clash", "w", "consistent", "inconsistent"),          // CorpusOnly
            cmp("beyond/card", "w", "incomplete", "consistent"),      // DlGap
        ],
    );
    let ledger = build_ledger(Vec::new(), Vec::new(), external);
    let findings = divergence_findings(&ledger);

    // Agreements are now findings too: one corroboration + one corpus-only + one dl-gap.
    assert_eq!(
        findings.len(),
        3,
        "agreement folds as a corroboration finding: {findings:?}"
    );
    assert!(
        findings
            .iter()
            .all(|f| f.tool.as_deref() == Some("conformance"))
    );

    // The agreement is a NON-blocking corroboration finding at the lowest severity —
    // graded Coherent, so it can never gate the lane.
    let agree = findings
        .iter()
        .find(|f| f.code == "reason.divergence.agreement")
        .expect("an agreement (corroboration) finding");
    assert_eq!(agree.severity, Severity::Info);
    assert_eq!(agree.category, Some(FindingCategory::Corroboration));

    // The failing divergences stay at Error severity.
    let corpus = findings
        .iter()
        .find(|f| f.code == "reason.divergence.corpus-only")
        .expect("a corpus-only finding");
    assert_eq!(corpus.severity, Severity::Error);
    // The raw published expected verdict rides verbatim in the message.
    assert!(
        corpus.message.contains("inconsistent"),
        "published expected is carried as provenance: {}",
        corpus.message
    );
    assert!(
        findings
            .iter()
            .any(|f| f.code == "reason.divergence.dl-gap")
    );

    // The gate still passes on the failing set only through the blocking kinds:
    // an all-agree ledger yields only corroboration findings and stays Collected.
    let all_agree = build_ledger(
        Vec::new(),
        Vec::new(),
        compare_external_corpus(
            "w3c-owl2-el",
            &[cmp("consistency/open", "w", "consistent", "consistent")],
        ),
    );
    let agree_findings = divergence_findings(&all_agree);
    assert_eq!(
        agree_findings.len(),
        1,
        "an all-agree ledger still emits its corroboration finding"
    );
    assert!(
        enforce(&all_agree).passed,
        "an all-agree ledger must still pass the gate"
    );
}
