// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native↔oracle parity comparator + the committed native-coverage floor.
//!
//! The native execution core ([`crate::physical::seminaive::materialize_native`] forward,
//! [`crate::physical::magic::resolve_native`] backward) is the PRIMARY runtime path; an
//! external engine, consulted through the [`crate::oracle`] seam, is the DEMOTED oracle. This
//! module makes that demotion EXPLICIT and GATED: over a representative corpus of stratifiable
//! binary Datalog± programs it runs native AND the oracle, classifies each derived row / answer
//! into a [`ParityLedger`], and exposes a strict verdict (passing iff zero non-`Agree` rows).
//!
//! # The two parity surfaces (generic over the oracle trait)
//!
//! * **Forward** — [`compare_materialization`] compares the native [`DerivedRow`] set against
//!   the closure materialized by any [`crate::oracle::ForwardOracle`] (Nemo in the gate). Both
//!   engines echo the asserted EDB AND emit the derived closure, so the comparison is on the
//!   full fact set.
//! * **Backward** — [`compare_answers`] compares the native [`AnswerSet`] bindings against the
//!   answers of any [`crate::oracle::BackwardOracle`] (the reference SLD oracle in the gate).
//!
//! Because both comparators take the oracle as a trait object, the divergence-ledger promotion
//! harness works unchanged whichever engine backs the oracle.
//!
//! # What is compared (and what is NOT)
//!
//! The parity gate compares the **derived FACT set** — the `(subject, predicate, object)`
//! triple in its world — NOT the provenance. A multiply-derivable fact may carry a different
//! `derivation_id` / `source_quad_ids` between the native first-wins tiebreak and the Nemo
//! chase; that derivation-id divergence is EXPECTED and is recorded separately (the
//! determinism gate in [`crate::physical::seminaive`] pins native↔reference provenance
//! byte-identity, a distinct concern). At the FACT level the two engines must agree exactly,
//! and any genuine `NativeOnly` / `OracleOnly` triple fails this gate — it is never weakened.
//!
//! # Reuse of the existing divergence-ledger shape
//!
//! This is a SIBLING of [`crate::reason::ledger`] (the EL/DL subsumption comparator), reusing
//! its [`DivergenceKind`] / [`LedgerRow`] / [`LedgerVerdict`] types and its `enforce()`
//! semantics (any non-`Agree` row fails, no severity knob — ETHOS §5/§19). It does not rebuild
//! the subsumption comparator; it adds the materialization-row + answer-set parity classifier
//! the native-vs-oracle execution gate keys on.
//!
//! # Phase dead code
//!
//! The comparator API ([`ParityLedger`], [`compare_materialization`], [`compare_answers`]) is
//! exercised by the gate `#[cfg(test)]` module in this file; it has no non-test caller yet (the
//! gate IS the consumer). Allow `dead_code` module-internally rather than scattering per-item
//! attributes, mirroring the sibling [`crate::physical::seminaive`] / [`crate::physical::magic`]
//! rungs.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use crate::facts::TypedFactSet;
use crate::oracle::{BackwardOracle, ForwardBudget, ForwardOracle, TypedRow};
use crate::provenance::term_display;
use crate::query_ir::{AnswerSet, Budget, QProgram};
use crate::reason::ledger::{DivergenceKind, LedgerRow, LedgerVerdict};
use crate::rule_ir::DerivedRow;
use crate::seam::WorldFactSource;

/// The native↔oracle parity ledger over one corpus program: every classified row plus the
/// per-kind tallies the verdict keys on.
///
/// `NativeOnly` rows are facts/answers the native engine produced that the oracle did not;
/// `OracleOnly` rows are the converse. For the execution-parity gate BOTH are failures (the
/// native engine and its demoted oracle must agree exactly on the binary Datalog± fragment),
/// so the verdict passes iff both tallies are zero.
#[derive(Debug, Clone)]
pub(crate) struct ParityLedger {
    /// Every classified row, in deterministic (sorted-key) order.
    pub(crate) rows: Vec<LedgerRow>,
    /// Count of `Agree` rows (facts/answers both engines produced).
    pub(crate) agree: usize,
    /// Count of `NativeOnly` rows (native produced, oracle did not).
    pub(crate) native_only: usize,
    /// Count of `OracleOnly` rows (oracle produced, native did not).
    pub(crate) oracle_only: usize,
}

impl ParityLedger {
    /// Decide the strict native↔oracle execution-parity verdict.
    ///
    /// `passed` is `true` iff the ledger has ZERO `NativeOnly` and ZERO `OracleOnly` rows —
    /// the native engine and its demoted oracle agree exactly. Each non-zero tally contributes
    /// one deterministic English reason. Mirrors [`crate::reason::ledger::enforce`]'s shape:
    /// no severity knob, any divergence fails.
    pub(crate) fn enforce(&self) -> LedgerVerdict {
        let mut reasons: Vec<String> = Vec::new();
        if self.native_only > 0 {
            reasons.push(format!(
                "{} native-only row(s): the native engine produced a fact/answer the oracle did not",
                self.native_only
            ));
        }
        if self.oracle_only > 0 {
            reasons.push(format!(
                "{} oracle-only row(s): the oracle produced a fact/answer the native engine did not",
                self.oracle_only
            ));
        }
        LedgerVerdict {
            passed: reasons.is_empty(),
            reasons,
        }
    }

    /// Assemble a [`ParityLedger`] from a set of classified rows, tallying each kind.
    ///
    /// Only `Agree` / `NativeOnly` / `OracleOnly` arise on the parity surface; a `DlGap` or
    /// `CorpusOnly` would be a programming error (those belong to the subsumption sibling), so
    /// they are not counted here and would leave the tallies unchanged.
    fn from_rows(rows: Vec<LedgerRow>) -> Self {
        let mut agree = 0usize;
        let mut native_only = 0usize;
        let mut oracle_only = 0usize;
        for row in &rows {
            match row.kind {
                DivergenceKind::Agree => agree += 1,
                DivergenceKind::NativeOnly => native_only += 1,
                DivergenceKind::OracleOnly => oracle_only += 1,
                DivergenceKind::DlGap | DivergenceKind::CorpusOnly => {}
            }
        }
        ParityLedger {
            rows,
            agree,
            native_only,
            oracle_only,
        }
    }
}

/// A comparable `(subject, predicate, object)` fact key. The world is carried on the
/// [`LedgerRow`] separately; for a single-world corpus program the world is constant, so the
/// triple is the discriminating key.
type FactKey = (String, String, String);

/// The fact key of a native [`DerivedRow`]: its `(subject, predicate, object)` N3 surfaces.
fn row_fact_key(row: &DerivedRow) -> FactKey {
    (
        term_display(&row.subject),
        row.predicate.as_str().to_owned(),
        term_display(&row.object),
    )
}

/// The fact key of an arity-3 forward-oracle [`TypedRow`]: `(subject, predicate, object)`.
///
/// A world-scoped quad is a ternary row whose columns are `subject`, `object`, `world`
/// (the relation name is the predicate) — the same coercion `materialize` applies. Only
/// arity-3 rows are quads; a helper-predicate row of any other arity is not a fact-level
/// comparand and yields `None`.
fn typed_row_fact_key(row: &TypedRow) -> Option<FactKey> {
    if row.args.len() != 3 {
        return None;
    }
    Some((
        term_display(&row.args[0]),
        row.predicate.clone(),
        term_display(&row.args[1]),
    ))
}

/// Compare the native [`DerivedRow`] fact set against a [`ForwardOracle`]'s materialized
/// closure, generically over which engine backs the oracle.
///
/// The oracle is run here (over the same typed EDB and rule text the native engine used), and
/// each `(subject, predicate, object)` triple is classified: present in BOTH ⇒
/// [`DivergenceKind::Agree`], native ∖ oracle ⇒ [`DivergenceKind::NativeOnly`], oracle ∖ native
/// ⇒ [`DivergenceKind::OracleOnly`]. Detail strings name the oracle via [`ForwardOracle::name`]
/// so the ledger is engine-agnostic. Rows are emitted in sorted-key order.
///
/// Only the FACT set is compared, NOT provenance: a multiply-derivable fact may legitimately
/// carry a different `derivation_id` between the native first-wins tiebreak and the oracle
/// chase — that derivation-id divergence is expected and is NOT a fact-level divergence (it is
/// pinned separately by the determinism gate in [`crate::physical::seminaive`]).
fn compare_materialization(
    native: &[DerivedRow],
    oracle: &dyn ForwardOracle,
    facts: &TypedFactSet,
    rules: &str,
    world: &str,
) -> gmeow_errors::Result<ParityLedger> {
    let closure = oracle.materialize(facts, rules, &ForwardBudget::UNBOUNDED)?;
    let oracle_name = oracle.name();

    let native_keys: BTreeSet<FactKey> = native.iter().map(row_fact_key).collect();
    let oracle_keys: BTreeSet<FactKey> = closure
        .rows
        .iter()
        .filter_map(|(row, _prov)| typed_row_fact_key(row))
        .collect();

    let mut rows: Vec<LedgerRow> = Vec::new();

    for key in native_keys.intersection(&oracle_keys) {
        let (subject, predicate, object) = key.clone();
        rows.push(LedgerRow {
            kind: DivergenceKind::Agree,
            category: "materialization".to_owned(),
            detail: format!(
                "native and {oracle_name} agree on fact: {subject} {predicate} {object}"
            ),
            subject,
            object,
            world: world.to_owned(),
        });
    }
    for key in native_keys.difference(&oracle_keys) {
        let (subject, predicate, object) = key.clone();
        rows.push(LedgerRow {
            kind: DivergenceKind::NativeOnly,
            category: "materialization".to_owned(),
            detail: format!(
                "derived natively but not by {oracle_name}: {subject} {predicate} {object}"
            ),
            subject,
            object,
            world: world.to_owned(),
        });
    }
    for key in oracle_keys.difference(&native_keys) {
        let (subject, predicate, object) = key.clone();
        rows.push(LedgerRow {
            kind: DivergenceKind::OracleOnly,
            category: "materialization".to_owned(),
            detail: format!(
                "derived by {oracle_name} but not natively: {subject} {predicate} {object}"
            ),
            subject,
            object,
            world: world.to_owned(),
        });
    }

    Ok(ParityLedger::from_rows(rows))
}

// ── Null-blind existential parity ──────────────────────────────────────────────
//
// The native chase and Nemo both value-invent, but name their nulls differently (the
// native chase mints a content-addressed Skolem IRI; Nemo mints a labeled null `_:0`).
// A fact-level comparison must therefore be **null-blind**: two fact sets agree when they
// are equal up to a consistent renaming of invented nulls.  Rather than a surface-position
// token (which would false-agree on non-isomorphic structures and false-diverge under a
// different firing order), each null is canonicalized by **colour refinement** of the
// null-labelled fact graph — a null's colour is the fixpoint of the multiset of
// `(predicate, role, neighbour-colour)` edges it participates in, grounded in the named
// (non-null) terms.  Isomorphic null structures converge to equal colours regardless of
// order or naming; non-isomorphic ones never do.  Named terms are never rewritten.

/// Whether a term surface denotes an invented null: a native chase Skolem IRI
/// (`…/skolem/…`) or a Nemo labeled null (rendered `<urn:gmeow:nemo-null:…>` by the
/// facts-only decoder, or a raw `_:…` blank).
fn is_null_surface(surface: &str) -> bool {
    surface.contains("/skolem/")
        || surface.contains("nemo-null:")
        || surface.starts_with("_:")
        || surface.starts_with("<_:")
}

/// Canonicalize the invented nulls in a fact-key set by colour refinement, returning
/// the rewritten keys as a **multiset** (`FactKey → occurrence count`) so witness
/// MULTIPLICITY is preserved.  Named terms pass through unchanged.
///
/// Two *automorphic* invented nulls (e.g. two witnesses of the same `≥2 p.D`
/// obligation on the same frontier) share a colour, hence a canonical token, so their
/// facts rewrite to the SAME `FactKey`.  Collapsing them to a set element would lose
/// the witness count and let a genuine `≥n` divergence (native invents 1 where the
/// oracle invents 2) false-agree; the multiset keeps `min(native, oracle)` as agreement
/// and the surplus as a native/oracle-only divergence.  The count is over the pre-canon
/// keys (distinct-named witnesses are distinct comparands), so a consistent renaming of
/// the SAME number of witnesses still yields byte-equal multisets.
fn canonicalize_nulls(keys: &BTreeSet<FactKey>) -> BTreeMap<FactKey, usize> {
    let nulls: BTreeSet<String> = keys
        .iter()
        .flat_map(|(s, _, o)| [s.clone(), o.clone()])
        .filter(|t| is_null_surface(t))
        .collect();
    if nulls.is_empty() {
        let mut counts: BTreeMap<FactKey, usize> = BTreeMap::new();
        for key in keys {
            *counts.entry(key.clone()).or_insert(0) += 1;
        }
        return counts;
    }

    // Colour every term: a named term is its own surface (a fixed anchor); a null starts
    // from a single uniform colour and is refined by its neighbourhood.
    let mut colour: BTreeMap<String, String> = BTreeMap::new();
    for (s, _, o) in keys {
        for t in [s, o] {
            colour.entry(t.clone()).or_insert_with(|| {
                if is_null_surface(t) {
                    "\u{0}".to_owned()
                } else {
                    t.clone()
                }
            });
        }
    }

    // Refine to a fixpoint (bounded by the null count — colours can only get finer).
    for _ in 0..=nulls.len() {
        let mut next = colour.clone();
        let mut changed = false;
        for n in &nulls {
            let mut sig: Vec<String> = Vec::new();
            for (s, p, o) in keys {
                if s == n {
                    sig.push(format!("s\u{1f}{p}\u{1f}{}", colour[o]));
                }
                if o == n {
                    sig.push(format!("o\u{1f}{p}\u{1f}{}", colour[s]));
                }
            }
            sig.sort();
            let refined = crate::provenance::sha1_hex(&sig.join("\u{1e}"));
            if next[n] != refined {
                changed = true;
                next.insert(n.clone(), refined);
            }
        }
        colour = next;
        if !changed {
            break;
        }
    }

    // Assign canonical tokens by sorted final colour — identical across both sides for
    // isomorphic structures, so the rewritten sets compare byte-equal.
    let distinct: BTreeSet<String> = nulls.iter().map(|n| colour[n].clone()).collect();
    let token: BTreeMap<String, String> = distinct
        .into_iter()
        .enumerate()
        .map(|(i, c)| (c, format!("gmeow:null#{i}")))
        .collect();
    let rewrite = |t: &String| -> String {
        if is_null_surface(t) {
            token[&colour[t]].clone()
        } else {
            t.clone()
        }
    };
    let mut counts: BTreeMap<FactKey, usize> = BTreeMap::new();
    for (s, p, o) in keys {
        *counts
            .entry((rewrite(s), p.clone(), rewrite(o)))
            .or_insert(0) += 1;
    }
    counts
}

/// Compare the native chase's derived facts against a forward oracle's closure
/// **null-blind** AND **cardinality-aware**: both fact sets are canonicalized to a
/// multiset ([`canonicalize_nulls`]) before the `Agree` / `NativeOnly` / `OracleOnly`
/// classification, so a consistent renaming of the SAME number of invented nulls is
/// agreement, but a differing witness COUNT — even between automorphic (symmetric)
/// nulls that share a canonical token — is divergence.  Used to oracle-gate the
/// existential fragment against Nemo.
///
/// For each canonical fact key the `min(native_count, oracle_count)` occurrences are
/// `Agree`; the native surplus is `NativeOnly` and the oracle surplus is `OracleOnly`
/// (one emitted row per surplus occurrence).  Without the multiset, a real `≥2`
/// divergence where native invents 1 witness and the oracle invents 2 (both rewriting
/// to the same `(a, p, #0)` token) would collapse to one set element on each side and
/// false-report `Agree`, defeating the oracle-gate's soundness guarantee.
fn compare_existential_materialization(
    native: &[DerivedRow],
    oracle: &dyn ForwardOracle,
    facts: &TypedFactSet,
    rules: &str,
    world: &str,
) -> gmeow_errors::Result<ParityLedger> {
    let closure = oracle.materialize(facts, rules, &ForwardBudget::UNBOUNDED)?;
    let oracle_name = oracle.name();

    let native_counts = canonicalize_nulls(&native.iter().map(row_fact_key).collect());
    let oracle_counts = canonicalize_nulls(
        &closure
            .rows
            .iter()
            .filter_map(|(row, _prov)| typed_row_fact_key(row))
            .collect(),
    );

    // The sorted union of canonical keys drives a deterministic multiset comparison.
    let all_keys: BTreeSet<FactKey> = native_counts
        .keys()
        .chain(oracle_counts.keys())
        .cloned()
        .collect();

    let mut rows: Vec<LedgerRow> = Vec::new();
    // Group by kind (Agree, then NativeOnly, then OracleOnly), each in sorted-key order,
    // mirroring the set-based comparator's row grouping.
    for key in &all_keys {
        let (subject, predicate, object) = key.clone();
        let native_n = native_counts.get(key).copied().unwrap_or(0);
        let oracle_n = oracle_counts.get(key).copied().unwrap_or(0);
        let agree = native_n.min(oracle_n);
        for _ in 0..agree {
            rows.push(LedgerRow {
                kind: DivergenceKind::Agree,
                category: "materialization".to_owned(),
                detail: format!(
                    "native and {oracle_name} agree on fact: {subject} {predicate} {object}"
                ),
                subject: subject.clone(),
                object: object.clone(),
                world: world.to_owned(),
            });
        }
    }
    for key in &all_keys {
        let (subject, predicate, object) = key.clone();
        let native_n = native_counts.get(key).copied().unwrap_or(0);
        let oracle_n = oracle_counts.get(key).copied().unwrap_or(0);
        for _ in oracle_n..native_n {
            rows.push(LedgerRow {
                kind: DivergenceKind::NativeOnly,
                category: "materialization".to_owned(),
                detail: format!(
                    "derived natively but not by {oracle_name}: {subject} {predicate} {object}"
                ),
                subject: subject.clone(),
                object: object.clone(),
                world: world.to_owned(),
            });
        }
    }
    for key in &all_keys {
        let (subject, predicate, object) = key.clone();
        let native_n = native_counts.get(key).copied().unwrap_or(0);
        let oracle_n = oracle_counts.get(key).copied().unwrap_or(0);
        for _ in native_n..oracle_n {
            rows.push(LedgerRow {
                kind: DivergenceKind::OracleOnly,
                category: "materialization".to_owned(),
                detail: format!(
                    "derived by {oracle_name} but not natively: {subject} {predicate} {object}"
                ),
                subject: subject.clone(),
                object: object.clone(),
                world: world.to_owned(),
            });
        }
    }
    Ok(ParityLedger::from_rows(rows))
}

/// A comparable answer-binding key: the sorted `var=value` pairs of one [`crate::query_ir::Binding`].
///
/// A binding is a `BTreeMap<String, String>`, so iterating it yields the variable/value pairs in
/// sorted key order; joining them gives a stable string surface. An empty binding (the ground
/// "yes" answer) maps to `"<yes>"` so it is a distinguishable, comparable key.
fn binding_key(binding: &crate::query_ir::Binding) -> String {
    if binding.is_empty() {
        return "<yes>".to_owned();
    }
    binding
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Compare the native [`AnswerSet`] bindings against a [`BackwardOracle`]'s answer set,
/// generically over which engine backs the oracle.
///
/// The oracle is run here (over the same world snapshot, program, and budget the native engine
/// used), and each binding is classified by its `binding_key`: present in BOTH ⇒
/// [`DivergenceKind::Agree`], native ∖ oracle ⇒ [`DivergenceKind::NativeOnly`], oracle ∖ native
/// ⇒ [`DivergenceKind::OracleOnly`]. Detail strings name the oracle via [`BackwardOracle::name`]
/// so the ledger is engine-agnostic. The comparison is on the binding SET; rows are emitted in
/// sorted-key order.
///
/// `tabling` is empty because the retained reference resolver ignores hints.
fn compare_answers(
    native: &AnswerSet,
    oracle: &dyn BackwardOracle,
    foreign: &dyn WorldFactSource,
    world: &str,
    program: &QProgram,
    budget: &Budget,
) -> gmeow_errors::Result<ParityLedger> {
    let answers = oracle.solve(foreign, world, program, &[], budget)?;
    let oracle_name = oracle.name();

    let native_keys: BTreeSet<String> = native.bindings.iter().map(binding_key).collect();
    let oracle_keys: BTreeSet<String> = answers.bindings.iter().map(binding_key).collect();

    let mut rows: Vec<LedgerRow> = Vec::new();

    for key in native_keys.intersection(&oracle_keys) {
        rows.push(LedgerRow {
            kind: DivergenceKind::Agree,
            category: "answer".to_owned(),
            subject: key.clone(),
            object: String::new(),
            world: String::new(),
            detail: format!("native and {oracle_name} agree on answer: {key}"),
        });
    }
    for key in native_keys.difference(&oracle_keys) {
        rows.push(LedgerRow {
            kind: DivergenceKind::NativeOnly,
            category: "answer".to_owned(),
            subject: key.clone(),
            object: String::new(),
            world: String::new(),
            detail: format!("answered natively but not by {oracle_name}: {key}"),
        });
    }
    for key in oracle_keys.difference(&native_keys) {
        rows.push(LedgerRow {
            kind: DivergenceKind::OracleOnly,
            category: "answer".to_owned(),
            subject: key.clone(),
            object: String::new(),
            world: String::new(),
            detail: format!("answered by {oracle_name} but not natively: {key}"),
        });
    }

    Ok(ParityLedger::from_rows(rows))
}

#[cfg(test)]
mod tests {
    //! The parity + native-coverage-floor GATE.
    //!
    //! `materialize_parity_*` drive the Nemo forward oracle and so MUST run in the `engine`
    //! nextest group (the `materialize` token in the test-fn name matches the engine-group
    //! regex `nemo_engine|materialize|reason|verify|certify|dispatch|...` in
    //! `.config/nextest.toml`). `dispatch_parity_*` (the `dispatch` token) likewise match.
    //! The floor test `native_coverage_floor` drives ONLY the native engines
    //! (`materialize_native` / `resolve_native`) — no external subprocess — so it has no
    //! multi-GB footprint and does not need the engine group; it is intentionally left
    //! ungrouped. Any NEW parity test that drives a real engine must keep a group token
    //! (`materialize`/`dispatch`) in its fn name.

    use super::*;
    use crate::oracle::{
        ForwardBudget, ForwardOracle, NativeForwardOracle, NemoFactsOracle, NemoForwardOracle,
        ReferenceBackwardOracle, TypedChaseResult, TypedProvenance, TypedRow,
    };
    use crate::physical::chase::{ChaseAdmission, ExistentialRule, chase_world, route_chase};
    use crate::physical::magic::resolve_native;
    use crate::physical::plan::Parsed;
    use crate::physical::seminaive::{NativeOutcome, materialize_native};
    use crate::query_ir::{
        Budget, QAtom, QBodyLit, QGoal, QProgram, QRule, QTerm, parse_query_program,
    };
    use crate::rule_ir::{EvalAtom, EvalTerm, Fact, parse_eval_rules};
    use crate::seam::{BudgetStatus, WorldFactSnapshot};
    use crate::store::WorldStore;
    use purrdf::TermValue;

    const PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";

    const TYPE_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const EX_C: &str = "http://ex/C";
    const EX_D: &str = "http://ex/D";
    const EX_P: &str = "http://ex/p";
    const EX_WORLD: &str = "urn:world";

    fn ex_iri(s: &str) -> TermValue {
        TermValue::iri(s)
    }

    fn ex_atom(subject: EvalTerm, predicate: &str, object: EvalTerm) -> EvalAtom {
        EvalAtom {
            subject,
            predicate: predicate.to_owned(),
            object,
            negated: false,
        }
    }

    /// The EL `C ⊑ ∃p.D` obligation as an `ExistentialRule` (native) plus the equivalent
    /// ternary Nemo `.rls` existential rule (`!y` shared across the conjunctive head).
    fn some_values_from_native() -> ExistentialRule {
        ExistentialRule {
            rule_iri: "http://ex/rule/svf".to_owned(),
            body: vec![ex_atom(
                EvalTerm::Var("?x".to_owned()),
                TYPE_IRI,
                EvalTerm::ConstNamed(EX_C.to_owned()),
            )],
            head: vec![
                ex_atom(
                    EvalTerm::Var("?x".to_owned()),
                    EX_P,
                    EvalTerm::Var("?y".to_owned()),
                ),
                ex_atom(
                    EvalTerm::Var("?y".to_owned()),
                    TYPE_IRI,
                    EvalTerm::ConstNamed(EX_D.to_owned()),
                ),
            ],
            distinct: vec![],
        }
    }

    fn some_values_from_nemo_rls() -> String {
        // Ternary (world-carrying) existential rule; `!y` is one shared invented null.
        format!(
            "<{EX_P}>(?x, !y, ?w), <{TYPE_IRI}>(!y, <{EX_D}>, ?w) :- <{TYPE_IRI}>(?x, <{EX_C}>, ?w) ."
        )
    }

    #[test]
    fn materialize_existential_parity_native_agrees_with_nemo() {
        // Two C-individuals in one world.  The native restricted chase and Nemo's chase
        // must produce the SAME facts up to null renaming (the null-blind gate).
        let individuals = ["http://ex/a", "http://ex/b"];

        // Native EDB (binary facts) + chase via the router.
        let native_edb: Vec<Fact> = individuals
            .iter()
            .map(|i| Fact {
                subject: ex_iri(i),
                predicate: TYPE_IRI.to_owned(),
                object: ex_iri(EX_C),
            })
            .collect();
        let (admission, outcome) =
            route_chase(EX_WORLD, &native_edb, &[some_values_from_native()], None).unwrap();
        assert!(
            admission.admits_native(),
            "acyclic EL restriction must certify"
        );
        let native_rows = match outcome {
            NativeOutcome::Decided(b) => b.rows,
            NativeOutcome::Unsupported(k) => panic!("certified program must run: {k:?}"),
        };

        // Oracle EDB (ternary typed quads) + the equivalent Nemo existential .rls.
        let mut oracle_edb = TypedFactSet::new();
        for i in individuals {
            oracle_edb.push_quad(&ex_iri(i), TYPE_IRI, &ex_iri(EX_C), EX_WORLD);
        }

        let ledger = compare_existential_materialization(
            &native_rows,
            &NemoFactsOracle,
            &oracle_edb,
            &some_values_from_nemo_rls(),
            EX_WORLD,
        )
        .expect("nemo facts-only chase should succeed");

        let verdict = ledger.enforce();
        assert!(
            verdict.passed,
            "native chase must agree with Nemo null-blind; reasons: {:?}; rows: {:#?}",
            verdict.reasons, ledger.rows
        );
        // Non-vacuous: they actually agree on the invented p-edges and D-types, not just
        // the echoed EDB (2 C-types + 2 p-edges + 2 D-types = 6 agreed facts).
        assert_eq!(ledger.agree, 6, "all six facts agree");
        assert_eq!(ledger.native_only, 0);
        assert_eq!(ledger.oracle_only, 0);
    }

    /// Build a native derived row for `subject predicate object` in [`EX_WORLD`], stamped
    /// with a non-assert rule IRI (so it is a genuine derived fact, not an EDB echo).
    fn native_derived(subject: &str, predicate: &str, object: &str) -> DerivedRow {
        DerivedRow {
            graph: EX_WORLD.to_owned(),
            subject: ex_iri(subject),
            predicate: predicate.to_owned(),
            object: ex_iri(object),
            rule_iri: "http://ex/rule/svf".to_owned(),
            source_quad_ids: vec![],
            derivation_id: format!("http://ex/deriv/{subject}/{predicate}/{object}"),
            proof_height: crate::provenance::ProofHeight::new(1).unwrap(),
            antecedents: vec![],
        }
    }

    /// Build a ternary (world-carrying) oracle row `predicate(subject, object, world)` —
    /// the arity-3 shape [`typed_row_fact_key`] coerces to a `(subject, predicate, object)`
    /// fact key.
    fn oracle_quad(subject: &str, predicate: &str, object: &str) -> (TypedRow, TypedProvenance) {
        (
            TypedRow {
                predicate: predicate.to_owned(),
                args: vec![ex_iri(subject), ex_iri(object), ex_iri(EX_WORLD)],
            },
            TypedProvenance {
                is_edb: false,
                rule_name: Some("http://ex/rule/svf".to_owned()),
                antecedents: vec![],
                proof_height: None,
                attributions: vec![],
            },
        )
    }

    // Distinct invented-null surfaces. `is_null_surface` keys native nulls off `/skolem/`
    // and Nemo-style nulls off `nemo-null:`; the two same-obligation witnesses are
    // AUTOMORPHIC (identical neighbourhoods: `a --p--> witness --type--> D`), so colour
    // refinement gives them one shared canonical token — the exact condition the old
    // set-based comparator collapsed.
    const SKOLEM_1: &str = "http://ex/skolem/w1";
    const NEMO_NULL_1: &str = "urn:gmeow:nemo-null:1";
    const NEMO_NULL_2: &str = "urn:gmeow:nemo-null:2";
    const EX_A: &str = "http://ex/a";

    /// C1 REGRESSION: a genuine `≥2` witness-COUNT divergence — native invents ONE witness
    /// where the oracle invents TWO automorphic (symmetric) witnesses of the same
    /// `a ⊑ ≥2 p.D` obligation — MUST be caught, not false-agreed.
    ///
    /// Both oracle witnesses share the SAME canonical null token (they are automorphic), so
    /// the pre-fix SET-based comparator canonicalized native `{(a,p,#0),(#0,type,D)}` and
    /// oracle `{(a,p,#0),(#0,type,D)}` to BYTE-EQUAL sets and reported full `Agree` — a false
    /// agreement. The multiset comparator keeps the counts: native has each key once, the
    /// oracle twice, so `min` yields the agreement and the oracle surplus is `OracleOnly`.
    #[test]
    fn existential_witness_multiplicity_divergence_is_caught() {
        // Native: one witness w1 of `a ⊑ ≥2 p.D`.
        let native = vec![
            native_derived(EX_A, EX_P, SKOLEM_1),
            native_derived(SKOLEM_1, TYPE_IRI, EX_D),
        ];
        // Oracle: TWO automorphic witnesses of the same obligation.
        let oracle = DivergentForwardOracle {
            rows: vec![
                oracle_quad(EX_A, EX_P, NEMO_NULL_1),
                oracle_quad(NEMO_NULL_1, TYPE_IRI, EX_D),
                oracle_quad(EX_A, EX_P, NEMO_NULL_2),
                oracle_quad(NEMO_NULL_2, TYPE_IRI, EX_D),
            ],
        };
        let facts = TypedFactSet::new();
        let ledger =
            compare_existential_materialization(&native, &oracle, &facts, "", EX_WORLD).unwrap();
        let verdict = ledger.enforce();
        assert!(
            !verdict.passed,
            "a native-1 vs oracle-2 automorphic-witness divergence must FAIL the gate, not \
             false-agree; rows: {:#?}",
            ledger.rows
        );
        // The one shared witness p-edge + D-type agree; the oracle's extra witness's p-edge
        // + D-type are the oracle surplus.
        assert_eq!(
            ledger.agree, 2,
            "the single shared witness's two facts agree"
        );
        assert_eq!(ledger.native_only, 0, "native invents no surplus witness");
        assert_eq!(
            ledger.oracle_only, 2,
            "the oracle's extra witness contributes two oracle-only facts"
        );
    }

    /// The POSITIVE companion: native AND the oracle each invent TWO automorphic witnesses of
    /// the same `a ⊑ ≥2 p.D` obligation. The witness COUNT matches, so a consistent renaming
    /// (both sides collapse to the shared token, each with multiplicity 2) is full `Agree` —
    /// the multiset fix must NOT over-diverge on a matched symmetric multiplicity.
    #[test]
    fn existential_symmetric_witnesses_multiplicity_agrees() {
        let native = vec![
            native_derived(EX_A, EX_P, "http://ex/skolem/w1"),
            native_derived("http://ex/skolem/w1", TYPE_IRI, EX_D),
            native_derived(EX_A, EX_P, "http://ex/skolem/w2"),
            native_derived("http://ex/skolem/w2", TYPE_IRI, EX_D),
        ];
        let oracle = DivergentForwardOracle {
            rows: vec![
                oracle_quad(EX_A, EX_P, NEMO_NULL_1),
                oracle_quad(NEMO_NULL_1, TYPE_IRI, EX_D),
                oracle_quad(EX_A, EX_P, NEMO_NULL_2),
                oracle_quad(NEMO_NULL_2, TYPE_IRI, EX_D),
            ],
        };
        let facts = TypedFactSet::new();
        let ledger =
            compare_existential_materialization(&native, &oracle, &facts, "", EX_WORLD).unwrap();
        let verdict = ledger.enforce();
        assert!(
            verdict.passed,
            "matched two-witness multiplicity must AGREE null-blind; reasons: {:?}; rows: {:#?}",
            verdict.reasons, ledger.rows
        );
        assert_eq!(
            ledger.agree, 4,
            "both witnesses' p-edge + D-type agree (2 witnesses × 2 facts)"
        );
        assert_eq!(ledger.native_only, 0);
        assert_eq!(ledger.oracle_only, 0);
    }

    // ── H5: adversarial certifier-SOUNDNESS differential (native ChaseAdmission ≡ Nemo) ──
    //
    // The load-bearing safety claim of `ChaseAdmission::certify` is that it NEVER wrongly
    // certifies: if it returns `WeaklyAcyclic`, the restricted chase genuinely reaches a
    // fixpoint.  A false `WeaklyAcyclic` would let the router run the chase UNBUDGETED and
    // loop forever.  The self-disclosed low-confidence mechanism is the bespoke CONSTANT
    // REFINEMENT — it SPLITS a `type(individual, class)` position by the constant in the
    // other slot (and can REMOVE the edge that a plain weak-acyclicity check would draw),
    // exactly where a hidden cycle could slip through as a false certificate — together with
    // its `add_wildcard_subsumption` over-approximation connecting variable-class (wildcard)
    // and constant-class refinements of one `(predicate, slot)`.
    //
    // This test pins the invariant over a set of ADVERSARIALLY-chosen mixed variable-class /
    // constant-class `type` programs: for every CERTIFIED fixture the budgeted native chase
    // MUST reach a natural fixpoint (`BudgetStatus::Ok`) strictly WITHIN a generous budget —
    // an `Exhausted` here is a FALSE certification (the program loops) and fails the test,
    // never hangs it — and its facts must agree with Nemo's chase null-blind.  A set of
    // shapes the certifier correctly REFUSES proves it is discriminating, not vacuous.

    const EX_E: &str = "http://ex/E";
    const EX_Q: &str = "http://ex/q";
    const EX_HASKIND: &str = "http://ex/hasKind";
    const EX_TAGGED: &str = "http://ex/tagged";
    const EX_HASCLASS: &str = "http://ex/hasClass";

    fn ex_var(name: &str) -> EvalTerm {
        EvalTerm::Var(name.to_owned())
    }

    fn ex_named(iri: &str) -> EvalTerm {
        EvalTerm::ConstNamed(iri.to_owned())
    }

    /// `type(?x, from) → ∃y. rel(x, y) ∧ type(y, to)` as a native `ExistentialRule` — the
    /// constant-refined shape (`from`/`to` are class constants co-occurring with a `type`
    /// subject variable, the exact input the refinement partitions).
    fn native_restriction(iri: &str, from: &str, rel: &str, to: &str) -> ExistentialRule {
        ExistentialRule {
            rule_iri: iri.to_owned(),
            body: vec![ex_atom(ex_var("?x"), TYPE_IRI, ex_named(from))],
            head: vec![
                ex_atom(ex_var("?x"), rel, ex_var("?y")),
                ex_atom(ex_var("?y"), TYPE_IRI, ex_named(to)),
            ],
            distinct: vec![],
        }
    }

    /// The ternary (world-carrying) Nemo `.rls` line for [`native_restriction`]; `yvar`
    /// names the shared invented null so concatenated rules keep distinct existential
    /// variables.
    fn nemo_restriction(rel: &str, from: &str, to: &str, yvar: &str) -> String {
        format!(
            "<{rel}>(?x, !{yvar}, ?w), <{TYPE_IRI}>(!{yvar}, <{to}>, ?w) :- \
             <{TYPE_IRI}>(?x, <{from}>, ?w) .\n"
        )
    }

    /// A single-individual `type(a, class)` native EDB fact.
    fn type_fact(individual: &str, class: &str) -> Fact {
        Fact {
            subject: ex_iri(individual),
            predicate: TYPE_IRI.to_owned(),
            object: ex_iri(class),
        }
    }

    /// One adversarial CERTIFIED fixture: a native rule set + EDB the certifier declares
    /// `WeaklyAcyclic`, paired with the equivalent ternary Nemo `.rls` + typed EDB so the
    /// certified facts can be cross-checked against Nemo's chase null-blind.
    struct CertifiedFixture {
        label: &'static str,
        rules: Vec<ExistentialRule>,
        edb: Vec<Fact>,
        nemo_rls: String,
        nemo_edb: TypedFactSet,
    }

    fn certified_fixtures() -> Vec<CertifiedFixture> {
        // CF1 — plain acyclic mixed-class `C ⊑ ∃p.D`: the D-typed witness lives at the
        // refined position `(type,S | D)` and never re-triggers the `(type,S | C)`-bodied
        // rule, so the refinement's SPLIT is exactly what keeps `(type,S)` acyclic.
        let mut cf1_edb = TypedFactSet::new();
        cf1_edb.push_quad(&ex_iri(EX_A), TYPE_IRI, &ex_iri(EX_C), EX_WORLD);
        cf1_edb.push_quad(&ex_iri("http://ex/b"), TYPE_IRI, &ex_iri(EX_C), EX_WORLD);
        let cf1 = CertifiedFixture {
            label: "acyclic-mixed-class-CtoD",
            rules: vec![native_restriction("http://ex/rule/c", EX_C, EX_P, EX_D)],
            edb: vec![type_fact(EX_A, EX_C), type_fact("http://ex/b", EX_C)],
            nemo_rls: nemo_restriction(EX_P, EX_C, EX_D, "y"),
            nemo_edb: cf1_edb,
        };

        // CF2 — the constant-refinement-is-load-bearing chain `C ⊑ ∃p.D`, `D ⊑ ∃q.E`:
        // PLAIN weak acyclicity collapses every class into one `(type,S)` node, so each
        // rule's fresh null lands where the OTHER rule reads and it spuriously reports a
        // self-cycle → non-terminating.  The refinement splits `(type,S)` into `|C`, `|D`,
        // `|E`; the D-null feeds only the `|D`-bodied rule and the E-null feeds nothing, so
        // the certificate holds — and the chase genuinely terminates (finite C→D→E chain).
        let mut cf2_edb = TypedFactSet::new();
        cf2_edb.push_quad(&ex_iri(EX_A), TYPE_IRI, &ex_iri(EX_C), EX_WORLD);
        let cf2 = CertifiedFixture {
            label: "refinement-makes-acyclic-chain-CtoDtoE",
            rules: vec![
                native_restriction("http://ex/rule/c", EX_C, EX_P, EX_D),
                native_restriction("http://ex/rule/d", EX_D, EX_Q, EX_E),
            ],
            edb: vec![type_fact(EX_A, EX_C)],
            nemo_rls: format!(
                "{}{}",
                nemo_restriction(EX_P, EX_C, EX_D, "y1"),
                nemo_restriction(EX_Q, EX_D, EX_E, "y2")
            ),
            nemo_edb: cf2_edb,
        };

        // CF3 — a CONSTANT-typed witness read by a VARIABLE-class (wildcard) consumer,
        // exercising `add_wildcard_subsumption`'s connect-both branch.  The existential
        // rule types its witness with the CONSTANT class D via a NON-`type` predicate
        // (`hasKind`); a second rule `hasKind(?z, ?c) → tagged(?z, ?c)` reads `hasKind`
        // with a VARIABLE class, so `(hasKind,S)` carries BOTH the `|D` constant refinement
        // and the `*` wildcard and the subsumption connects them bidirectionally.  The
        // wildcard hub does NOT reach back to the existential rule's `(type,S | C)` body
        // position, so the certificate correctly holds and the chase terminates.
        let mut cf3_edb = TypedFactSet::new();
        cf3_edb.push_quad(&ex_iri(EX_A), TYPE_IRI, &ex_iri(EX_C), EX_WORLD);
        let cf3 = CertifiedFixture {
            label: "wildcard-subsumption-connect-both-acyclic",
            rules: vec![
                ExistentialRule {
                    rule_iri: "http://ex/rule/kind".to_owned(),
                    body: vec![ex_atom(ex_var("?x"), TYPE_IRI, ex_named(EX_C))],
                    head: vec![
                        ex_atom(ex_var("?x"), EX_P, ex_var("?y")),
                        ex_atom(ex_var("?y"), EX_HASKIND, ex_named(EX_D)),
                    ],
                    distinct: vec![],
                },
                ExistentialRule {
                    rule_iri: "http://ex/rule/tag".to_owned(),
                    body: vec![ex_atom(ex_var("?z"), EX_HASKIND, ex_var("?k"))],
                    head: vec![ex_atom(ex_var("?z"), EX_TAGGED, ex_var("?k"))],
                    distinct: vec![],
                },
            ],
            edb: vec![type_fact(EX_A, EX_C)],
            nemo_rls: format!(
                "<{EX_P}>(?x, !y, ?w), <{EX_HASKIND}>(!y, <{EX_D}>, ?w) :- \
                 <{TYPE_IRI}>(?x, <{EX_C}>, ?w) .\n\
                 <{EX_TAGGED}>(?z, ?k, ?w) :- <{EX_HASKIND}>(?z, ?k, ?w) .\n"
            ),
            nemo_edb: cf3_edb,
        };

        vec![cf1, cf2, cf3]
    }

    /// One shape the certifier MUST refuse (`Uncertified`), with why.
    struct RefusedFixture {
        label: &'static str,
        rules: Vec<ExistentialRule>,
    }

    fn refused_fixtures() -> Vec<RefusedFixture> {
        vec![
            // RF1 — a genuinely non-terminating self-cycle `D ⊑ ∃p.D`: the witness is
            // itself D-typed, so it re-fires the same rule forever.  The refinement gives
            // both the body and the witness the SAME `(type,S | D)` node — no split can
            // break this real cycle, and the certifier must not.
            RefusedFixture {
                label: "genuine-self-cycle-DtoD",
                rules: vec![native_restriction(
                    "http://ex/rule/cyclic",
                    EX_D,
                    EX_P,
                    EX_D,
                )],
            },
            // RF2 — a genuine TWO-rule cycle `C ⊑ ∃p.D`, `D ⊑ ∃q.C`: C invents a D, D
            // invents a C, forever.  Across rules the refined `(type,S | C)` and
            // `(type,S | D)` nodes are mutually reachable through the special edges.
            RefusedFixture {
                label: "genuine-two-rule-cycle-CtoDtoC",
                rules: vec![
                    native_restriction("http://ex/rule/c", EX_C, EX_P, EX_D),
                    native_restriction("http://ex/rule/d", EX_D, EX_Q, EX_C),
                ],
            },
            // RF3 — the CONSERVATIVE wildcard-subsumption refusal: `C ⊑ ∃p.D` alongside a
            // VARIABLE-class reader OVER `type` itself (`type(?z, ?c) → hasClass(?z, ?c)`).
            // The wildcard `(type,S | *)` now co-occurs with the `|C` and `|D` constant
            // refinements at the SAME `(type,S)`, so `add_wildcard_subsumption` connects
            // `|C ↔ * ↔ |D` and the special edge `(type,S | C) → (type,S | D)` lands inside
            // a cycle through the wildcard hub.  This OVER-approximates (the program in fact
            // terminates), but the certifier errs toward refusal — the sound direction: it
            // never wrongly certifies.  Included to exercise that connect-both actually
            // fires and to prove the discrimination is not vacuous.
            RefusedFixture {
                label: "conservative-wildcard-hub-refusal",
                rules: vec![
                    native_restriction("http://ex/rule/c", EX_C, EX_P, EX_D),
                    ExistentialRule {
                        rule_iri: "http://ex/rule/class".to_owned(),
                        body: vec![ex_atom(ex_var("?z"), TYPE_IRI, ex_var("?c"))],
                        head: vec![ex_atom(ex_var("?z"), EX_HASCLASS, ex_var("?c"))],
                        distinct: vec![],
                    },
                ],
            },
        ]
    }

    #[test]
    fn materialize_certifier_soundness_differential_agrees_with_nemo() {
        // A budget that comfortably exceeds any legitimate fixpoint here (each fixture
        // saturates in well under a dozen derivations) but BOUNDS a runaway: a
        // wrongly-certified looping program hits this and reports `Exhausted` — the false
        // certification this test exists to catch — instead of hanging the run.
        const BIG: u64 = 100_000;

        for f in certified_fixtures() {
            // (1) The certifier declares this adversarial shape terminating.
            let admission = ChaseAdmission::certify(&f.rules);
            assert!(
                admission.admits_native(),
                "[{}] this adversarial mixed-class fixture must certify WeaklyAcyclic; got {:?}",
                f.label,
                admission
            );

            // (2)+(3) The soundness assertion: run the chase under a GENEROUS budget and
            // demand a NATURAL fixpoint STRICTLY within it.  `Exhausted` on a certified
            // program is a FALSE certification (it loops), never merely incomplete.
            let outcome = chase_world(EX_WORLD, &f.edb, &f.rules, Some(BIG))
                .unwrap_or_else(|e| panic!("[{}] chase errored: {e}", f.label));
            let budgeted = match outcome {
                NativeOutcome::Decided(b) => b,
                NativeOutcome::Unsupported(k) => {
                    panic!(
                        "[{}] a certified program must run natively, got {k:?}",
                        f.label
                    )
                }
            };
            assert_eq!(
                budgeted.status,
                BudgetStatus::Ok,
                "[{}] SOUNDNESS VIOLATION: the certifier said WeaklyAcyclic but the budgeted \
                 chase EXHAUSTED at {} of {BIG} steps — a program the certifier declared \
                 terminating actually LOOPS (a false certification)",
                f.label,
                budgeted.consumed_steps
            );
            assert!(
                budgeted.consumed_steps < BIG,
                "[{}] a certified chase must halt strictly within budget, consumed {}",
                f.label,
                budgeted.consumed_steps
            );

            // (4) The oracle half: the certified facts agree with Nemo's chase null-blind.
            let ledger = compare_existential_materialization(
                &budgeted.rows,
                &NemoFactsOracle,
                &f.nemo_edb,
                &f.nemo_rls,
                EX_WORLD,
            )
            .unwrap_or_else(|e| panic!("[{}] nemo facts-only chase failed: {e}", f.label));
            let verdict = ledger.enforce();
            assert!(
                verdict.passed,
                "[{}] the certified native chase must AGREE with Nemo null-blind; reasons {:?}; \
                 rows {:#?}",
                f.label, verdict.reasons, ledger.rows
            );
            assert!(
                ledger.agree > 0,
                "[{}] non-vacuous: native and Nemo actually agree on ≥1 chased fact",
                f.label
            );
            assert_eq!(
                ledger.native_only, 0,
                "[{}] no native-only fact vs Nemo",
                f.label
            );
            assert_eq!(
                ledger.oracle_only, 0,
                "[{}] no oracle-only fact vs Nemo",
                f.label
            );
        }

        // Discrimination: the certifier is not vacuously certifying — genuinely cyclic
        // shapes AND the conservative wildcard-hub over-approximation are REFUSED, each
        // carrying a weak-acyclicity violation.
        for f in refused_fixtures() {
            let admission = ChaseAdmission::certify(&f.rules);
            assert!(
                !admission.admits_native(),
                "[{}] this shape must be REFUSED (not certified terminating); got {:?}",
                f.label,
                admission
            );
            match admission {
                ChaseAdmission::Uncertified { violations } => assert!(
                    !violations.is_empty(),
                    "[{}] a refusal must carry ≥1 weak-acyclicity violation",
                    f.label
                ),
                ChaseAdmission::WeaklyAcyclic { .. } => {
                    unreachable!("[{}] just asserted not admits_native", f.label)
                }
            }
        }
    }

    // ── Forward corpus: stratifiable binary Datalog± programs ────────────────────────
    //
    // Each forward program is a SINGLE `.rls` rule string + a SINGLE N-Quads input string.
    // The SAME pair drives BOTH engines: native parses the `.rls` via `parse_eval_rules` and
    // loads the N-Quads into a `WorldStore`; Nemo runs `materialize_core(rls, nquads, ..)`.
    // So the two engines materialize the identical program.

    /// One forward corpus program: a human label, the world IRI, the `.rls` rules, the
    /// N-Quads EDB, and whether a non-trivial derived closure is expected (so the floor can
    /// demand `> 0` native-decided derived rows where a closure must exist).
    struct ForwardProgram {
        label: &'static str,
        world: &'static str,
        rls: String,
        nquads: String,
        expect_derived: bool,
    }

    const LNS: &str = "https://blackcatinformatics.ca/logic/";

    /// (a) subClassOf transitive closure: Dog ⊑ Mammal ⊑ Animal in one world.
    fn forward_subclass_chain() -> ForwardProgram {
        let world = "http://world/Alpha";
        let sco = format!("{LNS}subClassOf");
        let rls = format!(
            "#[name(\"{LNS}rules/subClassOf-transitivity\")]\n\
             <{sco}>(?X, ?Z, ?C0) :-\n\
                 <{sco}>(?X, ?Y, ?C0),\n\
                 <{sco}>(?Y, ?Z, ?C1) .\n"
        );
        let nquads = format!(
            "<http://example.org/Dog> <{sco}> <http://example.org/Mammal> <{world}> .\n\
             <http://example.org/Mammal> <{sco}> <http://example.org/Animal> <{world}> .\n"
        );
        ForwardProgram {
            label: "subclass-chain",
            world,
            rls,
            nquads,
            expect_derived: true,
        }
    }

    /// (a') ancestor transitive closure over a parentOf chain a→b→c→d.
    fn forward_ancestor_chain() -> ForwardProgram {
        let world = "http://world/Kin";
        let parent = "http://example.org/parentOf";
        let anc = "http://example.org/ancestor";
        let rls = format!(
            "#[name(\"http://example.org/rules/ancestorBase\")]\n\
             <{anc}>(?X, ?Y, ?W) :- <{parent}>(?X, ?Y, ?W) .\n\
             #[name(\"http://example.org/rules/ancestorStep\")]\n\
             <{anc}>(?X, ?Y, ?W) :-\n\
                 <{parent}>(?X, ?Z, ?W),\n\
                 <{anc}>(?Z, ?Y, ?W) .\n"
        );
        let nquads = format!(
            "<http://example.org/a> <{parent}> <http://example.org/b> <{world}> .\n\
             <http://example.org/b> <{parent}> <http://example.org/c> <{world}> .\n\
             <http://example.org/c> <{parent}> <http://example.org/d> <{world}> .\n"
        );
        ForwardProgram {
            label: "ancestor-chain",
            world,
            rls,
            nquads,
            expect_derived: true,
        }
    }

    /// (b) a multi-rule program: a type rule plus a transitive subClassOf, with an
    /// instance-of propagation up the class chain.
    /// `type(?I, ?C2) :- type(?I, ?C1), subClassOf(?C1, ?C2)` and subClassOf transitivity.
    fn forward_multi_rule() -> ForwardProgram {
        let world = "http://world/Multi";
        let sco = format!("{LNS}subClassOf");
        let typ = format!("{LNS}type");
        let rls = format!(
            "#[name(\"{LNS}rules/sco-trans\")]\n\
             <{sco}>(?X, ?Z, ?W) :- <{sco}>(?X, ?Y, ?W), <{sco}>(?Y, ?Z, ?W) .\n\
             #[name(\"{LNS}rules/type-propagate\")]\n\
             <{typ}>(?I, ?C2, ?W) :- <{typ}>(?I, ?C1, ?W), <{sco}>(?C1, ?C2, ?W) .\n"
        );
        let nquads = format!(
            "<http://example.org/Rex> <{typ}> <http://example.org/Dog> <{world}> .\n\
             <http://example.org/Dog> <{sco}> <http://example.org/Mammal> <{world}> .\n\
             <http://example.org/Mammal> <{sco}> <http://example.org/Animal> <{world}> .\n"
        );
        ForwardProgram {
            label: "multi-rule",
            world,
            rls,
            nquads,
            expect_derived: true,
        }
    }

    /// (c) stratified negation forward: reachable closure then unreachable via `~reachable`.
    /// (self-loop `s == o` encoding for the unary `reachable`/`unreachable`/`node` predicates.)
    fn forward_stratified_negation() -> ForwardProgram {
        let world = "http://world/Reach";
        let ns = "http://example.org/sn/";
        let rls = format!(
            "#[name(\"{ns}rReachSeed\")]\n\
             <{ns}reachable>(?X, ?X, ?W) :- <{ns}reachableSeed>(?X, ?X, ?W) .\n\
             #[name(\"{ns}rReachStep\")]\n\
             <{ns}reachable>(?Y, ?Y, ?W) :-\n\
                 <{ns}reachable>(?X, ?X, ?W),\n\
                 <{ns}edge>(?X, ?Y, ?W) .\n\
             #[name(\"{ns}rUnreach\")]\n\
             <{ns}unreachable>(?X, ?X, ?W) :-\n\
                 <{ns}node>(?X, ?X, ?W),\n\
                 ~<{ns}reachable>(?X, ?X, ?W) .\n"
        );
        let nquads = format!(
            "<{ns}a> <{ns}node> <{ns}a> <{world}> .\n\
             <{ns}b> <{ns}node> <{ns}b> <{world}> .\n\
             <{ns}c> <{ns}node> <{ns}c> <{world}> .\n\
             <{ns}a> <{ns}reachableSeed> <{ns}a> <{world}> .\n\
             <{ns}a> <{ns}edge> <{ns}b> <{world}> .\n"
        );
        ForwardProgram {
            label: "stratified-negation",
            world,
            rls,
            nquads,
            expect_derived: true,
        }
    }

    fn forward_corpus() -> Vec<ForwardProgram> {
        vec![
            forward_subclass_chain(),
            forward_ancestor_chain(),
            forward_multi_rule(),
            forward_stratified_negation(),
        ]
    }

    /// Run the native forward engine on a corpus program, asserting a `Decided` outcome.
    fn run_native_forward(p: &ForwardProgram) -> Vec<DerivedRow> {
        let store = WorldStore::new();
        store
            .load_nquads(&p.nquads)
            .unwrap_or_else(|e| panic!("[{}] WorldStore load failed: {e}", p.label));
        let rules = parse_eval_rules(&p.rls)
            .unwrap_or_else(|e| panic!("[{}] parse_eval_rules failed: {e}", p.label));
        // The coverage floor runs unbudgeted (`None`), so the native step governor never
        // trips: the returned status is always `Ok` and the full least model is produced.
        // The corpus program is stratifiable by construction, so the type-state pipeline's
        // `stratify()` always yields the `Executable` the executor requires.
        let executable = Parsed::uncached(&rules)
            .stratify()
            .unwrap_or_else(|| panic!("[{}] corpus program must be stratifiable", p.label))
            .plan()
            .into_executable();
        match materialize_native(&store, &executable, None)
            .unwrap_or_else(|e| panic!("[{}] materialize_native errored: {e}", p.label))
        {
            NativeOutcome::Decided(budgeted) => budgeted.rows,
            NativeOutcome::Unsupported(kind) => panic!(
                "[{}] native FELL BACK to Unsupported({kind:?}) — the coverage floor demands a \
                 Decided outcome for every stratifiable corpus program",
                p.label
            ),
        }
    }

    /// The derived (non-EDB-echo) rows: those whose firing rule is not the assert sentinel.
    fn derived_rows(rows: &[DerivedRow]) -> Vec<&DerivedRow> {
        rows.iter()
            .filter(|r| r.rule_iri != crate::provenance::ASSERT_RULE_IRI)
            .collect()
    }

    // ── Forward parity: native ≡ Nemo (THE GATE) ─────────────────────────────────────

    #[test]
    fn materialize_parity_native_agrees_with_nemo() {
        let mut total_native_decided = 0usize;
        for p in forward_corpus() {
            let native = run_native_forward(&p);
            // Drive the oracle through the ForwardOracle seam over the SAME typed EDB
            // (built by the shared `edb_from_nquads`) and rule text native used.
            let edb = crate::materialize::edb_from_nquads(&p.nquads)
                .unwrap_or_else(|e| panic!("[{}] edb_from_nquads failed: {e}", p.label));

            let ledger =
                compare_materialization(&native, &NemoForwardOracle, &edb, &p.rls, p.world)
                    .unwrap_or_else(|e| {
                        panic!("[{}] forward oracle materialize failed: {e}", p.label)
                    });
            let verdict = ledger.enforce();
            assert!(
                verdict.passed,
                "[{}] native↔Nemo materialization DIVERGED ({} native-only, {} oracle-only): {:?}\n\
                 divergent rows: {:?}",
                p.label,
                ledger.native_only,
                ledger.oracle_only,
                verdict.reasons,
                ledger
                    .rows
                    .iter()
                    .filter(|r| r.kind != DivergenceKind::Agree)
                    .collect::<Vec<_>>()
            );
            assert!(
                ledger.agree > 0,
                "[{}] parity ledger must have at least one agreeing fact",
                p.label
            );
            total_native_decided += native.len();
        }
        assert!(
            total_native_decided > 0,
            "the native engine decided ZERO forward rows across the whole corpus — a total \
             fallback is a coverage-floor failure"
        );
    }

    /// A forward oracle that returns a fixed closure independent of its inputs — used to prove
    /// the generic [`compare_materialization`] still CATCHES divergence after going generic.
    /// Every other forward test is a green-path agreement test, so a broken comparator could
    /// pass them all; this one gives the comparator a genuinely disagreeing oracle.
    struct DivergentForwardOracle {
        rows: Vec<(TypedRow, TypedProvenance)>,
    }

    impl ForwardOracle for DivergentForwardOracle {
        fn name(&self) -> &'static str {
            "divergent-mock"
        }
        fn materialize(
            &self,
            _facts: &crate::facts::TypedFactSet,
            _rules: &str,
            _budget: &ForwardBudget,
        ) -> gmeow_errors::Result<TypedChaseResult> {
            Ok(TypedChaseResult {
                rows: self.rows.clone(),
            })
        }
        fn provides_provenance(&self) -> bool {
            false
        }
    }

    #[test]
    fn materialize_parity_divergence_is_caught() {
        // Native decides a real, non-empty closure.
        let p = forward_subclass_chain();
        let native = run_native_forward(&p);
        assert!(!native.is_empty(), "native must decide the subclass chain");
        let edb = crate::materialize::edb_from_nquads(&p.nquads).unwrap();

        // An oracle returning an EMPTY closure disagrees with native on every fact.
        let empty_oracle = DivergentForwardOracle { rows: vec![] };
        let ledger = compare_materialization(&native, &empty_oracle, &edb, &p.rls, p.world)
            .expect("comparator must run");
        let verdict = ledger.enforce();
        assert!(
            !verdict.passed,
            "a divergent (empty) oracle closure must FAIL the generic gate, not pass it"
        );
        assert!(
            ledger.native_only > 0,
            "native facts absent from the oracle must be classified native-only"
        );
    }

    // ── Backward corpus: binary positive query programs ──────────────────────────────

    const BASE: &str = "https://example.org/";

    /// One backward corpus program: a label, the world's EDB triples, and the query source.
    struct BackwardProgram {
        label: &'static str,
        triples: Vec<(String, String, String)>,
        program: QProgram,
    }

    fn p(s: &str) -> String {
        format!("{BASE}{s}")
    }

    /// (a) recursive transitive-closure ancestor query (fb/ff/bf covered by variants).
    fn backward_ancestor_ff() -> BackwardProgram {
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(X, Y).\n"
        );
        BackwardProgram {
            label: "ancestor-ff",
            triples: vec![
                (p("a"), p("parentOf"), p("b")),
                (p("b"), p("parentOf"), p("c")),
                (p("c"), p("parentOf"), p("d")),
            ],
            program: parse_query_program(&src).expect("parse ancestor-ff"),
        }
    }

    /// (a') the same closure with a bound-subject (bf) goal.
    fn backward_ancestor_bf() -> BackwardProgram {
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, Y).\n"
        );
        BackwardProgram {
            label: "ancestor-bf",
            triples: vec![
                (p("a"), p("parentOf"), p("b")),
                (p("b"), p("parentOf"), p("c")),
                (p("c"), p("parentOf"), p("d")),
            ],
            program: parse_query_program(&src).expect("parse ancestor-bf"),
        }
    }

    /// (b) a multi-rule program: ancestor over parentOf PLUS a relative(X,Y) rule that is the
    /// symmetric closure of ancestor, queried free.
    fn backward_multi_rule() -> BackwardProgram {
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ex:descendant(X, Y) :- ex:ancestor(Y, X).\n\
             ?- ex:descendant(X, ex:a).\n"
        );
        BackwardProgram {
            label: "descendant-multi",
            triples: vec![
                (p("a"), p("parentOf"), p("b")),
                (p("b"), p("parentOf"), p("c")),
            ],
            program: parse_query_program(&src).expect("parse descendant-multi"),
        }
    }

    /// (c) a ground (bb) goal that is present, and an absent one — two answer shapes.
    fn backward_ground_present() -> BackwardProgram {
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Y).\n\
             ex:ancestor(X, Y) :- ex:parentOf(X, Z), ex:ancestor(Z, Y).\n\
             ?- ex:ancestor(ex:a, ex:c).\n"
        );
        BackwardProgram {
            label: "ground-present",
            triples: vec![
                (p("a"), p("parentOf"), p("b")),
                (p("b"), p("parentOf"), p("c")),
            ],
            program: parse_query_program(&src).expect("parse ground-present"),
        }
    }

    /// (d) a binary ARITHMETIC program: recursive list length via `N is M + 1`.
    /// Native magic and the reference SLD oracle both evaluate the builtin (via the
    /// shared moded evaluator), so this proves engine INTEGRATION (both decide and
    /// agree) — the per-operator semantic anchor is the hand-verified
    /// `builtin_eval` golden.
    fn backward_arithmetic_length() -> BackwardProgram {
        let rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- prefix(rdf, '{rdf}').\n\
             ex:len(rdf:nil, 0).\n\
             ex:len(L, N) :- rdf:rest(L, R), ex:len(R, M), N is M + 1.\n\
             ?- ex:len(ex:l0, N).\n"
        );
        BackwardProgram {
            label: "arithmetic-length",
            triples: vec![
                (p("l0"), format!("{rdf}rest"), p("l1")),
                (p("l1"), format!("{rdf}rest"), p("l2")),
                (p("l2"), format!("{rdf}rest"), format!("{rdf}nil")),
            ],
            program: parse_query_program(&src).expect("parse arithmetic-length"),
        }
    }

    /// (e) THE leading-bound recursive-IDB bug shape (F1): an ALL-FREE-head goal rule whose
    /// body LEADS with a recursive IDB atom carrying a bound argument FROM A CONSTANT.
    ///
    /// `ex:c(P, S) :- ex:reach(ex:self, P), ex:nameMatch(P, S).` under the all-free goal
    /// `?- ex:c(P, S).` mints NO head guard (the goal is `ff`), so the leading `reach(ex:self, P)`
    /// adorns `bf` FROM THE CONSTANT `ex:self` — the exact adornment whose per-atom demand rule
    /// had an EMPTY body (`magic/reach_bf(self, self) :- .`) and was silently dropped by the
    /// semi-naive engine pre-fix, returning an empty answer set with `status: Ok`. Placing an EDB
    /// atom first (the permuted order the gate also sweeps) hid the drop. `reach` is
    /// right-recursive (EDB-first), so the reference path-memo resolver is complete on it and the
    /// two engines MUST agree. World: `knows(self, a)`, `knows(a, b)` ⇒ `reach(self, ·) = {a, b}`;
    /// only `b` has a `nameMatch`, so the correct answer is the single `c(b, nameB)` — non-empty,
    /// so the pre-fix drop is observable.
    fn backward_leading_idb_reach() -> BackwardProgram {
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:reach(X, Y) :- ex:knows(X, Y).\n\
             ex:reach(X, Y) :- ex:knows(X, Z), ex:reach(Z, Y).\n\
             ex:c(P, S) :- ex:reach(ex:self, P), ex:nameMatch(P, S).\n\
             ?- ex:c(P, S).\n"
        );
        BackwardProgram {
            label: "leading-idb-reach",
            triples: vec![
                (p("self"), p("knows"), p("a")),
                (p("a"), p("knows"), p("b")),
                (p("b"), p("nameMatch"), p("nameB")),
            ],
            program: parse_query_program(&src).expect("parse leading-idb-reach"),
        }
    }

    /// (f) THE SECOND bodyless site (Site B): an all-free goal over a ground FACT-RULE.
    ///
    /// `ex:p(ex:a, ex:b).` is a bodyless positive rule whose head is ground; under the all-free
    /// goal `?- ex:p(X, Y).` the modified-rule site produced an empty `mod_body` with a ground
    /// head — an unconditional fact the semi-naive engine never fires — so `p(a, b)` was lost
    /// pre-fix. The correct answer is the single `p(a, b)` (no EDB triples: the fact-rule is the
    /// sole source), so a drop is observable.
    fn backward_ff_fact_rule() -> BackwardProgram {
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:p(ex:a, ex:b).\n\
             ?- ex:p(X, Y).\n"
        );
        BackwardProgram {
            label: "ff-fact-rule",
            triples: vec![],
            program: parse_query_program(&src).expect("parse ff-fact-rule"),
        }
    }

    fn backward_corpus() -> Vec<BackwardProgram> {
        vec![
            backward_ancestor_ff(),
            backward_ancestor_bf(),
            backward_multi_rule(),
            backward_ground_present(),
            backward_arithmetic_length(),
            backward_leading_idb_reach(),
            backward_ff_fact_rule(),
        ]
    }

    /// Build a `WorldFactSnapshot` from a backward program's EDB triples, returning it with the
    /// world IRI.
    fn backward_world(b: &BackwardProgram) -> (WorldStore, String) {
        const W: &str = "http://logic.test/world/parity";
        let store = WorldStore::new();
        for (s, pr, o) in &b.triples {
            store.insert_quad(W, s, pr, o);
        }
        (store, W.to_owned())
    }

    /// Run the native backward engine, asserting a `Decided` outcome (the coverage floor).
    fn run_native_backward(b: &BackwardProgram) -> AnswerSet {
        let (store, world_nn) = backward_world(b);
        const W: &str = "http://logic.test/world/parity";
        let foreign =
            WorldFactSnapshot::from_world(&store, W, PROFILE).expect("from_world must succeed");
        match resolve_native(&foreign, &world_nn, &b.program, &Budget::default())
            .unwrap_or_else(|e| panic!("[{}] resolve_native errored: {e}", b.label))
        {
            NativeOutcome::Decided(a) => a,
            NativeOutcome::Unsupported(kind) => panic!(
                "[{}] native backward FELL BACK to Unsupported({kind:?}) — the coverage floor \
                 demands a Decided outcome for every backward corpus program",
                b.label
            ),
        }
    }

    // ── Backward parity: native ≡ reference SLD oracle (THE GATE) ─────────────────────

    #[test]
    fn dispatch_parity_native_agrees_with_reference() {
        let mut total_native_answers = 0usize;
        for b in backward_corpus() {
            let native = run_native_backward(&b);
            // Drive the oracle through the BackwardOracle seam over the SAME world snapshot,
            // program, and budget native used.
            let (store, world_nn) = backward_world(&b);
            let foreign = WorldFactSnapshot::from_world(&store, &world_nn, PROFILE)
                .expect("from_world must succeed");

            let ledger = compare_answers(
                &native,
                &ReferenceBackwardOracle,
                &foreign,
                &world_nn,
                &b.program,
                &Budget::default(),
            )
            .unwrap_or_else(|e| panic!("[{}] backward oracle solve failed: {e}", b.label));
            let verdict = ledger.enforce();
            assert!(
                verdict.passed,
                "[{}] native↔reference answer set DIVERGED ({} native-only, {} oracle-only): \
                 {:?}\nnative {:?}\ndivergent rows {:?}",
                b.label,
                ledger.native_only,
                ledger.oracle_only,
                verdict.reasons,
                native.bindings,
                ledger
                    .rows
                    .iter()
                    .filter(|r| r.kind != DivergenceKind::Agree)
                    .collect::<Vec<_>>()
            );
            total_native_answers += native.bindings.len();
        }
        assert!(
            total_native_answers > 0,
            "the native engine answered ZERO backward queries across the whole corpus — a total \
             fallback is a coverage-floor failure"
        );
    }

    // ── Body-order permutation invariance: native ≡ reference for EVERY permutation ───
    //
    // The invariant under test is "any body permutation → identical answer set". The magic-sets
    // transform is SIPS-dependent, so different body orders legitimately mint different demand
    // predicates — only the GOAL ANSWER SET is order-invariant (the correctness theorem), which
    // is inherently semantic. So the gate sweeps every body-atom permutation of each permutable
    // rule and demands native == the `ReferenceBackwardOracle` for ALL of them.

    /// Every permutation of the index range `0..n`, in a FIXED lexicographic order — NO RNG, no
    /// clock. `n == 0` yields the single empty permutation. Determinism is a CONSTITUTION hard
    /// constraint, so the sweep must be reproducible byte-for-byte.
    fn index_permutations(n: usize) -> Vec<Vec<usize>> {
        let mut out: Vec<Vec<usize>> = Vec::new();
        let items: Vec<usize> = (0..n).collect();
        permute_into(&items, &mut Vec::new(), &mut out);
        out
    }

    /// Recursive lexicographic permutation builder: at each position pick the next unused index
    /// in ascending order, so the emitted permutations are sorted.
    fn permute_into(remaining: &[usize], acc: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if remaining.is_empty() {
            out.push(acc.clone());
            return;
        }
        for i in 0..remaining.len() {
            let mut rest = remaining.to_vec();
            let chosen = rest.remove(i);
            acc.push(chosen);
            permute_into(&rest, acc, out);
            acc.pop();
        }
    }

    #[test]
    fn dispatch_parity_body_permutations_agree_with_reference() {
        let mut checked_permutations = 0usize;
        let mut leading_idb_swept = false;
        for b in backward_corpus() {
            for (ri, rule) in b.program.rules.iter().enumerate() {
                // Permute ONLY a rule whose body is ≥2 pure positive atoms AND is NOT
                // self-recursive. Reordering a non-self-recursive positive conjunction never
                // changes the least model, so BOTH engines must return the same answer set —
                // whereas permuting a self-recursive rule (e.g. `ancestor`) would mint a
                // LEFT-recursive body on which the reference path-memo legitimately UNDER-produces
                // (a documented reference limitation, not a native bug), so it is excluded. A
                // builtin/negation body literal is likewise excluded (order can bind its operands).
                let head_pred = rule.head.pred.clone();
                let all_positive_atoms =
                    rule.body.iter().all(|lit| matches!(lit, QBodyLit::Atom(_)));
                let self_recursive = rule
                    .body
                    .iter()
                    .any(|lit| matches!(lit, QBodyLit::Atom(a) if a.pred == head_pred));
                if rule.body.len() < 2 || !all_positive_atoms || self_recursive {
                    continue;
                }

                for perm in index_permutations(rule.body.len()) {
                    let mut program = b.program.clone();
                    program.rules[ri].body = perm.iter().map(|&i| rule.body[i].clone()).collect();

                    let (store, world_nn) = backward_world(&b);
                    let foreign = WorldFactSnapshot::from_world(&store, &world_nn, PROFILE)
                        .expect("from_world must succeed");
                    let native = match resolve_native(
                        &foreign,
                        &world_nn,
                        &program,
                        &Budget::default(),
                    )
                    .unwrap_or_else(|e| {
                        panic!(
                            "[{} r{ri} perm {perm:?}] resolve_native errored: {e}",
                            b.label
                        )
                    }) {
                        NativeOutcome::Decided(a) => a,
                        NativeOutcome::Unsupported(kind) => panic!(
                            "[{} r{ri} perm {perm:?}] native FELL BACK to Unsupported({kind:?}) — \
                             every body permutation of a positive-Horn rule must still DECIDE",
                            b.label
                        ),
                    };

                    let ledger = compare_answers(
                        &native,
                        &ReferenceBackwardOracle,
                        &foreign,
                        &world_nn,
                        &program,
                        &Budget::default(),
                    )
                    .unwrap_or_else(|e| {
                        panic!(
                            "[{} r{ri} perm {perm:?}] backward oracle solve failed: {e}",
                            b.label
                        )
                    });
                    let verdict = ledger.enforce();
                    assert!(
                        verdict.passed,
                        "[{} r{ri} perm {perm:?}] native↔reference answer set DIVERGED under body \
                         permutation ({} native-only, {} oracle-only): {:?}\nnative {:?}\ndivergent \
                         rows {:?}",
                        b.label,
                        ledger.native_only,
                        ledger.oracle_only,
                        verdict.reasons,
                        native.bindings,
                        ledger
                            .rows
                            .iter()
                            .filter(|r| r.kind != DivergenceKind::Agree)
                            .collect::<Vec<_>>()
                    );

                    // F1 teeth: the leading-bound recursive-IDB program's answer set must be
                    // NON-empty under EVERY permutation — including the `reach(self, P)`-first
                    // order that dropped the demand seed pre-fix. A future regression that empties
                    // this join fails loudly here, not silently as an empty-`Ok`.
                    if b.label == "leading-idb-reach" {
                        assert!(
                            !native.bindings.is_empty(),
                            "[{} r{ri} perm {perm:?}] the reach/c join produced ZERO answers — the \
                             leading-bound recursive-IDB demand was dropped (a soundness bug); the \
                             correct answer set is the single c(b, nameB)",
                            b.label
                        );
                        leading_idb_swept = true;
                    }
                    checked_permutations += 1;
                }
            }
        }
        assert!(
            leading_idb_swept,
            "the leading-idb-reach bug-shape program must be in the corpus and its goal rule swept \
             — otherwise this gate is vacuous (F1)"
        );
        assert!(
            checked_permutations > 0,
            "the permutation gate exercised ZERO permutations — a vacuous gate"
        );
    }

    // ── The committed native-coverage floor (a zero-decided run is a FAILURE) ─────────

    #[test]
    fn native_coverage_floor() {
        // Forward: every stratifiable forward corpus program MUST be Decided natively, and a
        // program expecting a closure must produce > 0 derived (non-echo) rows.
        let mut fwd_decided_rows = 0usize;
        let mut fwd_decided_derived = 0usize;
        for p in forward_corpus() {
            let rows = run_native_forward(&p); // panics if Unsupported
            let derived = derived_rows(&rows);
            assert!(
                !rows.is_empty(),
                "[{}] native decided but produced no rows at all",
                p.label
            );
            if p.expect_derived {
                assert!(
                    !derived.is_empty(),
                    "[{}] a closure was expected but native derived zero non-echo rows",
                    p.label
                );
            }
            fwd_decided_rows += rows.len();
            fwd_decided_derived += derived.len();
        }

        // Backward: every backward corpus program MUST be Decided natively.
        let mut bwd_decided_answers = 0usize;
        for b in backward_corpus() {
            let answer = run_native_backward(&b); // panics if Unsupported
            bwd_decided_answers += answer.bindings.len();
        }

        // The floor: a run where native fell back EVERYWHERE (zero decided) is a hard failure.
        assert!(
            fwd_decided_rows > 0,
            "native coverage floor breached: ZERO forward rows decided natively"
        );
        assert!(
            fwd_decided_derived > 0,
            "native coverage floor breached: ZERO derived (closure) rows decided natively"
        );
        assert!(
            bwd_decided_answers > 0,
            "native coverage floor breached: ZERO backward answers decided natively"
        );

        // Audit print (surfaced on the slow/failure status level) so the floor is inspectable.
        println!(
            "native-coverage floor: forward decided rows={fwd_decided_rows} \
             (derived={fwd_decided_derived}), backward decided answers={bwd_decided_answers}"
        );
    }

    // ── Termination parity against the retained SLD reference ──────────────────
    //
    // The reference SLD oracle is a path-memo resolver: complete + terminating for
    // RIGHT-recursive (EDB-first) programs, but it UNDER-PRODUCES on LEFT-recursion — an
    // on-stack re-entry of the recursive goal returns no fresh binding. Native demand
    // transformation saturates the finite Herbrand base and is complete for both
    // recursion shapes. These tests pin equality for right recursion and strict native
    // subsumption for the reference resolver's documented left-recursion limitation.

    /// The three cyclic edge triples shared by both termination-parity tests: a→b, b→c,
    /// c→a — a genuine 3-cycle, so a path-memo MUST cut a back-edge to terminate and an
    /// un-tabled SLD engine would loop forever.
    fn cyclic_edge_triples() -> Vec<(String, String, String)> {
        vec![
            (p("a"), p("edge"), p("b")),
            (p("b"), p("edge"), p("c")),
            (p("c"), p("edge"), p("a")),
        ]
    }

    /// Seed the cyclic edge world, parse `src`, and resolve it natively (asserting a
    /// `Decided` outcome — the coverage floor). Returns the native answer set with the
    /// backing store, world IRI, and parsed program so each oracle can be replayed over the
    /// SAME inputs. The call RETURNING is the native-termination proof.
    fn native_over_cyclic(src: &str) -> (AnswerSet, WorldStore, String, QProgram) {
        const W: &str = "http://logic.test/world/termination-parity";
        let store = WorldStore::new();
        for (s, pr, o) in cyclic_edge_triples() {
            store.insert_quad(W, &s, &pr, &o);
        }
        let program = parse_query_program(src).expect("parse cyclic program");
        let foreign = WorldFactSnapshot::from_world(&store, W, PROFILE).expect("from_world");
        let native = match resolve_native(&foreign, W, &program, &Budget::default())
            .expect("resolve_native must not error on the cyclic termination corpus")
        {
            NativeOutcome::Decided(a) => a,
            NativeOutcome::Unsupported(k) => panic!(
                "native must DECIDE the cyclic termination program (magic-sets saturates the \
                 finite Herbrand base), got Unsupported({k:?})"
            ),
        };
        (native, store, W.to_owned(), program)
    }

    #[test]
    fn dispatch_termination_parity_right_recursive_matches_reference() {
        // Right-recursive transitive closure over the cyclic edge graph. The body is
        // EDB-first, so the path-memo reference resolver is complete and terminating here.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:path(X, Y) :- ex:edge(X, Y).\n\
             ex:path(X, Y) :- ex:edge(X, Z), ex:path(Z, Y).\n\
             ?- ex:path(X, Y).\n"
        );
        let (native, store, world, program) = native_over_cyclic(&src);
        assert!(
            !native.bindings.is_empty(),
            "native produced ZERO answers on the cyclic right-recursive closure — a cyclic \
             transitive closure must decide a non-empty answer set"
        );
        let foreign = WorldFactSnapshot::from_world(&store, &world, PROFILE).expect("from_world");

        // Native ≡ reference-SLD, gap-zero (the path-memo is complete for right recursion).
        let ref_ledger = compare_answers(
            &native,
            &ReferenceBackwardOracle,
            &foreign,
            &world,
            &program,
            &Budget::default(),
        )
        .expect("reference-SLD oracle solve");
        let ref_verdict = ref_ledger.enforce();
        assert!(
            ref_verdict.passed,
            "native↔reference-SLD DIVERGED on cyclic right recursion ({} native-only, {} \
             oracle-only): {:?}",
            ref_ledger.native_only, ref_ledger.oracle_only, ref_verdict.reasons
        );
    }

    #[test]
    fn dispatch_termination_left_recursive_native_subsumes_reference() {
        // Left-recursive transitive closure over the SAME cyclic edge graph. The recursive
        // goal is leftmost, so an un-tabled SLD engine loops immediately; the path-memo
        // reference resolver terminates but under-produces; native remains complete.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:path(X, Y) :- ex:edge(X, Y).\n\
             ex:path(X, Y) :- ex:path(X, Z), ex:edge(Z, Y).\n\
             ?- ex:path(X, Y).\n"
        );
        let (native, store, world, program) = native_over_cyclic(&src);
        assert!(
            !native.bindings.is_empty(),
            "native produced ZERO answers on the cyclic left-recursive closure"
        );
        let foreign = WorldFactSnapshot::from_world(&store, &world, PROFILE).expect("from_world");
        // Now EMPIRICALLY observe the path-memo reference oracle on LEFT recursion. Native is
        // complete, so it can never lack an answer the incomplete path-memo produced: the
        // reference answer set is a SUBSET of native's — `oracle_only` must be zero.
        let ref_ledger = compare_answers(
            &native,
            &ReferenceBackwardOracle,
            &foreign,
            &world,
            &program,
            &Budget::default(),
        )
        .expect("reference-SLD oracle solve");
        assert_eq!(
            ref_ledger.oracle_only, 0,
            "the path-memo reference resolver produced {} answer(s) native did not — native is \
             complete for the fragment, so this must never happen",
            ref_ledger.oracle_only
        );
        // The demonstrated completeness win: the path-memo under-produces on left recursion,
        // so native answers strictly more (native_only > 0). Assert the empirically-true
        // relation observed on this cyclic graph.
        assert!(
            ref_ledger.native_only > 0,
            "expected the path-memo reference resolver to UNDER-PRODUCE on cyclic left recursion \
             (native completes where the on-stack re-entry returns no binding), but it agreed \
             exactly ({} native-only) — native still subsumes it, but the completeness gap this \
             test documents did not materialise; re-verify the reference-resolver contract",
            ref_ledger.native_only
        );
    }

    // ── Per-consumer gate: counterfactual per-world native resolution ──────────
    //
    // `crate::counterfactual::resolve_in_world` resolves each closest world's goal via
    // `crate::dispatch::dispatch_query`, whose sole engine is `resolve_native`. This gate
    // covers the representative counterfactual fragments:
    //   * a RECURSIVE closure (reachability over a cyclic a→b→c→a graph) — native
    //     native demand transformation agrees with the retained reference resolver;
    //   * an N-ARY (world-carrying generic-triple `triple/4`) sub-property program —
    //     native decides the predicate-as-data goal a binary store cannot express;
    //   * a CUT program — `resolve_native` returns `Unsupported(Cut)` and production
    //     dispatch rejects it rather than simulating procedural control.

    /// The n-ary (world-carrying generic-triple `triple/4`) sub-property program:
    /// `triple(?s,<p2>,?o,?w) :- triple(?s,<p1>,?o,?w)` with the arity-4 backward goal
    /// `triple(?s,<p2>,?o,?w)`. The predicate rides in a DATA position, so this is the
    /// native n-ary backward capability a binary store cannot express.
    fn cf_nary_subproperty_program() -> QProgram {
        let triple_atom = |pred: &str| QAtom {
            pred: "triple".to_owned(),
            args: vec![
                QTerm::Var("s".to_owned()),
                QTerm::Const(format!("<{pred}>")),
                QTerm::Var("o".to_owned()),
                QTerm::Var("w".to_owned()),
            ],
        };
        QProgram {
            rules: vec![QRule {
                head: triple_atom("http://ex/p2"),
                body: vec![QBodyLit::Atom(triple_atom("http://ex/p1"))],
            }],
            goal: QGoal {
                atoms: vec![triple_atom("http://ex/p2")],
            },
            counterfactual: None,
            prob_facts: vec![],
            prob_model: None,
            confidences: vec![],
        }
    }

    #[test]
    fn dispatch_parity_counterfactual_fragment_is_native() {
        // (1) RECURSIVE closure — reachability over the cyclic edge graph a→b→c→a. Native
        // saturates the finite Herbrand base and matches the retained reference resolver.
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:reach(X, Y) :- ex:edge(X, Y).\n\
             ex:reach(X, Y) :- ex:edge(X, Z), ex:reach(Z, Y).\n\
             ?- ex:reach(X, Y).\n"
        );
        let (native, store, world, program) = native_over_cyclic(&src);
        assert!(
            !native.bindings.is_empty(),
            "cyclic reachability must decide a non-empty answer set"
        );
        let foreign = WorldFactSnapshot::from_world(&store, &world, PROFILE).expect("from_world");
        let ledger = compare_answers(
            &native,
            &ReferenceBackwardOracle,
            &foreign,
            &world,
            &program,
            &Budget::default(),
        )
        .expect("reference resolver solve");
        let verdict = ledger.enforce();
        assert!(
            verdict.passed,
            "counterfactual recursive fragment: native↔reference DIVERGED ({} native-only, {} \
             oracle-only): {:?}",
            ledger.native_only, ledger.oracle_only, verdict.reasons
        );

        // (2) N-ARY generic-triple sub-property — assert native `Decided` with the
        // derived edge.
        const NW: &str = "http://logic.test/world/cf-nary";
        let nstore = WorldStore::new();
        nstore.insert_quad(NW, "http://ex/x", "http://ex/p1", "http://ex/y");
        let nforeign = WorldFactSnapshot::from_world(&nstore, NW, PROFILE).expect("from_world");
        let nprog = cf_nary_subproperty_program();
        assert_eq!(
            nprog.goal.atoms[0].args.len(),
            4,
            "arity-4 generic-triple goal ⇒ native n-ary backward path"
        );
        let nanswer = match resolve_native(&nforeign, NW, &nprog, &Budget::default())
            .expect("resolve_native must not error on the n-ary generic program")
        {
            NativeOutcome::Decided(a) => a,
            NativeOutcome::Unsupported(k) => panic!(
                "n-ary generic-triple backward must DECIDE natively (predicate-as-data \
                 resolution the binary store cannot express), got Unsupported({k:?})"
            ),
        };
        assert_eq!(
            nanswer.bindings.len(),
            1,
            "exactly one derived <p2> edge: {nanswer:?}"
        );
        assert_eq!(nanswer.bindings[0]["s"], "<http://ex/x>", "subject binding");
        assert_eq!(nanswer.bindings[0]["o"], "<http://ex/y>", "object binding");

        // (3) CUT — a declared native gap, never a native cut simulation.
        const CW: &str = "http://logic.test/world/cf-cut";
        let cstore = WorldStore::new();
        cstore.insert_quad(CW, &p("a"), &p("edge"), &p("b"));
        cstore.insert_quad(CW, &p("a"), &p("edge"), &p("c"));
        let cforeign = WorldFactSnapshot::from_world(&cstore, CW, PROFILE).expect("from_world");
        let cut_src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:first(X, Y) :- ex:edge(X, Y), !.\n\
             ?- ex:first(ex:a, Y).\n"
        );
        let cut_prog = parse_query_program(&cut_src).expect("parse cut program");
        let cut_outcome =
            resolve_native(&cforeign, CW, &cut_prog, &Budget::default()).expect("resolve_native");
        assert!(
            matches!(
                cut_outcome,
                NativeOutcome::Unsupported(crate::physical::seminaive::UnsupportedKind::Cut)
            ),
            "a cut-bearing counterfactual goal must be a declared native gap, never a \
             native cut simulation: {cut_outcome:?}"
        );
    }

    #[test]
    fn arithmetic_division_by_zero_is_declared_gap() {
        // A ÷0 in a supported binary program is a declared native gap
        // (`Unsupported(Arithmetic)`), surfaced as a production refusal — never a wrong answer.
        const W: &str = "http://logic.test/world/arith-gap";
        let store = WorldStore::new();
        store.insert_quad(W, &p("a"), &p("node"), &p("b"));
        let prof = "https://blackcatinformatics.ca/logic/ProceduralPrologProfile";
        let foreign = WorldFactSnapshot::from_world(&store, W, prof).expect("from_world");
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:bad(X, R) :- ex:node(X, Y), R is 1 // 0.\n\
             ?- ex:bad(ex:a, R).\n"
        );
        let prog = parse_query_program(&src).expect("parse");
        let outcome =
            resolve_native(&foreign, W, &prog, &Budget::default()).expect("resolve_native");
        assert!(
            matches!(
                outcome,
                NativeOutcome::Unsupported(crate::physical::seminaive::UnsupportedKind::Arithmetic)
            ),
            "÷0 must be a declared Arithmetic gap, not a wrong answer: {outcome:?}"
        );
    }

    // ── Backward parity UNDER a step budget ───────────────────────────────────────────

    /// Run the native backward engine at `budget`, asserting a `Decided` outcome.
    fn run_native_backward_budget(b: &BackwardProgram, budget: &Budget) -> AnswerSet {
        let (store, world_nn) = backward_world(b);
        let foreign = WorldFactSnapshot::from_world(&store, &world_nn, PROFILE)
            .expect("from_world must succeed");
        match resolve_native(&foreign, &world_nn, &b.program, budget)
            .unwrap_or_else(|e| panic!("[{}] resolve_native errored: {e}", b.label))
        {
            NativeOutcome::Decided(a) => a,
            NativeOutcome::Unsupported(kind) => {
                panic!(
                    "[{}] native backward Unsupported({kind:?}) under budget",
                    b.label
                )
            }
        }
    }

    /// Under a step budget the native backward engine is (1) DETERMINISTIC — byte-identical
    /// run-to-run at the same budget (the cut is the Nth FactKey-sorted committed winner) —
    /// and (2) SOUND — every budget-cut answer is present in the reference oracle's UNBOUNDED
    /// answer set.
    ///
    /// The soundness comparison is against reference-**unbounded**, NEVER reference-at-the-
    /// same-budget: the two engines count different step units (native counts committed
    /// derivations bottom-up; the reference counts rule expansions / EDB lookups top-down), so
    /// under the same `max_steps` native generally gets further and its answer set can be a
    /// strict SUPERSET of the reference's at that budget. The contract is outcome soundness
    /// (sound subset of the full model + `Exhausted`-not-wrong), never cross-engine step-count
    /// equivalence. Do NOT tighten this to reference-at-same-budget.
    #[test]
    fn dispatch_parity_native_sound_subset_and_deterministic_under_step_budget() {
        let tight = Budget {
            max_steps: Some(1),
            max_answers: None,
        };
        let mut any_exhausted = false;
        for b in backward_corpus() {
            // The reference oracle's UNBOUNDED answer set = the full sound model.
            let (store, world_nn) = backward_world(&b);
            let foreign = WorldFactSnapshot::from_world(&store, &world_nn, PROFILE)
                .expect("from_world must succeed");
            let full = ReferenceBackwardOracle
                .solve(&foreign, &world_nn, &b.program, &[], &Budget::default())
                .unwrap_or_else(|e| panic!("[{}] reference solve failed: {e}", b.label));
            let full_keys: BTreeSet<String> = full.bindings.iter().map(binding_key).collect();

            // Determinism: two runs at the same tight budget are byte-identical.
            let run1 = run_native_backward_budget(&b, &tight);
            let run2 = run_native_backward_budget(&b, &tight);
            assert_eq!(
                run1.bindings, run2.bindings,
                "[{}] native backward budget cut must be byte-identical run-to-run",
                b.label
            );
            assert_eq!(
                run1.status, run2.status,
                "[{}] status is deterministic",
                b.label
            );

            // Soundness: every budget-cut answer is in the reference-UNBOUNDED model.
            for bind in &run1.bindings {
                assert!(
                    full_keys.contains(&binding_key(bind)),
                    "[{}] budget-cut answer {bind:?} is NOT in the reference-unbounded model \
                     — a step budget must never fabricate an answer",
                    b.label
                );
            }
            if run1.status == BudgetStatus::Exhausted {
                any_exhausted = true;
            }
        }
        assert!(
            any_exhausted,
            "a 1-step budget must exhaust at least one recursive corpus program — the native \
             step governor must actually fire in the parity harness"
        );
    }

    /// The frontier win at the query surface: native COMPLETES (`Ok`) a pure-EDB goal under a
    /// zero-step budget, where the reference oracle — which counts the EDB lookup as a step —
    /// stamps `Exhausted`. This is the intended, documented divergence (different step units):
    /// native reports a complete answer needing no derivation; no cross-engine status parity is
    /// asserted.
    #[test]
    fn dispatch_parity_native_completes_where_reference_exhausts() {
        // A pure-EDB goal (the goal predicate is EDB; the program carries NO rules).
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ?- ex:parentOf(ex:a, Y).\n"
        );
        let b = BackwardProgram {
            label: "pure-edb",
            triples: vec![
                (p("a"), p("parentOf"), p("b")),
                (p("a"), p("parentOf"), p("c")),
            ],
            program: parse_query_program(&src).expect("parse pure-edb"),
        };
        let (store, world_nn) = backward_world(&b);
        let foreign = WorldFactSnapshot::from_world(&store, &world_nn, PROFILE)
            .expect("from_world must succeed");
        let zero = Budget {
            max_steps: Some(0),
            max_answers: None,
        };

        let native = run_native_backward_budget(&b, &zero);
        assert_eq!(
            native.status,
            BudgetStatus::Ok,
            "a pure-EDB goal needs no derivation ⇒ complete `Ok` under any step budget"
        );
        assert_eq!(
            native.bindings.len(),
            2,
            "native returns the complete pure-EDB answer"
        );

        let reference = ReferenceBackwardOracle
            .solve(&foreign, &world_nn, &b.program, &[], &zero)
            .expect("reference solve");
        assert_eq!(
            reference.status,
            BudgetStatus::Exhausted,
            "the reference oracle exhausts at budget 0 — native intentionally diverges (more faithful)"
        );
    }

    // ── Comparator unit coverage (no engine) ─────────────────────────────────────────

    #[test]
    fn parity_ledger_enforce_passes_on_pure_agreement() {
        let ledger = ParityLedger::from_rows(vec![LedgerRow {
            kind: DivergenceKind::Agree,
            category: "materialization".to_owned(),
            subject: "s".to_owned(),
            object: "o".to_owned(),
            world: "w".to_owned(),
            detail: "agree".to_owned(),
        }]);
        let verdict = ledger.enforce();
        assert!(verdict.passed, "pure agreement passes: {verdict:?}");
        assert_eq!(ledger.agree, 1);
        assert_eq!(ledger.native_only, 0);
        assert_eq!(ledger.oracle_only, 0);
    }

    #[test]
    fn parity_ledger_enforce_fails_on_native_only() {
        let ledger = ParityLedger::from_rows(vec![LedgerRow {
            kind: DivergenceKind::NativeOnly,
            category: "materialization".to_owned(),
            subject: "s".to_owned(),
            object: "o".to_owned(),
            world: "w".to_owned(),
            detail: "native only".to_owned(),
        }]);
        let verdict = ledger.enforce();
        assert!(!verdict.passed, "a native-only row must fail the gate");
        assert!(verdict.reasons.iter().any(|r| r.contains("native-only")));
    }

    #[test]
    fn parity_ledger_enforce_fails_on_oracle_only() {
        let ledger = ParityLedger::from_rows(vec![LedgerRow {
            kind: DivergenceKind::OracleOnly,
            category: "answer".to_owned(),
            subject: "X=<a>".to_owned(),
            object: String::new(),
            world: String::new(),
            detail: "oracle only".to_owned(),
        }]);
        let verdict = ledger.enforce();
        assert!(!verdict.passed, "an oracle-only row must fail the gate");
        assert!(verdict.reasons.iter().any(|r| r.contains("oracle-only")));
    }

    // ── NativeForwardOracle ↔ NemoForwardOracle parity (Task 1 seam check) ────────────

    /// Classify two forward oracles' materialized fact sets into a [`ParityLedger`]:
    /// `native` (the engine under test) vs `oracle` (the parity reference). Only arity-3
    /// [`TypedRow`]s are fact-level comparands ([`typed_row_fact_key`]); the world is the
    /// single-world constant carried on each row.
    fn ledger_between_oracles(
        native: &TypedChaseResult,
        oracle: &TypedChaseResult,
        world: &str,
    ) -> ParityLedger {
        let native_keys: BTreeSet<FactKey> = native
            .rows
            .iter()
            .filter_map(|(row, _prov)| typed_row_fact_key(row))
            .collect();
        let oracle_keys: BTreeSet<FactKey> = oracle
            .rows
            .iter()
            .filter_map(|(row, _prov)| typed_row_fact_key(row))
            .collect();

        let row_of = |kind: DivergenceKind, key: &FactKey| {
            let (subject, predicate, object) = key.clone();
            LedgerRow {
                kind,
                category: "materialization".to_owned(),
                detail: format!("native↔nemo {subject} {predicate} {object}"),
                subject,
                object,
                world: world.to_owned(),
            }
        };

        let mut rows: Vec<LedgerRow> = Vec::new();
        for key in native_keys.intersection(&oracle_keys) {
            rows.push(row_of(DivergenceKind::Agree, key));
        }
        for key in native_keys.difference(&oracle_keys) {
            rows.push(row_of(DivergenceKind::NativeOnly, key));
        }
        for key in oracle_keys.difference(&native_keys) {
            rows.push(row_of(DivergenceKind::OracleOnly, key));
        }
        ParityLedger::from_rows(rows)
    }

    /// The Task-1 seam check: [`NativeForwardOracle`] (gmeow's native semi-naive core,
    /// behind the [`ForwardOracle`] seam) agrees EXACTLY with [`NemoForwardOracle`] on the
    /// EL subsumption closure of a `subClassOf` chain — gap-zero, non-vacuously.
    ///
    /// A ⊑ B, B ⊑ C in one world under [`EL_RULES`](crate::reason::el::EL_RULES): both
    /// engines must echo the two asserted edges AND derive the transitive A ⊑ C.
    #[test]
    fn native_forward_oracle_el_ledger_gap_zero() {
        const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
        const WORLD: &str = "urn:world:el";
        let a = ex_iri("http://ex/A");
        let b = ex_iri("http://ex/B");
        let c = ex_iri("http://ex/C");

        // EL EDB: A ⊑ B, B ⊑ C (ternary subClassOf facts in one world).
        let mut facts = TypedFactSet::new();
        facts.push_quad(&a, SUBCLASS, &b, WORLD);
        facts.push_quad(&b, SUBCLASS, &c, WORLD);

        let rules = crate::reason::el::EL_RULES;

        // Drive BOTH oracles over the SAME typed EDB + rule text, unbudgeted.
        let native = NativeForwardOracle
            .materialize(&facts, rules, &ForwardBudget::UNBOUNDED)
            .expect("native EL chase must decide the stratifiable EL rule set");
        let nemo = NemoForwardOracle
            .materialize(&facts, rules, &ForwardBudget::UNBOUNDED)
            .expect("nemo EL chase must succeed");

        let ledger = ledger_between_oracles(&native, &nemo, WORLD);
        let verdict = ledger.enforce();
        assert!(
            verdict.passed,
            "NativeForwardOracle↔Nemo DIVERGED on the EL closure ({} native-only, {} oracle-only): \
             {:?}\ndivergent rows: {:?}",
            ledger.native_only,
            ledger.oracle_only,
            verdict.reasons,
            ledger
                .rows
                .iter()
                .filter(|r| r.kind != DivergenceKind::Agree)
                .collect::<Vec<_>>()
        );
        // Non-vacuity floor: they must actually AGREE on facts, not trivially both-empty.
        assert!(
            ledger.agree > 0,
            "the parity ledger must have at least one agreeing fact"
        );

        // The native oracle must genuinely DERIVE the transitive edge A ⊑ C (not just echo
        // the two asserted edges) — proving the native chase, not a pass-through, ran.
        let derived_a_c = native.rows.iter().any(|(row, prov)| {
            !prov.is_edb
                && row.predicate == SUBCLASS
                && typed_row_fact_key(row)
                    == Some((term_display(&a), SUBCLASS.to_owned(), term_display(&c)))
        });
        assert!(
            derived_a_c,
            "native oracle must DERIVE A ⊑ C by subClassOf-transitivity; rows: {:#?}",
            native.rows
        );

        // The native adapter advertises provenance (firing-rule identity per row).
        assert!(NativeForwardOracle.provides_provenance());
        assert_eq!(NativeForwardOracle.name(), "native");
    }

    // ── EL promotion corpus gate + reified correspondence (Task 2) ────────────────

    const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
    const EL_WORLD: &str = "urn:world:el-corpus";

    /// Seed a real EL corpus into a ternary typed EDB: a four-link `subClassOf`
    /// chain `A ⊑ B ⊑ C ⊑ D`, a `D ≡ E` equivalence, and a `p ⊑ q` sub-property
    /// edge — enough that the EL closure is non-trivial (transitive `subClassOf`,
    /// both equivalence directions) rather than one triple.
    fn el_corpus_edb() -> TypedFactSet {
        let cls = |n: &str| ex_iri(&format!("http://ex/el/{n}"));
        let mut facts = TypedFactSet::new();
        // A ⊑ B ⊑ C ⊑ D (transitive closure derives A⊑C, A⊑D, B⊑D).
        facts.push_quad(&cls("A"), RDFS_SUBCLASS_OF, &cls("B"), EL_WORLD);
        facts.push_quad(&cls("B"), RDFS_SUBCLASS_OF, &cls("C"), EL_WORLD);
        facts.push_quad(&cls("C"), RDFS_SUBCLASS_OF, &cls("D"), EL_WORLD);
        // D ≡ E (derives D⊑E and E⊑D, and by transitivity A⊑E, B⊑E, C⊑E).
        facts.push_quad(&cls("D"), OWL_EQUIVALENT_CLASS, &cls("E"), EL_WORLD);
        // p ⊑ q (sub-property transitivity is in EL_RULES; a lone edge just echoes).
        facts.push_quad(&cls("p"), SUBCLASS_PROP, &cls("q"), EL_WORLD);
        facts
    }

    const SUBCLASS_PROP: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";

    /// The `subClassOf` subsumption tuples `(subject, object, world)` of a typed
    /// closure — the comparable key [`crate::reason::ledger::compare_subsumption`]
    /// keys on. Restricted to the canonical `subClassOf` subsumption relation so
    /// the (subject, object) key is unambiguous.
    fn el_subclass_tuples(closure: &TypedChaseResult) -> Vec<(String, String, String)> {
        closure
            .rows
            .iter()
            .filter_map(|(row, _prov)| {
                if row.predicate != RDFS_SUBCLASS_OF || row.args.len() != 3 {
                    return None;
                }
                Some((
                    term_display(&row.args[0]),
                    term_display(&row.args[1]),
                    EL_WORLD.to_owned(),
                ))
            })
            .collect()
    }

    /// Build the native↔oracle EL divergence ledger over the shared corpus: run
    /// BOTH forward oracles over the EL rule set, then classify the `subClassOf`
    /// closures via [`crate::reason::ledger::compare_subsumption`].
    fn el_divergence_ledger() -> crate::reason::ledger::DivergenceLedger {
        let facts = el_corpus_edb();
        let rules = crate::reason::el::EL_RULES;
        let native = NativeForwardOracle
            .materialize(&facts, rules, &ForwardBudget::UNBOUNDED)
            .expect("native EL chase must decide the stratifiable EL rule set");
        let nemo = NemoForwardOracle
            .materialize(&facts, rules, &ForwardBudget::UNBOUNDED)
            .expect("nemo EL chase must succeed");
        let rows = crate::reason::ledger::compare_subsumption(
            &el_subclass_tuples(&native),
            &el_subclass_tuples(&nemo),
        );
        crate::reason::ledger::build_ledger(rows, Vec::new(), Vec::new(), Vec::new())
    }

    /// Part A — the EL promotion parity gate (gap-zero, non-vacuous): over the EL
    /// fixture corpus the native forward engine SUBSUMES the Nemo oracle. The
    /// strict verdict must pass (zero OracleOnly / DlGap / CorpusOnly) AND the
    /// non-vacuity floor `agree > 0` must hold. Any OracleOnly divergence is a
    /// native-coverage regression to be fixed in the native path — never by
    /// relaxing this gate.
    #[test]
    fn el_native_oracle_ledger_gap_zero() {
        let ledger = el_divergence_ledger();
        let verdict = crate::reason::ledger::enforce(&ledger);
        assert!(
            verdict.passed,
            "native ⊉ Nemo on the EL corpus ({} oracle-only, {} dl-gap): {:?}\nrows: {:#?}",
            ledger.oracle_only,
            ledger.dl_gap,
            verdict.reasons,
            ledger
                .rows
                .iter()
                .filter(|r| r.kind != DivergenceKind::Agree)
                .collect::<Vec<_>>()
        );
        // Non-vacuity floor: the two engines actually AGREE on a real closure.
        assert!(
            ledger.agree > 0,
            "the EL parity ledger must have at least one agreeing subsumption"
        );
        // Pin the committed certificate constants to the live measurement: the shipped
        // `subsumption-correspondence.ttl` sources its `agreeCount` from these, and the
        // pipeline drift-gate refuses a bundle that disagrees. If the engine or fixtures
        // shift the count, this gate goes red until the constant is re-minted.
        assert_eq!(
            ledger.agree,
            crate::reason::artifacts::EL_CERTIFIED_AGREE,
            "EL_CERTIFIED_AGREE is stale — re-mint it to the measured native↔Nemo agree count"
        );
        assert_eq!(
            ledger.native_only,
            crate::reason::artifacts::EL_CERTIFIED_NATIVE_ONLY,
            "EL_CERTIFIED_NATIVE_ONLY is stale — re-mint it to the measured native-only count"
        );
        // The corpus is genuinely non-trivial: the transitive edge A ⊑ D and both
        // equivalence directions must be among the agreed subsumptions, proving the
        // chase ran (not a bare EDB echo).
        let agreed = |s: &str, o: &str| {
            let sub = format!("http://ex/el/{s}");
            let obj = format!("http://ex/el/{o}");
            ledger
                .rows
                .iter()
                .any(|r| r.kind == DivergenceKind::Agree && r.subject == sub && r.object == obj)
        };
        assert!(agreed("A", "D"), "transitive A ⊑ D must be agreed");
        assert!(agreed("D", "E"), "equivalence D ⊑ E must be agreed");
        assert!(agreed("E", "D"), "equivalence E ⊑ D must be agreed");
    }

    /// Part B — the reified `logic:Correspondence`: the gap-zero EL parity verdict
    /// is emitted as a bundle-borne correspondence recording "native ⊒ Nemo on the
    /// certified EL fragment", carrying the divergence ledger as its loss cell and
    /// the native contract hash as its proof-certificate binding.
    #[test]
    fn el_subsumption_correspondence_emitted() {
        let ledger = el_divergence_ledger();
        // Precondition: only reify a PASSING (gap-zero) verdict.
        assert!(crate::reason::ledger::enforce(&ledger).passed);

        let contract = crate::reason::native_contract_hash();
        let ttl = crate::reason::artifacts::build_el_subsumption_correspondence_ttl(
            &ledger, &contract, "nemo",
        );

        // The correspondence is a real logic:Correspondence with the declared
        // relation / law-rung / preservation polarity (all reused vocabulary).
        assert!(
            ttl.contains("#type> <https://blackcatinformatics.ca/logic/Correspondence>"),
            "must reify a logic:Correspondence: {ttl}"
        );
        assert!(
            ttl.contains(
                "logic/correspondenceRelation> <https://blackcatinformatics.ca/logic/Subsumes>"
            ),
            "relation must be logic:Subsumes: {ttl}"
        );
        assert!(
            ttl.contains(
                "logic/preservationKind> \
                 <https://blackcatinformatics.ca/logic/CompleteOverApproximation>"
            ),
            "preservation must be logic:CompleteOverApproximation: {ttl}"
        );
        assert!(
            ttl.contains(
                "logic/morphismClass> <https://blackcatinformatics.ca/logic/SectionRetraction>"
            ),
            "law-rung must be logic:SectionRetraction: {ttl}"
        );
        // "proved in a certified fragment" — the discharged section-law claim.
        assert!(
            ttl.contains("logic/lawClaimed> <https://blackcatinformatics.ca/logic/SectionLaw>")
                && ttl.contains(
                    "logic/lawDischargeVerdict> \
                     <https://blackcatinformatics.ca/logic/ObligationDischarged>"
                )
                && ttl.contains(
                    "logic/lawDischargeCondition> \
                     <https://blackcatinformatics.ca/logic/DischargeCertifiedFragment>"
                ),
            "the section law must be discharged within the certified fragment: {ttl}"
        );
        // The proof-certificate binding: the contract hash equals native_contract_hash().
        assert!(
            ttl.contains(&format!("logic/contractHash> \"{contract}\"")),
            "contractHash must equal native_contract_hash(): {ttl}"
        );
        // A gap-zero ledger carries zero oracle-only findings in its loss cell.
        assert!(
            ttl.contains(&format!("gmeow/oracleOnlyCount> {}", ledger.oracle_only)),
            "the loss cell records the oracle-only tally: {ttl}"
        );
    }

    // ── RL promotion corpus gate + reified correspondence (Task 3) ────────────────
    //
    // The OWL 2 RL/RDF closure runs over the 4-ary generic-triple encoding
    // `triple(?s, ?p, ?o, ?w)` (predicate-as-DATA), so `NativeForwardOracle`
    // dispatches to the arity-generic evaluator `crate::physical::generic` while
    // Nemo runs the same rule set over the same typed EDB. The gate compares the
    // full `triple` closure fact set and demands gap-zero.

    const RL_WORLD: &str = "urn:world:rl-corpus";
    const RL_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const RL_SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const RL_SUBPROP: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
    const RL_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
    const RL_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    const RL_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    const RL_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
    const RL_TRANSITIVE: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
    const RL_ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
    /// A literal-surrogate object IRI — the shape `crate::reason::rl` interns a
    /// literal object to before the chase (RL never inspects the literal value).
    const RL_LIT_SURROGATE: &str = "urn:gmeow-rl-lit:0";

    /// Push one generic-triple `triple(subject, predicate, object, world)` fact into
    /// a typed EDB — the RL predicate-as-data encoding (arity 4, relation `triple`).
    fn push_rl_triple(facts: &mut TypedFactSet, s: &str, p: &str, o: &TermValue) {
        let s = facts.intern(&ex_iri(s));
        let p = facts.intern(&ex_iri(p));
        let o = facts.intern(o);
        let w = facts.intern(&TermValue::simple_literal(RL_WORLD));
        facts.push_fact("triple", vec![s, p, o, w]);
    }

    /// Seed a real OWL 2 RL/RDF corpus into the 4-ary generic-triple encoding,
    /// exercising the variable-predicate meta-rules (`prp-spo1`, `prp-dom`,
    /// `prp-trp`), class subsumption (`cax-sco`/`scm-sco`), the finite-list surface
    /// (`cls-oneOf` over `list_member`), AND a literal-surrogate object round-trip
    /// (a `prp-spo1` over a data property).
    fn rl_corpus_edb() -> TypedFactSet {
        let cls = |n: &str| format!("http://ex/rl/{n}");
        let iri = |n: &str| ex_iri(&cls(n));
        let mut facts = TypedFactSet::new();
        // cax-sco + scm-sco: x a A, A ⊑ B ⊑ C ⇒ x a B, x a C, A ⊑ C.
        push_rl_triple(&mut facts, &cls("x"), RL_TYPE, &iri("A"));
        push_rl_triple(&mut facts, &cls("A"), RL_SUBCLASS, &iri("B"));
        push_rl_triple(&mut facts, &cls("B"), RL_SUBCLASS, &iri("C"));
        // prp-spo1 over an object property: p1 ⊑ p2, x p1 y ⇒ x p2 y.
        push_rl_triple(&mut facts, &cls("p1"), RL_SUBPROP, &iri("p2"));
        push_rl_triple(&mut facts, &cls("x"), &cls("p1"), &iri("y"));
        // prp-spo1 carrying a literal object (surrogate IRI): d1 ⊑ d2, x d1 "lit".
        push_rl_triple(&mut facts, &cls("d1"), RL_SUBPROP, &iri("d2"));
        push_rl_triple(&mut facts, &cls("x"), &cls("d1"), &ex_iri(RL_LIT_SURROGATE));
        // prp-dom: p1 domain A ⇒ x a A (from x p1 y).
        push_rl_triple(&mut facts, &cls("p1"), RL_DOMAIN, &iri("Dom"));
        // prp-trp: pt a TransitiveProperty, m pt n, n pt o ⇒ m pt o.
        push_rl_triple(&mut facts, &cls("pt"), RL_TYPE, &ex_iri(RL_TRANSITIVE));
        push_rl_triple(&mut facts, &cls("m"), &cls("pt"), &iri("n"));
        push_rl_triple(&mut facts, &cls("n"), &cls("pt"), &iri("o"));
        // cls-oneOf over an RDF list: E oneOf ( u v ) ⇒ u a E, v a E.
        push_rl_triple(&mut facts, &cls("E"), RL_ONE_OF, &iri("l0"));
        push_rl_triple(&mut facts, &cls("l0"), RL_FIRST, &iri("u"));
        push_rl_triple(&mut facts, &cls("l0"), RL_REST, &iri("l1"));
        push_rl_triple(&mut facts, &cls("l1"), RL_FIRST, &iri("v"));
        push_rl_triple(&mut facts, &cls("l1"), RL_REST, &ex_iri(RL_NIL));
        facts
    }

    /// The full `triple` closure fact set of a chase result, as comparable
    /// `(subject, predicate, object)` N3-surface triples (the single-world corpus
    /// carries a constant world, so it is not part of the discriminating key). The
    /// `list_member/3` bookkeeping rows are internal, not closure facts, so they are
    /// excluded — exactly what `crate::reason::rl::rl_closure` coerces back.
    fn rl_triple_tuples(closure: &TypedChaseResult) -> Vec<(String, String, String)> {
        closure
            .rows
            .iter()
            .filter_map(|(row, _prov)| {
                if row.predicate != "triple" || row.args.len() != 4 {
                    return None;
                }
                Some((
                    term_display(&row.args[0]),
                    term_display(&row.args[1]),
                    term_display(&row.args[2]),
                ))
            })
            .collect()
    }

    /// Build the native↔oracle RL divergence ledger over the shared corpus: run BOTH
    /// forward oracles over `RL_RULES`, then classify the full `triple` closures via
    /// `crate::reason::ledger::compare_subsumption` (which compares the two fact sets
    /// component-wise).
    fn rl_divergence_ledger() -> crate::reason::ledger::DivergenceLedger {
        let facts = rl_corpus_edb();
        let rules = crate::reason::rl::RL_RULES;
        let native = NativeForwardOracle
            .materialize(&facts, rules, &ForwardBudget::UNBOUNDED)
            .expect("native RL chase must decide the positive-Datalog RL rule set");
        let nemo = NemoForwardOracle
            .materialize(&facts, rules, &ForwardBudget::UNBOUNDED)
            .expect("nemo RL chase must succeed");
        let rows = crate::reason::ledger::compare_subsumption(
            &rl_triple_tuples(&native),
            &rl_triple_tuples(&nemo),
        );
        crate::reason::ledger::build_ledger(rows, Vec::new(), Vec::new(), Vec::new())
    }

    /// Part A — the RL promotion parity gate (gap-zero, non-vacuous): over the RL
    /// fixture corpus the native arity-generic forward engine SUBSUMES the Nemo
    /// oracle on the FULL `triple` closure. The strict verdict must pass (zero
    /// OracleOnly / DlGap / CorpusOnly) AND `agree > 0`. Any OracleOnly divergence is
    /// a native-coverage regression to fix in the generic evaluator — never by
    /// relaxing this gate.
    #[test]
    fn rl_native_oracle_ledger_gap_zero() {
        let ledger = rl_divergence_ledger();
        let verdict = crate::reason::ledger::enforce(&ledger);
        assert!(
            verdict.passed,
            "native ⊉ Nemo on the RL corpus ({} oracle-only, {} dl-gap): {:?}\nrows: {:#?}",
            ledger.oracle_only,
            ledger.dl_gap,
            verdict.reasons,
            ledger
                .rows
                .iter()
                .filter(|r| r.kind != DivergenceKind::Agree)
                .collect::<Vec<_>>()
        );
        assert!(
            ledger.agree > 0,
            "the RL parity ledger must have at least one agreeing fact"
        );
        // Pin the committed certificate constants to the live RL measurement.
        assert_eq!(
            ledger.agree,
            crate::reason::artifacts::RL_CERTIFIED_AGREE,
            "RL_CERTIFIED_AGREE is stale — re-mint it to the measured native↔Nemo agree count"
        );
        assert_eq!(
            ledger.native_only,
            crate::reason::artifacts::RL_CERTIFIED_NATIVE_ONLY,
            "RL_CERTIFIED_NATIVE_ONLY is stale — re-mint it to the measured native-only count"
        );
        // Non-trivial closure: the variable-predicate meta-rules, class-subsumption
        // transitivity, the literal-surrogate carry, and the finite-list surface must
        // all be among the AGREED facts (proving the generic chase ran, not an echo).
        //
        // `compare_subsumption` maps each `(a, b, c)` comparand to `(subject, object,
        // world)` and unbrackets every component, so an agreed `triple(s, p, o)`
        // surfaces as `subject == bare(s)`, `object == bare(p)`, `world == bare(o)`.
        let agreed = |s: &str, p: &str, o: &str| {
            ledger.rows.iter().any(|r| {
                r.kind == DivergenceKind::Agree && r.subject == s && r.object == p && r.world == o
            })
        };
        let rl = |n: &str| format!("http://ex/rl/{n}");
        // prp-spo1 (variable predicate) — x p2 y.
        assert!(
            agreed(&rl("x"), &rl("p2"), &rl("y")),
            "x p2 y via prp-spo1 (variable predicate)"
        );
        // prp-spo1 carrying a literal surrogate — x d2 <lit-surrogate>.
        assert!(
            agreed(&rl("x"), &rl("d2"), RL_LIT_SURROGATE),
            "x d2 <lit-surrogate> via prp-spo1"
        );
        // cax-sco + scm-sco — x a C (via A ⊑ B ⊑ C).
        assert!(
            agreed(&rl("x"), RL_TYPE, &rl("C")),
            "x a C via cax-sco + scm-sco"
        );
        // prp-trp — m pt o.
        assert!(agreed(&rl("m"), &rl("pt"), &rl("o")), "m pt o via prp-trp");
        // cls-oneOf over list_member — u a E.
        assert!(agreed(&rl("u"), RL_TYPE, &rl("E")), "u a E via cls-oneOf");
    }

    /// Part B — the reified RL `logic:Correspondence`: the gap-zero RL parity verdict
    /// is emitted as a bundle-borne correspondence ("native ⊒ Nemo on the certified
    /// RL fragment"), the next edge of the EL ⊂ RL promotion lattice, carrying the
    /// divergence ledger as its loss cell and the native contract hash as its
    /// proof-certificate binding. Same claim shape as EL (only the lattice-edge slug
    /// differs).
    #[test]
    fn rl_subsumption_correspondence_emitted() {
        let ledger = rl_divergence_ledger();
        assert!(crate::reason::ledger::enforce(&ledger).passed);

        let contract = crate::reason::native_contract_hash();
        let ttl = crate::reason::artifacts::build_rl_subsumption_correspondence_ttl(
            &ledger, &contract, "nemo",
        );

        assert!(
            ttl.contains("#type> <https://blackcatinformatics.ca/logic/Correspondence>"),
            "must reify a logic:Correspondence: {ttl}"
        );
        assert!(
            ttl.contains(
                "logic/correspondenceRelation> <https://blackcatinformatics.ca/logic/Subsumes>"
            ),
            "relation must be logic:Subsumes: {ttl}"
        );
        assert!(
            ttl.contains(
                "logic/preservationKind> \
                 <https://blackcatinformatics.ca/logic/CompleteOverApproximation>"
            ),
            "preservation must be logic:CompleteOverApproximation: {ttl}"
        );
        assert!(
            ttl.contains(
                "logic/morphismClass> <https://blackcatinformatics.ca/logic/SectionRetraction>"
            ),
            "law-rung must be logic:SectionRetraction: {ttl}"
        );
        assert!(
            ttl.contains("logic/lawClaimed> <https://blackcatinformatics.ca/logic/SectionLaw>")
                && ttl.contains(
                    "logic/lawDischargeVerdict> \
                     <https://blackcatinformatics.ca/logic/ObligationDischarged>"
                )
                && ttl.contains(
                    "logic/lawDischargeCondition> \
                     <https://blackcatinformatics.ca/logic/DischargeCertifiedFragment>"
                ),
            "the section law must be discharged within the certified fragment: {ttl}"
        );
        assert!(
            ttl.contains(&format!("logic/contractHash> \"{contract}\"")),
            "contractHash must equal native_contract_hash(): {ttl}"
        );
        assert!(
            ttl.contains(&format!("gmeow/oracleOnlyCount> {}", ledger.oracle_only)),
            "the loss cell records the oracle-only tally: {ttl}"
        );
        // The RL lattice edge keys its reified subject on the `rl` slug (distinct from
        // the EL edge's subjects), so both correspondences can coexist in the bundle.
        assert!(
            ttl.contains("gmeow/rl-native-subsumption-correspondence>"),
            "the RL correspondence subject must be slugged `rl`: {ttl}"
        );
    }

    // ── DL-Horn promotion corpus gate + engine-invariant witness pass + reified
    //    correspondence (Task 4) ───────────────────────────────────────────────────
    //
    // `crate::reason::dl::dl_rules()` = `EL_RULES` + `DL_EXTRA_RULES`; BOTH bodies use
    // NAMED-ternary relations (`<type>(?i,?c,?w)`, `<owl#disjointWith>(?c1,?c2,?w)` —
    // the predicate is the relation NAME, variables live only in subject/object/world),
    // so the DL Horn closure runs on the EXISTING binary native path (arity-3 dispatch
    // in `NativeForwardOracle`), exactly like the EL fragment (Task 2). The DL
    // VALUE-INVENTION (someValuesFrom / cardinality witnesses) is NOT rule text — it is
    // the provenance-blind Rust post-pass `crate::reason::dl::augment_inferred_with_dl`,
    // downstream of the oracle seam and INVARIANT across the engine flip; its
    // unification into the chase is tracked separately and out of scope here.

    const DL_WORLD: &str = "urn:world:dl-corpus";
    const DL_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const DL_SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const DL_DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";
    const DL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";

    /// The shared DL Horn fixture as `(subject, predicate, object)` triples in
    /// [`DL_WORLD`], exercising the EL rules AND every clause of `DL_EXTRA_RULES`:
    ///
    /// * `el:subClassOf-transitive` — `A ⊑ B ⊑ C` ⇒ `A ⊑ C`;
    /// * `el:type-propagation` — `i a A`, `A ⊑ B` ⇒ `i a B`, `i a C`;
    /// * `dl:individual-clash` — `x a D`, `x a E`, `D ⊥ E` ⇒ `x a owl:Nothing`;
    /// * `dl:unsatisfiable-class` — `F ⊑ D`, `F ⊑ E`, `D ⊥ E` ⇒ `F ⊑ owl:Nothing`;
    /// * `dl:nothing-membership` — `y a F`, `F ⊑ owl:Nothing` (derived above) ⇒
    ///   `y a owl:Nothing` (a two-step chain off the unsatisfiable-class head).
    fn dl_corpus_triples() -> Vec<(String, String, String)> {
        let iri = |n: &str| format!("http://ex/dl/{n}");
        let t = |s: &str, p: &str, o: &str| (iri(s), p.to_owned(), iri(o));
        vec![
            // EL class hierarchy + a typed individual.
            t("A", DL_SUBCLASS, "B"),
            t("B", DL_SUBCLASS, "C"),
            t("i", DL_TYPE, "A"),
            // Disjointness driving the clash rules.
            t("D", DL_DISJOINT, "E"),
            // dl:individual-clash — x is both D and E.
            t("x", DL_TYPE, "D"),
            t("x", DL_TYPE, "E"),
            // dl:unsatisfiable-class — F ⊑ D and F ⊑ E.
            t("F", DL_SUBCLASS, "D"),
            t("F", DL_SUBCLASS, "E"),
            // dl:nothing-membership — y a F, then F ⊑ Nothing (derived) ⇒ y a Nothing.
            t("y", DL_TYPE, "F"),
        ]
    }

    /// The shared DL fixture as a ternary typed EDB (the shape the forward oracles
    /// chase over).
    fn dl_corpus_edb() -> TypedFactSet {
        let mut facts = TypedFactSet::new();
        for (s, p, o) in dl_corpus_triples() {
            facts.push_quad(&ex_iri(&s), &p, &ex_iri(&o), DL_WORLD);
        }
        facts
    }

    /// The SAME shared DL fixture as an `RdfDataset` (the shape the provenance-blind
    /// DL witness/augment post-pass reads). Built from the identical triple list as
    /// [`dl_corpus_edb`], so the two representations denote the same facts.
    fn dl_corpus_dataset() -> std::sync::Arc<purrdf::RdfDataset> {
        let mut builder = purrdf::RdfDatasetBuilder::new();
        for (s, p, o) in dl_corpus_triples() {
            let quad = purrdf::RdfQuad::new(purrdf::RdfTerm::iri(&s), &p, purrdf::RdfTerm::iri(&o))
                .in_graph(purrdf::RdfTerm::iri(DL_WORLD));
            builder.push_owned_quad(&quad);
        }
        builder.freeze().expect("valid DL corpus dataset")
    }

    /// The FULL DL Horn closure of a chase result as comparable
    /// `(subject, predicate, object)` triples — every arity-3 row (the single-world
    /// corpus carries a constant world, dropped from the discriminating key). Unlike
    /// the EL helper this keeps the RELATION NAME (`predicate`) in the key so the
    /// `type(i, owl:Nothing)` and `subClassOf(c, owl:Nothing)` clash facts are
    /// distinguished, not just `subClassOf` subsumptions.
    fn dl_fact_tuples(closure: &TypedChaseResult) -> Vec<(String, String, String)> {
        closure
            .rows
            .iter()
            .filter_map(|(row, _prov)| {
                if row.args.len() != 3 {
                    return None;
                }
                Some((
                    term_display(&row.args[0]),
                    row.predicate.clone(),
                    term_display(&row.args[1]),
                ))
            })
            .collect()
    }

    /// Build the native↔oracle DL-Horn divergence ledger over the shared corpus: run
    /// BOTH forward oracles over `dl_rules()`, then classify the FULL Horn closure
    /// fact set via [`crate::reason::ledger::compare_subsumption`] (the
    /// `(subject, predicate, object)` comparand distinguishes the clash facts).
    fn dl_horn_divergence_ledger() -> crate::reason::ledger::DivergenceLedger {
        let facts = dl_corpus_edb();
        let rules = crate::reason::dl::dl_rules();
        let native = NativeForwardOracle
            .materialize(&facts, &rules, &ForwardBudget::UNBOUNDED)
            .expect("native DL Horn chase must decide the stratifiable dl_rules() set");
        let nemo = NemoForwardOracle
            .materialize(&facts, &rules, &ForwardBudget::UNBOUNDED)
            .expect("nemo DL Horn chase must succeed");
        let rows = crate::reason::ledger::compare_subsumption(
            &dl_fact_tuples(&native),
            &dl_fact_tuples(&nemo),
        );
        crate::reason::ledger::build_ledger(rows, Vec::new(), Vec::new(), Vec::new())
    }

    /// Part A — the DL-Horn promotion parity gate (gap-zero, non-vacuous): over the
    /// DL fixture corpus the native forward engine agrees EXACTLY with the Nemo oracle
    /// on the FULL `dl_rules()` Horn closure (`EL_RULES` + `DL_EXTRA_RULES`). The
    /// strict verdict must pass (zero OracleOnly / NativeOnly / DlGap / CorpusOnly) AND
    /// `agree > 0`. Any divergence is a native-coverage regression to fix in the native
    /// path — never by relaxing this gate. This runs the BINARY (arity-3, named-ternary)
    /// native path, confirming the same dispatch that carried EL handles `dl_rules()`.
    #[test]
    fn dl_horn_closure_native_oracle_ledger_gap_zero() {
        let ledger = dl_horn_divergence_ledger();
        let verdict = crate::reason::ledger::enforce(&ledger);
        assert!(
            verdict.passed,
            "native ⊉ Nemo on the DL Horn corpus ({} native-only, {} oracle-only, {} dl-gap): \
             {:?}\nrows: {:#?}",
            ledger.native_only,
            ledger.oracle_only,
            ledger.dl_gap,
            verdict.reasons,
            ledger
                .rows
                .iter()
                .filter(|r| r.kind != DivergenceKind::Agree)
                .collect::<Vec<_>>()
        );
        assert!(
            ledger.agree > 0,
            "the DL Horn parity ledger must have at least one agreeing fact"
        );
        // Pin the committed certificate constants to the live DL-Horn measurement.
        assert_eq!(
            ledger.agree,
            crate::reason::artifacts::DL_CERTIFIED_AGREE,
            "DL_CERTIFIED_AGREE is stale — re-mint it to the measured native↔Nemo agree count"
        );
        assert_eq!(
            ledger.native_only,
            crate::reason::artifacts::DL_CERTIFIED_NATIVE_ONLY,
            "DL_CERTIFIED_NATIVE_ONLY is stale — re-mint it to the measured native-only count"
        );
        // Non-trivial closure: each DL_EXTRA_RULES clause AND the EL rules must be among
        // the AGREED facts (proving the DL Horn chase ran, not a bare EDB echo).
        // `compare_subsumption` maps each `(a, b, c)` comparand to `(subject, object,
        // world)` and unbrackets every component, so an agreed `type(s, o)` surfaces as
        // `subject == bare(s)`, `object == bare(predicate)`, `world == bare(o)`.
        let agreed = |s: &str, p: &str, o: &str| {
            ledger.rows.iter().any(|r| {
                r.kind == DivergenceKind::Agree && r.subject == s && r.object == p && r.world == o
            })
        };
        let dl = |n: &str| format!("http://ex/dl/{n}");
        // el:subClassOf-transitive — A ⊑ C.
        assert!(
            agreed(&dl("A"), DL_SUBCLASS, &dl("C")),
            "A ⊑ C via el:subClassOf-transitive"
        );
        // el:type-propagation — i a C (via i a A, A ⊑ B ⊑ C).
        assert!(
            agreed(&dl("i"), DL_TYPE, &dl("C")),
            "i a C via el:type-propagation"
        );
        // dl:individual-clash — x a owl:Nothing.
        assert!(
            agreed(&dl("x"), DL_TYPE, DL_NOTHING),
            "x a owl:Nothing via dl:individual-clash"
        );
        // dl:unsatisfiable-class — F ⊑ owl:Nothing.
        assert!(
            agreed(&dl("F"), DL_SUBCLASS, DL_NOTHING),
            "F ⊑ owl:Nothing via dl:unsatisfiable-class"
        );
        // dl:nothing-membership — y a owl:Nothing (chains off unsatisfiable-class).
        assert!(
            agreed(&dl("y"), DL_TYPE, DL_NOTHING),
            "y a owl:Nothing via dl:nothing-membership"
        );
    }

    /// Part B — the DL consistency VERDICT is engine-invariant under the flip.
    ///
    /// Approach (and why it is honest): `crate::reason::dl::dl_consistency` runs through
    /// `run_reasoning`, which currently hard-wires `forward_oracle()` (Nemo); native
    /// cannot be injected there without the Task-6 flip. So rather than fake an
    /// injection, this test attacks the invariance claim DIRECTLY at the seam that the
    /// flip changes: it produces the DL Horn closure of the SAME fixture TWICE — once
    /// via `NativeForwardOracle`, once via `NemoForwardOracle` — coerces BOTH through
    /// the shared, oracle-agnostic `crate::reason::chase_rows_to_inferred`, then feeds
    /// each closure through the provenance-blind post-pass
    /// `crate::reason::dl::augment_inferred_with_dl` and reads the verdict via
    /// `crate::reason::dl::verdict_from_inferred`. Because the witness/augment pass and
    /// the verdict reader are pure functions of the FACT set (never of derivation
    /// provenance), and Part A proves the two Horn closures are fact-identical, the
    /// resulting verdicts MUST coincide on their semantic content — demonstrating the
    /// witness pass is invariant across the engine flip WITHOUT touching or retiring it
    /// (its chase-unification is tracked as separate work). Premise provenance is expected to
    /// differ between engines (Part A compares facts, not derivation ids), so the
    /// comparison is on the semantic verdict — the consistency decision and the SETS of
    /// clash witnesses / unsatisfiable classes — plus the augmented fact set itself.
    #[test]
    fn dl_consistency_verdict_unchanged_under_native() {
        let facts = dl_corpus_edb();
        let dataset = dl_corpus_dataset();
        let rules = crate::reason::dl::dl_rules();

        // Two engines, ONE fixture: produce and coerce both Horn closures identically.
        let native_chase = NativeForwardOracle
            .materialize(&facts, &rules, &ForwardBudget::UNBOUNDED)
            .expect("native DL Horn chase must decide dl_rules()");
        let nemo_chase = NemoForwardOracle
            .materialize(&facts, &rules, &ForwardBudget::UNBOUNDED)
            .expect("nemo DL Horn chase must succeed");
        let mut native_inferred = crate::reason::chase_rows_to_inferred(&native_chase)
            .expect("native closure coerces to InferredAxioms");
        let mut nemo_inferred = crate::reason::chase_rows_to_inferred(&nemo_chase)
            .expect("nemo closure coerces to InferredAxioms");

        // The provenance-blind DL witness/augment post-pass over each closure + the
        // SAME edb. This is the pass the flip must NOT perturb (its chase-unification
        // is separate work, out of scope here — we only prove it is engine-invariant).
        crate::reason::dl::augment_inferred_with_dl(&mut native_inferred, &dataset)
            .expect("augment over the native closure");
        crate::reason::dl::augment_inferred_with_dl(&mut nemo_inferred, &dataset)
            .expect("augment over the nemo closure");

        // (1) The augmented FACT sets are identical — the witness pass added the same
        //     consequences regardless of which engine produced the Horn closure.
        let fact_set = |axioms: &[crate::reason::el::InferredAxiom]| {
            axioms
                .iter()
                .map(|a| {
                    (
                        a.subject.clone(),
                        a.predicate.clone(),
                        a.object.clone(),
                        a.world.clone(),
                    )
                })
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(
            fact_set(&native_inferred),
            fact_set(&nemo_inferred),
            "the DL witness/augment post-pass must be engine-invariant on the fact set"
        );

        // (2) The DL consistency VERDICT (semantic content — the consistency decision,
        //     the clash-witness set, the unsatisfiable-class set) coincides.
        let native_verdict = crate::reason::dl::verdict_from_inferred(&native_inferred, &dataset)
            .expect("verdict from native closure");
        let nemo_verdict = crate::reason::dl::verdict_from_inferred(&nemo_inferred, &dataset)
            .expect("verdict from nemo closure");

        assert_eq!(
            native_verdict.consistent, nemo_verdict.consistent,
            "the consistency decision must be engine-invariant"
        );
        let witness_set = |v: &crate::reason::dl::DlVerdict| {
            v.inconsistencies
                .iter()
                .map(|w| (w.individual.clone(), w.world.clone()))
                .collect::<BTreeSet<_>>()
        };
        let unsat_set = |v: &crate::reason::dl::DlVerdict| {
            v.unsatisfiable_classes
                .iter()
                .map(|u| (u.class.clone(), u.world.clone()))
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(
            witness_set(&native_verdict),
            witness_set(&nemo_verdict),
            "the inconsistency-witness set must be engine-invariant"
        );
        assert_eq!(
            unsat_set(&native_verdict),
            unsat_set(&nemo_verdict),
            "the unsatisfiable-class set must be engine-invariant"
        );
        assert_eq!(
            native_verdict.coverage, nemo_verdict.coverage,
            "the construct-coverage inventory must be engine-invariant"
        );

        // Non-vacuity: the fixture genuinely triggers the DL clash pass, so the shared
        // verdict is INCONSISTENT with the expected witnesses (`x`, `y` a owl:Nothing)
        // and the expected unsatisfiable class (`F`) — not a trivially-empty agreement.
        assert!(
            !native_verdict.consistent,
            "the DL fixture must drive the ontology inconsistent"
        );
        let dl = |n: &str| format!("http://ex/dl/{n}");
        let ws = witness_set(&native_verdict);
        assert!(
            ws.contains(&(dl("x"), DL_WORLD.to_owned())),
            "x must be a clash witness (dl:individual-clash); witnesses: {ws:?}"
        );
        assert!(
            ws.contains(&(dl("y"), DL_WORLD.to_owned())),
            "y must be a clash witness (dl:nothing-membership); witnesses: {ws:?}"
        );
        assert!(
            unsat_set(&native_verdict).contains(&(dl("F"), DL_WORLD.to_owned())),
            "F must be an unsatisfiable class (dl:unsatisfiable-class)"
        );
    }

    /// Part C — the reified DL `logic:Correspondence`: the gap-zero DL-Horn parity
    /// verdict is emitted as a bundle-borne correspondence ("native ⊒ Nemo on the
    /// certified DL fragment"), the TERMINAL edge of the EL ⊂ RL ⊂ DL promotion
    /// lattice, carrying the divergence ledger as its loss cell and the native contract
    /// hash as its proof-certificate binding. Same claim shape as EL/RL (only the
    /// lattice-edge slug differs).
    #[test]
    fn dl_subsumption_correspondence_bound_to_contract_hash() {
        let ledger = dl_horn_divergence_ledger();
        // Precondition: only reify a PASSING (gap-zero) verdict.
        assert!(crate::reason::ledger::enforce(&ledger).passed);

        let contract = crate::reason::native_contract_hash();
        let ttl = crate::reason::artifacts::build_dl_subsumption_correspondence_ttl(
            &ledger, &contract, "nemo",
        );

        assert!(
            ttl.contains("#type> <https://blackcatinformatics.ca/logic/Correspondence>"),
            "must reify a logic:Correspondence: {ttl}"
        );
        assert!(
            ttl.contains(
                "logic/correspondenceRelation> <https://blackcatinformatics.ca/logic/Subsumes>"
            ),
            "relation must be logic:Subsumes: {ttl}"
        );
        assert!(
            ttl.contains(
                "logic/preservationKind> \
                 <https://blackcatinformatics.ca/logic/CompleteOverApproximation>"
            ),
            "preservation must be logic:CompleteOverApproximation: {ttl}"
        );
        assert!(
            ttl.contains(
                "logic/morphismClass> <https://blackcatinformatics.ca/logic/SectionRetraction>"
            ),
            "law-rung must be logic:SectionRetraction: {ttl}"
        );
        assert!(
            ttl.contains("logic/lawClaimed> <https://blackcatinformatics.ca/logic/SectionLaw>")
                && ttl.contains(
                    "logic/lawDischargeVerdict> \
                     <https://blackcatinformatics.ca/logic/ObligationDischarged>"
                )
                && ttl.contains(
                    "logic/lawDischargeCondition> \
                     <https://blackcatinformatics.ca/logic/DischargeCertifiedFragment>"
                ),
            "the section law must be discharged within the certified fragment: {ttl}"
        );
        // The proof-certificate binding: the contract hash equals native_contract_hash().
        assert!(
            ttl.contains(&format!("logic/contractHash> \"{contract}\"")),
            "contractHash must equal native_contract_hash(): {ttl}"
        );
        assert!(
            ttl.contains(&format!("gmeow/oracleOnlyCount> {}", ledger.oracle_only)),
            "the loss cell records the oracle-only tally: {ttl}"
        );
        // The DL lattice edge keys its reified subject on the `dl` slug (distinct from
        // the EL/RL edges), so all three correspondences can coexist in the bundle.
        assert!(
            ttl.contains("gmeow/dl-native-subsumption-correspondence>"),
            "the DL correspondence subject must be slugged `dl`: {ttl}"
        );
    }

    // ── USER MATERIALIZE / program-carrying path promotion gate (Task 5) ───────────
    //
    // Tasks 2–4 promoted the FIXED profile texts (EL/RL/DL).  This gate promotes the
    // last surface the oracle boundary carries: a PROGRAM-CARRYING run — the shape
    // `crate::reason::reason_program` builds (`dl_rules()` + the program's own Horn
    // rules, combined into ONE chase text) and the shape `crate::materialize`
    // accepts over the FFI (a user rule program that may declare a HELPER predicate
    // of a non-ternary arity, per `materialize.rs`'s NonQuadRow contract).
    //
    // The representative program is faithful-synthetic (NOT a single real .rls file):
    // it is BUILT from real, canonical parts to exercise the whole program surface in
    // one fixture — the fixed `crate::reason::dl::dl_rules()` VERBATIM (the calculus
    // every `reason_program` run combines in), a real arity-3 program Horn rule (a
    // domain transitivity, the shape `project_nemo` emits for every user rule — every
    // projected program rule is arity-3 named-ternary), AND the binary `helperEdge`
    // helper that `crate::materialize`'s pinned `HELPER_RULES` test fixes as a
    // legitimate user-program feature.  A single committed .rls program would exercise
    // AT MOST one of these; the built fixture is the minimal witness that the combined
    // program-carrying text — dl_rules() ⊕ program rules ⊕ a non-ternary helper — runs
    // native and agrees with Nemo.  Because the helper atom is arity-2, the whole
    // program is NOT binary-eligible, so `NativeForwardOracle` MUST route it to the
    // arity-generic evaluator (`rules_are_pure_ternary == false`); the gate confirms
    // that generic run is gap-zero against Nemo AND that the helper rows are present
    // (the binary core could never produce them faithfully).

    const PROG_WORLD: &str = "urn:world:program-corpus";
    const PROG_RELATED: &str = "http://ex/prog/relatedTo";
    const PROG_HELPER: &str = "helperEdge";

    /// The representative program's EDB as `(subject, predicate, object)` triples in
    /// [`PROG_WORLD`]: a DL clash + subclass chain (drives `dl_rules()`) plus a
    /// `relatedTo` chain (drives the arity-3 program rule).
    fn program_corpus_triples() -> Vec<(String, String, String)> {
        let dl = |n: &str| format!("http://ex/dl/{n}");
        let pr = |n: &str| format!("http://ex/prog/{n}");
        vec![
            // dl_rules() drivers: A ⊑ B ⊑ C, i a A; D ⊥ E, x a D, x a E (clash).
            (dl("A"), DL_SUBCLASS.to_owned(), dl("B")),
            (dl("B"), DL_SUBCLASS.to_owned(), dl("C")),
            (dl("i"), DL_TYPE.to_owned(), dl("A")),
            (dl("D"), DL_DISJOINT.to_owned(), dl("E")),
            (dl("x"), DL_TYPE.to_owned(), dl("D")),
            (dl("x"), DL_TYPE.to_owned(), dl("E")),
            // Program-rule driver: relatedTo p→q→r (transitively closed by the rule).
            (pr("p"), PROG_RELATED.to_owned(), pr("q")),
            (pr("q"), PROG_RELATED.to_owned(), pr("r")),
        ]
    }

    /// The representative program's EDB as a ternary typed EDB.
    fn program_corpus_edb() -> TypedFactSet {
        let mut facts = TypedFactSet::new();
        for (s, p, o) in program_corpus_triples() {
            facts.push_quad(&ex_iri(&s), &p, &ex_iri(&o), PROG_WORLD);
        }
        facts
    }

    /// The combined program-carrying rule text: the fixed DL calculus VERBATIM ⊕ an
    /// arity-3 program Horn rule (relatedTo transitivity) ⊕ the binary `helperEdge`
    /// helper — the way `reason_program` unions `dl_rules()` with the program's own
    /// rules, extended with the non-ternary helper `crate::materialize` accepts.
    fn program_corpus_rules() -> String {
        let program_rule = format!(
            "#[name(\"prog:relatedTo-transitive\")]\n\
             <{PROG_RELATED}>(?a,?c,?w) :- <{PROG_RELATED}>(?a,?b,?w), <{PROG_RELATED}>(?b,?c,?w) .\n"
        );
        // A binary helper: legal Nemo, arity-2 head, so the whole program is NOT
        // binary-eligible and must run on the generic evaluator.
        let helper_rule = format!(
            "#[name(\"prog:helper-edge\")]\n\
             {PROG_HELPER}(?x,?y) :- <{DL_SUBCLASS}>(?x,?y,?w) .\n"
        );
        format!(
            "{}\n{program_rule}\n{helper_rule}",
            crate::reason::dl::dl_rules()
        )
    }

    /// Every chase row as a `(subject, predicate, object)` comparand: the ternary
    /// reasoning rows (world dropped — single-world corpus) AND the binary helper
    /// rows (`helperEdge(x, y)` → `(x, helperEdge, y)`), so the helper facts the
    /// generic evaluator produces are compared too, not silently exempted.
    fn program_fact_tuples(closure: &TypedChaseResult) -> Vec<(String, String, String)> {
        closure
            .rows
            .iter()
            .filter_map(|(row, _prov)| match row.args.len() {
                2 | 3 => Some((
                    term_display(&row.args[0]),
                    row.predicate.clone(),
                    term_display(&row.args[1]),
                )),
                _ => None,
            })
            .collect()
    }

    /// Build the native↔oracle divergence ledger over the representative program.
    fn program_divergence_ledger() -> crate::reason::ledger::DivergenceLedger {
        let facts = program_corpus_edb();
        let rules = program_corpus_rules();
        let native = NativeForwardOracle
            .materialize(&facts, &rules, &ForwardBudget::UNBOUNDED)
            .expect("native chase must decide the program-carrying rule set (generic evaluator)");
        let nemo = NemoForwardOracle
            .materialize(&facts, &rules, &ForwardBudget::UNBOUNDED)
            .expect("nemo chase must succeed");
        let rows = crate::reason::ledger::compare_subsumption(
            &program_fact_tuples(&native),
            &program_fact_tuples(&nemo),
        );
        crate::reason::ledger::build_ledger(rows, Vec::new(), Vec::new(), Vec::new())
    }

    /// The program-path promotion parity gate (gap-zero, non-vacuous): over the
    /// representative program — `dl_rules()` ⊕ an arity-3 program rule ⊕ a binary
    /// helper — the native forward engine (routed to the GENERIC evaluator, since the
    /// helper makes the program non-binary-eligible) agrees EXACTLY with the Nemo
    /// oracle on the FULL closure, including the non-ternary helper rows. Zero
    /// OracleOnly / NativeOnly / DlGap AND `agree > 0`. Any divergence is a native
    /// coverage regression to fix in the native path — never by relaxing this gate.
    #[test]
    fn materialize_program_native_oracle_gap_zero() {
        // Precondition: the combined program is genuinely NOT binary-eligible, so the
        // dispatch routes it to the generic evaluator (the promotion this gate proves).
        assert!(
            !crate::oracle::rules_are_pure_ternary(&program_corpus_rules())
                .expect("program rules parse for arity inspection"),
            "the representative program carries a binary helper, so it must NOT be \
             binary-eligible (it must run on the generic evaluator)"
        );

        let ledger = program_divergence_ledger();
        let verdict = crate::reason::ledger::enforce(&ledger);
        assert!(
            verdict.passed,
            "native ⊉ Nemo on the program-carrying corpus ({} native-only, {} oracle-only, \
             {} dl-gap): {:?}\nrows: {:#?}",
            ledger.native_only,
            ledger.oracle_only,
            ledger.dl_gap,
            verdict.reasons,
            ledger
                .rows
                .iter()
                .filter(|r| r.kind != DivergenceKind::Agree)
                .collect::<Vec<_>>()
        );
        assert!(
            ledger.agree > 0,
            "the program-path parity ledger must have at least one agreeing fact"
        );

        // Non-vacuity: each combined surface must contribute an AGREED fact — the DL
        // calculus, the arity-3 program rule, AND the binary helper — proving the whole
        // program-carrying text ran, not a bare EDB echo.
        let agreed = |s: &str, p: &str, o: &str| {
            ledger.rows.iter().any(|r| {
                r.kind == DivergenceKind::Agree && r.subject == s && r.object == p && r.world == o
            })
        };
        let dl = |n: &str| format!("http://ex/dl/{n}");
        let pr = |n: &str| format!("http://ex/prog/{n}");
        // dl_rules() — el:subClassOf-transitive (A ⊑ C) and dl:individual-clash
        // (x a owl:Nothing).
        assert!(
            agreed(&dl("A"), DL_SUBCLASS, &dl("C")),
            "A ⊑ C via el:subClassOf-transitive (dl_rules())"
        );
        assert!(
            agreed(&dl("x"), DL_TYPE, DL_NOTHING),
            "x a owl:Nothing via dl:individual-clash (dl_rules())"
        );
        // The arity-3 program rule — relatedTo transitivity (p → r).
        assert!(
            agreed(&pr("p"), PROG_RELATED, &pr("r")),
            "p relatedTo r via the arity-3 program rule"
        );
        // The binary helper — helperEdge(A, B) mirrors the subClassOf EDB edge. This
        // fact can ONLY come from the generic evaluator (the binary core cannot
        // faithfully produce a non-ternary row), so its agreement witnesses the
        // program-carrying path ran native on the generic evaluator.
        assert!(
            agreed(&dl("A"), PROG_HELPER, &dl("B")),
            "helperEdge(A, B) via the binary helper (generic evaluator)"
        );

        // The native closure genuinely carries the arity-2 helper rows (the direct
        // witness that the dispatch routed to the generic evaluator, not the binary
        // core which drops terms past the object).
        let native = NativeForwardOracle
            .materialize(
                &program_corpus_edb(),
                &program_corpus_rules(),
                &ForwardBudget::UNBOUNDED,
            )
            .expect("native program chase");
        assert!(
            native
                .rows
                .iter()
                .any(|(row, _)| row.predicate == PROG_HELPER && row.args.len() == 2),
            "the native closure must carry the binary helperEdge rows (generic evaluator)"
        );
    }
}
