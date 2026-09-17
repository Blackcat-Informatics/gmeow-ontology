// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact query selections and negative controls for the authored GMN math contract.

use super::Case;

const OPERATOR_ARITY_Q: &str =
    "slices/grounding/lang/queries/verify/gmn-operator-arity-coherence.rq";
const FORM_SIGNATURE_Q: &str =
    "slices/grounding/lang/queries/verify/gmn-form-signature-completeness.rq";
const PRECEDENCE_CONSISTENCY_Q: &str =
    "slices/grounding/lang/queries/verify/gmn-infix-precedence-consistency.rq";
const FALLBACK_UNIQUENESS_Q: &str =
    "slices/grounding/lang/queries/verify/gmn-ascii-fallback-uniqueness.rq";

const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang:  <https://blackcatinformatics.ca/lang/> .
@prefix math:  <https://blackcatinformatics.ca/math/> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
";

pub(super) fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "gmn_operator_arity_coherence_has_no_violations",
            query_path: OPERATOR_ARITY_Q,
            injection: None,
        },
        Case {
            name: "gmn_operator_arity_coherence_fires_on_wrong_arity",
            query_path: OPERATOR_ARITY_Q,
            // A math: ObjectProperty operator whose denoting form declares gmnArity 3 (not 2).
            injection: Some(format!(
                "{PREFIXES}
        math:badBinaryRel a owl:ObjectProperty .
        gmeow:gmnFormMathBadArity a lang:Form ;
            lang:inSignSystem gmeow:gmnModelNotation ;
            gmeow:gmnFixity gmeow:gmnFixityInfix ;
            gmeow:gmnArity 3 .
        gmeow:gmnDenMathBadArity a lang:Denotation ;
            lang:denotationTarget math:badBinaryRel ;
            lang:denotedForm gmeow:gmnFormMathBadArity .
        "
            )),
        },
        Case {
            name: "gmn_form_signature_completeness_has_no_violations",
            query_path: FORM_SIGNATURE_Q,
            injection: None,
        },
        Case {
            name: "gmn_form_signature_completeness_fires_on_missing_precedence",
            query_path: FORM_SIGNATURE_Q,
            // A fixity-bearing form that declares arity but omits precedence — a half-specified
            // operator signature.
            injection: Some(format!(
                "{PREFIXES}
        gmeow:gmnFormMathIncomplete a lang:Form ;
            lang:inSignSystem gmeow:gmnModelNotation ;
            gmeow:gmnFixity gmeow:gmnFixityInfix ;
            gmeow:gmnArity 2 .
        "
            )),
        },
        Case {
            name: "gmn_infix_precedence_consistency_has_no_violations",
            query_path: PRECEDENCE_CONSISTENCY_Q,
            injection: None,
        },
        Case {
            name: "gmn_infix_precedence_consistency_fires_on_double_precedence",
            query_path: PRECEDENCE_CONSISTENCY_Q,
            // A SECOND math-plane infix form for math:Addition at a clashing precedence (99) — the
            // shipped gmnFormMathAddition already binds it at 60, so the operator now carries two
            // binding strengths.
            injection: Some(format!(
                "{PREFIXES}
        gmeow:gmnFormMathAdditionClash a lang:Form ;
            lang:inSignSystem gmeow:gmnModelNotation ;
            gmeow:gmnFixity gmeow:gmnFixityInfix ;
            gmeow:gmnArity 2 ;
            gmeow:gmnPrecedence 99 .
        gmeow:gmnDenMathAdditionClash a lang:Denotation ;
            lang:denotationContext gmeow:gmnMathGlyphContext ;
            lang:denotationTarget math:Addition ;
            lang:denotedForm gmeow:gmnFormMathAdditionClash .
        "
            )),
        },
        Case {
            name: "gmn_ascii_fallback_uniqueness_has_no_violations",
            query_path: FALLBACK_UNIQUENESS_Q,
            injection: None,
        },
        Case {
            name: "gmn_ascii_fallback_uniqueness_fires_on_collision",
            query_path: FALLBACK_UNIQUENESS_Q,
            // A candidate reusing the `in` fallback (shipped for math:hasElement / ∈) on a
            // different target math:subsetOf — an ambiguous ASCII key stream.
            injection: Some(format!(
                "{PREFIXES}
        gmeow:gmnCandidateMathFallbackClash a gmeow:GmnSymbolCandidate ;
            gmeow:gmnCandidateTarget math:subsetOf ;
            gmeow:gmnCandidateGlyph \"⊆\" ;
            gmeow:gmnAsciiFallback \"in\" .
        "
            )),
        },
    ]
}
