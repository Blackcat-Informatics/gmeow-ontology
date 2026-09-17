// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Original GMN math assertions over authenticated producer observations.
//! No source loading, query execution or corpus mutation occurs in this consumer.

fn gmn_operator_arity_coherence_has_no_violations() {
    let rows = super::gmn_signatures::rows("gmn_operator_arity_coherence_has_no_violations");
    assert_eq!(
        rows, 0,
        "gmn-operator-arity-coherence.rq must return zero rows against the shipped graph: \
         every math: owl:ObjectProperty operator (∈ ⊆ ∘) declares gmnArity 2"
    );
}

fn gmn_operator_arity_coherence_fires_on_wrong_arity() {
    let rows = super::gmn_signatures::rows("gmn_operator_arity_coherence_fires_on_wrong_arity");
    assert_eq!(
        rows, 1,
        "the arity gate must return exactly the one injected ObjectProperty operator whose \
         form declares gmnArity != 2"
    );
}

fn gmn_form_signature_completeness_has_no_violations() {
    let rows = super::gmn_signatures::rows("gmn_form_signature_completeness_has_no_violations");
    assert_eq!(
        rows, 0,
        "gmn-form-signature-completeness.rq must return zero rows against the shipped graph: \
         every fixity-bearing GMN form declares both gmnPrecedence and gmnArity"
    );
}

fn gmn_form_signature_completeness_fires_on_missing_precedence() {
    let rows =
        super::gmn_signatures::rows("gmn_form_signature_completeness_fires_on_missing_precedence");
    assert_eq!(
        rows, 1,
        "the completeness gate must return exactly the one injected fixity-bearing form \
         missing its gmnPrecedence"
    );
}

fn gmn_infix_precedence_consistency_has_no_violations() {
    let rows = super::gmn_signatures::rows("gmn_infix_precedence_consistency_has_no_violations");
    assert_eq!(
        rows, 0,
        "gmn-infix-precedence-consistency.rq must return zero rows against the shipped graph: \
         each math-plane operator binds a single consistent precedence (band-sharing across \
         distinct operators is by design and not flagged)"
    );
}

fn gmn_infix_precedence_consistency_fires_on_double_precedence() {
    let rows =
        super::gmn_signatures::rows("gmn_infix_precedence_consistency_fires_on_double_precedence");
    assert_eq!(
        rows, 1,
        "the precedence-consistency gate must return exactly the one injected clash: \
         math:Addition bound at two different precedences"
    );
}

fn gmn_ascii_fallback_uniqueness_has_no_violations() {
    let rows = super::gmn_signatures::rows("gmn_ascii_fallback_uniqueness_has_no_violations");
    assert_eq!(
        rows, 0,
        "gmn-ascii-fallback-uniqueness.rq must return zero rows against the shipped graph: \
         each math-plane ASCII fallback key is unique across distinct targets"
    );
}

fn gmn_ascii_fallback_uniqueness_fires_on_collision() {
    let rows = super::gmn_signatures::rows("gmn_ascii_fallback_uniqueness_fires_on_collision");
    assert_eq!(
        rows, 1,
        "the fallback-uniqueness gate must return exactly the one injected collision: two \
         distinct math: targets sharing the `in` fallback key"
    );
}

#[test]
fn authored_gmn_math_signature_contracts() {
    gmn_operator_arity_coherence_has_no_violations();
    gmn_operator_arity_coherence_fires_on_wrong_arity();
    gmn_form_signature_completeness_has_no_violations();
    gmn_form_signature_completeness_fires_on_missing_precedence();
    gmn_infix_precedence_consistency_has_no_violations();
    gmn_infix_precedence_consistency_fires_on_double_precedence();
    gmn_ascii_fallback_uniqueness_has_no_violations();
    gmn_ascii_fallback_uniqueness_fires_on_collision();
}
