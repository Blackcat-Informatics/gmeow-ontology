// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Divergence ledger over the native reasoner versus a committed reference set.
//!
//! This module compares the native engine's results ([`crate::reason::el`] /
//! [`crate::reason::dl`]) against a committed engine-independent reference (the
//! native DL·EL fragment closure or a frozen external corpus) on **structured
//! tuples, not message bytes**, mirroring the doctrine that comparison happens on
//! the structured shape, never on rendered human strings.
//!
//! It classifies each row as agreeing, a native DL coverage defect, or a
//! disagreement with published external ground truth, and tallies the counts.
//! This Rust module owns the comparison logic and the structured ledger shape.
//!
//! There is no I/O and no TTL emission here — this module produces only the
//! in-memory structured ledger; serialization is a separate concern.

use gmeow_errors::{
    Diag, DiagLedger, Finding, FindingCategory, GateVerdict, Grade, Severity, StageId, Standpoint,
    register_code,
};

use crate::reason::dl::DlGap;

/// How a single compared row relates between the native engine and the reference set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DivergenceKind {
    /// Native and the reference set both derived the row.
    Agree,
    /// A construct the native path did not decide (a coverage defect).
    DlGap,
    /// The native path **decided**, but its verdict disagrees with a published
    /// external corpus's expected result (the W3C OWL 2 / ORE ground truth).
    ///
    /// This is disjoint from [`DivergenceKind::DlGap`]: a case the native path
    /// **cannot decide** (beyond the EL/RL fragment) is a `DlGap`, never a
    /// `CorpusOnly`; only a wrong *decided* answer is a `CorpusOnly`.
    CorpusOnly,
}

/// One classified row of the divergence ledger.
///
/// `subject`/`object`/`world` carry the normalized comparable key (bare IRIs,
/// angle brackets stripped); `category` is `"subsumption"` or `"consistency"`;
/// `detail` is a human-readable English explanation. Positional fields are `""`
/// when not applicable to the row.
#[derive(Debug, Clone)]
pub struct LedgerRow {
    pub kind: DivergenceKind,
    pub category: String,
    pub subject: String,
    pub object: String,
    pub world: String,
    pub detail: String,
}

/// The full in-memory divergence ledger: every classified row plus per-kind tallies.
#[derive(Debug, Clone)]
pub struct DivergenceLedger {
    pub rows: Vec<LedgerRow>,
    pub agree: usize,
    pub dl_gap: usize,
    pub corpus_only: usize,
}

/// The strict native⊇reference cross-check verdict over a [`DivergenceLedger`].
///
/// `passed` is the gate decision; `reasons` is a short, deterministic English
/// list naming each failing category (empty when `passed` is `true`). There is
/// **no severity knob** (ETHOS §5/§19): any `DlGap` or `CorpusOnly` row fails the
/// lane. This is the single authority for the decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerVerdict {
    pub passed: bool,
    pub reasons: Vec<String>,
}

/// Decide the native⊇reference cross-check verdict (the anti-regression **superset**
/// gate criterion 3): the native-decided construct set must cover the committed
/// reference, with no native coverage defect.
///
/// The verdict is `passed` only when the ledger has **zero** `DlGap` and
/// `CorpusOnly` rows:
///
/// * a `DlGap` row is a native coverage defect (a construct the native path did
///   not decide at all);
/// * a `CorpusOnly` row is a disagreement with published external ground truth.
///
/// Each non-zero tally contributes one deterministic English reason.
///
/// The pass/fail decision now flows through the single diagnostics gate morphism:
/// the ledger's rows are interned into a [`gmeow_errors::DiagLedger`]
/// ([`divergence_diag_ledger`]) — agreements as NON-blocking corroboration witnesses
/// that can never gate — and `passed` is `verdict() == Collected` — the same
/// `gate()`/`verdict()` join-fold every other verdict surface reduces to (dogfooding
/// the one gate authority). The `reasons` remain the per-failing-category English
/// counts consumed and printed by the ingest CLI and surfaced by the cross-check.
pub fn enforce(ledger: &DivergenceLedger) -> LedgerVerdict {
    let mut reasons: Vec<String> = Vec::new();
    if ledger.dl_gap > 0 {
        reasons.push(format!(
            "{} native DL coverage gap(s): a construct the native path did not decide",
            ledger.dl_gap
        ));
    }
    if ledger.corpus_only > 0 {
        reasons.push(format!(
            "{} corpus-only row(s): the native path decided a verdict that disagrees with \
             the published external corpus's expected result (native ⊉ external ground truth)",
            ledger.corpus_only
        ));
    }
    // Derive the gate decision from the single gate()/verdict() morphism rather
    // than re-implementing it: the ledger is Collected exactly when it holds no
    // Fatal witness, i.e. no DlGap/CorpusOnly row — identical to `reasons.is_empty()`.
    let passed = divergence_diag_ledger(ledger).verdict() == GateVerdict::Collected;
    LedgerVerdict { passed, reasons }
}

/// Emit one [`DivergenceKind::DlGap`] row per native DL coverage defect.
///
/// Each gap is a construct whose consistency the native check did not decide;
/// the row carries the gap's message and code as its `detail`, with empty
/// positional fields.
pub fn dl_gap_rows(gaps: &[DlGap]) -> Vec<LedgerRow> {
    gaps.iter()
        .map(|gap| LedgerRow {
            kind: DivergenceKind::DlGap,
            category: "consistency".to_owned(),
            subject: String::new(),
            object: String::new(),
            world: String::new(),
            detail: format!("{} [{}]", gap.message, gap.code),
        })
        .collect()
}

/// The ledger category stamped on a native existential-chase capability-gap row.
///
/// Distinct from the DL/EL crosscheck categories (`"subsumption"`, `"consistency"`,
/// `"external-corpus"`) so a refused existential program's gap rows are scoped OUT of the
/// committed DL/EL crosscheck corpus whose gate asserts `gapCount == 0`: that ledger
/// (`crate::reason::artifacts::build_dl_el_ledger_ttl`) reconstructs its gaps from the
/// shared model's unsupported constructs, never from [`existential_gap_rows`].
pub const EXISTENTIAL_CHASE_CATEGORY: &str = "existential-chase";

/// Emit one [`DivergenceKind::DlGap`] row per weak-acyclicity violation of a refused
/// (uncertified, unbudgeted) native existential-chase program.
///
/// The native restricted chase refuses a program it cannot certify terminating — a
/// special existential edge lies in a cycle — rather than loop.  Each such refusal is a
/// native capability-gap (a construct the native path did not decide), so it is exactly a
/// [`DivergenceKind::DlGap`], reusing the counted divergence-ledger kind rather than a
/// parallel one.  Each `violation` string (the offending special-edge-in-cycle evidence,
/// already deterministically sorted by the `ChaseAdmission` certifier) rides verbatim in
/// the row's `detail`.  The rows carry [`EXISTENTIAL_CHASE_CATEGORY`], so
/// [`build_ledger`] / [`enforce`] COUNT them as gaps when a caller routes them into a
/// ledger, while their disjoint category keeps them out of the DL/EL crosscheck corpus.
pub fn existential_gap_rows(violations: &[String]) -> Vec<LedgerRow> {
    violations
        .iter()
        .map(|violation| LedgerRow {
            kind: DivergenceKind::DlGap,
            category: EXISTENTIAL_CHASE_CATEGORY.to_owned(),
            subject: String::new(),
            object: String::new(),
            world: String::new(),
            detail: violation.clone(),
        })
        .collect()
}

/// One native-vs-published verdict comparison for a single external-corpus
/// case/world.
///
/// `native` and `published` are the lowercase verdict tokens
/// (`"consistent"` / `"inconsistent"` / `"incomplete"`): `native` is what the
/// native reasoner decided, `published` is the corpus's frozen expected result.
#[derive(Debug, Clone)]
pub struct ExternalComparison {
    pub case: String,
    pub world: String,
    pub native: String,
    pub published: String,
}

/// Classify native verdicts against an external corpus's published expected
/// verdicts, emitting one [`LedgerRow`] per comparison.
///
/// Disjointness (undecidable ≠ wrong-answer) is enforced here so an unanswerable
/// case can never masquerade as a corpus disagreement:
///
/// * native `"incomplete"` — the native path could NOT decide — yields a
///   [`DivergenceKind::DlGap`] row (a coverage defect), regardless of `published`;
/// * native decided and equal to `published` yields a [`DivergenceKind::Agree`] row;
/// * native decided and different from `published` yields a
///   [`DivergenceKind::CorpusOnly`] row whose `object` carries the published
///   expected verdict verbatim as the raw external provenance.
///
/// `subject` is the case id and `world` the scoped world IRI; `category` is
/// `"external-corpus"`. Rows are emitted in input order (callers pass a
/// deterministically-ordered slice).
pub fn compare_external_corpus(corpus: &str, comparisons: &[ExternalComparison]) -> Vec<LedgerRow> {
    comparisons
        .iter()
        .map(|c| {
            let native = c.native.trim();
            let published = c.published.trim();
            if native == "incomplete" {
                // Undecidable → a native coverage gap, NOT a corpus disagreement.
                LedgerRow {
                    kind: DivergenceKind::DlGap,
                    category: "external-corpus".to_owned(),
                    subject: c.case.clone(),
                    object: published.to_owned(),
                    world: c.world.clone(),
                    detail: format!(
                        "native could not decide {corpus} case {} (world {}); \
                         the published expected verdict is {published}",
                        c.case, c.world
                    ),
                }
            } else if native == published {
                LedgerRow {
                    kind: DivergenceKind::Agree,
                    category: "external-corpus".to_owned(),
                    subject: c.case.clone(),
                    object: published.to_owned(),
                    world: c.world.clone(),
                    detail: format!("native and the {corpus} published expected agree: {native}"),
                }
            } else {
                LedgerRow {
                    kind: DivergenceKind::CorpusOnly,
                    category: "external-corpus".to_owned(),
                    subject: c.case.clone(),
                    object: published.to_owned(),
                    world: c.world.clone(),
                    detail: format!(
                        "native decided {native} but the {corpus} published expected \
                         is {published} for case {} (world {})",
                        c.case, c.world
                    ),
                }
            }
        })
        .collect()
}

/// Assemble the final ledger from the three classified row groups.
///
/// Rows are concatenated in order (consistency, then native DL gaps, then
/// external-corpus divergences) and each [`DivergenceKind`] is tallied into the
/// corresponding count. No I/O.
pub fn build_ledger(
    consistency: Vec<LedgerRow>,
    gaps: Vec<LedgerRow>,
    corpus: Vec<LedgerRow>,
) -> DivergenceLedger {
    let mut rows: Vec<LedgerRow> =
        Vec::with_capacity(consistency.len() + gaps.len() + corpus.len());
    rows.extend(consistency);
    rows.extend(gaps);
    rows.extend(corpus);

    let mut agree = 0usize;
    let mut dl_gap = 0usize;
    let mut corpus_only = 0usize;
    for row in &rows {
        match row.kind {
            DivergenceKind::Agree => agree += 1,
            DivergenceKind::DlGap => dl_gap += 1,
            DivergenceKind::CorpusOnly => corpus_only += 1,
        }
    }

    DivergenceLedger {
        rows,
        agree,
        dl_gap,
        corpus_only,
    }
}

/// The stable kebab code suffix for a divergence kind (the structured signal
/// that feeds the native⊇external coverage gate). `Agree` now carries the
/// `agreement` suffix so a native↔published agreement folds as a NON-blocking
/// corroboration finding (positive evidence), rather than being dropped.
fn divergence_code_suffix(kind: &DivergenceKind) -> Option<&'static str> {
    match kind {
        DivergenceKind::Agree => Some("agreement"),
        DivergenceKind::DlGap => Some("dl-gap"),
        DivergenceKind::CorpusOnly => Some("corpus-only"),
    }
}

/// The [`gmeow_errors::StageId`] every divergence witness is attached under — the
/// conformance divergence producer on the single diagnostics substrate.
const DIVERGENCE_STAGE: &str = "conformance.divergence";

/// The ASCII unit separator (`U+001F`) joining a row's structural distinctness
/// fields into a message-independent fingerprint `focus`. It cannot occur in an
/// IRI, a verdict token, or a category label, so the joined key is unambiguous.
const FOCUS_SEP: &str = "\u{1f}";

/// The diagnostic [`Grade`] a divergence row is interned at, chosen so the ledger's
/// [`gate`](gmeow_errors::gate)/[`verdict`](DiagLedger::verdict) reproduces
/// [`enforce`]'s pass/fail EXACTLY:
///
/// * the FAILING kinds (`DlGap`, `CorpusOnly`) take a BLOCKING
///   category ([`FindingCategory::ContradictionWitness`]) at [`Standpoint::Binding`]
///   and [`Severity::Error`], so each one gates `Fatal` — the gate fails the lane;
/// * `Agree` is a native↔published agreement — the opposite of an incomplete check —
///   so it takes the honest NON-blocking [`FindingCategory::Corroboration`] at the
///   lowest severity ([`Severity::Info`]). The Corroboration category is Coherent, so
///   the corroboration finding stays `Collected` and never gates, whatever its
///   standpoint; it is retained as positive corroborating evidence rather than dropped.
fn divergence_grade(kind: &DivergenceKind) -> Grade {
    let (severity, category) = match kind {
        DivergenceKind::DlGap | DivergenceKind::CorpusOnly => {
            (Severity::Error, FindingCategory::ContradictionWitness)
        }
        DivergenceKind::Agree => (Severity::Info, FindingCategory::Corroboration),
    };
    Grade::new(severity, category, Standpoint::Binding)
}

/// Intern a [`DivergenceLedger`]'s rows into a fresh [`gmeow_errors::DiagLedger`] —
/// one [`Diag`] per row (agreements included, as NON-blocking corroboration
/// findings) — so both the diagnostic PROJECTION ([`divergence_findings`]) and the
/// gate VERDICT ([`enforce`]) flow through the single diagnostics substrate.
///
/// Each witness carries:
///
/// * `code` = `reason.divergence.{suffix}` (the existing [`divergence_code_suffix`]
///   kind the native⊇external coverage gate keys on), registered via
///   [`register_code`];
/// * `message` = the row's `detail`, UNCHANGED — for a `CorpusOnly` row this still
///   carries the native verdict AND the raw published external expected verbatim
///   (the external ground-truth provenance the N-Quads projection depends on);
/// * `grade` = [`divergence_grade`] (blocking for the failing kinds, non-blocking
///   for `Agree`), so [`verdict`](DiagLedger::verdict) reproduces [`enforce`];
/// * `focus` = a message-INDEPENDENT distinctness key over the row's structural
///   fields (`subject`, `object`, `world`, `category`, kind), joined by
///   [`FOCUS_SEP`], so distinct rows never hash-cons-merge and no row is dropped.
///
/// [`DivergenceLedger`]/[`LedgerRow`] remain the structured comparison layer; only
/// the findings projection and the verdict now flow through the [`DiagLedger`].
pub fn divergence_diag_ledger(ledger: &DivergenceLedger) -> DiagLedger {
    let mut diag_ledger = DiagLedger::new();
    let stage = StageId::new(DIVERGENCE_STAGE);
    for row in &ledger.rows {
        let Some(suffix) = divergence_code_suffix(&row.kind) else {
            continue;
        };
        let code = register_code(&format!("reason.divergence.{suffix}"));
        let focus = [
            row.subject.as_str(),
            row.object.as_str(),
            row.world.as_str(),
            row.category.as_str(),
            suffix,
        ]
        .join(FOCUS_SEP);
        let diag =
            Diag::new(code, divergence_grade(&row.kind), row.detail.clone()).with_focus(focus);
        diag_ledger.attach(diag, stage.clone());
    }
    diag_ledger
}

/// Project a [`DivergenceLedger`] into restricted [`gmeow_errors::Finding`]s —
/// one per row, agreements folded as NON-blocking corroboration findings — as the
/// conformance-tool projection of the [`divergence_diag_ledger`] witnesses.
///
/// The diagnostics doctrine declares the native↔oracle / native↔corpus
/// divergence-ledger entries to BE `gmeow:Finding`s (a `gmeow:Observation` whose
/// vantage is the conformance tooling), so this reuses the canonical Finding model
/// rather than minting a parallel vocabulary. Each finding keeps its
/// [`Severity::Error`], its `reason.divergence.{kind}` `code`, its `message` (the
/// row's `detail`), and `tool` = `"conformance"`; it now ADDITIONALLY carries the
/// grade's category + standpoint (additive enrichment from the shared substrate).
pub fn divergence_findings(ledger: &DivergenceLedger) -> Vec<Finding> {
    divergence_diag_ledger(ledger).findings("conformance")
}

#[cfg(test)]
mod tests {
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
}
