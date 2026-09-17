// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Project a typed [`OntoumlModel`] onto the world-scoped, all-IRI `logic:`
//! stereotype ABox the foundation-discipline chase consumes.
//!
//! The chase reads only IRI triples (`rdf:type`, `logic:subClassOf`,
//! `logic:mediates`, `owl:FunctionalProperty`); every emitted term is an IRI, so
//! no literal escaping is needed. The emitted EDB fact shapes mirror the
//! committed `conformance/logic/cases/foundation/*/input.nq` fixtures exactly, so
//! the disciplines fire (or abstain) on the same fact shapes the goldens pin:
//!
//! * a stereotyped class → `<class> rdf:type <logic:{Stereotype}>` (the chase
//!   derives `hasMetaClass`/`isClass`/`hasSomeStereotype` from it);
//! * a zero-stereotype class not otherwise referenced by a generalization →
//!   `<class> logic:subClassOf <class>`, a class-presence marker so
//!   StereotypeCardinality's "no stereotype at all" rule (which keys on
//!   `isClass ∧ ¬hasSomeStereotype`) can fire;
//! * a generalization → `<specific> logic:subClassOf <general>` (the chase reads
//!   `logic:subClassOf`, NOT `rdfs:subClassOf`);
//! * a mediation → one `<relator> logic:mediates <role>` per mediated relatum
//!   (a distinct role IRI per end so two ends satisfy `hasTwoMediatedRelata`),
//!   plus `<role> rdf:type <owl:FunctionalProperty>` when the mediated end is
//!   functional (a single functional role is the RelComp anti-pattern shape).

use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm};
use std::sync::Arc;

use super::model::{OntoumlError, OntoumlModel, logic_local_for_stereotype};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const OWL_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
use gmeow_ns::LOGIC_NS;

/// Lower a model into a sorted, deduped world-scoped N-Quads string, returning
/// the text and its quad count.
///
/// Every line is `<{s}> <{p}> <{o}> <{world_iri}> .`. An unrecognized stereotype
/// on any class is an [`OntoumlError::Unsupported`] gap raised here (the
/// stereotype→`logic:` map is applied at lower time, never at parse time).
pub fn lower_model(model: &OntoumlModel, world_iri: &str) -> Result<(String, usize), OntoumlError> {
    let dataset = lower_model_dataset(model, world_iri)?;
    let count = dataset.quads().count();
    let text = purrdf::canonical_flat_nquads(&dataset)
        .map_err(|error| OntoumlError::Syntax(format!("render lowered model: {error}")))?;
    Ok((text, count))
}

/// Lower directly into a validated native world-scoped dataset. RDF text is only
/// emitted by the selected external corpus-writing boundary.
///
/// # Errors
/// Unsupported stereotypes are capability gaps; invalid native terms are hard failures.
pub fn lower_model_dataset(
    model: &OntoumlModel,
    world_iri: &str,
) -> Result<Arc<RdfDataset>, OntoumlError> {
    use std::collections::BTreeSet;

    let mut builder = RdfDatasetBuilder::new();
    let mut push = |s: &str, p: &str, o: &str| {
        builder.push_owned_quad(
            &RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(world_iri)),
        );
    };

    let logic = |local: &str| format!("{LOGIC_NS}{local}");

    // Predicate IRIs reused across every edge — built once, not per iteration.
    let p_sub_class_of = logic("subClassOf");
    let p_mediates = logic("mediates");

    // Classes: one rdf:type stereotype pun per asserted stereotype.
    for class in &model.classes {
        for stereo in &class.stereotypes {
            let logic_local = logic_local_for_stereotype(stereo)?;
            push(&class.iri, RDF_TYPE, &logic(logic_local));
        }
    }

    // Generalizations: logic:subClassOf (the chase reads this, not rdfs:).
    let mut referenced: BTreeSet<&str> = BTreeSet::new();
    for edge in &model.generalizations {
        push(&edge.specific, &p_sub_class_of, &edge.general);
        referenced.insert(edge.specific.as_str());
        referenced.insert(edge.general.as_str());
    }

    // Zero-stereotype class presence markers: a bare class the chase would
    // otherwise never see needs an `isClass` witness for StereotypeCardinality's
    // "no stereotype at all" rule. `logic:subClassOf` is the only EDB predicate
    // that yields `isClass` without also asserting a stereotype, so a class not
    // already referenced by a generalization is marked with a self-edge. A class
    // already at either end of a generalization is `isClass` via that edge, so no
    // marker is emitted (mirrors the committed exactly-one-stereotype fixture,
    // where the bare NoStereo class is seen purely through its generalization).
    for class in &model.classes {
        if class.stereotypes.is_empty() && !referenced.contains(class.iri.as_str()) {
            push(&class.iri, &p_sub_class_of, &class.iri);
        }
    }

    // Mediations: one distinct mediates role per mediated relatum, marked
    // functional when the mediated end is upper-bound-1.
    for med in &model.mediations {
        for (idx, _relatum) in med.mediated.iter().enumerate() {
            let role = format!("{}#end{idx}", med.relation_iri);
            push(&med.relator, &p_mediates, &role);
            if med.functional {
                push(&role, RDF_TYPE, OWL_FUNCTIONAL_PROPERTY);
            }
        }
    }

    builder
        .freeze()
        .map_err(|error| OntoumlError::Syntax(format!("invalid lowered model: {error}")))
}

#[path = "lower.tests.rs"]
#[cfg(test)]
mod tests;
