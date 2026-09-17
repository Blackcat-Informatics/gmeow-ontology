// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Original GMN logic assertions over authenticated producer observations.
//! No source loading, query execution or corpus mutation occurs in this consumer.

fn gmn_logic_coverage_complete_has_no_violations() {
    let rows = super::gmn_signatures::rows("gmn_logic_coverage_complete_has_no_violations");
    assert_eq!(
        rows, 0,
        "gmn-logic-coverage-complete.rq must return zero rows against the shipped graph: every \
         in-scope logic-glyph-plane term is rendered by a candidate, a dictionary alias, or a \
         compositional structural link"
    );
}

fn gmn_logic_coverage_complete_fires_on_uncovered_in_scope_term() {
    let rows =
        super::gmn_signatures::rows("gmn_logic_coverage_complete_fires_on_uncovered_in_scope_term");
    assert_eq!(
        rows, 1,
        "the coverage gate must return exactly the one injected in-scope term the notation \
         cannot write"
    );
}

fn gmn_logic_no_double_binding_has_no_violations() {
    let rows = super::gmn_signatures::rows("gmn_logic_no_double_binding_has_no_violations");
    assert_eq!(
        rows, 0,
        "gmn-logic-no-double-binding.rq must return zero rows against the shipped graph: each \
         logic term carries exactly one disposition and the adopted-glyph terms hold no \
         dictionary alias"
    );
}

fn gmn_logic_no_double_binding_fires_on_adopted_plus_dictionary_alias() {
    let rows = super::gmn_signatures::rows(
        "gmn_logic_no_double_binding_fires_on_adopted_plus_dictionary_alias",
    );
    assert_eq!(
        rows, 1,
        "the double-binding gate must return exactly the one injected clash: logic:BelnapTrue \
         adopted as ● yet also aliased in a dictionary entry"
    );
}

fn gmn_logic_precedence_fibered_has_no_violations() {
    let rows = super::gmn_signatures::rows("gmn_logic_precedence_fibered_has_no_violations");
    assert_eq!(
        rows, 0,
        "gmn-logic-precedence-fibered.rq must return zero rows against the shipped graph: each \
         logic-plane operator binds one consistent precedence within its result-sort fiber"
    );
}

fn gmn_logic_precedence_fibered_fires_on_double_precedence_in_one_fiber() {
    let rows = super::gmn_signatures::rows(
        "gmn_logic_precedence_fibered_fires_on_double_precedence_in_one_fiber",
    );
    assert_eq!(
        rows, 1,
        "the precedence-fibered gate must return exactly the one injected clash: one operator \
         bound at two precedences inside a single result-sort fiber"
    );
}

fn gmn_logic_signature_coherence_has_no_violations() {
    let rows = super::gmn_signatures::rows("gmn_logic_signature_coherence_has_no_violations");
    assert_eq!(
        rows, 0,
        "gmn-logic-signature-coherence.rq must return zero rows against the shipped graph: every \
         logic operator form declares its result sort, arity kind, infix associativity, and \
         structured-operator argument roles"
    );
}

fn gmn_logic_signature_coherence_fires_on_incomplete_order_sorted_signature() {
    let rows = super::gmn_signatures::rows(
        "gmn_logic_signature_coherence_fires_on_incomplete_order_sorted_signature",
    );
    assert_eq!(
        rows, 3,
        "the signature gate must return exactly the three missing-piece rows for one infix form \
         with no result sort, no arity kind, and no associativity"
    );
}

fn gmn_logic_signature_coherence_fires_on_missing_structured_arg_role() {
    let rows = super::gmn_signatures::rows(
        "gmn_logic_signature_coherence_fires_on_missing_structured_arg_role",
    );
    assert_eq!(
        rows, 1,
        "the signature gate must return exactly the one injected structured operator missing \
         its required gmnArgRoleAntecedent slot"
    );
}

fn gmn_modal_accessibility_typed_has_no_violations() {
    let rows = super::gmn_signatures::rows("gmn_modal_accessibility_typed_has_no_violations");
    assert_eq!(
        rows, 0,
        "gmn-modal-accessibility-typed.rq must return zero rows against the shipped graph: it \
         instantiates no modal formula node over the blurred accessibility union or a modal-force \
         value"
    );
}

fn gmn_modal_accessibility_typed_fires_on_bare_accessible_from() {
    let rows =
        super::gmn_signatures::rows("gmn_modal_accessibility_typed_fires_on_bare_accessible_from");
    assert_eq!(
        rows, 1,
        "the modal gate must return exactly the one injected modal node pinned to the bare \
         logic:accessibleFrom union"
    );
}

fn gmn_turnstile_entailment_distinct_has_no_violations() {
    let rows = super::gmn_signatures::rows("gmn_turnstile_entailment_distinct_has_no_violations");
    assert_eq!(
        rows, 0,
        "gmn-turnstile-entailment-distinct.rq must return zero rows against the shipped graph: \
         logic:derives (⊢) and logic:entails (⊨) are signed with distinct glyph, key, and \
         codepoint"
    );
}

fn gmn_turnstile_entailment_distinct_fires_on_collapsed_candidate() {
    let rows = super::gmn_signatures::rows(
        "gmn_turnstile_entailment_distinct_fires_on_collapsed_candidate",
    );
    assert_eq!(
        rows, 1,
        "the turnstile gate must return exactly the one injected candidate collapsing ⊢ and ⊨ \
         onto a single sign"
    );
}

fn gmn_belnap_distinctness_has_no_violations() {
    let rows = super::gmn_signatures::rows("gmn_belnap_distinctness_has_no_violations");
    assert_eq!(
        rows, 0,
        "gmn-belnap-distinctness.rq must return zero rows against the shipped graph: the eight \
         grade signs use eight distinct geometric codepoints, all distinct from ⊤/⊥"
    );
}

fn gmn_belnap_distinctness_fires_on_shared_codepoint_and_target() {
    let rows =
        super::gmn_signatures::rows("gmn_belnap_distinctness_fires_on_shared_codepoint_and_target");
    assert_eq!(
        rows, 2,
        "the belnap gate must return exactly two rows for the one injected duplicate: a shared \
         codepoint with the logic:BelnapTrue sign and a shared target with the logic:BelnapFalse \
         sign"
    );
}

fn gmn_logic_ascii_fallback_uniqueness_has_no_violations() {
    let rows = super::gmn_signatures::rows("gmn_logic_ascii_fallback_uniqueness_has_no_violations");
    assert_eq!(
        rows, 0,
        "gmn-logic-ascii-fallback-uniqueness.rq must return zero rows against the shipped graph: \
         each logic-plane ASCII fallback key is unique across distinct logic: targets"
    );
}

fn gmn_logic_ascii_fallback_uniqueness_fires_on_collision() {
    let rows =
        super::gmn_signatures::rows("gmn_logic_ascii_fallback_uniqueness_fires_on_collision");
    assert_eq!(
        rows, 1,
        "the logic-plane fallback gate must return exactly the one injected collision: two \
         distinct logic: targets sharing an ASCII fallback key"
    );
}

fn gmn_glyph_fallback_global_unique_has_no_violations() {
    let rows = super::gmn_signatures::rows("gmn_glyph_fallback_global_unique_has_no_violations");
    assert_eq!(
        rows, 0,
        "gmn-glyph-fallback-global-unique.rq must return zero rows against the shipped graph: no \
         two distinct targets share one fallback key anywhere across the planes"
    );
}

fn gmn_glyph_fallback_global_unique_fires_on_cross_plane_collision() {
    let rows = super::gmn_signatures::rows(
        "gmn_glyph_fallback_global_unique_fires_on_cross_plane_collision",
    );
    assert_eq!(
        rows, 1,
        "the global fallback gate must return exactly the one injected cross-plane collision: a \
         logic: target and a math: target sharing an ASCII fallback key"
    );
}

fn gmn_modal_accessibility_typed_fires_on_untyped_relation() {
    let rows =
        super::gmn_signatures::rows("gmn_modal_accessibility_typed_fires_on_untyped_relation");
    assert_eq!(
        rows, 1,
        "the modal gate must reject a modal node pinned to a relation outside the six typed \
         accessibility relations, not only the two hard-coded bad values"
    );
}

fn gmn_belnap_distinctness_fires_on_top_collision_regardless_of_iri_order() {
    let rows = super::gmn_signatures::rows(
        "gmn_belnap_distinctness_fires_on_top_collision_regardless_of_iri_order",
    );
    assert_eq!(
        rows, 1,
        "the belnap gate must catch a grade sign colliding on codepoint with a ⊤/⊥ sign even when \
         the non-grade sign's IRI sorts first — the order-dependent filter missed this"
    );
}

fn gmn_logic_precedence_fibered_fires_on_single_form_two_precedences() {
    let rows = super::gmn_signatures::rows(
        "gmn_logic_precedence_fibered_fires_on_single_form_two_precedences",
    );
    assert_eq!(
        rows, 2,
        "the precedence gate must catch one form bound at two precedences in a single fiber: the \
         two ordered (precA, precB) solutions of the self-clash"
    );
}

fn gmn_logic_glyph_scope_disjoint_has_no_violations() {
    let rows = super::gmn_signatures::rows("gmn_logic_glyph_scope_disjoint_has_no_violations");
    assert_eq!(
        rows, 0,
        "gmn-logic-glyph-scope-disjoint.rq must return zero rows against the shipped graph: no \
         term carries BOTH gmnGlyphInScope and gmnGlyphNamedKeyRuled"
    );
}

fn gmn_logic_glyph_scope_disjoint_fires_on_both_markers() {
    let rows = super::gmn_signatures::rows("gmn_logic_glyph_scope_disjoint_fires_on_both_markers");
    assert_eq!(
        rows, 1,
        "the disjointness gate must return exactly the one injected term carrying both the \
         in-scope and named-key-ruled markers"
    );
}

#[test]
fn authored_gmn_logic_signature_contracts() {
    gmn_logic_coverage_complete_has_no_violations();
    gmn_logic_coverage_complete_fires_on_uncovered_in_scope_term();
    gmn_logic_no_double_binding_has_no_violations();
    gmn_logic_no_double_binding_fires_on_adopted_plus_dictionary_alias();
    gmn_logic_precedence_fibered_has_no_violations();
    gmn_logic_precedence_fibered_fires_on_double_precedence_in_one_fiber();
    gmn_logic_signature_coherence_has_no_violations();
    gmn_logic_signature_coherence_fires_on_incomplete_order_sorted_signature();
    gmn_logic_signature_coherence_fires_on_missing_structured_arg_role();
    gmn_modal_accessibility_typed_has_no_violations();
    gmn_modal_accessibility_typed_fires_on_bare_accessible_from();
    gmn_turnstile_entailment_distinct_has_no_violations();
    gmn_turnstile_entailment_distinct_fires_on_collapsed_candidate();
    gmn_belnap_distinctness_has_no_violations();
    gmn_belnap_distinctness_fires_on_shared_codepoint_and_target();
    gmn_logic_ascii_fallback_uniqueness_has_no_violations();
    gmn_logic_ascii_fallback_uniqueness_fires_on_collision();
    gmn_glyph_fallback_global_unique_has_no_violations();
    gmn_glyph_fallback_global_unique_fires_on_cross_plane_collision();
    gmn_modal_accessibility_typed_fires_on_untyped_relation();
    gmn_belnap_distinctness_fires_on_top_collision_regardless_of_iri_order();
    gmn_logic_precedence_fibered_fires_on_single_form_two_precedences();
    gmn_logic_glyph_scope_disjoint_has_no_violations();
    gmn_logic_glyph_scope_disjoint_fires_on_both_markers();
}
