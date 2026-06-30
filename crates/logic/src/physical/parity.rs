// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native↔oracle parity comparator + the committed native-coverage floor.
//!
//! The native execution core ([`crate::physical::seminaive::materialize_native`] forward,
//! [`crate::physical::magic::resolve_native`] backward) is the PRIMARY runtime path;
//! Nemo ([`crate::materialize::materialize_core`]) and the declarative SLD oracle
//! ([`crate::reference_resolver::resolve`]) are the DEMOTED oracles. This module makes that
//! demotion EXPLICIT and GATED: over a representative corpus of stratifiable binary Datalog±
//! programs it runs native AND the oracle, classifies each derived row / answer into a
//! [`ParityLedger`], and exposes a strict verdict (passing iff zero non-`Agree` rows).
//!
//! # The two parity surfaces
//!
//! * **Forward** — [`compare_materialization`] compares the native [`DerivedRow`] set against
//!   the Nemo [`DerivedQuad`] set. Both engines echo the asserted EDB AND emit the derived
//!   closure, so the comparison is on the full fact set.
//! * **Backward** — [`compare_answers`] compares the native [`AnswerSet`] bindings against the
//!   reference SLD oracle's bindings.
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

use std::collections::BTreeSet;

use crate::query_ir::AnswerSet;
use crate::reason::ledger::{DivergenceKind, LedgerRow, LedgerVerdict};
use crate::rule_ir::DerivedRow;
use crate::seam::DerivedQuad;

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
        row.subject.to_string(),
        row.predicate.as_str().to_owned(),
        row.object.to_string(),
    )
}

/// The fact key of a Nemo [`DerivedQuad`]: its `(subject, predicate, object)` N3 surfaces.
fn quad_fact_key(quad: &DerivedQuad) -> FactKey {
    (
        quad.subject.to_string(),
        quad.predicate.as_str().to_owned(),
        quad.object.to_string(),
    )
}

/// Compare the native [`DerivedRow`] fact set against the Nemo [`DerivedQuad`] fact set.
///
/// Each `(subject, predicate, object)` triple is classified: present in BOTH ⇒
/// [`DivergenceKind::Agree`], native ∖ oracle ⇒ [`DivergenceKind::NativeOnly`], oracle ∖ native
/// ⇒ [`DivergenceKind::OracleOnly`]. Rows are emitted in sorted-key order so the ledger is
/// deterministic.
///
/// Only the FACT set is compared, NOT provenance: a multiply-derivable fact may legitimately
/// carry a different `derivation_id` between the native first-wins tiebreak and the Nemo chase
/// — that derivation-id divergence is expected and is NOT a fact-level divergence (it is pinned
/// separately by the determinism gate in [`crate::physical::seminaive`]).
fn compare_materialization(
    native: &[DerivedRow],
    oracle: &[DerivedQuad],
    world: &str,
) -> ParityLedger {
    let native_keys: BTreeSet<FactKey> = native.iter().map(row_fact_key).collect();
    let oracle_keys: BTreeSet<FactKey> = oracle.iter().map(quad_fact_key).collect();

    let mut rows: Vec<LedgerRow> = Vec::new();

    for key in native_keys.intersection(&oracle_keys) {
        let (subject, predicate, object) = key.clone();
        rows.push(LedgerRow {
            kind: DivergenceKind::Agree,
            category: "materialization".to_owned(),
            detail: format!("native and Nemo agree on fact: {subject} {predicate} {object}"),
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
            detail: format!("derived natively but not by Nemo: {subject} {predicate} {object}"),
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
            detail: format!("derived by Nemo but not natively: {subject} {predicate} {object}"),
            subject,
            object,
            world: world.to_owned(),
        });
    }

    ParityLedger::from_rows(rows)
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

/// Compare the native [`AnswerSet`] bindings against the reference SLD oracle's bindings.
///
/// Each binding is classified by its `binding_key`: present in BOTH ⇒ [`DivergenceKind::Agree`],
/// native ∖ oracle ⇒ [`DivergenceKind::NativeOnly`], oracle ∖ native ⇒
/// [`DivergenceKind::OracleOnly`]. Callers pass `AnswerSet`s already `canonicalize()`d; the
/// comparison is on the binding SET (duplicate bindings, which the canonicalized sets never
/// carry distinctly, collapse). Rows are emitted in sorted-key order.
fn compare_answers(native: &AnswerSet, oracle: &AnswerSet) -> ParityLedger {
    let native_keys: BTreeSet<String> = native.bindings.iter().map(binding_key).collect();
    let oracle_keys: BTreeSet<String> = oracle.bindings.iter().map(binding_key).collect();

    let mut rows: Vec<LedgerRow> = Vec::new();

    for key in native_keys.intersection(&oracle_keys) {
        rows.push(LedgerRow {
            kind: DivergenceKind::Agree,
            category: "answer".to_owned(),
            subject: key.clone(),
            object: String::new(),
            world: String::new(),
            detail: format!("native and the SLD oracle agree on answer: {key}"),
        });
    }
    for key in native_keys.difference(&oracle_keys) {
        rows.push(LedgerRow {
            kind: DivergenceKind::NativeOnly,
            category: "answer".to_owned(),
            subject: key.clone(),
            object: String::new(),
            world: String::new(),
            detail: format!("answered natively but not by the SLD oracle: {key}"),
        });
    }
    for key in oracle_keys.difference(&native_keys) {
        rows.push(LedgerRow {
            kind: DivergenceKind::OracleOnly,
            category: "answer".to_owned(),
            subject: key.clone(),
            object: String::new(),
            world: String::new(),
            detail: format!("answered by the SLD oracle but not natively: {key}"),
        });
    }

    ParityLedger::from_rows(rows)
}

#[cfg(test)]
mod tests {
    //! The parity + native-coverage-floor GATE.
    //!
    //! `materialize_parity_*` invoke Nemo (`materialize_core`) and so MUST run in the `engine`
    //! nextest group (the `materialize` token in the test-fn name matches the engine-group
    //! regex `nemo_engine|scryer_engine|materialize|reason|verify|certify|dispatch|...`).
    //! `dispatch_parity_*` (the `dispatch` token) likewise match. The floor test
    //! `native_coverage_floor` drives both and is named to match `materialize`/`dispatch`-free
    //! but is grouped via the explicit `physical::parity` filter clause added to
    //! `.config/nextest.toml`'s engine override.

    use super::*;
    use crate::physical::magic::resolve_native;
    use crate::physical::seminaive::{materialize_native, NativeOutcome};
    use crate::query_ir::{parse_query_program, Budget, QProgram};
    use crate::reference_resolver;
    use crate::rule_ir::parse_eval_rules;
    use crate::seam::WorldStoreForeign;
    use crate::store::WorldStore;
    use oxigraph::model::NamedNode;

    const PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";

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
        match materialize_native(&store, &rules)
            .unwrap_or_else(|e| panic!("[{}] materialize_native errored: {e}", p.label))
        {
            NativeOutcome::Decided(rows) => rows,
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
            let oracle = crate::materialize::materialize_core(&p.rls, &p.nquads, None, None, None)
                .unwrap_or_else(|e| panic!("[{}] materialize_core (Nemo) failed: {e}", p.label));

            let ledger = compare_materialization(&native, &oracle, p.world);
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

    fn backward_corpus() -> Vec<BackwardProgram> {
        vec![
            backward_ancestor_ff(),
            backward_ancestor_bf(),
            backward_multi_rule(),
            backward_ground_present(),
        ]
    }

    /// Build a `WorldStoreForeign` from a backward program's EDB triples, returning it with the
    /// world `NamedNode`.
    fn backward_world(b: &BackwardProgram) -> (WorldStore, NamedNode) {
        const W: &str = "http://logic.test/world/parity";
        let store = WorldStore::new();
        for (s, pr, o) in &b.triples {
            store.insert_quad(W, s, pr, o);
        }
        (store, NamedNode::new(W).expect("valid world IRI"))
    }

    /// Run the native backward engine, asserting a `Decided` outcome (the coverage floor).
    fn run_native_backward(b: &BackwardProgram) -> AnswerSet {
        let (store, world_nn) = backward_world(b);
        const W: &str = "http://logic.test/world/parity";
        let foreign =
            WorldStoreForeign::from_world(&store, W, PROFILE).expect("from_world must succeed");
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

    /// Run the reference SLD oracle for a backward program.
    fn run_oracle_backward(b: &BackwardProgram) -> AnswerSet {
        let (store, world_nn) = backward_world(b);
        const W: &str = "http://logic.test/world/parity";
        let foreign =
            WorldStoreForeign::from_world(&store, W, PROFILE).expect("from_world must succeed");
        reference_resolver::resolve(&foreign, &world_nn, &b.program, &Budget::default())
            .unwrap_or_else(|e| panic!("[{}] reference_resolver::resolve failed: {e}", b.label))
    }

    // ── Backward parity: native ≡ reference SLD oracle (THE GATE) ─────────────────────

    #[test]
    fn dispatch_parity_native_agrees_with_reference() {
        let mut total_native_answers = 0usize;
        for b in backward_corpus() {
            let native = run_native_backward(&b);
            let oracle = run_oracle_backward(&b);

            let ledger = compare_answers(&native, &oracle);
            let verdict = ledger.enforce();
            assert!(
                verdict.passed,
                "[{}] native↔reference answer set DIVERGED ({} native-only, {} oracle-only): \
                 {:?}\nnative {:?}\noracle {:?}",
                b.label,
                ledger.native_only,
                ledger.oracle_only,
                verdict.reasons,
                native.bindings,
                oracle.bindings
            );
            total_native_answers += native.bindings.len();
        }
        assert!(
            total_native_answers > 0,
            "the native engine answered ZERO backward queries across the whole corpus — a total \
             fallback is a coverage-floor failure"
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
}
