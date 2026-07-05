// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Gate 4 — the quantified subject–verb–object sentence class lowers compositionally to a
//! `logic:` formula that the NATIVE reasoner consumes, with per-stage preservation records
//! present.
//!
//! This is the cross-crate consumption gate: the pure `gmeow-lang-bridge` crate lowers "every
//! cat chases a mouse" to `∀x(cat(x) → ∃y(mouse(y) ∧ chase(x, y)))` (with per-stage
//! preservation records), and this pipeline test — which depends on BOTH `gmeow-logic` and
//! `gmeow-lang-bridge` — hands the lowered formula to the in-process native reasoner
//! (`gmeow_logic::reason::reason_program`) and asserts a CONCRETE entailment.
//!
//! The native reasoner's evaluable fragment is Horn+NAF over a Datalog-style chase; it cannot
//! introduce a fresh existential witness under a universal (an `∃y` in the head of a `∀x`
//! rule needs a Skolem FUNCTION / value invention the chase does not mint). So the lowering to
//! the reasoner's fragment is an EXPLICIT, RECORDED stage: the `∃y` is Skolemized to a Skolem
//! constant. Collapsing the per-subject witness to one individual is an over-approximation
//! (complete over the original's consequences, not sound), so the stage is honestly recorded
//! as [`PreservationKind::CompleteOver`] — never `Exact`. The gate is "the reasoner consumes it
//! with per-stage preservation records present," which this honest staged lowering satisfies:
//! from `∀x(cat(x) → ∃y(mouse(y) ∧ chase(x, y)))` and `cat(Tom)` the chase derives the witness
//! `chase(Tom, sk)` and `mouse(sk)`.

use gmeow_lang_bridge::lower::{flagship_svo_sentence, lower_svo, LoweringStage};
use gmeow_logic::reason::reason_program;
use gmeow_logic_compile::ir::{
    ContextualScope, Formula, LogicAxiom, LogicProgram, PreservationKind, Term,
};
use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

/// The named graph / world the facts and rules are chased in.
const W: &str = "http://gmeow.example/w";
/// `rdf:type`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The individual the ground fact `cat(Tom)` is about.
const TOM: &str = "http://gmeow.example/tom";
/// The Skolem constant the `∃y` existential witness is lowered to.
const SK: &str = "http://example.org/lang/skolem/mouse-witness";

fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
}

fn dataset(quads: Vec<RdfQuad>) -> std::sync::Arc<purrdf::RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for q in quads {
        builder.push_owned_quad(&q);
    }
    builder.freeze().expect("valid test dataset")
}

/// The predicate IRI of a unary/binary [`Formula::Atom`] (the reified relation).
fn predicate_iri(atom: &Formula) -> String {
    match atom {
        Formula::Atom {
            relation: Term::Iri(iri),
            ..
        } => iri.clone(),
        other => panic!("expected an IRI-relation atom, got {other:?}"),
    }
}

/// Read the three predicate IRIs `(cat, mouse, chase)` out of the compositional lowering of
/// "every cat chases a mouse": `∀x(cat(x) → ∃y(mouse(y) ∧ chase(x, y)))`. Reading them off the
/// lowered formula (rather than hard-coding) makes the Skolemization a genuine function OF the
/// lowering, not a parallel re-statement of it.
fn svo_predicates(formula: &Formula) -> (String, String, String) {
    let Formula::Forall { body, .. } = formula else {
        panic!("expected a leading universal, got {formula:?}");
    };
    let Formula::Implies(restrictor, scope) = body.as_ref() else {
        panic!("expected a restricted universal `∀x(restrictor → scope)`");
    };
    let cat = predicate_iri(restrictor);
    let Formula::Exists { body: inner, .. } = scope.as_ref() else {
        panic!("expected an existential scope `∃y(...)`");
    };
    let Formula::And(conj) = inner.as_ref() else {
        panic!("expected a conjunctive existential body `mouse(y) ∧ chase(x, y)`");
    };
    assert_eq!(conj.len(), 2, "existential body is a two-part conjunction");
    let mouse = predicate_iri(&conj[0]);
    let chase = predicate_iri(&conj[1]);
    (cat, mouse, chase)
}

#[test]
fn svo_lowering_is_consumed_by_the_native_reasoner_with_staged_preservation() {
    // ── 1. Compositional lowering (in the pure lang-bridge crate) ──────────────────
    let lowering = lower_svo(&flagship_svo_sentence()).expect("flagship SVO lowers");
    // Every compositional lowering step is declared — no undeclared stage.
    lowering
        .assert_all_stages_declared()
        .expect("every compositional lowering stage is declared");
    let (cat_iri, mouse_iri, chase_iri) = svo_predicates(&lowering.formula);

    // ── 2. Explicit, RECORDED lowering to the reasoner's Horn fragment ─────────────
    // The `∃y` under `∀x` is Skolemized to the constant `sk`. Unary predicates realize as
    // `rdf:type` triples, the binary verb as a direct triple:
    //     chase(?x, sk)  :-  cat(?x)          [ (?x, chase, sk)     :- (?x, rdf:type, cat) ]
    //     mouse(sk)      :-  cat(?x)          [ (sk, rdf:type, mouse) :- (?x, rdf:type, cat) ]
    let cat_body = LogicAxiom::ground("?x", RDF_TYPE, &cat_iri, false).expect("cat body");
    let chase_rule = gmeow_logic_compile::ir::LogicRule::new(
        LogicAxiom::ground("?x", &chase_iri, SK, false).expect("chase head"),
        vec![cat_body.clone()],
        vec![],
        ContextualScope::default(),
    );
    let mouse_rule = gmeow_logic_compile::ir::LogicRule::new(
        LogicAxiom::ground(SK, RDF_TYPE, &mouse_iri, false).expect("mouse head"),
        vec![cat_body],
        vec![],
        ContextualScope::default(),
    );

    // The Skolemization is an OVER-approximation (a single Skolem constant collapses the
    // per-subject existential witness), so the honest preservation kind is CompleteOver — the
    // reasoner derives at least everything the original entails (a witness for each cat),
    // possibly more (all cats sharing one witness), and never Exact.
    let skolem_stage = LoweringStage {
        name: "existential-skolemization".to_owned(),
        preservation: PreservationKind::CompleteOver,
        note: "lowered `∃y` under `∀x` to a Skolem constant and realized the unary/binary \
                predicates as `rdf:type` / direct triples for the Horn+NAF chase; the constant \
                collapses the per-subject witness, an over-approximation (complete, not sound)"
            .to_owned(),
    };

    // The full staged record that reaches the reasoner: the compositional stages PLUS the
    // fragment-lowering stage. Every stage carries a preservation record (the
    // `lang:UndeclaredLoweringStage` floor holds across the reasoner handoff).
    let mut staged: Vec<LoweringStage> = lowering.stages.clone();
    staged.push(skolem_stage);
    assert_eq!(
        staged.len(),
        4,
        "three compositional stages + one Skolemization stage"
    );
    for stage in &staged {
        assert!(
            !stage.note.trim().is_empty(),
            "stage '{}' has a preservation note",
            stage.name
        );
    }
    // The compositional stages are exact; only the fragment-lowering Skolemization approximates.
    assert!(staged[..3]
        .iter()
        .all(|s| s.preservation == PreservationKind::Exact));
    assert_eq!(staged[3].preservation, PreservationKind::CompleteOver);

    // ── 3. Native reasoner consumes it ────────────────────────────────────────────
    // The program CARRIES the original full-FOL formula (via `with_formulas`, honestly
    // disclosed as unsupported residue by the reasoner) AND the Skolemized Horn rules the
    // chase actually fires; the ground fact `cat(Tom)` lives in the EDB.
    let program = LogicProgram::new(vec![], vec![chase_rule, mouse_rule], vec![], None)
        .with_formulas(vec![lowering.formula.clone()]);
    assert_eq!(
        program.formulas.len(),
        1,
        "the lowered full-FOL formula is carried in the program"
    );
    let edb = dataset(vec![quad(TOM, RDF_TYPE, &cat_iri)]);

    let result =
        reason_program(&program, edb.as_ref()).expect("reason_program consumes the program");

    // ── 4. Concrete entailment: chase(Tom, sk) and mouse(sk) ───────────────────────
    // Objects decode to their N3 surface (`<iri>`); subjects/predicates are bare IRIs.
    let sk_obj = format!("<{SK}>");
    let mouse_obj = format!("<{mouse_iri}>");
    assert!(
        result
            .inferred()
            .iter()
            .any(|ax| ax.subject == TOM && ax.predicate == chase_iri && ax.object == sk_obj),
        "the reasoner must derive chase(Tom, sk); closure: {:?}",
        result.inferred()
    );
    assert!(
        result
            .inferred()
            .iter()
            .any(|ax| ax.subject == SK && ax.predicate == RDF_TYPE && ax.object == mouse_obj),
        "the reasoner must derive mouse(sk) [sk rdf:type mouse]; closure: {:?}",
        result.inferred()
    );
    // The derived witnesses are genuinely INFERRED, not asserted input.
    assert!(
        result
            .inferred()
            .iter()
            .any(|ax| !ax.is_edb && ax.subject == TOM && ax.predicate == chase_iri),
        "chase(Tom, sk) is a derived (non-EDB) consequence"
    );
    // The program is consistent (no contradiction glut from this lowering).
    assert!(result.is_consistent(), "the lowered program is consistent");
}
