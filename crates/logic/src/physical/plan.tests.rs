// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::query_ir::StructNode;
use gmeow_term_arena::engine::StructNodeParts;
use gmeow_term_arena::engine::TermDag;

#[test]
fn compiled_literal_identity_uses_native_fields_not_presentation() {
    let plain = TermValue::simple_literal("a");
    let langless = TermValue::Literal {
        lexical_form: "a".to_owned(),
        datatype: gmeow_term_arena::engine::RDF_LANG_STRING.to_owned(),
        language: None,
        direction: None,
    };
    let rule = |value| {
        EvalRule::positive(
            "urn:identity:rule",
            EvalAtom::positive(
                EvalTerm::named("urn:s"),
                "urn:out",
                EvalTerm::Var("value".into()),
            ),
            vec![EvalAtom::positive(
                EvalTerm::Var("value".into()),
                "urn:in",
                EvalTerm::ConstLit(value),
            )],
        )
    };
    assert_ne!(
        canonical_rule_hash(&[rule(plain)]),
        canonical_rule_hash(&[rule(langless)])
    );
}

#[test]
fn joint_layouts_release_when_their_template_and_consumers_are_dropped() {
    let rules = [EvalRule::positive(
        "urn:layout:lifetime",
        EvalAtom::positive(EvalTerm::named("urn:s"), "urn:p", EvalTerm::named("urn:o")),
        Vec::new(),
    )];
    let layouts = RuleLayouts::new(&rules).unwrap();
    let weak = Arc::downgrade(&layouts.rules);
    let effects = rules
        .iter()
        .map(super::super::effects::ProducerEffect::rule)
        .collect::<Vec<_>>();
    let schedule = super::super::effects::schedule(
        &effects,
        crate::native_semantics::SemanticVocabulary::Exact,
        &BTreeSet::new(),
    )
    .unwrap();
    let executable =
        compile_certified_stratum(schedule.ordinary_strata(&layouts).remove(0)).unwrap();
    drop(layouts);
    assert!(
        weak.upgrade().is_some(),
        "the live executable retains its rules"
    );
    drop(executable);
    assert!(
        weak.upgrade().is_none(),
        "no secondary cache retains the template"
    );
}

#[test]
#[should_panic(expected = "joint schedule must bind the exact ordinary producer IR")]
fn joint_certificate_rejects_layouts_for_a_different_rule_with_the_same_name() {
    let rule = EvalRule::positive(
        "urn:layout:identity",
        EvalAtom::positive(EvalTerm::named("urn:s"), "urn:p", EvalTerm::named("urn:o")),
        Vec::new(),
    );
    let effect = super::super::effects::ProducerEffect::rule(&rule);
    let schedule = super::super::effects::schedule(
        &[effect],
        crate::native_semantics::SemanticVocabulary::Exact,
        &BTreeSet::new(),
    )
    .unwrap();
    let mut changed = rule;
    changed.head.object = EvalTerm::named("urn:changed");
    schedule.ordinary_strata(&RuleLayouts::new(&[changed]).unwrap());
}

/// A minimal single-fact `EvalRule` (`rule_iri` "r") carrying one `QBuiltin::Compare`
/// whose `lhs` operand is `term` — the shape `canonical_rule_hash`'s `hash_qterm`
/// dispatches on.
fn rule_with_builtin_operand(term: QTerm) -> EvalRule {
    let mut rule = EvalRule::positive(
        "https://example.org/r",
        EvalAtom::positive(
            EvalTerm::var("?X"),
            "https://example.org/p",
            EvalTerm::var("?X"),
        ),
        Vec::new(),
    );
    rule.builtins.push(QBuiltin::Compare {
        lhs: term,
        op: crate::query_ir::CmpOp::Eq,
        rhs: QTerm::Num(0),
    });
    rule
}

/// G13 lock: `canonical_rule_hash`'s flat rule-IR pipeline never threads a `TermDag` (it
/// is a distinct, arena-free pipeline from the structured-term `gmeow_term_arena` term DAG
/// world), so a `QTerm::Struct` reaching `hash_qterm` cannot be content-hashed by
/// `TermDag::key` here — hashing its arena-local `NodeId::index()` instead would risk a
/// false collision between two DIFFERENT structured terms from unrelated arenas that
/// happen to share a raw index. `hash_qterm`'s `QTerm::Struct` arm is therefore a hard
/// `unreachable!` (never a silent index hash), justified by `QBuiltin`'s own contract
/// (`query_ir::QBuiltin` operands are documented `Var`/`Num` only — arithmetic never
/// carries a compound term) rather than papered over.
///
/// This test proves the arm's chosen failure mode DIRECTLY: two independently-built
/// `TermDag`s each intern one leaf node first, so their first `NodeId`s share
/// `index() == 0` by construction — exactly the collision a naive `NodeId::index()`
/// hash would silently forge into an equal digest for two DIFFERENT structured terms.
/// Wrapping each into a `QBuiltin` operand and hashing the enclosing rule must instead
/// PANIC (the documented `unreachable!` firing), never silently succeed with a
/// forged-equal (or any) hash.
#[test]
fn canonical_rule_hash_hard_fails_on_a_struct_builtin_operand_rather_than_hashing_arena_index() {
    let mut dag_a = TermDag::new();
    let leaf_a = dag_a.intern_leaf(purrdf::TermValue::iri("https://example.org/a"));
    let mut dag_b = TermDag::new();
    let leaf_b = dag_b.intern_leaf(purrdf::TermValue::iri("https://example.org/b"));
    assert_eq!(
        leaf_a.index(),
        leaf_b.index(),
        "two independently-built arenas' first interned node share the same raw index \
             — exactly the collision NodeId::index() hashing would silently forge"
    );

    let struct_a = QTerm::Struct(StructNode::wrap(leaf_a, dag_a.arena()));
    let struct_b = QTerm::Struct(StructNode::wrap(leaf_b, dag_b.arena()));

    let rule_a = rule_with_builtin_operand(struct_a);
    let rule_b = rule_with_builtin_operand(struct_b);

    let result_a = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        canonical_rule_hash(std::slice::from_ref(&rule_a))
    }));
    assert!(
        result_a.is_err(),
        "a Struct QBuiltin operand must hard-panic canonical_rule_hash, never silently \
             hash a raw NodeId::index()"
    );
    let result_b = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        canonical_rule_hash(std::slice::from_ref(&rule_b))
    }));
    assert!(
        result_b.is_err(),
        "the SAME guard must fire for the second (index-colliding, different-arena) \
             Struct rule — never silently forging an equal hash for two distinct terms"
    );
}
