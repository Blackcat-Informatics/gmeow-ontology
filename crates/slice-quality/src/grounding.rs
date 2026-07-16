// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One fail-closed definition of a valid authored `logic:GroundingCorrespondence`.
//!
//! The linkage score and the external-vocabulary ownership ratchet consume this same
//! predicate. A marker type alone therefore grants neither calculus credit nor permission
//! to name a guarded target vocabulary: both require the exact authoring envelope the
//! correspondence compiler accepts.

use std::collections::BTreeSet;

use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef};

use crate::graph::{self, all_iris, all_lits, g, id, instances_of};

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const IDENTITY_PREDICATES: &[&str] = &[
    "http://www.w3.org/2004/02/skos/core#exactMatch",
    "http://www.w3.org/2002/07/owl#equivalentClass",
    "http://www.w3.org/2002/07/owl#equivalentProperty",
    "http://www.w3.org/2002/07/owl#sameAs",
];
const MORPHISM_CLASSES: &[&str] = &[
    "Isomorphism",
    "SectionRetraction",
    "WellBehavedLens",
    "LossyLens",
    "Prism",
    "AffineCorrespondence",
    "BridgeView",
];
const PRESERVATION_KINDS: &[&str] = &[
    "ExactPreservation",
    "SoundUnderApproximation",
    "CompleteOverApproximation",
    "InconsistencyPreserving",
    "InconsistencyReflecting",
    "ValidationOnly",
    "Unsupported",
];

fn exactly_one_iri(ds: &RdfDataset, subject: TermId, predicate: &str) -> Option<String> {
    let values = all_iris(ds, subject, id(ds, predicate)?);
    (values.len() == 1).then(|| values[0].clone())
}

fn exactly_one_lit(ds: &RdfDataset, subject: TermId, predicate: &str) -> Option<String> {
    let values = all_lits(ds, subject, id(ds, predicate)?);
    (values.len() == 1).then(|| values[0].clone())
}

fn objects(ds: &RdfDataset, subject: TermId, predicate: &str) -> Vec<TermId> {
    let Some(predicate) = id(ds, predicate) else {
        return Vec::new();
    };
    ds.quads_for_pattern(Some(subject), Some(predicate), None, GraphMatch::Any)
        .map(|q| q.o)
        .collect()
}

fn has_type(ds: &RdfDataset, subject: TermId, class: &str) -> bool {
    let (Some(predicate), Some(object)) = (id(ds, graph::RDF_TYPE), id(ds, class)) else {
        return false;
    };
    graph::has(ds, subject, predicate, object)
}

fn exactly_one_logic_value(
    ds: &RdfDataset,
    subject: TermId,
    property: &str,
    allowed: &[&str],
) -> Option<String> {
    let value = exactly_one_iri(ds, subject, &format!("{LOGIC}{property}"))?;
    let local = value.strip_prefix(LOGIC)?;
    allowed.contains(&local).then_some(value)
}

/// Whether `cell_iri` is a complete, internally consistent grounding frontend cell.
pub(crate) fn is_validated_grounding_correspondence(ds: &RdfDataset, cell_iri: &str) -> bool {
    let Some(cell) = id(ds, cell_iri) else {
        return false;
    };
    if !has_type(ds, cell, &format!("{LOGIC}GroundingCorrespondence")) {
        return false;
    }

    let term_frontend = has_type(ds, cell, &g("TermEquivalence"));
    let projection_frontend = has_type(ds, cell, &g("ProjectionMapping"));
    if term_frontend == projection_frontend {
        return false;
    }

    let Some(_justification) = exactly_one_iri(ds, cell, &g("justification")) else {
        return false;
    };
    let Some(source) = exactly_one_iri(ds, cell, &format!("{LOGIC}sourceEndpoint")) else {
        return false;
    };
    let Some(target) = exactly_one_iri(ds, cell, &format!("{LOGIC}targetEndpoint")) else {
        return false;
    };
    let Some(morphism_class) = exactly_one_logic_value(ds, cell, "morphismClass", MORPHISM_CLASSES)
    else {
        return false;
    };
    let Some(morphism_kind) = exactly_one_logic_value(
        ds,
        cell,
        "morphismKind",
        &["InstitutionMorphism", "CommitmentShiftingBridge"],
    ) else {
        return false;
    };
    if exactly_one_logic_value(ds, cell, "preservationKind", PRESERVATION_KINDS).is_none() {
        return false;
    }
    let bridge = morphism_class == format!("{LOGIC}BridgeView");
    let commitment_shift = morphism_kind == format!("{LOGIC}CommitmentShiftingBridge");
    if bridge != commitment_shift {
        return false;
    }

    if term_frontend {
        let Some(aligned_source) = exactly_one_iri(ds, cell, &g("alignSubject")) else {
            return false;
        };
        let Some(predicate) = exactly_one_iri(ds, cell, &g("alignPredicate")) else {
            return false;
        };
        let Some(aligned_target) = exactly_one_iri(ds, cell, &g("alignObject")) else {
            return false;
        };
        if exactly_one_lit(ds, cell, &g("sssomFile")).is_none()
            || source != aligned_source
            || target != aligned_target
        {
            return false;
        }
        return !(bridge && IDENTITY_PREDICATES.contains(&predicate.as_str()));
    }

    let bindings = objects(ds, cell, &g("hasBinding"));
    if bindings.len() != 1 || objects(ds, cell, &g("hasMappingPattern")).len() != 1 {
        return false;
    }
    let binding = bindings[0];
    if exactly_one_lit(ds, binding, &g("profile")).is_none() {
        return false;
    }
    let Some(relation) = exactly_one_lit(ds, binding, &g("relation")) else {
        return false;
    };
    let mut targets = Vec::new();
    for property in ["toPredicate", "toClass", "edoalTarget"] {
        if let Some(predicate) = id(ds, &g(property)) {
            targets.extend(all_iris(ds, binding, predicate));
        }
    }
    targets.len() == 1 && targets[0] == target && !(bridge && relation == "=")
}

/// Every valid grounding frontend cell in deterministic IRI order.
pub(crate) fn validated_grounding_cells(ds: &RdfDataset) -> BTreeSet<String> {
    instances_of(ds, &format!("{LOGIC}GroundingCorrespondence"))
        .into_iter()
        .filter(|iri| is_validated_grounding_correspondence(ds, iri))
        .collect()
}

/// The valid grounding `ProjectionMapping` whose sole binding is `binding`, if any.
pub(crate) fn validated_projection_owner(ds: &RdfDataset, binding: TermId) -> Option<String> {
    let has_binding = id(ds, &g("hasBinding"))?;
    ds.quads_for_pattern(None, Some(has_binding), Some(binding), GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.s) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
        .find(|iri| {
            is_validated_grounding_correspondence(ds, iri)
                && id(ds, iri).is_some_and(|cell| has_type(ds, cell, &g("ProjectionMapping")))
        })
}
