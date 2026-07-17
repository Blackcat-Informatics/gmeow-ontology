// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The **lift** — the Galois adjoint of [`crate::frontend::derive_validation_shapes`].
//!
//! `derive_validation_shapes` is the FORWARD derive: it reads the OWL/RDFS constraint axioms of
//! the merged authored ontology and lowers each to a [`ValidationShapeIr`] (its closed-world
//! `logic:ValidationOnly` reading). [`lift`] inverts it: given a shape, it emits the OWL/RDFS +
//! `logic:ClosureEntry` **Turtle axiom text** that — when fed back through the *real*
//! `derive_validation_shapes` — reproduces the shape's enforcement.
//!
//! The lift is an **emit-only human-review PROPOSAL**: the returned Turtle is NEVER auto-committed
//! into `slices/**` (Principle 4 — the authoring surface is emit-only; a human reviews the proposal
//! and, if correct, authors it). [`lift`] performs no filesystem write of any kind; it returns an
//! owned [`String`] plus a residue list.
//!
//! Any component with **no faithful OWL/RDFS antecedent** — a lossy component
//! ([`ConstraintComponent::is_lossy`]: `Pattern`, `TerminologyBinding`, `OrdinalSet`,
//! `DateTimePattern`), or a SHACL-Core-faithful component that OWL simply cannot state
//! (`NumericRange`, `DateTimeRange`, `LanguageIn`, a literal-bearing `In`, a datatype/lang-tagged
//! `HasValue`, a non-single-`Class` qualified value shape, a reifier obligation, …) — is **recorded
//! as residue**, never emitted as a weaker lossy axiom. The residue reuses the exhaustive SHACL-Core
//! classifier ([`super::shapes::shacl_residue`] via [`super::subsumption::residue_normal_form`]) for
//! its lossy fragment.
//!
//! [`certify`] closes the loop against the REAL forward derive (never a reimplementation): it lifts,
//! re-parses the proposal with `purrdf`, runs `derive_validation_shapes`, and asserts the derived
//! shape (a) SOUNDLY enforces at least the shape-expressible core (`⊑`, deletion never loses
//! enforcement) and (b) is enforcement-EQUIVALENT to that core (`≡`). That makes
//! "equivalence-before-deletion" a machine-checkable idempotence certificate.

use gmeow_errors::Diag;
use std::collections::BTreeSet;

use crate::frontend::derive_validation_shapes;
use crate::ir::{
    ConstraintComponent, PropertyConstraintIr, ShaclNodeKind, ShapeTarget, ShapeValue,
    ValidationShapeIr,
};

use super::subsumption::{equivalent, residue_normal_form, subsumes};

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
const RDFS_LITERAL: &str = "http://www.w3.org/2000/01/rdf-schema#Literal";

/// The proposal a [`lift`] emits: the OWL/RDFS + `logic:ClosureEntry` axiom Turtle that re-derives
/// the shape, plus the per-component residue for everything with no faithful axiom antecedent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiftProposal {
    /// The emit-only OWL/RDFS + closure axiom text (a human-review proposal, never written to
    /// `slices/**`). Byte-stable: the statements are sorted + deduped, blank nodes are anonymous.
    pub axioms_ttl: String,
    /// The residue: components with no faithful OWL/RDFS antecedent, carried for review rather than
    /// emitted as a lossy axiom. Deterministic (sorted, deduped).
    pub residue: Vec<String>,
}

/// The Turtle prefix header prepended to every proposal (constant — determinism-safe).
const HEADER: &str = "\
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
";

/// The internal single-pass analysis both [`lift`] and [`certify`] share, so the emitted axioms,
/// the residue, and the shape-expressible core can never diverge (one source of truth for which
/// component is invertible).
struct Analysis {
    axioms_ttl: String,
    residue: Vec<String>,
    /// The shape restricted to its OWL/RDFS-expressible core (every residue component removed,
    /// `standpoint` cleared — the forward derive is standpoint-blind). `certify` asserts the derived
    /// shape is enforcement-equivalent to this.
    core: ValidationShapeIr,
}

/// A Turtle string-literal escape (`\`, `"`, and the C0 whitespace).
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// `<iri>`.
fn ang(iri: &str) -> String {
    format!("<{iri}>")
}

/// A non-negative-integer literal in the exact lexical form the forward `card_of` / facet readers
/// parse (`"n"^^xsd:nonNegativeInteger`).
fn nn_int(n: u32) -> String {
    format!("\"{n}\"^^xsd:nonNegativeInteger")
}

/// The OWL filler IRI a `classify()`-style value component inverts to, or `None` when the component
/// has no `owl:allValuesFrom` / `rdfs:domain` / `rdfs:range` antecedent. Mirrors the forward
/// `classify`: `Class` → the class IRI; `Datatype` → the datatype IRI; `sh:BlankNodeOrIRI` →
/// `owl:Thing`; `sh:Literal` → `rdfs:Literal`. (`rdfs:Resource` is never produced — the forward
/// `classify` maps it to `None`, i.e. NO component — so it never needs inverting.)
fn classify_filler(c: &ConstraintComponent) -> Option<String> {
    match c {
        ConstraintComponent::Class(cls) => Some(cls.clone()),
        ConstraintComponent::Datatype(dt) => Some(dt.clone()),
        ConstraintComponent::NodeKindShacl(ShaclNodeKind::BlankNodeOrIri) => {
            Some(OWL_THING.to_owned())
        }
        ConstraintComponent::NodeKindShacl(ShaclNodeKind::Literal) => Some(RDFS_LITERAL.to_owned()),
        _ => None,
    }
}

/// Declare a non-XSD, non-`rdfs:Literal` datatype filler `a rdfs:Datatype` so the forward
/// `is_datatype` recognises it (XSD-namespaced and `rdfs:Literal` fillers are recognised without a
/// declaration). A no-op for XSD / `rdfs:Literal`.
fn declare_datatype_if_needed(dt: &str, stmts: &mut BTreeSet<String>) {
    if !dt.starts_with(XSD) && dt != RDFS_LITERAL {
        stmts.insert(format!("{} a rdfs:Datatype .", ang(dt)));
    }
}

/// Whether a qualified value shape is the single-`Class` (non-`owl:Thing`) form the forward derive
/// produces — the only qualified shape with a faithful `owl:onClass` + qualified-cardinality
/// antecedent. Returns the qualifying class IRI when it is.
fn qvs_single_class(shape: &[ConstraintComponent]) -> Option<&str> {
    match shape {
        [ConstraintComponent::Class(q)] if q != OWL_THING => Some(q),
        _ => None,
    }
}

/// Invert ONE value component of a `Class(K)`-targeted property shape (path `P`) into its OWL
/// restriction axiom(s), or record it as residue. Returns `true` iff the component is invertible
/// (and therefore kept in the core). Facet components (`MinLength`/`MaxLength`/`Pattern`) are handled
/// at the property level ([`invert_class_property`]), NOT here.
fn invert_class_component(
    k: &str,
    p: &str,
    c: &ConstraintComponent,
    stmts: &mut BTreeSet<String>,
    residue: &mut BTreeSet<String>,
) -> bool {
    // `owl:allValuesFrom` filler (class / datatype / node-kind top).
    if let Some(filler) = classify_filler(c) {
        if let ConstraintComponent::Datatype(dt) = c {
            declare_datatype_if_needed(dt, stmts);
        }
        stmts.insert(format!(
            "{} rdfs:subClassOf [ a owl:Restriction ; owl:onProperty {} ; owl:allValuesFrom {} ] .",
            ang(k),
            ang(p),
            ang(&filler),
        ));
        return true;
    }
    match c {
        ConstraintComponent::HasValue(ShapeValue::Iri(v)) => {
            stmts.insert(format!(
                "{} rdfs:subClassOf [ a owl:Restriction ; owl:onProperty {} ; owl:hasValue {} ] .",
                ang(k),
                ang(p),
                ang(v),
            ));
            true
        }
        // The forward `owl:hasValue` reader stores a literal value as a bare lexical (datatype /
        // language are dropped by the graph view), so only a plain literal round-trips; a
        // datatype/lang-tagged `HasValue` has no faithful antecedent → residue.
        ConstraintComponent::HasValue(ShapeValue::Literal {
            lexical,
            datatype: None,
            lang: None,
        }) => {
            stmts.insert(format!(
                "{} rdfs:subClassOf [ a owl:Restriction ; owl:onProperty {} ; owl:hasValue \"{}\" ] .",
                ang(k),
                ang(p),
                esc(lexical),
            ));
            true
        }
        // Qualified value shape → `owl:onClass` + qualified cardinality (single-`Class` form only).
        ConstraintComponent::QualifiedValueShape { shape, min, max } => {
            match qvs_single_class(shape) {
                Some(q) => {
                    let mut facets = format!("owl:onClass {}", ang(q));
                    match (min, max) {
                        (Some(lo), Some(hi)) if lo == hi => {
                            facets
                                .push_str(&format!(" ; owl:qualifiedCardinality {}", nn_int(*lo)));
                        }
                        (lo, hi) => {
                            if let Some(lo) = lo {
                                facets.push_str(&format!(
                                    " ; owl:minQualifiedCardinality {}",
                                    nn_int(*lo)
                                ));
                            }
                            if let Some(hi) = hi {
                                facets.push_str(&format!(
                                    " ; owl:maxQualifiedCardinality {}",
                                    nn_int(*hi)
                                ));
                            }
                        }
                    }
                    stmts.insert(format!(
                        "{} rdfs:subClassOf [ a owl:Restriction ; owl:onProperty {} ; {} ] .",
                        ang(k),
                        ang(p),
                        facets,
                    ));
                    true
                }
                None => {
                    residue.insert(format!(
                        "qualified value shape on {p} is not a single owl:onClass form; its inner \
                         shape has no faithful OWL restriction antecedent (carried in the canonical \
                         logic: layer)"
                    ));
                    false
                }
            }
        }
        // No OWL antecedent (or lossy — the lossy message is added by `shacl_residue`).
        other => {
            if !other.is_lossy() {
                residue.insert(format!(
                    "component {} on {p} is SHACL-expressible but has no faithful OWL/RDFS axiom \
                     antecedent (carried in the canonical logic: layer)",
                    component_label(other)
                ));
            }
            false
        }
    }
}

/// A short human label for a component in a residue message (no full content key needed).
fn component_label(c: &ConstraintComponent) -> &'static str {
    match c {
        ConstraintComponent::NumericRange { .. } => "sh:minInclusive/maxInclusive numeric range",
        ConstraintComponent::PrecisionRange { .. } => "precision range",
        ConstraintComponent::Datatype(_) => "sh:datatype",
        ConstraintComponent::Class(_) => "sh:class",
        ConstraintComponent::NodeKindShacl(_) => "sh:nodeKind",
        ConstraintComponent::In(_) => "sh:in",
        ConstraintComponent::Pattern { .. } => "sh:pattern",
        ConstraintComponent::MinLength(_) => "sh:minLength",
        ConstraintComponent::MaxLength(_) => "sh:maxLength",
        ConstraintComponent::LanguageIn(_) => "sh:languageIn",
        ConstraintComponent::DateTimeRange { .. } => "sh:minInclusive/maxInclusive datetime range",
        ConstraintComponent::TerminologyBinding { .. } => "terminology binding",
        ConstraintComponent::OrdinalSet { .. } => "ordinal set",
        ConstraintComponent::DateTimePattern(_) => "datetime validity pattern",
        ConstraintComponent::HasValue(_) => "sh:hasValue",
        ConstraintComponent::QualifiedValueShape { .. } => "sh:qualifiedValueShape",
        ConstraintComponent::Not(_) => "sh:not",
        ConstraintComponent::Or(_) => "sh:or",
        ConstraintComponent::Xone(_) => "sh:xone",
        ConstraintComponent::OrProperties(_) => "sh:or over sh:path branches",
        ConstraintComponent::UniqueLang => "sh:uniqueLang",
    }
}

/// The `owl:withRestrictions` facet element for a length facet, or `None` (a `Pattern` facet has no
/// faithful antecedent and is dropped to residue by the caller).
fn length_facet(c: &ConstraintComponent) -> Option<String> {
    match c {
        ConstraintComponent::MinLength(n) => Some(format!("[ xsd:minLength {} ]", nn_int(*n))),
        ConstraintComponent::MaxLength(n) => Some(format!("[ xsd:maxLength {} ]", nn_int(*n))),
        _ => None,
    }
}

/// Invert one property shape of a `Class(K)` target: its cardinality (unqualified `owl:cardinality`
/// restriction) and each value component. Faceted-datatype property shapes (a `Datatype` base plus
/// `MinLength`/`MaxLength`/`Pattern` facets) lower to an `owl:onDatatype` + `owl:withRestrictions`
/// filler. Returns the kept (invertible) components for the core; the cardinality is kept whenever
/// present. A reifier / inverse property shape in a class target has no restriction antecedent → its
/// whole property is residue.
fn invert_class_property(
    k: &str,
    pc: &PropertyConstraintIr,
    stmts: &mut BTreeSet<String>,
    residue: &mut BTreeSet<String>,
) -> Option<PropertyConstraintIr> {
    // A reifier obligation or an inverse path has no `owl:Restriction` antecedent on a class target.
    if pc.reifier_shape.is_some() || pc.reification_required {
        residue.insert(format!(
            "reifier obligation on {} (sh:reifierShape / sh:reificationRequired) has no ordinary-OWL \
             antecedent; it is the statement-layer condition carried in the canonical logic: layer",
            pc.path
        ));
        return None;
    }
    if pc.inverse {
        residue.insert(format!(
            "inverse path on {} in a class target has no owl:Restriction antecedent (carried in the \
             canonical logic: layer)",
            pc.path
        ));
        return None;
    }

    let mut kept: Vec<ConstraintComponent> = Vec::new();

    // Unqualified cardinality → `owl:cardinality` / `owl:minCardinality` / `owl:maxCardinality`.
    if pc.min_count.is_some() || pc.max_count.is_some() {
        let facets = match (pc.min_count, pc.max_count) {
            (Some(lo), Some(hi)) if lo == hi => format!("owl:cardinality {}", nn_int(lo)),
            (lo, hi) => {
                let mut parts: Vec<String> = Vec::new();
                if let Some(lo) = lo {
                    parts.push(format!("owl:minCardinality {}", nn_int(lo)));
                }
                if let Some(hi) = hi {
                    parts.push(format!("owl:maxCardinality {}", nn_int(hi)));
                }
                parts.join(" ; ")
            }
        };
        stmts.insert(format!(
            "{} rdfs:subClassOf [ a owl:Restriction ; owl:onProperty {} ; {} ] .",
            ang(k),
            ang(&pc.path),
            facets,
        ));
        // Cardinality is kept in the core verbatim (enforcement ignores provenance).
    }

    // Faceted-datatype property shape: a `Datatype` base with ≥1 length/pattern facet.
    let has_facet = pc.components.iter().any(|c| {
        matches!(
            c,
            ConstraintComponent::MinLength(_)
                | ConstraintComponent::MaxLength(_)
                | ConstraintComponent::Pattern { .. }
        )
    });
    if has_facet {
        let base = pc.components.iter().find_map(|c| match c {
            ConstraintComponent::Datatype(dt) => Some(dt.clone()),
            _ => None,
        });
        match base {
            Some(dt) => {
                // Length facets → `owl:withRestrictions`; a `Pattern` facet is lossy → residue
                // (recorded by `shacl_residue`); any non-facet component is inverted individually.
                let mut facet_elems: Vec<String> = Vec::new();
                for c in &pc.components {
                    match c {
                        ConstraintComponent::Datatype(_) => {
                            kept.push(c.clone());
                        }
                        ConstraintComponent::MinLength(_) | ConstraintComponent::MaxLength(_) => {
                            if let Some(f) = length_facet(c) {
                                facet_elems.push(f);
                                kept.push(c.clone());
                            }
                        }
                        ConstraintComponent::Pattern { .. } => {
                            // Lossy: carried by `shacl_residue`, never emitted as `sh:pattern`.
                        }
                        other => {
                            if invert_class_component(k, &pc.path, other, stmts, residue) {
                                kept.push(other.clone());
                            }
                        }
                    }
                }
                declare_datatype_if_needed(&dt, stmts);
                facet_elems.sort();
                let restrictions = facet_elems.join(" ");
                stmts.insert(format!(
                    "{} rdfs:subClassOf [ a owl:Restriction ; owl:onProperty {} ; owl:allValuesFrom \
                     [ a rdfs:Datatype ; owl:onDatatype {} ; owl:withRestrictions ( {} ) ] ] .",
                    ang(k),
                    ang(&pc.path),
                    ang(&dt),
                    restrictions,
                ));
            }
            None => {
                // No datatype base for the length/pattern facets → no faceted-datatype antecedent.
                for c in &pc.components {
                    if invert_class_component(k, &pc.path, c, stmts, residue) {
                        kept.push(c.clone());
                    }
                }
            }
        }
    } else {
        for c in &pc.components {
            if invert_class_component(k, &pc.path, c, stmts, residue) {
                kept.push(c.clone());
            }
        }
    }

    // Keep the property in the core iff it has any invertible cardinality or component.
    if pc.min_count.is_none() && pc.max_count.is_none() && kept.is_empty() {
        return None;
    }
    Some(
        PropertyConstraintIr::new(
            &pc.path,
            pc.min_count,
            pc.max_count,
            pc.cardinality_provenance,
            kept,
        )
        .expect("core property reconstruction from a validated shape cannot fail"),
    )
}

/// Invert a `Class(K)` node-level component (`owl:disjointWith` / `owl:oneOf`), or record residue.
/// Returns the kept component for the core.
fn invert_class_node_component(
    k: &str,
    c: &ConstraintComponent,
    stmts: &mut BTreeSet<String>,
    residue: &mut BTreeSet<String>,
) -> Option<ConstraintComponent> {
    match c {
        // `owl:disjointWith D` (also the reading of `owl:complementOf` / `owl:AllDisjointClasses`).
        ConstraintComponent::Not(inner) => match inner.as_ref() {
            ConstraintComponent::Class(d) => {
                stmts.insert(format!("{} owl:disjointWith {} .", ang(k), ang(d)));
                Some(c.clone())
            }
            _ => {
                residue.insert(format!(
                    "sh:not on {k} negates a non-class shape; only owl:disjointWith (sh:not [ \
                     sh:class D ]) has a faithful antecedent (carried in the canonical logic: layer)"
                ));
                None
            }
        },
        // `owl:oneOf ( … )` — the forward `read_iri_list` reads only IRI members, so a literal-bearing
        // value set has no faithful antecedent.
        ConstraintComponent::In(vals) => {
            let iris: Option<Vec<&str>> = vals
                .iter()
                .map(|v| match v {
                    ShapeValue::Iri(i) => Some(i.as_str()),
                    ShapeValue::Literal { .. } => None,
                })
                .collect();
            match iris {
                Some(iris) if !iris.is_empty() => {
                    let members = iris.iter().map(|i| ang(i)).collect::<Vec<_>>().join(" ");
                    stmts.insert(format!("{} owl:oneOf ( {} ) .", ang(k), members));
                    Some(c.clone())
                }
                _ => {
                    residue.insert(format!(
                        "sh:in on {k} carries literal members; owl:oneOf can only enumerate IRIs, so \
                         the literal value set is carried in the canonical logic: layer"
                    ));
                    None
                }
            }
        }
        other => {
            if !other.is_lossy() {
                residue.insert(format!(
                    "node-level component {} on {k} has no faithful OWL/RDFS axiom antecedent \
                     (carried in the canonical logic: layer)",
                    component_label(other)
                ));
            }
            None
        }
    }
}

/// Emit the `logic:ClosedWorldClosure` opt-in closure entry for property `p` (idempotent — the
/// statement set dedups). This is the single authored signal the forward derive reads to take the
/// closed-world validation reading of `p`'s open-world `rdfs:domain`/`rdfs:range` axioms.
fn emit_closed_optin(p: &str, stmts: &mut BTreeSet<String>) {
    stmts.insert(format!(
        "[ logic:closureKey \"{}\" ; logic:closureValue logic:ClosedWorldClosure ] .",
        esc(p),
    ));
}

/// Invert a `SubjectsOf(P)` / `ObjectsOf(P)` domain/range node component into `rdfs:domain` /
/// `rdfs:range` plus the `ClosedWorldClosure` opt-in, or record residue. `is_range` selects the
/// axiom. Returns the kept component for the core.
fn invert_domain_range_component(
    p: &str,
    c: &ConstraintComponent,
    is_range: bool,
    stmts: &mut BTreeSet<String>,
    residue: &mut BTreeSet<String>,
) -> Option<ConstraintComponent> {
    match classify_filler(c) {
        Some(filler) => {
            if let ConstraintComponent::Datatype(dt) = c {
                declare_datatype_if_needed(dt, stmts);
            }
            let pred = if is_range {
                "rdfs:range"
            } else {
                "rdfs:domain"
            };
            stmts.insert(format!("{} {} {} .", ang(p), pred, ang(&filler)));
            emit_closed_optin(p, stmts);
            Some(c.clone())
        }
        None => {
            if !c.is_lossy() {
                let axis = if is_range { "range" } else { "domain" };
                residue.insert(format!(
                    "node component {} on the {axis} of {p} is not a class/datatype/node-kind \
                     condition; it has no rdfs:{axis} antecedent (carried in the canonical logic: \
                     layer)",
                    component_label(c)
                ));
            }
            None
        }
    }
}

/// Whether a property shape is the forward `owl:FunctionalProperty` reading: `sh:maxCount 1`, no
/// `minCount`, no components, forward path.
fn is_functional_pc(pc: &PropertyConstraintIr) -> bool {
    pc.min_count.is_none()
        && pc.max_count == Some(1)
        && pc.components.is_empty()
        && !pc.inverse
        && pc.reifier_shape.is_none()
        && !pc.reification_required
}

/// Whether a property shape is the forward `owl:InverseFunctionalProperty` / single-property
/// `owl:hasKey` reading: inverse `sh:maxCount 1`, no `minCount`, no components.
fn is_inverse_functional_pc(pc: &PropertyConstraintIr) -> bool {
    pc.min_count.is_none()
        && pc.max_count == Some(1)
        && pc.components.is_empty()
        && pc.inverse
        && pc.reifier_shape.is_none()
        && !pc.reification_required
}

/// The single-pass analysis: emit the axioms, collect the residue, and build the shape-expressible
/// core. One source of truth so [`lift`] and [`certify`] can never disagree on invertibility.
fn analyze(shape: &ValidationShapeIr) -> Analysis {
    let mut stmts: BTreeSet<String> = BTreeSet::new();
    let mut residue: BTreeSet<String> = BTreeSet::new();
    let mut core_props: Vec<PropertyConstraintIr> = Vec::new();
    let mut core_nodes: Vec<ConstraintComponent> = Vec::new();

    // Seed the residue with the exhaustive SHACL-Core lossy classifier (Pattern / TerminologyBinding
    // / OrdinalSet / DateTimePattern / standpoint) so lossy components are flagged once, not twice.
    residue.extend(residue_normal_form(shape));

    match &shape.target {
        ShapeTarget::Class(k) => {
            // A class target requires the class declared `owl:Class` for the forward FAMILY-1/2 walk.
            stmts.insert(format!("{} a owl:Class .", ang(k)));
            for pc in &shape.properties {
                if let Some(kept) = invert_class_property(k, pc, &mut stmts, &mut residue) {
                    core_props.push(kept);
                }
            }
            for c in &shape.node_components {
                if let Some(kept) = invert_class_node_component(k, c, &mut stmts, &mut residue) {
                    core_nodes.push(kept);
                }
            }
        }
        ShapeTarget::SubjectsOf(p) | ShapeTarget::ObjectsOf(p) => {
            let is_range = matches!(&shape.target, ShapeTarget::ObjectsOf(_));
            for c in &shape.node_components {
                if let Some(kept) =
                    invert_domain_range_component(p, c, is_range, &mut stmts, &mut residue)
                {
                    core_nodes.push(kept);
                }
            }
            for pc in &shape.properties {
                if !is_range && is_functional_pc(pc) && pc.path == *p {
                    stmts.insert(format!("{} a owl:FunctionalProperty .", ang(p)));
                    core_props.push(pc.clone());
                } else if is_range && is_inverse_functional_pc(pc) && pc.path == *p {
                    stmts.insert(format!("{} a owl:InverseFunctionalProperty .", ang(p)));
                    core_props.push(pc.clone());
                } else {
                    residue.insert(format!(
                        "property shape on {} under a {} target has no owl:Functional / \
                         InverseFunctionalProperty antecedent (carried in the canonical logic: layer)",
                        pc.path,
                        if is_range { "range" } else { "domain" }
                    ));
                }
            }
        }
        ShapeTarget::ValueKeyed { predicate, value } => {
            // The forward derive never produces a value-keyed (SPARQL) target; it has no OWL/RDFS
            // antecedent at all, so the entire shape is residue.
            residue.insert(format!(
                "value-keyed target ({predicate} = {value}) is an sh:SPARQLTarget with no OWL/RDFS \
                 antecedent; the whole shape is carried in the canonical logic: layer"
            ));
        }
        ShapeTarget::DirectClass(k) => {
            // A direct-class target is a subclass-excluding sh:SPARQLTarget; the plain
            // `owl:Class` antecedent a bare class target inverts to cannot capture the
            // `FILTER NOT EXISTS` subclass refinement, so the whole shape is residue.
            residue.insert(format!(
                "direct-class target ({k}) is a subclass-excluding sh:SPARQLTarget with no OWL/RDFS \
                 antecedent; the whole shape is carried in the canonical logic: layer"
            ));
        }
        ShapeTarget::Sparql(select) => {
            // A raw SPARQL-select target has no class / domain / range form, so it has no OWL/RDFS
            // antecedent at all; the entire shape is residue.
            residue.insert(format!(
                "raw sh:SPARQLTarget ({select}) has no OWL/RDFS antecedent; the whole shape is \
                 carried in the canonical logic: layer"
            ));
        }
    }

    // The forward derive is standpoint-blind, so the core carries no standpoint (the scope is
    // recorded in the residue by `shacl_residue`).
    let core = ValidationShapeIr::new(shape.iri.clone(), shape.target.clone(), core_props, None)
        .expect("core shape reconstruction from a validated shape cannot fail")
        .with_node_components(core_nodes)
        .expect("core node-component reconstruction from a validated shape cannot fail");

    let axioms_ttl = {
        let body: Vec<String> = stmts.into_iter().collect();
        if body.is_empty() {
            HEADER.to_owned()
        } else {
            format!("{HEADER}{}\n", body.join("\n"))
        }
    };

    Analysis {
        axioms_ttl,
        residue: residue.into_iter().collect(),
        core,
    }
}

/// Lift a validation shape to the OWL/RDFS + `logic:ClosureEntry` axiom-text PROPOSAL that re-derives
/// it, plus the residue for every component with no faithful axiom antecedent. **Emit-only**: the
/// returned Turtle is a human-review proposal, NEVER auto-committed into `slices/**` (Principle 4).
/// Performs no filesystem access; deterministic (the statements are sorted + deduped).
pub fn lift(shape: &ValidationShapeIr) -> LiftProposal {
    let a = analyze(shape);
    LiftProposal {
        axioms_ttl: a.axioms_ttl,
        residue: a.residue,
    }
}

/// The equivalence-before-deletion **certificate**, checked against the REAL forward derive (never a
/// reimplementation): lift `shape`, re-parse the proposal with `purrdf`, run
/// [`derive_validation_shapes`], and assert the derived shape for the same target both
/// SOUNDLY-enforces (`⊑`) and is ENFORCEMENT-EQUIVALENT (`≡`) to `shape`'s OWL/RDFS-expressible core.
///
/// `Ok(())` witnesses `derive_validation_shapes(lift(shape)) ≡ core(shape)` — a machine-checkable
/// idempotence certificate that deleting the shape and re-deriving it from the proposal loses no
/// enforcement over the shape-expressible fragment. Residue-only components are excluded from the
/// core (they have no axiom antecedent and are carried in the canonical logic: layer).
pub fn certify(shape: &ValidationShapeIr) -> gmeow_errors::Result<()> {
    let a = analyze(shape);
    let ds = purrdf::parse_dataset(a.axioms_ttl.as_bytes(), "text/turtle", None).map_err(|e| {
        Diag::of_kind(crate::error::Frontend {
            detail: format!("lift certify: re-parsing the lifted proposal failed: {e}"),
        })
    })?;
    let derived_all = derive_validation_shapes(ds.as_ref()).map_err(|e| {
        Diag::of_kind(crate::error::Frontend {
            detail: format!("lift certify: derive_validation_shapes over the proposal failed: {e}"),
        })
    })?;
    let derived = derived_all.iter().find(|d| d.target == a.core.target);

    let core_is_empty = a.core.properties.is_empty() && a.core.node_components.is_empty();

    match derived {
        None => {
            if core_is_empty {
                // The whole shape is residue: there is no OWL/RDFS-expressible enforcement to
                // re-derive, so the (vacuous) round-trip holds.
                Ok(())
            } else {
                Err(Diag::of_kind(crate::error::Frontend {
                    detail: format!(
                        "lift certify: no derived shape for target {:?}; the proposal did not \
                         re-derive the shape-expressible core",
                        a.core.target
                    ),
                }))
            }
        }
        Some(derived) => {
            // SOUNDNESS (`⊑`): the derived shape enforces at least the shape-expressible core —
            // deletion-then-rederivation never loses enforcement over the core fragment.
            if !subsumes(derived, &a.core) {
                return Err(Diag::of_kind(crate::error::Frontend {
                    detail: format!(
                        "lift certify: soundness failed — the derived shape does not enforce the \
                         core (derived enforcement={}, core enforcement={})",
                        super::subsumption::enforcement_key(derived),
                        super::subsumption::enforcement_key(&a.core),
                    ),
                }));
            }
            // ROUND-TRIP (`≡`): the derived shape is enforcement-equivalent to the core.
            if !equivalent(derived, &a.core) {
                return Err(Diag::of_kind(crate::error::Frontend {
                    detail: format!(
                        "lift certify: round-trip failed — derived shape is not equivalent to the \
                         core (derived enforcement={}, core enforcement={})",
                        super::subsumption::enforcement_key(derived),
                        super::subsumption::enforcement_key(&a.core),
                    ),
                }));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ConstraintProvenance, ShaclNodeKind, ShapeTarget, ShapeValue};

    const NS: &str = "https://blackcatinformatics.ca/gmeow/";

    fn g(local: &str) -> String {
        format!("{NS}{local}")
    }

    /// A one-property `Class(K)` shape.
    fn class_shape(local: &str, pc: PropertyConstraintIr) -> ValidationShapeIr {
        ValidationShapeIr::new(
            format!("{}-shape", g(local)),
            ShapeTarget::Class(g(local)),
            vec![pc],
            None,
        )
        .unwrap()
    }

    /// A `Class(K)` shape carrying only node-level components.
    fn class_node_shape(local: &str, nodes: Vec<ConstraintComponent>) -> ValidationShapeIr {
        ValidationShapeIr::new(
            format!("{}-shape", g(local)),
            ShapeTarget::Class(g(local)),
            vec![],
            None,
        )
        .unwrap()
        .with_node_components(nodes)
        .unwrap()
    }

    #[test]
    fn certify_cardinality_min_max() {
        let pc = PropertyConstraintIr::new(
            g("hasPart"),
            Some(1),
            Some(3),
            Some(ConstraintProvenance::OwlRestriction),
            vec![],
        )
        .unwrap();
        let s = class_shape("Widget", pc);
        certify(&s).expect("cardinality min/max must certify");
    }

    #[test]
    fn certify_exact_cardinality() {
        let pc = PropertyConstraintIr::new(
            g("hasSerial"),
            Some(1),
            Some(1),
            Some(ConstraintProvenance::OwlRestriction),
            vec![],
        )
        .unwrap();
        let s = class_shape("Gadget", pc);
        // Exact cardinality lowers to owl:cardinality; the round-trip must still hold.
        assert!(lift(&s).axioms_ttl.contains("owl:cardinality"));
        certify(&s).expect("exact cardinality must certify");
    }

    #[test]
    fn certify_functional() {
        let pc = PropertyConstraintIr::new(
            g("hasOwner"),
            None,
            Some(1),
            Some(ConstraintProvenance::OwlRestriction),
            vec![],
        )
        .unwrap();
        let s = ValidationShapeIr::new(
            format!("{}-domain-shape", g("hasOwner")),
            ShapeTarget::SubjectsOf(g("hasOwner")),
            vec![pc],
            None,
        )
        .unwrap();
        assert!(lift(&s).axioms_ttl.contains("owl:FunctionalProperty"));
        certify(&s).expect("functional property must certify");
    }

    #[test]
    fn certify_inverse_functional() {
        let pc = PropertyConstraintIr::new(
            g("hasSsn"),
            None,
            Some(1),
            Some(ConstraintProvenance::OwlRestriction),
            vec![],
        )
        .unwrap()
        .inverted();
        let s = ValidationShapeIr::new(
            format!("{}-range-shape", g("hasSsn")),
            ShapeTarget::ObjectsOf(g("hasSsn")),
            vec![pc],
            None,
        )
        .unwrap();
        assert!(
            lift(&s)
                .axioms_ttl
                .contains("owl:InverseFunctionalProperty")
        );
        certify(&s).expect("inverse-functional property must certify");
    }

    #[test]
    fn certify_qualified_value_shape() {
        let pc = PropertyConstraintIr::new(
            g("hasWheel"),
            None,
            None,
            None,
            vec![ConstraintComponent::QualifiedValueShape {
                shape: vec![ConstraintComponent::Class(g("Wheel"))],
                min: Some(1),
                max: None,
            }],
        )
        .unwrap();
        let s = class_shape("Car", pc);
        let prop = lift(&s);
        assert!(prop.axioms_ttl.contains("owl:onClass"));
        assert!(prop.axioms_ttl.contains("owl:minQualifiedCardinality"));
        certify(&s).expect("qualified value shape must certify");
    }

    #[test]
    fn certify_some_values_from_class() {
        // `someValuesFrom C` under-approximates to a bare `sh:class C` component.
        let pc = PropertyConstraintIr::new(
            g("hasEngine"),
            None,
            None,
            None,
            vec![ConstraintComponent::Class(g("Engine"))],
        )
        .unwrap();
        let s = class_shape("Vehicle", pc);
        assert!(lift(&s).axioms_ttl.contains("owl:allValuesFrom"));
        certify(&s).expect("class-membership component must certify");
    }

    #[test]
    fn certify_has_value_iri() {
        let pc = PropertyConstraintIr::new(
            g("hasStatus"),
            None,
            None,
            None,
            vec![ConstraintComponent::HasValue(ShapeValue::Iri(g("Active")))],
        )
        .unwrap();
        let s = class_shape("Account", pc);
        assert!(lift(&s).axioms_ttl.contains("owl:hasValue"));
        certify(&s).expect("hasValue IRI must certify");
    }

    #[test]
    fn certify_in_value_set() {
        let nodes = vec![ConstraintComponent::In(vec![
            ShapeValue::Iri(g("Red")),
            ShapeValue::Iri(g("Green")),
        ])];
        let s = class_node_shape("Signal", nodes);
        assert!(lift(&s).axioms_ttl.contains("owl:oneOf"));
        certify(&s).expect("oneOf value set must certify");
    }

    #[test]
    fn certify_datatype() {
        let pc = PropertyConstraintIr::new(
            g("hasLabel"),
            None,
            None,
            None,
            vec![ConstraintComponent::Datatype(format!("{XSD}string"))],
        )
        .unwrap();
        let s = class_shape("Node", pc);
        certify(&s).expect("datatype component must certify");
    }

    #[test]
    fn certify_node_kind() {
        for nk in [ShaclNodeKind::BlankNodeOrIri, ShaclNodeKind::Literal] {
            let pc = PropertyConstraintIr::new(
                g("hasRef"),
                None,
                None,
                None,
                vec![ConstraintComponent::NodeKindShacl(nk)],
            )
            .unwrap();
            let s = class_shape("Ref", pc);
            certify(&s).unwrap_or_else(|e| panic!("node-kind {nk:?} must certify: {e}"));
        }
    }

    #[test]
    fn certify_disjoint_not() {
        let nodes = vec![ConstraintComponent::Not(Box::new(
            ConstraintComponent::Class(g("Liquid")),
        ))];
        let s = class_node_shape("Solid", nodes);
        assert!(lift(&s).axioms_ttl.contains("owl:disjointWith"));
        certify(&s).expect("disjointness must certify");
    }

    #[test]
    fn certify_domain_with_closure_optin() {
        let nodes = vec![ConstraintComponent::Class(g("Person"))];
        let s = ValidationShapeIr::new(
            format!("{}-domain-shape", g("knows")),
            ShapeTarget::SubjectsOf(g("knows")),
            vec![],
            None,
        )
        .unwrap()
        .with_node_components(nodes)
        .unwrap();
        let prop = lift(&s);
        assert!(prop.axioms_ttl.contains("rdfs:domain"));
        assert!(prop.axioms_ttl.contains("logic:ClosedWorldClosure"));
        certify(&s).expect("domain with closure opt-in must certify");
    }

    #[test]
    fn certify_range_with_closure_optin() {
        let nodes = vec![ConstraintComponent::Datatype(format!("{XSD}dateTime"))];
        let s = ValidationShapeIr::new(
            format!("{}-range-shape", g("bornOn")),
            ShapeTarget::ObjectsOf(g("bornOn")),
            vec![],
            None,
        )
        .unwrap()
        .with_node_components(nodes)
        .unwrap();
        let prop = lift(&s);
        assert!(prop.axioms_ttl.contains("rdfs:range"));
        assert!(prop.axioms_ttl.contains("logic:ClosedWorldClosure"));
        certify(&s).expect("range with closure opt-in must certify");
    }

    #[test]
    fn pattern_is_residue_never_emitted_and_core_still_certifies() {
        // A faceted-datatype property shape: a `Datatype` base with a lossy `Pattern` facet.
        let pc = PropertyConstraintIr::new(
            g("hasCode"),
            None,
            None,
            None,
            vec![
                ConstraintComponent::Datatype(format!("{XSD}string")),
                ConstraintComponent::Pattern {
                    regex: "^[A-Z]{3}$".into(),
                    flags: None,
                },
            ],
        )
        .unwrap();
        let s = class_shape("Product", pc);
        let prop = lift(&s);
        // The lossy pattern is carried in residue, NOT emitted as any sh:pattern-equivalent axiom.
        assert!(
            !prop.axioms_ttl.contains("pattern") && !prop.axioms_ttl.contains("Pattern"),
            "no pattern-equivalent axiom may be emitted: {}",
            prop.axioms_ttl
        );
        assert!(
            prop.residue.iter().any(|r| r.contains("regex-dialect")),
            "the pattern must be recorded as residue: {:?}",
            prop.residue
        );
        // The base datatype IS emitted, and the core (pattern excluded) round-trips.
        assert!(prop.axioms_ttl.contains("owl:onDatatype"));
        certify(&s).expect("the shape-expressible core (pattern excluded) must certify");
    }

    #[test]
    fn length_facets_round_trip() {
        let pc = PropertyConstraintIr::new(
            g("hasName"),
            None,
            None,
            None,
            vec![
                ConstraintComponent::Datatype(format!("{XSD}string")),
                ConstraintComponent::MinLength(2),
                ConstraintComponent::MaxLength(64),
            ],
        )
        .unwrap();
        let s = class_shape("Named", pc);
        let prop = lift(&s);
        assert!(prop.axioms_ttl.contains("xsd:minLength"));
        assert!(prop.axioms_ttl.contains("xsd:maxLength"));
        certify(&s).expect("length facets must round-trip");
    }

    #[test]
    fn numeric_range_has_no_owl_antecedent_and_is_residue() {
        // A SHACL-faithful component OWL cannot state: recorded as residue, never emitted, and the
        // (now-empty) core certifies vacuously.
        let pc = PropertyConstraintIr::new(
            g("hasWeight"),
            None,
            None,
            None,
            vec![ConstraintComponent::NumericRange {
                min: Some(0.0),
                max: Some(100.0),
                min_inclusive: true,
                max_inclusive: true,
            }],
        )
        .unwrap();
        let s = class_shape("Parcel", pc);
        let prop = lift(&s);
        assert!(
            prop.residue
                .iter()
                .any(|r| r.contains("no faithful OWL/RDFS")),
            "numeric range must be residue: {:?}",
            prop.residue
        );
        certify(&s).expect("a fully-residue shape certifies vacuously");
    }

    #[test]
    fn lift_is_deterministic() {
        // A shape mixing several families → the two lifts must be byte-identical.
        let pc1 = PropertyConstraintIr::new(
            g("hasPart"),
            Some(1),
            Some(3),
            Some(ConstraintProvenance::OwlRestriction),
            vec![],
        )
        .unwrap();
        let pc2 = PropertyConstraintIr::new(
            g("hasColour"),
            None,
            None,
            None,
            vec![ConstraintComponent::HasValue(ShapeValue::Iri(g("Blue")))],
        )
        .unwrap();
        let s = ValidationShapeIr::new(
            format!("{}-shape", g("Toy")),
            ShapeTarget::Class(g("Toy")),
            vec![pc1, pc2],
            None,
        )
        .unwrap()
        .with_node_components(vec![ConstraintComponent::Not(Box::new(
            ConstraintComponent::Class(g("Tool")),
        ))])
        .unwrap();
        assert_eq!(lift(&s).axioms_ttl, lift(&s).axioms_ttl);
        certify(&s).expect("the mixed shape's core must certify");
    }

    #[test]
    fn lift_writes_no_files_returns_owned_string() {
        // Guard: the lift surface is emit-only (Principle 4). It returns an owned String; it holds no
        // filesystem-write path. Exercising it must not touch `slices/**` — asserted here by the
        // absence of any I/O in `lift`/`analyze` and by the value-returning signature.
        let pc = PropertyConstraintIr::new(
            g("hasThing"),
            Some(1),
            None,
            Some(ConstraintProvenance::OwlRestriction),
            vec![],
        )
        .unwrap();
        let s = class_shape("Holder", pc);
        let prop = lift(&s);
        // The proposal is in-memory text; a `slices/` path never appears in it.
        assert!(!prop.axioms_ttl.contains("slices/"));
        assert!(prop.axioms_ttl.starts_with("@prefix"));
    }
}
