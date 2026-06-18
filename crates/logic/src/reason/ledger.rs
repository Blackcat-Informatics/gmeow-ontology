// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Report-only divergence ledger over the native reasoner versus classic oracles.
//!
//! This module compares the native engine's results ([`crate::reason::el`] for EL
//! subsumption, [`crate::reason::dl`] for DL consistency) against the classic
//! oracles (ELK for subsumption, HermiT for consistency) on **structured tuples,
//! not message bytes** — mirroring the #578 doctrine that comparison happens on
//! the structured shape, never on rendered human strings.
//!
//! It is strictly **report-only**: it classifies each tuple as agreeing, native-
//! only, oracle-only, or a known DL gap, and tallies the counts. It never decides
//! pass/fail and never enforces anything on divergence — *enforcing on divergence
//! is deferred to #666*. The Docker oracles themselves run in Python (Task 4); this
//! Rust module owns ONLY the comparison logic and the structured ledger shape.
//!
//! There is no I/O and no TTL emission here — serialization is the job of a later
//! task. This module produces only the in-memory structured ledger.

use gmeow_rdf::RdfLoss;
use std::collections::BTreeSet;

/// How a single compared tuple relates between the native engine and an oracle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DivergenceKind {
    /// Native and the oracle both derived the tuple.
    Agree,
    /// Derived natively but not by the oracle.
    NativeOnly,
    /// Derived by the oracle but not natively.
    OracleOnly,
    /// A construct the native encoding cannot decide (a known DL gap).
    DlGap,
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
    pub native_only: usize,
    pub oracle_only: usize,
    pub dl_gap: usize,
}

/// Strip a single surrounding pair of angle brackets from `s`.
///
/// The native [`crate::reason::InferredAxiom`] object comes through as a Nemo
/// display form (`<iri>`); ELK tuples arrive as bare IRIs. Normalizing both by
/// trimming one leading `<` and one trailing `>` lets the two forms compare equal.
fn unbracket(s: &str) -> String {
    s.strip_prefix('<')
        .and_then(|inner| inner.strip_suffix('>'))
        .unwrap_or(s)
        .to_owned()
}

/// Canonicalize a subsumption tuple into a comparable `(subject, object, world)`
/// key of owned `String`s, stripping any surrounding angle brackets from each
/// component so native display forms and bare-IRI oracle forms compare equal.
fn normalize_key(tuple: &(String, String, String)) -> (String, String, String) {
    (
        unbracket(&tuple.0),
        unbracket(&tuple.1),
        unbracket(&tuple.2),
    )
}

/// Compare native and ELK subsumption tuples, emitting one classified row per key.
///
/// Both inputs are `(subject, object, world)` tuples; they are normalized (angle
/// brackets stripped) and collected into [`BTreeSet`]s so the result is
/// deterministic. The intersection yields [`DivergenceKind::Agree`] rows, native
/// ∖ ELK yields [`DivergenceKind::NativeOnly`], and ELK ∖ native yields
/// [`DivergenceKind::OracleOnly`]. Rows are emitted in sorted-key order.
pub fn compare_subsumption(
    native: &[(String, String, String)],
    elk: &[(String, String, String)],
) -> Vec<LedgerRow> {
    let native_keys: BTreeSet<(String, String, String)> =
        native.iter().map(normalize_key).collect();
    let elk_keys: BTreeSet<(String, String, String)> = elk.iter().map(normalize_key).collect();

    let mut rows: Vec<LedgerRow> = Vec::new();

    // Agree: the intersection. Iterating one sorted set and testing membership
    // preserves sorted-key order.
    for key in native_keys.intersection(&elk_keys) {
        let (subject, object, world) = key.clone();
        rows.push(LedgerRow {
            kind: DivergenceKind::Agree,
            category: "subsumption".to_owned(),
            detail: format!("native and ELK agree: {subject} ⊑ {object}"),
            subject,
            object,
            world,
        });
    }

    // NativeOnly: native ∖ ELK.
    for key in native_keys.difference(&elk_keys) {
        let (subject, object, world) = key.clone();
        rows.push(LedgerRow {
            kind: DivergenceKind::NativeOnly,
            category: "subsumption".to_owned(),
            detail: format!("derived natively but not by ELK: {subject} ⊑ {object}"),
            subject,
            object,
            world,
        });
    }

    // OracleOnly: ELK ∖ native.
    for key in elk_keys.difference(&native_keys) {
        let (subject, object, world) = key.clone();
        rows.push(LedgerRow {
            kind: DivergenceKind::OracleOnly,
            category: "subsumption".to_owned(),
            detail: format!("derived by ELK but not natively: {subject} ⊑ {object}"),
            subject,
            object,
            world,
        });
    }

    rows
}

/// Render a boolean consistency verdict as its English token.
fn verdict_token(consistent: bool) -> &'static str {
    if consistent {
        "consistent"
    } else {
        "inconsistent"
    }
}

/// Compare native and HermiT consistency verdicts (and their unsat class sets).
///
/// If `hermit_consistent` is `None` the oracle was not run: a single note row is
/// emitted (classified [`DivergenceKind::OracleOnly`]) recording that only the
/// native verdict is known, with `object` set to the native verdict token.
///
/// If `Some(h)`, one [`DivergenceKind::Agree`] row is emitted when the verdicts
/// match; otherwise the disagreement is recorded as a [`DivergenceKind::NativeOnly`]
/// row (native verdict) plus a [`DivergenceKind::OracleOnly`] row (HermiT verdict).
///
/// The two unsatisfiable-class sets are then compared like subsumption keys:
/// intersection ⇒ `Agree`, native ∖ hermit ⇒ `NativeOnly`, hermit ∖ native ⇒
/// `OracleOnly`, each `object` being `"owl:Nothing"`. All output is deterministic.
pub fn compare_consistency(
    native_consistent: bool,
    native_unsat: &[String],
    hermit_consistent: Option<bool>,
    hermit_unsat: &[String],
) -> Vec<LedgerRow> {
    let mut rows: Vec<LedgerRow> = Vec::new();
    let native_token = verdict_token(native_consistent).to_owned();

    match hermit_consistent {
        None => {
            rows.push(LedgerRow {
                kind: DivergenceKind::OracleOnly,
                category: "consistency".to_owned(),
                subject: String::new(),
                object: native_token.clone(),
                world: String::new(),
                detail: format!(
                    "HermiT was not run; only the native verdict is recorded: {native_token}"
                ),
            });
        }
        Some(h) => {
            if native_consistent == h {
                rows.push(LedgerRow {
                    kind: DivergenceKind::Agree,
                    category: "consistency".to_owned(),
                    subject: String::new(),
                    object: native_token.clone(),
                    world: String::new(),
                    detail: format!("native and HermiT agree: ontology is {native_token}"),
                });
            } else {
                let hermit_token = verdict_token(h).to_owned();
                rows.push(LedgerRow {
                    kind: DivergenceKind::NativeOnly,
                    category: "consistency".to_owned(),
                    subject: String::new(),
                    object: native_token.clone(),
                    world: String::new(),
                    detail: format!("native says {native_token} but HermiT says {hermit_token}"),
                });
                rows.push(LedgerRow {
                    kind: DivergenceKind::OracleOnly,
                    category: "consistency".to_owned(),
                    subject: String::new(),
                    object: hermit_token.clone(),
                    world: String::new(),
                    detail: format!("HermiT says {hermit_token} but native says {native_token}"),
                });
            }
        }
    }

    // Compare the unsatisfiable-class sets, normalized like subsumption.
    let native_unsat_keys: BTreeSet<String> = native_unsat.iter().map(|c| unbracket(c)).collect();
    let hermit_unsat_keys: BTreeSet<String> = hermit_unsat.iter().map(|c| unbracket(c)).collect();

    for class in native_unsat_keys.intersection(&hermit_unsat_keys) {
        rows.push(LedgerRow {
            kind: DivergenceKind::Agree,
            category: "consistency".to_owned(),
            subject: class.clone(),
            object: "owl:Nothing".to_owned(),
            world: String::new(),
            detail: format!("native and HermiT agree: {class} is unsatisfiable"),
        });
    }
    for class in native_unsat_keys.difference(&hermit_unsat_keys) {
        rows.push(LedgerRow {
            kind: DivergenceKind::NativeOnly,
            category: "consistency".to_owned(),
            subject: class.clone(),
            object: "owl:Nothing".to_owned(),
            world: String::new(),
            detail: format!("{class} is unsatisfiable natively but not per HermiT"),
        });
    }
    for class in hermit_unsat_keys.difference(&native_unsat_keys) {
        rows.push(LedgerRow {
            kind: DivergenceKind::OracleOnly,
            category: "consistency".to_owned(),
            subject: class.clone(),
            object: "owl:Nothing".to_owned(),
            world: String::new(),
            detail: format!("{class} is unsatisfiable per HermiT but not natively"),
        });
    }

    rows
}

/// Emit one [`DivergenceKind::DlGap`] row per native DL gap.
///
/// Each gap is a known limitation of the predicate-as-symbol encoding (a construct
/// whose consistency the native check does not decide); the row carries the gap's
/// message and code as its `detail`, with empty positional fields.
pub fn dl_gap_rows(gaps: &[RdfLoss]) -> Vec<LedgerRow> {
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

/// Assemble the final ledger from the three classified row groups.
///
/// Rows are concatenated in order (subsumption, then consistency, then gaps) and
/// each [`DivergenceKind`] is tallied into the corresponding count. No I/O.
pub fn build_ledger(
    subsumption: Vec<LedgerRow>,
    consistency: Vec<LedgerRow>,
    gaps: Vec<LedgerRow>,
) -> DivergenceLedger {
    let mut rows: Vec<LedgerRow> =
        Vec::with_capacity(subsumption.len() + consistency.len() + gaps.len());
    rows.extend(subsumption);
    rows.extend(consistency);
    rows.extend(gaps);

    let mut agree = 0usize;
    let mut native_only = 0usize;
    let mut oracle_only = 0usize;
    let mut dl_gap = 0usize;
    for row in &rows {
        match row.kind {
            DivergenceKind::Agree => agree += 1,
            DivergenceKind::NativeOnly => native_only += 1,
            DivergenceKind::OracleOnly => oracle_only += 1,
            DivergenceKind::DlGap => dl_gap += 1,
        }
    }

    DivergenceLedger {
        rows,
        agree,
        native_only,
        oracle_only,
        dl_gap,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str, o: &str, w: &str) -> (String, String, String) {
        (s.to_owned(), o.to_owned(), w.to_owned())
    }

    const A: &str = "http://ex/A";
    const B: &str = "http://ex/B";
    const C: &str = "http://ex/C";
    const W: &str = "http://ex/w";

    #[test]
    fn identical_lists_all_agree() {
        let native = vec![t(A, B, W), t(B, C, W)];
        let elk = vec![t(A, B, W), t(B, C, W)];
        let rows = compare_subsumption(&native, &elk);
        assert!(
            rows.iter().all(|r| r.kind == DivergenceKind::Agree),
            "identical lists must all be Agree: {rows:?}"
        );
        let ledger = build_ledger(rows, Vec::new(), Vec::new());
        assert_eq!(ledger.native_only, 0, "no native-only rows");
        assert_eq!(ledger.oracle_only, 0, "no oracle-only rows");
        assert_eq!(ledger.agree, 2, "both tuples agree");
    }

    #[test]
    fn native_extra_tuple_is_native_only() {
        let native = vec![t(A, B, W), t(B, C, W)];
        let elk = vec![t(A, B, W)];
        let rows = compare_subsumption(&native, &elk);
        let native_only: Vec<&LedgerRow> = rows
            .iter()
            .filter(|r| r.kind == DivergenceKind::NativeOnly)
            .collect();
        assert_eq!(native_only.len(), 1, "exactly one NativeOnly row: {rows:?}");
        assert_eq!(native_only[0].subject, B);
        assert_eq!(native_only[0].object, C);
    }

    #[test]
    fn elk_extra_tuple_is_oracle_only() {
        let native = vec![t(A, B, W)];
        let elk = vec![t(A, B, W), t(B, C, W)];
        let rows = compare_subsumption(&native, &elk);
        let oracle_only: Vec<&LedgerRow> = rows
            .iter()
            .filter(|r| r.kind == DivergenceKind::OracleOnly)
            .collect();
        assert_eq!(oracle_only.len(), 1, "exactly one OracleOnly row: {rows:?}");
        assert_eq!(oracle_only[0].subject, B);
        assert_eq!(oracle_only[0].object, C);
    }

    #[test]
    fn bracket_normalization_makes_them_agree() {
        // Native carries angle-bracketed display forms; ELK carries bare IRIs.
        let native = vec![t("<http://ex/A>", "<http://ex/B>", "<http://ex/w>")];
        let elk = vec![t(A, B, W)];
        let rows = compare_subsumption(&native, &elk);
        assert_eq!(
            rows.len(),
            1,
            "exactly one row, not two divergences: {rows:?}"
        );
        assert_eq!(
            rows[0].kind,
            DivergenceKind::Agree,
            "bracket forms must normalize to Agree"
        );
        assert_eq!(rows[0].subject, A, "subject normalized to bare IRI");
    }

    #[test]
    fn consistency_agreement_when_both_consistent() {
        let rows = compare_consistency(true, &[], Some(true), &[]);
        assert_eq!(rows.len(), 1, "single agree row: {rows:?}");
        assert_eq!(rows[0].kind, DivergenceKind::Agree);
        assert_eq!(rows[0].object, "consistent");
    }

    #[test]
    fn consistency_disagreement_native_vs_hermit() {
        let rows = compare_consistency(true, &[], Some(false), &[]);
        assert_eq!(rows.len(), 2, "one NativeOnly + one OracleOnly: {rows:?}");
        assert!(
            rows.iter().any(|r| r.kind == DivergenceKind::NativeOnly),
            "native says consistent — a NativeOnly divergence"
        );
        assert!(
            rows.iter().any(|r| r.kind == DivergenceKind::OracleOnly),
            "HermiT says inconsistent — an OracleOnly divergence"
        );
    }

    #[test]
    fn consistency_oracle_not_run_records_native_only() {
        let rows = compare_consistency(false, &[], None, &[]);
        assert_eq!(
            rows.len(),
            1,
            "single note row when HermiT not run: {rows:?}"
        );
        assert_eq!(rows[0].kind, DivergenceKind::OracleOnly);
        assert_eq!(rows[0].object, "inconsistent", "records the native verdict");
        assert!(rows[0].detail.contains("not run"));
    }

    #[test]
    fn dl_gap_rows_and_tally() {
        let gaps = vec![RdfLoss::new("reason.dl-gap.complementOf", "msg")];
        let rows = dl_gap_rows(&gaps);
        assert_eq!(rows.len(), 1, "one DlGap row per gap");
        assert_eq!(rows[0].kind, DivergenceKind::DlGap);
        assert!(
            rows[0].detail.contains("complementOf"),
            "detail must mention complementOf: {:?}",
            rows[0].detail
        );

        let ledger = build_ledger(Vec::new(), Vec::new(), rows);
        assert_eq!(ledger.dl_gap, 1, "build_ledger tallies dl_gap == 1");
    }
}
