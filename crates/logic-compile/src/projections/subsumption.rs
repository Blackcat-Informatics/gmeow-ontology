// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shape-component subsumption — the enforcement pre-order over closed-world validation shapes.
//!
//! A [`ValidationShapeIr`] is identified by its content-addressed `content_key`, but that key
//! folds in *presentation and provenance* (`iri`, `severity`, `message`, `cardinality_provenance`)
//! that never changes which focus nodes a validator flags. This module projects out that layer
//! to reason about ENFORCEMENT — the findings a shape produces over every graph:
//!
//! * [`enforcement_key`] is the deterministic canonical key over exactly the enforcement content
//!   (`target`, each property's `path`/`min_count`/`max_count`/`components`/`inverse`/
//!   `reifier_shape`/`reification_required`, the node-level components, and the `standpoint`),
//!   built from per-path sub-keys sorted so property supply order is irrelevant.
//! * [`equivalent`] is exact enforcement equivalence (`≡`): `enforcement_key(a) == enforcement_key(b)`.
//! * [`subsumes`] is a SOUND under-approximation of the enforcement pre-order (`strong ⊑ weak` =
//!   "strong flags at least everything weak flags"): exact-superset on component key sets and
//!   interval-containment on cardinality. It never returns `true` for a non-subsuming pair, but
//!   may return `false` for a semantically-subsuming pair whose components are equivalent yet
//!   syntactically distinct. By construction `equivalent(a, b)` implies `subsumes(a, b) &&
//!   subsumes(b, a)`.
//! * [`residue_normal_form`] reuses the exhaustive SHACL Core classifier ([`super::shapes::shacl_residue`])
//!   as the residue normal form, so callers never re-derive it.

use std::collections::BTreeSet;

use crate::ir::{PropertyConstraintIr, ValidationShapeIr};

/// Length-prefix a fragment so field boundaries can never collide when fragments are
/// concatenated — mirrors the IR's own `key_field` so the enforcement key is unambiguous.
fn key_field(s: &str) -> String {
    format!("{}:{s}", s.len())
}

/// Concatenate already-formatted fragments unambiguously — a count prefix plus every fragment
/// length-prefixed — so neither the element count nor any element boundary can be forged.
fn key_list(items: &[String]) -> String {
    let body: String = items.iter().map(|s| key_field(s)).collect();
    format!("{}[{body}]", items.len())
}

/// The deterministic canonical enforcement key of a shape — captures ONLY what determines
/// which focus nodes a validator flags, EXCLUDING the presentation/provenance that never
/// changes findings (`iri`, `severity`, `message`, `cardinality_provenance`). Built from
/// per-path property sub-keys sorted so property order is irrelevant. Two shapes with equal
/// enforcement keys flag exactly the same focus nodes over every graph.
pub fn enforcement_key(shape: &ValidationShapeIr) -> String {
    let mut props: Vec<String> = shape
        .properties
        .iter()
        .map(PropertyConstraintIr::enforcement_key)
        .collect();
    props.sort();
    let mut nodes: Vec<String> = shape
        .node_components
        .iter()
        .map(|c| c.enforcement_key())
        .collect();
    nodes.sort();
    format!(
        "target={}\u{1f}sp={}\u{1f}props={}\u{1f}nodes={}",
        key_field(&shape.target.enforcement_key()),
        key_field(shape.standpoint.as_deref().unwrap_or("")),
        key_list(&props),
        key_list(&nodes),
    )
}

/// Exact enforcement equivalence (`≡`): the two shapes flag exactly the same focus nodes over
/// every graph. Since the enforcement key projects out presentation/provenance, two shapes that
/// differ only in `iri` / `severity` / `message` / `cardinality_provenance` are equivalent.
pub fn equivalent(a: &ValidationShapeIr, b: &ValidationShapeIr) -> bool {
    enforcement_key(a) == enforcement_key(b)
}

/// The cardinality interval `[strong.min, strong.max]` is contained in `[weak.min, weak.max]` —
/// a tighter or equal interval. `min` defaults to `0`, `max` defaults to `∞` (`None`). Strong ⊆
/// weak iff `strong.min ≥ weak.min` AND `strong.max ≤ weak.max`.
fn cardinality_contained(strong: &PropertyConstraintIr, weak: &PropertyConstraintIr) -> bool {
    if strong.min_count.unwrap_or(0) < weak.min_count.unwrap_or(0) {
        return false;
    }
    match (strong.max_count, weak.max_count) {
        // weak is unbounded above — every strong bound is contained.
        (_, None) => true,
        // strong is unbounded above but weak is bounded — strong exceeds weak.
        (None, Some(_)) => false,
        (Some(s), Some(w)) => s <= w,
    }
}

/// Whether `strong` flags at least everything `weak` flags on the SAME property path (assumes
/// the two share a `path`): the strong component-key SET ⊇ weak's, the strong cardinality
/// interval ⊆ weak's, equal `inverse` direction (a forward and an inverse path constrain
/// different statements — never comparable), `reification_required` strengthened
/// (true ⊒ false), and a `reifier_shape` that matches or strengthens weak's (present ⊒ absent;
/// a DIFFERENT reifier IRI is not comparable).
fn property_subsumes(strong: &PropertyConstraintIr, weak: &PropertyConstraintIr) -> bool {
    // An inverse path and a forward path over the same predicate constrain different statements;
    // there is no strengthening between them, so the direction must match exactly.
    if strong.inverse != weak.inverse {
        return false;
    }
    let strong_comps: BTreeSet<String> = strong
        .components
        .iter()
        .map(|c| c.enforcement_key())
        .collect();
    let weak_comps: BTreeSet<String> = weak
        .components
        .iter()
        .map(|c| c.enforcement_key())
        .collect();
    if !weak_comps.is_subset(&strong_comps) {
        return false;
    }
    if !cardinality_contained(strong, weak) {
        return false;
    }
    // reification_required true is stronger than false: if weak demands a reifier, strong must.
    if weak.reification_required && !strong.reification_required {
        return false;
    }
    // reifier_shape present is stronger than absent; a different IRI is not comparable.
    match (&strong.reifier_shape, &weak.reifier_shape) {
        // weak imposes no reifier shape — strong may impose one (stronger) or none.
        (_, None) => true,
        // weak imposes one; strong must impose the SAME one. A missing or different strong
        // reifier shape does not subsume.
        (Some(s), Some(w)) => s == w,
        (None, Some(_)) => false,
    }
}

/// A SOUND (not necessarily complete) test of `strong ⊑ weak` — "strong enforces at least
/// everything weak does". Requires: the same `target`; the same `standpoint` (a standpoint-scoped
/// shape enforces only under its world, so a differing scope breaks soundness); the strong
/// node-level component-key SET ⊇ weak's; and, for EVERY property path in `weak`, SOME strong
/// property on that same path that [`property_subsumes`] weak's.
///
/// This is a sound under-approximation of the enforcement pre-order `⊑`: exact-superset on
/// component key sets and interval-containment on cardinality. It never returns `true` for a
/// non-subsuming pair, but may return `false` for a semantically-subsuming pair whose components
/// are equivalent yet syntactically distinct. By construction `equivalent(a, b)` implies
/// `subsumes(a, b) && subsumes(b, a)`.
pub fn subsumes(strong: &ValidationShapeIr, weak: &ValidationShapeIr) -> bool {
    if strong.target != weak.target {
        return false;
    }
    // A standpoint-scoped shape holds only under its world; a differing (or missing) scope means
    // strong does not enforce everything weak does over every world, so soundness requires equality.
    if strong.standpoint != weak.standpoint {
        return false;
    }
    let strong_nodes: BTreeSet<String> = strong
        .node_components
        .iter()
        .map(|c| c.enforcement_key())
        .collect();
    let weak_nodes: BTreeSet<String> = weak
        .node_components
        .iter()
        .map(|c| c.enforcement_key())
        .collect();
    if !weak_nodes.is_subset(&strong_nodes) {
        return false;
    }
    weak.properties.iter().all(|wp| {
        // A weak property that enforces NOTHING (no cardinality floor/ceiling, no components, no
        // reifier obligation) imposes no requirement, so strong trivially subsumes it — it needs
        // no counterpart. This arises when a legacy `sh:property` carries only an unsupported
        // construct (e.g. a nested `sh:node`) whose enforcement is recorded as residue, leaving an
        // empty property shell; that residue is grounded separately, never through subsumption.
        let enforces = wp.min_count.is_some()
            || wp.max_count.is_some()
            || !wp.components.is_empty()
            || wp.reifier_shape.is_some()
            || wp.reification_required;
        if !enforces {
            return true;
        }
        strong
            .properties
            .iter()
            .any(|sp| sp.path == wp.path && property_subsumes(sp, wp))
    })
}

/// The residue normal form of a shape: the exhaustive SHACL Core residue classifier
/// ([`super::shapes::shacl_residue`]) IS the normal form, so callers reuse it rather than
/// re-deriving which constructs a shape surface cannot faithfully hold.
pub fn residue_normal_form(shape: &ValidationShapeIr) -> Vec<String> {
    super::shapes::shacl_residue(shape)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        ConstraintComponent, ConstraintProvenance, PropertyConstraintIr, ShaclSeverity,
        ShapeTarget, ValidationShapeIr,
    };

    /// A one-property class-targeted shape over the given path/cardinality/components.
    fn shape(
        iri: &str,
        target_class: &str,
        path: &str,
        min: Option<u32>,
        max: Option<u32>,
        prov: Option<ConstraintProvenance>,
        components: Vec<ConstraintComponent>,
    ) -> ValidationShapeIr {
        ValidationShapeIr::new(
            iri,
            ShapeTarget::Class(target_class.to_owned()),
            vec![PropertyConstraintIr::new(path, min, max, prov, components).unwrap()],
            None,
        )
        .unwrap()
    }

    #[test]
    fn equivalent_is_reflexive_and_symmetric() {
        let s = shape(
            "https://ex/S",
            "https://ex/C",
            "https://ex/p",
            Some(1),
            Some(1),
            Some(ConstraintProvenance::OwlRestriction),
            vec![ConstraintComponent::Class("https://ex/D".into())],
        );
        assert!(equivalent(&s, &s), "equivalence must be reflexive");
        let t = shape(
            "https://ex/T",
            "https://ex/C",
            "https://ex/p",
            Some(1),
            Some(1),
            Some(ConstraintProvenance::OwlRestriction),
            vec![ConstraintComponent::Class("https://ex/D".into())],
        );
        assert_eq!(equivalent(&s, &t), equivalent(&t, &s), "must be symmetric");
        assert!(equivalent(&s, &t));
    }

    #[test]
    fn presentation_and_provenance_are_projected_out() {
        // Two shapes differing ONLY in iri / severity / message / cardinality_provenance are
        // enforcement-equivalent (presentation and ledger polarity never change findings).
        let base_prop = |prov| {
            PropertyConstraintIr::new(
                "https://ex/p",
                Some(1),
                Some(1),
                Some(prov),
                vec![ConstraintComponent::Class("https://ex/D".into())],
            )
            .unwrap()
        };
        let a = ValidationShapeIr::new(
            "https://ex/A",
            ShapeTarget::Class("https://ex/C".into()),
            vec![base_prop(ConstraintProvenance::OwlRestriction)],
            None,
        )
        .unwrap();
        let b = ValidationShapeIr::new(
            "https://ex/B-different-iri",
            ShapeTarget::Class("https://ex/C".into()),
            vec![
                base_prop(ConstraintProvenance::OptNative)
                    .with_severity(ShaclSeverity::Warning)
                    .with_message("distinct message")
                    .unwrap(),
            ],
            None,
        )
        .unwrap();
        assert_ne!(
            a.content_key(),
            b.content_key(),
            "the IDENTITY keys must differ (presentation/provenance/iri all differ)"
        );
        assert!(
            equivalent(&a, &b),
            "enforcement must ignore iri/severity/message/provenance"
        );
        // Equivalence must imply mutual subsumption.
        assert!(subsumes(&a, &b) && subsumes(&b, &a));
    }

    #[test]
    fn equivalent_false_when_component_or_cardinality_differs() {
        let a = shape(
            "https://ex/A",
            "https://ex/C",
            "https://ex/p",
            Some(1),
            Some(1),
            Some(ConstraintProvenance::OwlRestriction),
            vec![ConstraintComponent::Class("https://ex/D".into())],
        );
        // Different component value.
        let b = shape(
            "https://ex/A",
            "https://ex/C",
            "https://ex/p",
            Some(1),
            Some(1),
            Some(ConstraintProvenance::OwlRestriction),
            vec![ConstraintComponent::Class("https://ex/Other".into())],
        );
        assert!(!equivalent(&a, &b), "differing component ⇒ not equivalent");
        // Different cardinality.
        let c = shape(
            "https://ex/A",
            "https://ex/C",
            "https://ex/p",
            Some(0),
            None,
            Some(ConstraintProvenance::OwlRestriction),
            vec![ConstraintComponent::Class("https://ex/D".into())],
        );
        assert!(
            !equivalent(&a, &c),
            "differing cardinality ⇒ not equivalent"
        );
    }

    #[test]
    fn extra_component_strictly_subsumes() {
        // `strong` carries an EXTRA component the `weak` shape lacks — same path/cardinality.
        let weak = shape(
            "https://ex/W",
            "https://ex/C",
            "https://ex/p",
            Some(1),
            Some(1),
            Some(ConstraintProvenance::OwlRestriction),
            vec![ConstraintComponent::Class("https://ex/D".into())],
        );
        let strong = shape(
            "https://ex/S",
            "https://ex/C",
            "https://ex/p",
            Some(1),
            Some(1),
            Some(ConstraintProvenance::OwlRestriction),
            vec![
                ConstraintComponent::Class("https://ex/D".into()),
                ConstraintComponent::MinLength(3),
            ],
        );
        assert!(subsumes(&strong, &weak), "extra component ⇒ strong ⊑ weak");
        assert!(
            !subsumes(&weak, &strong),
            "the weaker shape does NOT subsume the stronger (strict)"
        );
    }

    #[test]
    fn cardinality_interval_containment() {
        // min 1..=1 ⊆ 0..=unbounded on the same path+components.
        let weak = shape(
            "https://ex/W",
            "https://ex/C",
            "https://ex/p",
            Some(0),
            None,
            Some(ConstraintProvenance::OwlRestriction),
            vec![ConstraintComponent::Class("https://ex/D".into())],
        );
        let strong = shape(
            "https://ex/S",
            "https://ex/C",
            "https://ex/p",
            Some(1),
            Some(1),
            Some(ConstraintProvenance::OwlRestriction),
            vec![ConstraintComponent::Class("https://ex/D".into())],
        );
        assert!(subsumes(&strong, &weak), "[1,1] ⊆ [0,∞] ⇒ strong ⊑ weak");
        assert!(!subsumes(&weak, &strong), "[0,∞] ⊄ [1,1] ⇒ not the reverse");
    }

    #[test]
    fn equivalence_implies_mutual_subsumption() {
        let a = shape(
            "https://ex/A",
            "https://ex/C",
            "https://ex/p",
            Some(2),
            Some(5),
            Some(ConstraintProvenance::OptNative),
            vec![
                ConstraintComponent::Class("https://ex/D".into()),
                ConstraintComponent::MinLength(1),
            ],
        );
        // Same enforcement, components supplied in a different order + different iri.
        let b = shape(
            "https://ex/B",
            "https://ex/C",
            "https://ex/p",
            Some(2),
            Some(5),
            Some(ConstraintProvenance::OptNative),
            vec![
                ConstraintComponent::MinLength(1),
                ConstraintComponent::Class("https://ex/D".into()),
            ],
        );
        assert!(equivalent(&a, &b));
        assert!(
            subsumes(&a, &b) && subsumes(&b, &a),
            "equivalent ⇒ mutual subsumption"
        );
    }

    #[test]
    fn reifier_strengthening_and_incomparable_shapes() {
        let prop = |reifier: Option<String>, required: bool| {
            let p = PropertyConstraintIr::new(
                "https://ex/p",
                Some(1),
                Some(1),
                Some(ConstraintProvenance::OwlRestriction),
                vec![ConstraintComponent::Class("https://ex/D".into())],
            )
            .unwrap();
            if reifier.is_some() || required {
                p.with_reifier(reifier, required).unwrap()
            } else {
                p
            }
        };
        let mk = |iri: &str, reifier: Option<String>, required: bool| {
            ValidationShapeIr::new(
                iri,
                ShapeTarget::Class("https://ex/C".into()),
                vec![prop(reifier, required)],
                None,
            )
            .unwrap()
        };
        // reification_required=true subsumes false.
        let req = mk("https://ex/Req", None, true);
        let plain = mk("https://ex/Plain", None, false);
        assert!(subsumes(&req, &plain), "reifreq true ⊒ false");
        assert!(!subsumes(&plain, &req), "false ⋢ true");
        // Two DIFFERENT reifier_shape IRIs are not comparable in either direction.
        let ra = mk("https://ex/Ra", Some("https://ex/ShapeA".into()), false);
        let rb = mk("https://ex/Rb", Some("https://ex/ShapeB".into()), false);
        assert!(
            !subsumes(&ra, &rb) && !subsumes(&rb, &ra),
            "distinct reifier shapes are incomparable"
        );
        // A present reifier shape subsumes an absent one.
        assert!(subsumes(&ra, &plain), "reifier_shape present ⊒ absent");
        assert!(!subsumes(&plain, &ra), "absent reifier_shape ⋢ present");
    }

    #[test]
    fn residue_normal_form_equals_shacl_residue_for_a_lossy_shape() {
        let s = shape(
            "https://ex/Lossy",
            "https://ex/C",
            "https://ex/p",
            None,
            None,
            None,
            vec![ConstraintComponent::Pattern {
                regex: "^[A-Z]+$".into(),
                flags: None,
            }],
        );
        let normal = residue_normal_form(&s);
        assert_eq!(normal, super::super::shapes::shacl_residue(&s));
        assert!(
            !normal.is_empty(),
            "a Pattern component must produce regex-dialect residue"
        );
    }
}
