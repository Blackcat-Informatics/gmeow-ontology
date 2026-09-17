// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact query selections and negative controls for the authored GMN logic contract.

use super::Case;

const COVERAGE_COMPLETE_Q: &str =
    "slices/grounding/logic/queries/verify/gmn-logic-coverage-complete.rq";
const NO_DOUBLE_BINDING_Q: &str =
    "slices/grounding/logic/queries/verify/gmn-logic-no-double-binding.rq";
const PRECEDENCE_FIBERED_Q: &str =
    "slices/grounding/logic/queries/verify/gmn-logic-precedence-fibered.rq";
const SIGNATURE_COHERENCE_Q: &str =
    "slices/grounding/logic/queries/verify/gmn-logic-signature-coherence.rq";
const MODAL_ACCESSIBILITY_Q: &str =
    "slices/grounding/logic/queries/verify/gmn-modal-accessibility-typed.rq";
const TURNSTILE_DISTINCT_Q: &str =
    "slices/grounding/logic/queries/verify/gmn-turnstile-entailment-distinct.rq";
const BELNAP_DISTINCTNESS_Q: &str =
    "slices/grounding/logic/queries/verify/gmn-belnap-distinctness.rq";
const LOGIC_FALLBACK_UNIQUENESS_Q: &str =
    "slices/grounding/lang/queries/verify/gmn-logic-ascii-fallback-uniqueness.rq";
const GLOBAL_FALLBACK_UNIQUE_Q: &str =
    "slices/grounding/lang/queries/verify/gmn-glyph-fallback-global-unique.rq";
const GLYPH_SCOPE_DISJOINT_Q: &str =
    "slices/grounding/logic/queries/verify/gmn-logic-glyph-scope-disjoint.rq";

const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang:  <https://blackcatinformatics.ca/lang/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix math:  <https://blackcatinformatics.ca/math/> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
@prefix ex:    <http://example.org/logic/> .
";

pub(super) fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "gmn_logic_coverage_complete_has_no_violations",
            query_path: COVERAGE_COMPLETE_Q,
            injection: None,
        },
        Case {
            name: "gmn_logic_coverage_complete_fires_on_uncovered_in_scope_term",
            query_path: COVERAGE_COMPLETE_Q,
            // An in-scope glyph-plane term with NO rendering path at all: no candidate, no dictionary
            // alias, no implication-guard pairing, and not an owl:Class structural node — the
            // uncovered-gap the coverage anti-join catches.
            injection: Some(format!(
                "{PREFIXES}
        ex:orphanInScopeTerm gmeow:gmnGlyphInScope true .
        "
            )),
        },
        Case {
            name: "gmn_logic_no_double_binding_has_no_violations",
            query_path: NO_DOUBLE_BINDING_Q,
            injection: None,
        },
        Case {
            name: "gmn_logic_no_double_binding_fires_on_adopted_plus_dictionary_alias",
            query_path: NO_DOUBLE_BINDING_Q,
            // logic:BelnapTrue is already an ADOPTED glyph (gmnCandidateLogicBelnapTrue, ●). Overlay a
            // fragmenting dictionary alias for the SAME target — now one term is simultaneously an
            // executable glyph AND a named-key alias, and a writer cannot decide which spelling is
            // canonical.
            injection: Some(format!(
                "{PREFIXES}
        ex:fragmentingBelnapAlias gmeow:gmnDictionaryEntryTerm logic:BelnapTrue .
        "
            )),
        },
        Case {
            name: "gmn_logic_precedence_fibered_has_no_violations",
            query_path: PRECEDENCE_FIBERED_Q,
            injection: None,
        },
        Case {
            name: "gmn_logic_precedence_fibered_fires_on_double_precedence_in_one_fiber",
            query_path: PRECEDENCE_FIBERED_Q,
            // One fresh operator target rendered by TWO infix logic-plane forms in the SAME
            // result-sort fiber (gmnSortFormula) declaring DIFFERENT precedences (10 vs 20) — its
            // binding strength within the Formula ladder is undecidable.
            injection: Some(format!(
                "{PREFIXES}
        ex:clashingOp a owl:ObjectProperty .
        ex:formPrecA a lang:Form ;
            lang:inSignSystem gmeow:gmnModelNotation ;
            gmeow:gmnFixity gmeow:gmnFixityInfix ;
            gmeow:gmnResultSort gmeow:gmnSortFormula ;
            gmeow:gmnPrecedence 10 .
        ex:formPrecB a lang:Form ;
            lang:inSignSystem gmeow:gmnModelNotation ;
            gmeow:gmnFixity gmeow:gmnFixityInfix ;
            gmeow:gmnResultSort gmeow:gmnSortFormula ;
            gmeow:gmnPrecedence 20 .
        ex:denPrecA lang:denotationContext gmeow:gmnLogicGlyphContext ;
            lang:denotationTarget ex:clashingOp ;
            lang:denotedForm ex:formPrecA .
        ex:denPrecB lang:denotationContext gmeow:gmnLogicGlyphContext ;
            lang:denotationTarget ex:clashingOp ;
            lang:denotedForm ex:formPrecB .
        "
            )),
        },
        Case {
            name: "gmn_logic_signature_coherence_has_no_violations",
            query_path: SIGNATURE_COHERENCE_Q,
            injection: None,
        },
        Case {
            name: "gmn_logic_signature_coherence_fires_on_incomplete_order_sorted_signature",
            query_path: SIGNATURE_COHERENCE_Q,
            // An infix logic-plane form missing all three order-sorted signature pieces at once — no
            // gmnResultSort, no gmnArityKind, and (being infix) no gmnAssociativity. The gate emits one
            // row per missing piece, so exactly three fire.
            injection: Some(format!(
                "{PREFIXES}
        ex:sigFormIncomplete a lang:Form ;
            lang:inSignSystem gmeow:gmnModelNotation ;
            gmeow:gmnFixity gmeow:gmnFixityInfix .
        ex:sigDenIncomplete lang:denotationContext gmeow:gmnLogicGlyphContext ;
            lang:denotationTarget ex:sigOpaqueTarget ;
            lang:denotedForm ex:sigFormIncomplete .
        "
            )),
        },
        Case {
            name: "gmn_logic_signature_coherence_fires_on_missing_structured_arg_role",
            query_path: SIGNATURE_COHERENCE_Q,
            // A structured constructor: a fresh implication form (denotes logic:consequent, the → head)
            // that declares its result sort and arity kind but omits the required
            // gmnArgRoleAntecedent operand slot — the implication-no-antecedent branch fires once.
            injection: Some(format!(
                "{PREFIXES}
        ex:sigFormNoAntecedent a lang:Form ;
            lang:inSignSystem gmeow:gmnModelNotation ;
            gmeow:gmnResultSort gmeow:gmnSortFormula ;
            gmeow:gmnArityKind gmeow:gmnArityKindFixed .
        ex:sigDenNoAntecedent lang:denotationContext gmeow:gmnLogicGlyphContext ;
            lang:denotationTarget logic:consequent ;
            lang:denotedForm ex:sigFormNoAntecedent .
        "
            )),
        },
        Case {
            name: "gmn_modal_accessibility_typed_has_no_violations",
            query_path: MODAL_ACCESSIBILITY_Q,
            injection: None,
        },
        Case {
            name: "gmn_modal_accessibility_typed_fires_on_bare_accessible_from",
            query_path: MODAL_ACCESSIBILITY_Q,
            // A box (logic:necessarily) modal node evaluated over the bare logic:accessibleFrom union
            // instead of one typed accessibility relation — the cross-type-entailment mistyping the gate
            // forbids.
            injection: Some(format!(
                "{PREFIXES}
        ex:mistypedModalNode logic:necessarily ex:someBodyFormula ;
            logic:overAccessibility logic:accessibleFrom .
        "
            )),
        },
        Case {
            name: "gmn_turnstile_entailment_distinct_has_no_violations",
            query_path: TURNSTILE_DISTINCT_Q,
            injection: None,
        },
        Case {
            name: "gmn_turnstile_entailment_distinct_fires_on_collapsed_candidate",
            query_path: TURNSTILE_DISTINCT_Q,
            // A single candidate claiming BOTH turnstile targets at once — the collapse that would erase
            // the derivability / entailment distinction at the surface.
            injection: Some(format!(
                "{PREFIXES}
        ex:collapsedTurnstileCandidate gmeow:gmnCandidateTarget logic:derives , logic:entails .
        "
            )),
        },
        Case {
            name: "gmn_belnap_distinctness_has_no_violations",
            query_path: BELNAP_DISTINCTNESS_Q,
            injection: None,
        },
        Case {
            name: "gmn_belnap_distinctness_fires_on_shared_codepoint_and_target",
            query_path: BELNAP_DISTINCTNESS_Q,
            // One injected duplicate grade sign for logic:BelnapFalse whose codepoint "U+25CF" is the
            // one already adopted for logic:BelnapTrue (●). This single injection collides on TWO axes,
            // so the pairwise gate emits two rows:
            //   * codepoint clash — the new sign shares U+25CF with the shipped logic:BelnapTrue sign;
            //   * target clash — the new sign shares the target logic:BelnapFalse with the shipped
            //     logic:BelnapFalse sign (the gmnCandidateTarget-collision branch, cp-independent).
            injection: Some(format!(
                "{PREFIXES}
        ex:duplicateBelnapSign a gmeow:GmnSymbolCandidate ;
            gmeow:gmnCandidateTarget logic:BelnapFalse ;
            gmeow:gmnCodepoints \"U+25CF\" .
        "
            )),
        },
        Case {
            name: "gmn_logic_ascii_fallback_uniqueness_has_no_violations",
            query_path: LOGIC_FALLBACK_UNIQUENESS_Q,
            injection: None,
        },
        Case {
            name: "gmn_logic_ascii_fallback_uniqueness_fires_on_collision",
            query_path: LOGIC_FALLBACK_UNIQUENESS_Q,
            // Two candidates on DISTINCT logic: targets (logic:derives, logic:entails) declaring the SAME
            // ASCII fallback key — the second parseable notation stream becomes ambiguous.
            injection: Some(format!(
                "{PREFIXES}
        ex:logicFallbackClashA a gmeow:GmnSymbolCandidate ;
            gmeow:gmnCandidateTarget logic:derives ;
            gmeow:gmnAsciiFallback \"zzlogicclash\" .
        ex:logicFallbackClashB a gmeow:GmnSymbolCandidate ;
            gmeow:gmnCandidateTarget logic:entails ;
            gmeow:gmnAsciiFallback \"zzlogicclash\" .
        "
            )),
        },
        Case {
            name: "gmn_glyph_fallback_global_unique_has_no_violations",
            query_path: GLOBAL_FALLBACK_UNIQUE_Q,
            injection: None,
        },
        Case {
            name: "gmn_glyph_fallback_global_unique_fires_on_cross_plane_collision",
            query_path: GLOBAL_FALLBACK_UNIQUE_Q,
            // Two candidates on DISTINCT targets in DIFFERENT planes (logic:derives, math:Addition)
            // sharing one ASCII fallback key — the global stream a downstream ASCII reader cannot
            // disambiguate.
            injection: Some(format!(
                "{PREFIXES}
        ex:globalFallbackClashA a gmeow:GmnSymbolCandidate ;
            gmeow:gmnCandidateTarget logic:derives ;
            gmeow:gmnAsciiFallback \"zzglobalclash\" .
        ex:globalFallbackClashB a gmeow:GmnSymbolCandidate ;
            gmeow:gmnCandidateTarget math:Addition ;
            gmeow:gmnAsciiFallback \"zzglobalclash\" .
        "
            )),
        },
        Case {
            name: "gmn_modal_accessibility_typed_fires_on_untyped_relation",
            query_path: MODAL_ACCESSIBILITY_Q,
            // A box modal node pinned to an ARBITRARY relation that is neither one of the six typed
            // accessibility relations nor the bare logic:accessibleFrom union nor a modal-force value.
            // The denylist form of the gate would have let this through; the allowlist form rejects any
            // relation outside the closed typed set.
            injection: Some(format!(
                "{PREFIXES}
        ex:untypedModalNode logic:necessarily ex:someBodyFormula ;
            logic:overAccessibility ex:homebrewRelation .
        "
            )),
        },
        Case {
            name: "gmn_belnap_distinctness_fires_on_top_collision_regardless_of_iri_order",
            query_path: BELNAP_DISTINCTNESS_Q,
            // A ⊤ (logic:Top, a NON-grade) sign that shares the shipped logic:BelnapTrue codepoint
            // U+25CF, minted with an http://example.org/ IRI that sorts BEFORE the shipped grade sign's
            // https://blackcatinformatics.ca/ IRI. Under the old STR(?signA) < STR(?signB) filter — which
            // could only place the grade in ?signA and demanded it sort first — this collision was
            // invisible. The order-independent gate catches it because ?otherB (logic:Top) is a
            // non-grade, so no IRI ordering is required.
            injection: Some(format!(
                "{PREFIXES}
        ex:aTopCollision a gmeow:GmnSymbolCandidate ;
            gmeow:gmnCandidateTarget logic:Top ;
            gmeow:gmnCodepoints \"U+25CF\" .
        "
            )),
        },
        Case {
            name: "gmn_logic_precedence_fibered_fires_on_single_form_two_precedences",
            query_path: PRECEDENCE_FIBERED_Q,
            // ONE infix logic-plane form carrying TWO gmnPrecedence values in the same result-sort fiber.
            // The old STR(?formA) < STR(?formB) filter excluded the ?formA = ?formB case, so a single
            // self-clashing form was never caught; the same-form arm now catches it.
            injection: Some(format!(
                "{PREFIXES}
        ex:selfClashOp a owl:ObjectProperty .
        ex:formTwoPrec a lang:Form ;
            lang:inSignSystem gmeow:gmnModelNotation ;
            gmeow:gmnFixity gmeow:gmnFixityInfix ;
            gmeow:gmnResultSort gmeow:gmnSortFormula ;
            gmeow:gmnPrecedence 10 , 20 .
        ex:denTwoPrec lang:denotationContext gmeow:gmnLogicGlyphContext ;
            lang:denotationTarget ex:selfClashOp ;
            lang:denotedForm ex:formTwoPrec .
        "
            )),
        },
        Case {
            name: "gmn_logic_glyph_scope_disjoint_has_no_violations",
            query_path: GLYPH_SCOPE_DISJOINT_Q,
            injection: None,
        },
        Case {
            name: "gmn_logic_glyph_scope_disjoint_fires_on_both_markers",
            query_path: GLYPH_SCOPE_DISJOINT_Q,
            // One term marked BOTH in-scope for a rendered glyph AND ruled to a named key — the
            // partition violation that would let the coverage gate silently exempt an in-scope term.
            injection: Some(format!(
                "{PREFIXES}
        ex:doubleMarkedTerm gmeow:gmnGlyphInScope true ;
            gmeow:gmnGlyphNamedKeyRuled true .
        "
            )),
        },
    ]
}
