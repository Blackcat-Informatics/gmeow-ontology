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

#[path = "subsumption.tests.rs"]
#[cfg(test)]
mod tests;
