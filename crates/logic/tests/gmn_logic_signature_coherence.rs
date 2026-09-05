// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMN logic-glyph-plane verify queries return ZERO rows against the CURRENT shipped
//! graph, and each FIRES on an injected violation.
//!
//! `slices/grounding/{logic,lang}/queries/verify/*.rq` are harvested by
//! `crates/logic/build.rs` and run over the reasoned graph at `make reason-verify` (a
//! returned row = a violation). The nine GMN logic-glyph gates —
//! `gmn-logic-coverage-complete.rq`, `gmn-logic-no-double-binding.rq`,
//! `gmn-logic-precedence-fibered.rq`, `gmn-logic-signature-coherence.rq`,
//! `gmn-modal-accessibility-typed.rq`, `gmn-turnstile-entailment-distinct.rq`,
//! `gmn-belnap-distinctness.rq` (logic slice) and `gmn-logic-ascii-fallback-uniqueness.rq`,
//! `gmn-glyph-fallback-global-unique.rq` (lang slice) — are pure ABox/schema joins over
//! asserted triples (no derived edges), so these tests prove — cheaply, without a full reason
//! chase — BOTH halves of a real gate: it stays silent on the shipped merged lang + math +
//! logic graph, AND it returns exactly the offending row(s) when a counter-example is
//! injected. Without the injected-negative half, a gate that can never fire would pass the
//! "zero rows" obligation vacuously.

use std::sync::{Arc, OnceLock};

use purrdf::sparql::NativeSparqlEngine;
use purrdf::{
    DatasetMut, MutableDataset, RdfDataset, SparqlEngine, SparqlRequest, SparqlResult,
    parse_dataset,
};

const COVERAGE_COMPLETE_Q: &str =
    include_str!("../../../slices/grounding/logic/queries/verify/gmn-logic-coverage-complete.rq");
const NO_DOUBLE_BINDING_Q: &str =
    include_str!("../../../slices/grounding/logic/queries/verify/gmn-logic-no-double-binding.rq");
const PRECEDENCE_FIBERED_Q: &str =
    include_str!("../../../slices/grounding/logic/queries/verify/gmn-logic-precedence-fibered.rq");
const SIGNATURE_COHERENCE_Q: &str =
    include_str!("../../../slices/grounding/logic/queries/verify/gmn-logic-signature-coherence.rq");
const MODAL_ACCESSIBILITY_Q: &str =
    include_str!("../../../slices/grounding/logic/queries/verify/gmn-modal-accessibility-typed.rq");
const TURNSTILE_DISTINCT_Q: &str = include_str!(
    "../../../slices/grounding/logic/queries/verify/gmn-turnstile-entailment-distinct.rq"
);
const BELNAP_DISTINCTNESS_Q: &str =
    include_str!("../../../slices/grounding/logic/queries/verify/gmn-belnap-distinctness.rq");
const LOGIC_FALLBACK_UNIQUENESS_Q: &str = include_str!(
    "../../../slices/grounding/lang/queries/verify/gmn-logic-ascii-fallback-uniqueness.rq"
);
const GLOBAL_FALLBACK_UNIQUE_Q: &str = include_str!(
    "../../../slices/grounding/lang/queries/verify/gmn-glyph-fallback-global-unique.rq"
);
const GLYPH_SCOPE_DISJOINT_Q: &str = include_str!(
    "../../../slices/grounding/logic/queries/verify/gmn-logic-glyph-scope-disjoint.rq"
);

/// Parse one grounding slice's `module.ttl` into a dataset.
fn grounding_module(slice: &str) -> Arc<RdfDataset> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices/grounding")
        .join(slice)
        .join("module.ttl");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    parse_dataset(&bytes, "text/turtle", None).unwrap_or_else(|e| panic!("parse {path:?}: {e}"))
}

/// The frozen union of the lang + math + logic module graphs, flattened to the default
/// graph so a no-GRAPH verify SELECT matches it (mirrors the native verify substrate).
/// Cached once across the suite — the three modules are static fixtures, so re-parsing
/// them per test is wasted I/O and CPU.
fn merged_grounding_graph() -> Arc<RdfDataset> {
    static MERGED: OnceLock<Arc<RdfDataset>> = OnceLock::new();
    MERGED
        .get_or_init(|| {
            let mut store = MutableDataset::new(Arc::new(RdfDataset::union(&[])));
            for slice in ["lang", "math", "logic"] {
                let ds = grounding_module(slice);
                for quad in ds.flat_default_graph_quads() {
                    store
                        .insert(quad)
                        .expect("grounding-module quads are absolute IRIs");
                }
            }
            store.freeze().expect("freeze merged grounding graph")
        })
        .clone()
}

/// The merged grounding graph with an extra Turtle fragment overlaid — the seam for a
/// falsifiable negative: inject a counter-example and assert the gate returns exactly it.
/// Built with the same insert-then-freeze path as `merged_grounding_graph` so the base is
/// genuinely present alongside the injected quads.
fn merged_plus(extra_ttl: &str) -> Arc<RdfDataset> {
    let mut store = MutableDataset::new(Arc::new(RdfDataset::union(&[])));
    let base = merged_grounding_graph();
    for quad in base.flat_default_graph_quads() {
        store
            .insert(quad)
            .expect("grounding-module quads are absolute IRIs");
    }
    let extra = parse_dataset(extra_ttl.as_bytes(), "text/turtle", None)
        .expect("parse injected counter-example turtle");
    for quad in extra.flat_default_graph_quads() {
        store
            .insert(quad)
            .expect("grounding-module quads are absolute IRIs");
    }
    store
        .freeze()
        .expect("freeze merged grounding graph plus injection")
}

/// Row count of a SELECT verify query over the frozen graph.
fn violation_rows(graph: &Arc<RdfDataset>, sparql: &str) -> usize {
    let engine = NativeSparqlEngine::new();
    let result = engine
        .query(
            graph,
            SparqlRequest {
                query: sparql,
                base_iri: None,
                substitutions: &[],
            },
        )
        .expect("verify query parses and evaluates");
    match result {
        SparqlResult::Solutions { rows, .. } => rows.len(),
        other => panic!("a verify query must be a SELECT, got {other:?}"),
    }
}

// The Turtle prefix preamble every injected counter-example shares.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang:  <https://blackcatinformatics.ca/lang/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix math:  <https://blackcatinformatics.ca/math/> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
@prefix ex:    <http://example.org/logic/> .
";

// ── 1. gmn-logic-coverage-complete ───────────────────────────────────────────────

#[test]
fn gmn_logic_coverage_complete_has_no_violations() {
    let graph = merged_grounding_graph();
    let rows = violation_rows(&graph, COVERAGE_COMPLETE_Q);
    assert_eq!(
        rows, 0,
        "gmn-logic-coverage-complete.rq must return zero rows against the shipped graph: every \
         in-scope logic-glyph-plane term is rendered by a candidate, a dictionary alias, or a \
         compositional structural link"
    );
}

#[test]
fn gmn_logic_coverage_complete_fires_on_uncovered_in_scope_term() {
    // An in-scope glyph-plane term with NO rendering path at all: no candidate, no dictionary
    // alias, no implication-guard pairing, and not an owl:Class structural node — the
    // uncovered-gap the coverage anti-join catches.
    let injection = format!(
        "{PREFIXES}
        ex:orphanInScopeTerm gmeow:gmnGlyphInScope true .
        "
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, COVERAGE_COMPLETE_Q);
    assert_eq!(
        rows, 1,
        "the coverage gate must return exactly the one injected in-scope term the notation \
         cannot write"
    );
}

// ── 2. gmn-logic-no-double-binding ───────────────────────────────────────────────

#[test]
fn gmn_logic_no_double_binding_has_no_violations() {
    let graph = merged_grounding_graph();
    let rows = violation_rows(&graph, NO_DOUBLE_BINDING_Q);
    assert_eq!(
        rows, 0,
        "gmn-logic-no-double-binding.rq must return zero rows against the shipped graph: each \
         logic term carries exactly one disposition and the adopted-glyph terms hold no \
         dictionary alias"
    );
}

#[test]
fn gmn_logic_no_double_binding_fires_on_adopted_plus_dictionary_alias() {
    // logic:BelnapTrue is already an ADOPTED glyph (gmnCandidateLogicBelnapTrue, ●). Overlay a
    // fragmenting dictionary alias for the SAME target — now one term is simultaneously an
    // executable glyph AND a named-key alias, and a writer cannot decide which spelling is
    // canonical.
    let injection = format!(
        "{PREFIXES}
        ex:fragmentingBelnapAlias gmeow:gmnDictionaryEntryTerm logic:BelnapTrue .
        "
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, NO_DOUBLE_BINDING_Q);
    assert_eq!(
        rows, 1,
        "the double-binding gate must return exactly the one injected clash: logic:BelnapTrue \
         adopted as ● yet also aliased in a dictionary entry"
    );
}

// ── 3. gmn-logic-precedence-fibered ──────────────────────────────────────────────

#[test]
fn gmn_logic_precedence_fibered_has_no_violations() {
    let graph = merged_grounding_graph();
    let rows = violation_rows(&graph, PRECEDENCE_FIBERED_Q);
    assert_eq!(
        rows, 0,
        "gmn-logic-precedence-fibered.rq must return zero rows against the shipped graph: each \
         logic-plane operator binds one consistent precedence within its result-sort fiber"
    );
}

#[test]
fn gmn_logic_precedence_fibered_fires_on_double_precedence_in_one_fiber() {
    // One fresh operator target rendered by TWO infix logic-plane forms in the SAME
    // result-sort fiber (gmnSortFormula) declaring DIFFERENT precedences (10 vs 20) — its
    // binding strength within the Formula ladder is undecidable.
    let injection = format!(
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
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, PRECEDENCE_FIBERED_Q);
    assert_eq!(
        rows, 1,
        "the precedence-fibered gate must return exactly the one injected clash: one operator \
         bound at two precedences inside a single result-sort fiber"
    );
}

// ── 4. gmn-logic-signature-coherence (split: order-sorted signature + arg-role) ──

#[test]
fn gmn_logic_signature_coherence_has_no_violations() {
    let graph = merged_grounding_graph();
    let rows = violation_rows(&graph, SIGNATURE_COHERENCE_Q);
    assert_eq!(
        rows, 0,
        "gmn-logic-signature-coherence.rq must return zero rows against the shipped graph: every \
         logic operator form declares its result sort, arity kind, infix associativity, and \
         structured-operator argument roles"
    );
}

#[test]
fn gmn_logic_signature_coherence_fires_on_incomplete_order_sorted_signature() {
    // An infix logic-plane form missing all three order-sorted signature pieces at once — no
    // gmnResultSort, no gmnArityKind, and (being infix) no gmnAssociativity. The gate emits one
    // row per missing piece, so exactly three fire.
    let injection = format!(
        "{PREFIXES}
        ex:sigFormIncomplete a lang:Form ;
            lang:inSignSystem gmeow:gmnModelNotation ;
            gmeow:gmnFixity gmeow:gmnFixityInfix .
        ex:sigDenIncomplete lang:denotationContext gmeow:gmnLogicGlyphContext ;
            lang:denotationTarget ex:sigOpaqueTarget ;
            lang:denotedForm ex:sigFormIncomplete .
        "
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, SIGNATURE_COHERENCE_Q);
    assert_eq!(
        rows, 3,
        "the signature gate must return exactly the three missing-piece rows for one infix form \
         with no result sort, no arity kind, and no associativity"
    );
}

#[test]
fn gmn_logic_signature_coherence_fires_on_missing_structured_arg_role() {
    // A structured constructor: a fresh implication form (denotes logic:consequent, the → head)
    // that declares its result sort and arity kind but omits the required
    // gmnArgRoleAntecedent operand slot — the implication-no-antecedent branch fires once.
    let injection = format!(
        "{PREFIXES}
        ex:sigFormNoAntecedent a lang:Form ;
            lang:inSignSystem gmeow:gmnModelNotation ;
            gmeow:gmnResultSort gmeow:gmnSortFormula ;
            gmeow:gmnArityKind gmeow:gmnArityKindFixed .
        ex:sigDenNoAntecedent lang:denotationContext gmeow:gmnLogicGlyphContext ;
            lang:denotationTarget logic:consequent ;
            lang:denotedForm ex:sigFormNoAntecedent .
        "
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, SIGNATURE_COHERENCE_Q);
    assert_eq!(
        rows, 1,
        "the signature gate must return exactly the one injected structured operator missing \
         its required gmnArgRoleAntecedent slot"
    );
}

// ── 5. gmn-modal-accessibility-typed ─────────────────────────────────────────────

#[test]
fn gmn_modal_accessibility_typed_has_no_violations() {
    let graph = merged_grounding_graph();
    let rows = violation_rows(&graph, MODAL_ACCESSIBILITY_Q);
    assert_eq!(
        rows, 0,
        "gmn-modal-accessibility-typed.rq must return zero rows against the shipped graph: it \
         instantiates no modal formula node over the blurred accessibility union or a modal-force \
         value"
    );
}

#[test]
fn gmn_modal_accessibility_typed_fires_on_bare_accessible_from() {
    // A box (logic:necessarily) modal node evaluated over the bare logic:accessibleFrom union
    // instead of one typed accessibility relation — the cross-type-entailment mistyping the gate
    // forbids.
    let injection = format!(
        "{PREFIXES}
        ex:mistypedModalNode logic:necessarily ex:someBodyFormula ;
            logic:overAccessibility logic:accessibleFrom .
        "
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, MODAL_ACCESSIBILITY_Q);
    assert_eq!(
        rows, 1,
        "the modal gate must return exactly the one injected modal node pinned to the bare \
         logic:accessibleFrom union"
    );
}

// ── 6. gmn-turnstile-entailment-distinct ─────────────────────────────────────────

#[test]
fn gmn_turnstile_entailment_distinct_has_no_violations() {
    let graph = merged_grounding_graph();
    let rows = violation_rows(&graph, TURNSTILE_DISTINCT_Q);
    assert_eq!(
        rows, 0,
        "gmn-turnstile-entailment-distinct.rq must return zero rows against the shipped graph: \
         logic:derives (⊢) and logic:entails (⊨) are signed with distinct glyph, key, and \
         codepoint"
    );
}

#[test]
fn gmn_turnstile_entailment_distinct_fires_on_collapsed_candidate() {
    // A single candidate claiming BOTH turnstile targets at once — the collapse that would erase
    // the derivability / entailment distinction at the surface.
    let injection = format!(
        "{PREFIXES}
        ex:collapsedTurnstileCandidate gmeow:gmnCandidateTarget logic:derives , logic:entails .
        "
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, TURNSTILE_DISTINCT_Q);
    assert_eq!(
        rows, 1,
        "the turnstile gate must return exactly the one injected candidate collapsing ⊢ and ⊨ \
         onto a single sign"
    );
}

// ── 7. gmn-belnap-distinctness ───────────────────────────────────────────────────

#[test]
fn gmn_belnap_distinctness_has_no_violations() {
    let graph = merged_grounding_graph();
    let rows = violation_rows(&graph, BELNAP_DISTINCTNESS_Q);
    assert_eq!(
        rows, 0,
        "gmn-belnap-distinctness.rq must return zero rows against the shipped graph: the eight \
         grade signs use eight distinct geometric codepoints, all distinct from ⊤/⊥"
    );
}

#[test]
fn gmn_belnap_distinctness_fires_on_shared_codepoint_and_target() {
    // One injected duplicate grade sign for logic:BelnapFalse whose codepoint "U+25CF" is the
    // one already adopted for logic:BelnapTrue (●). This single injection collides on TWO axes,
    // so the pairwise gate emits two rows:
    //   * codepoint clash — the new sign shares U+25CF with the shipped logic:BelnapTrue sign;
    //   * target clash — the new sign shares the target logic:BelnapFalse with the shipped
    //     logic:BelnapFalse sign (the gmnCandidateTarget-collision branch, cp-independent).
    let injection = format!(
        "{PREFIXES}
        ex:duplicateBelnapSign a gmeow:GmnSymbolCandidate ;
            gmeow:gmnCandidateTarget logic:BelnapFalse ;
            gmeow:gmnCodepoints \"U+25CF\" .
        "
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, BELNAP_DISTINCTNESS_Q);
    assert_eq!(
        rows, 2,
        "the belnap gate must return exactly two rows for the one injected duplicate: a shared \
         codepoint with the logic:BelnapTrue sign and a shared target with the logic:BelnapFalse \
         sign"
    );
}

// ── 8. gmn-logic-ascii-fallback-uniqueness (lang plane, logic: targets) ──────────

#[test]
fn gmn_logic_ascii_fallback_uniqueness_has_no_violations() {
    let graph = merged_grounding_graph();
    let rows = violation_rows(&graph, LOGIC_FALLBACK_UNIQUENESS_Q);
    assert_eq!(
        rows, 0,
        "gmn-logic-ascii-fallback-uniqueness.rq must return zero rows against the shipped graph: \
         each logic-plane ASCII fallback key is unique across distinct logic: targets"
    );
}

#[test]
fn gmn_logic_ascii_fallback_uniqueness_fires_on_collision() {
    // Two candidates on DISTINCT logic: targets (logic:derives, logic:entails) declaring the SAME
    // ASCII fallback key — the second parseable notation stream becomes ambiguous.
    let injection = format!(
        "{PREFIXES}
        ex:logicFallbackClashA a gmeow:GmnSymbolCandidate ;
            gmeow:gmnCandidateTarget logic:derives ;
            gmeow:gmnAsciiFallback \"zzlogicclash\" .
        ex:logicFallbackClashB a gmeow:GmnSymbolCandidate ;
            gmeow:gmnCandidateTarget logic:entails ;
            gmeow:gmnAsciiFallback \"zzlogicclash\" .
        "
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, LOGIC_FALLBACK_UNIQUENESS_Q);
    assert_eq!(
        rows, 1,
        "the logic-plane fallback gate must return exactly the one injected collision: two \
         distinct logic: targets sharing an ASCII fallback key"
    );
}

// ── 9. gmn-glyph-fallback-global-unique (cross-plane) ────────────────────────────

#[test]
fn gmn_glyph_fallback_global_unique_has_no_violations() {
    let graph = merged_grounding_graph();
    let rows = violation_rows(&graph, GLOBAL_FALLBACK_UNIQUE_Q);
    assert_eq!(
        rows, 0,
        "gmn-glyph-fallback-global-unique.rq must return zero rows against the shipped graph: no \
         two distinct targets share one fallback key anywhere across the planes"
    );
}

#[test]
fn gmn_glyph_fallback_global_unique_fires_on_cross_plane_collision() {
    // Two candidates on DISTINCT targets in DIFFERENT planes (logic:derives, math:Addition)
    // sharing one ASCII fallback key — the global stream a downstream ASCII reader cannot
    // disambiguate.
    let injection = format!(
        "{PREFIXES}
        ex:globalFallbackClashA a gmeow:GmnSymbolCandidate ;
            gmeow:gmnCandidateTarget logic:derives ;
            gmeow:gmnAsciiFallback \"zzglobalclash\" .
        ex:globalFallbackClashB a gmeow:GmnSymbolCandidate ;
            gmeow:gmnCandidateTarget math:Addition ;
            gmeow:gmnAsciiFallback \"zzglobalclash\" .
        "
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, GLOBAL_FALLBACK_UNIQUE_Q);
    assert_eq!(
        rows, 1,
        "the global fallback gate must return exactly the one injected cross-plane collision: a \
         logic: target and a math: target sharing an ASCII fallback key"
    );
}

#[test]
fn gmn_modal_accessibility_typed_fires_on_untyped_relation() {
    // A box modal node pinned to an ARBITRARY relation that is neither one of the six typed
    // accessibility relations nor the bare logic:accessibleFrom union nor a modal-force value.
    // The denylist form of the gate would have let this through; the allowlist form rejects any
    // relation outside the closed typed set.
    let injection = format!(
        "{PREFIXES}
        ex:untypedModalNode logic:necessarily ex:someBodyFormula ;
            logic:overAccessibility ex:homebrewRelation .
        "
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, MODAL_ACCESSIBILITY_Q);
    assert_eq!(
        rows, 1,
        "the modal gate must reject a modal node pinned to a relation outside the six typed \
         accessibility relations, not only the two hard-coded bad values"
    );
}

#[test]
fn gmn_belnap_distinctness_fires_on_top_collision_regardless_of_iri_order() {
    // A ⊤ (logic:Top, a NON-grade) sign that shares the shipped logic:BelnapTrue codepoint
    // U+25CF, minted with an http://example.org/ IRI that sorts BEFORE the shipped grade sign's
    // https://blackcatinformatics.ca/ IRI. Under the old STR(?signA) < STR(?signB) filter — which
    // could only place the grade in ?signA and demanded it sort first — this collision was
    // invisible. The order-independent gate catches it because ?otherB (logic:Top) is a
    // non-grade, so no IRI ordering is required.
    let injection = format!(
        "{PREFIXES}
        ex:aTopCollision a gmeow:GmnSymbolCandidate ;
            gmeow:gmnCandidateTarget logic:Top ;
            gmeow:gmnCodepoints \"U+25CF\" .
        "
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, BELNAP_DISTINCTNESS_Q);
    assert_eq!(
        rows, 1,
        "the belnap gate must catch a grade sign colliding on codepoint with a ⊤/⊥ sign even when \
         the non-grade sign's IRI sorts first — the order-dependent filter missed this"
    );
}

#[test]
fn gmn_logic_precedence_fibered_fires_on_single_form_two_precedences() {
    // ONE infix logic-plane form carrying TWO gmnPrecedence values in the same result-sort fiber.
    // The old STR(?formA) < STR(?formB) filter excluded the ?formA = ?formB case, so a single
    // self-clashing form was never caught; the same-form arm now catches it.
    let injection = format!(
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
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, PRECEDENCE_FIBERED_Q);
    assert_eq!(
        rows, 2,
        "the precedence gate must catch one form bound at two precedences in a single fiber: the \
         two ordered (precA, precB) solutions of the self-clash"
    );
}

// ── gmn-logic-glyph-scope-disjoint ───────────────────────────────────────────────

#[test]
fn gmn_logic_glyph_scope_disjoint_has_no_violations() {
    let graph = merged_grounding_graph();
    let rows = violation_rows(&graph, GLYPH_SCOPE_DISJOINT_Q);
    assert_eq!(
        rows, 0,
        "gmn-logic-glyph-scope-disjoint.rq must return zero rows against the shipped graph: no \
         term carries BOTH gmnGlyphInScope and gmnGlyphNamedKeyRuled"
    );
}

#[test]
fn gmn_logic_glyph_scope_disjoint_fires_on_both_markers() {
    // One term marked BOTH in-scope for a rendered glyph AND ruled to a named key — the
    // partition violation that would let the coverage gate silently exempt an in-scope term.
    let injection = format!(
        "{PREFIXES}
        ex:doubleMarkedTerm gmeow:gmnGlyphInScope true ;
            gmeow:gmnGlyphNamedKeyRuled true .
        "
    );
    let graph = merged_plus(&injection);
    let rows = violation_rows(&graph, GLYPH_SCOPE_DISJOINT_Q);
    assert_eq!(
        rows, 1,
        "the disjointness gate must return exactly the one injected term carrying both the \
         in-scope and named-key-ruled markers"
    );
}
