// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The **correspondence overclaim gate** — the Principle-5 (bridge-by-reference, never
//! `owl:sameAs` collapse) enforcement point for the alignment lowerings.
//!
//! The preservation-vs-residue [`assert_no_overclaim`](super::assert_no_overclaim)
//! gate cannot see the relation a lowering emits, so it cannot catch a *bridge* view
//! emitting an equivalence predicate, or a *caveated overlap* emitting `exactMatch`.
//! This gate closes that hole: an **equivalence** predicate
//! (`skos:exactMatch` / SSSOM `exactMatch`, `owl:equivalentClass`/`equivalentProperty`,
//! the EDOAL `=` relation, the alignment `Equiv` relation) may be emitted **only** when
//! the correspondence's relation is [`CorrespondenceRelation::Equiv`] AND its morphism
//! class is not a [`MorphismClass::BridgeView`] AND its morphism kind is not a
//! [`MorphismKind::CommitmentShiftingBridge`]. Any other case — a bridge emitting
//! equivalence, or a subsuming/overlapping/related/disjoint relation emitting
//! `exactMatch` — is a build failure.

use super::super::ir::{CorrespondenceRelation, MorphismClass, MorphismKind};
use super::OverclaimError;

/// Whether `predicate` is an **equivalence** marker (the strongest, symmetric claim).
/// Accepts the full-IRI, CURIE, and bare-local forms each dialect emits, plus the
/// EDOAL `=` relation token and the alignment `Equiv` relation.
pub fn is_equivalence_predicate(predicate: &str) -> bool {
    let p = predicate.trim();
    if p == "=" {
        return true;
    }
    // Strip a `prefix:` or full-IRI namespace to the local name.
    let local = p.rsplit(['#', '/', ':']).next().unwrap_or(p);
    matches!(
        local,
        "exactMatch" | "equivalentClass" | "equivalentProperty" | "sameAs" | "Equiv"
    )
}

/// Enforce the relation/morphism overclaim contract for one emitted alignment
/// statement. `target` names the lowering (`"sssom"`, `"edoal"`, …) for the error.
///
/// # Errors
///
/// Returns [`OverclaimError`] if `emitted_predicate` is an equivalence marker but the
/// correspondence is not a genuine equivalence — i.e. its relation is not `Equiv`, or
/// it is a bridge view / commitment-shifting bridge (which by Principle 5 may never
/// assert equivalence, only an honest by-reference bridge).
pub fn assert_relation_no_overclaim(
    target: &str,
    relation: CorrespondenceRelation,
    morphism_class: MorphismClass,
    morphism_kind: MorphismKind,
    emitted_predicate: &str,
) -> Result<(), OverclaimError> {
    if !is_equivalence_predicate(emitted_predicate) {
        return Ok(());
    }
    if morphism_class == MorphismClass::BridgeView
        || morphism_kind == MorphismKind::CommitmentShiftingBridge
    {
        return Err(OverclaimError(format!(
            "Overclaim in lowering '{target}': a {} / {} (a by-reference bridge) emitted the \
             equivalence predicate '{emitted_predicate}'. A bridge view may never assert \
             equivalence (Constitution Principle 5 — bridge by reference, never a sameAs collapse).",
            morphism_class.as_str(),
            morphism_kind.as_str(),
        )));
    }
    if relation != CorrespondenceRelation::Equiv {
        return Err(OverclaimError(format!(
            "Overclaim in lowering '{target}': a caveated logic:{} correspondence emitted the \
             equivalence predicate '{emitted_predicate}'. Only a logic:Equiv correspondence may \
             emit an equivalence (exactMatch / equivalentClass); a {} is strictly weaker.",
            relation.as_str(),
            relation.as_str(),
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivalence_predicates_are_recognized() {
        for p in [
            "skos:exactMatch",
            "http://www.w3.org/2004/02/skos/core#exactMatch",
            "exactMatch",
            "owl:equivalentClass",
            "owl:equivalentProperty",
            "=",
            "Equiv",
        ] {
            assert!(is_equivalence_predicate(p), "{p} should be equivalence");
        }
        for p in [
            "skos:closeMatch",
            "skos:broadMatch",
            "<=",
            "rdfs:subClassOf",
        ] {
            assert!(
                !is_equivalence_predicate(p),
                "{p} should NOT be equivalence"
            );
        }
    }

    #[test]
    fn equiv_correspondence_may_emit_exact_match() {
        // The honest case: a genuine equivalence emits exactMatch — allowed.
        assert!(assert_relation_no_overclaim(
            "sssom",
            CorrespondenceRelation::Equiv,
            MorphismClass::Isomorphism,
            MorphismKind::InstitutionMorphism,
            "skos:exactMatch",
        )
        .is_ok());
        // A weaker relation emitting a weaker predicate is fine too.
        assert!(assert_relation_no_overclaim(
            "sssom",
            CorrespondenceRelation::Overlaps,
            MorphismClass::LossyLens,
            MorphismKind::InstitutionMorphism,
            "skos:closeMatch",
        )
        .is_ok());
    }

    #[test]
    fn bridge_view_emitting_equivalence_is_red() {
        // The issue's first RED witness: a bridge view emitting equivalence.
        let err = assert_relation_no_overclaim(
            "edoal",
            CorrespondenceRelation::Equiv,
            MorphismClass::BridgeView,
            MorphismKind::CommitmentShiftingBridge,
            "=",
        )
        .unwrap_err();
        assert!(err.0.contains("bridge"), "{}", err.0);
        assert!(err.0.contains("Principle 5"), "{}", err.0);
    }

    #[test]
    fn caveated_overlap_emitting_exact_match_is_red() {
        // The issue's second RED witness: a caveated overlap emitting sssom exactMatch.
        let err = assert_relation_no_overclaim(
            "sssom",
            CorrespondenceRelation::Overlaps,
            MorphismClass::LossyLens,
            MorphismKind::InstitutionMorphism,
            "skos:exactMatch",
        )
        .unwrap_err();
        assert!(err.0.contains("Overlaps"), "{}", err.0);
        assert!(err.0.contains("equivalence"), "{}", err.0);
    }
}
