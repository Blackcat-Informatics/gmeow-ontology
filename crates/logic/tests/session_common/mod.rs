// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared builders + the canonical derived-fact / provenance fingerprints the
//! `reasoning_session_*` acceptance suites compare against the full-recompute oracle.
//!
//! Every helper drives the PUBLIC surface only: [`gmeow_logic::runtime`] for the façade,
//! [`gmeow_logic::reason::reason_program`] for the from-scratch oracle,
//! [`gmeow_logic::cost::DerivedProvenance`] for the witness shape, and
//! [`gmeow_logic::provenance::term_display`] for the term rendering. No private module is
//! touched.
//!
//! ## Rendering normalization
//!
//! The oracle renders a derived fact's SUBJECT bare (`subject_iri` → `https://…/a`) but its
//! OBJECT bracketed (`term_display` → `<https://…/a>`); the session brackets BOTH (both go
//! through `term_display`). [`canon`] strips the angle brackets so the two display
//! conventions compare as the same term — a pure rendering normalization, never a
//! semantic one.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gmeow_logic::annotation::AnnotationContract;
use gmeow_logic::cost::DerivedProvenance;
use gmeow_logic::provenance::term_display;
use gmeow_logic::reason::reason_program;
use gmeow_logic::runtime::ReasoningSession;
use gmeow_logic_compile::ir::{
    ContextualScope, LogicAxiom, LogicProgram, LogicRule, ReasoningContract,
};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm};

pub const WORLD: &str = "https://example.org/world";
pub const EX: &str = "https://example.org/";

/// A canonical `(subject, predicate, object)` triple with brackets stripped.
pub type Triple = (String, String, String);

/// The full IRI of the local name `local` under the shared example namespace.
#[must_use]
pub fn iri(local: &str) -> String {
    format!("{EX}{local}")
}

/// The EDB `edge` predicate IRI.
#[must_use]
pub fn edge_pred() -> String {
    format!("{EX}edge")
}

/// Strip a single leading `<` and trailing `>` from a rendered IRI term, so the oracle's
/// bare-subject convention and the session's bracketed convention compare equal.
#[must_use]
pub fn canon(term: &str) -> String {
    term.trim_start_matches('<')
        .trim_end_matches('>')
        .to_owned()
}

// ── EDB builders ─────────────────────────────────────────────────────────────────

fn edge_page(edges: &[(&str, &str)]) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for (s, o) in edges {
        builder.push_owned_quad(
            &RdfQuad::new(RdfTerm::iri(iri(s)), edge_pred(), RdfTerm::iri(iri(o)))
                .in_graph(RdfTerm::iri(WORLD)),
        );
    }
    builder.freeze().expect("valid edge page")
}

/// An owned single-world `edge` EDB over the given `(subject, object)` local-name pairs.
#[must_use]
pub fn edge_dataset(edges: &[(&str, &str)]) -> RdfDataset {
    Arc::try_unwrap(edge_page(edges)).expect("a freshly built page has a single owner")
}

/// The same pages exposed as a shared `Arc` (for the paged-composition suite).
#[must_use]
pub fn edge_arc(edges: &[(&str, &str)]) -> Arc<RdfDataset> {
    edge_page(edges)
}

/// An owned, empty single-world dataset (a suppression-only delta's additions slot).
#[must_use]
pub fn empty_dataset() -> RdfDataset {
    Arc::try_unwrap(
        RdfDatasetBuilder::new()
            .freeze()
            .expect("valid empty dataset"),
    )
    .expect("a freshly built empty dataset has a single owner")
}

// ── Program builders (all within finite positive binary Datalog) ────────────────────

fn atom(subject: &str, predicate: &str, object: &str, negated: bool) -> LogicAxiom {
    LogicAxiom::new(
        subject.to_owned(),
        predicate.to_owned(),
        object.to_owned(),
        false,
        negated,
        ContextualScope::default(),
    )
    .expect("valid axiom")
}

fn rule(head: LogicAxiom, body: Vec<LogicAxiom>, name: &str) -> LogicRule {
    LogicRule::new(
        head,
        body,
        vec![],
        ContextualScope {
            provenance: Some(format!("{EX}rule/{name}")),
            ..ContextualScope::default()
        },
    )
}

/// Non-recursive projection: `reach(x,y) :- edge(x,y)`. IDB = `{reach}`.
#[must_use]
pub fn projection_program() -> LogicProgram {
    LogicProgram::new(
        vec![],
        vec![rule(
            atom("?x", &iri("reach"), "?y", false),
            vec![atom("?x", &edge_pred(), "?y", false)],
            "proj",
        )],
        vec![],
        None,
    )
}

/// Linear-recursive transitive closure over `edge`. IDB = `{reach}`.
#[must_use]
pub fn transitive_program() -> LogicProgram {
    LogicProgram::new(
        vec![],
        vec![
            rule(
                atom("?x", &iri("reach"), "?y", false),
                vec![atom("?x", &edge_pred(), "?y", false)],
                "base",
            ),
            rule(
                atom("?x", &iri("reach"), "?z", false),
                vec![
                    atom("?x", &iri("reach"), "?y", false),
                    atom("?y", &edge_pred(), "?z", false),
                ],
                "step",
            ),
        ],
        vec![],
        None,
    )
}

/// Mutually-recursive `p`/`q` over `edge` (exercises the DBSP retraction path).
/// IDB = `{p, q}`.
#[must_use]
pub fn mutual_program() -> LogicProgram {
    LogicProgram::new(
        vec![],
        vec![
            rule(
                atom("?x", &iri("p"), "?y", false),
                vec![atom("?x", &edge_pred(), "?y", false)],
                "p-base",
            ),
            rule(
                atom("?x", &iri("q"), "?z", false),
                vec![
                    atom("?x", &iri("p"), "?y", false),
                    atom("?y", &edge_pred(), "?z", false),
                ],
                "q-from-p",
            ),
            rule(
                atom("?x", &iri("p"), "?z", false),
                vec![
                    atom("?x", &iri("q"), "?y", false),
                    atom("?y", &edge_pred(), "?z", false),
                ],
                "p-from-q",
            ),
        ],
        vec![],
        None,
    )
}

/// The IDB (derived) predicate IRIs for `program_key` — the head predicates that only ever
/// appear as derived rows, so filtering on them isolates the derived closure on both the
/// session and oracle sides without a circular dependency.
#[must_use]
pub fn idb_reach() -> Vec<String> {
    vec![iri("reach")]
}

#[must_use]
pub fn idb_pq() -> Vec<String> {
    vec![iri("p"), iri("q")]
}

// ── Derived-closure fingerprints ─────────────────────────────────────────────────

fn in_idb(predicate: &str, idb: &[String]) -> bool {
    idb.iter().any(|p| p == predicate)
}

/// The derived closure the SESSION maintains, projected to `(subject, predicate, object)`
/// over the IDB predicates. `ForwardRow.args` is `[subject, object, world-literal]`.
#[must_use]
pub fn session_derived(session: &ReasoningSession, idb: &[String]) -> BTreeSet<Triple> {
    session
        .facts()
        .rows
        .iter()
        .filter(|row| in_idb(&row.predicate, idb))
        .map(|row| {
            (
                canon(&term_display(&row.args[0])),
                row.predicate.clone(),
                canon(&term_display(&row.args[1])),
            )
        })
        .collect()
}

/// The from-scratch ORACLE derived closure over `edb`: `reason_program` → the `!is_edb`
/// inferred axioms, projected to `(subject, predicate, object)` over the IDB predicates.
#[must_use]
pub fn oracle_derived(
    program: &LogicProgram,
    edb: &RdfDataset,
    idb: &[String],
) -> BTreeSet<Triple> {
    let result = reason_program(program, edb).expect("oracle full recompute");
    result
        .inferred()
        .iter()
        .filter(|axiom| !axiom.is_edb && in_idb(&axiom.predicate, idb))
        .map(|axiom| {
            (
                canon(&axiom.subject),
                axiom.predicate.clone(),
                canon(&axiom.object),
            )
        })
        .collect()
}

/// One canonical proof witness, normalized for cross-engine comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonWitness {
    pub rule: String,
    pub premises: BTreeSet<Triple>,
    pub weight: i64,
    pub proof_height: u32,
}

/// The ORACLE derived witnesses, keyed by the canonical `(s,p,o)` triple: `rule_name`
/// (Some for a derived fact) and the premise SET.
#[must_use]
pub fn oracle_witnesses(
    program: &LogicProgram,
    edb: &RdfDataset,
    idb: &[String],
) -> BTreeMap<Triple, (Option<String>, BTreeSet<Triple>)> {
    let result = reason_program(program, edb).expect("oracle full recompute");
    result
        .inferred()
        .iter()
        .filter(|axiom| !axiom.is_edb && in_idb(&axiom.predicate, idb))
        .map(|axiom| {
            let key = (
                canon(&axiom.subject),
                axiom.predicate.clone(),
                canon(&axiom.object),
            );
            let premises = axiom
                .premises
                .iter()
                .map(|(s, p, o)| (canon(s), p.clone(), canon(o)))
                .collect::<BTreeSet<Triple>>();
            (key, (axiom.rule_name.clone(), premises))
        })
        .collect()
}

/// The SESSION provenance, normalized to a `(s,p,o) → CanonWitness` map.
#[must_use]
pub fn session_witnesses(session: &ReasoningSession) -> BTreeMap<Triple, CanonWitness> {
    session
        .provenance()
        .iter()
        .map(|witness: &DerivedProvenance| {
            let key = (
                canon(&witness.subject),
                witness.predicate.clone(),
                canon(&witness.object),
            );
            let premises = witness
                .premises
                .iter()
                .map(|(s, p, o)| (canon(s), p.clone(), canon(o)))
                .collect::<BTreeSet<Triple>>();
            (
                key,
                CanonWitness {
                    rule: witness.rule_iri.clone(),
                    premises,
                    weight: witness.weight,
                    proof_height: witness.proof_height,
                },
            )
        })
        .collect()
}

/// The ORACLE minimal proof height of every derived IDB fact, computed independently of
/// the session as a pure function of the from-scratch reasoner's canonical witness DAG.
///
/// The full reasoner ([`reason_program`]) selects the minimal-height witness per fact but
/// does not surface the height on [`gmeow_logic::reason::InferredAxiom`]. The minimal
/// proof height is, by definition, `1 + max(premise heights)` over that canonical witness
/// (an asserted EDB premise has height `0`), so it is reconstructed here by a memoized
/// descent over [`oracle_witnesses`]. This is the ground-truth `MinProofHeight` recurrence
/// — the same recurrence the maintainer computes over its OWN settled closure — so an
/// equal result is genuine cross-engine parity, not a tautology.
#[must_use]
pub fn oracle_proof_heights(
    program: &LogicProgram,
    edb: &RdfDataset,
    idb: &[String],
) -> BTreeMap<Triple, u32> {
    let witnesses = oracle_witnesses(program, edb, idb);
    let mut heights: BTreeMap<Triple, u32> = BTreeMap::new();
    for key in witnesses.keys() {
        let height = oracle_height_of(key, &witnesses, &mut heights, &mut BTreeSet::new());
        heights.insert(key.clone(), height);
    }
    heights
}

/// Memoized `1 + max(premise heights)` over the oracle witness DAG; a premise absent from
/// `witnesses` is an asserted EDB leaf (height `0`).
fn oracle_height_of(
    fact: &Triple,
    witnesses: &BTreeMap<Triple, (Option<String>, BTreeSet<Triple>)>,
    memo: &mut BTreeMap<Triple, u32>,
    visiting: &mut BTreeSet<Triple>,
) -> u32 {
    if let Some(&height) = memo.get(fact) {
        return height;
    }
    let Some((_, premises)) = witnesses.get(fact) else {
        return 0; // an asserted EDB fact is a proof leaf
    };
    assert!(
        visiting.insert(fact.clone()),
        "the minimal-height witness DAG must be acyclic at {fact:?}"
    );
    let max_premise = premises
        .iter()
        .map(|premise| oracle_height_of(premise, witnesses, memo, visiting))
        .max()
        .unwrap_or(0);
    visiting.remove(fact);
    let height = max_premise + 1;
    memo.insert(fact.clone(), height);
    height
}

/// A default reasoning contract + exact annotation contract (the AC1/AC2 baseline).
#[must_use]
pub fn baseline_contracts() -> (ReasoningContract, AnnotationContract) {
    (ReasoningContract::new(), AnnotationContract::exact())
}
