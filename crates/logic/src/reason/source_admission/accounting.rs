// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native model accounting for interpreted relations and equality.

use super::{ENGINE_NATIVE, NativeSourceDrop as DropRecord};
use std::collections::BTreeSet;

const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const RDFS_NS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const OWL_NS: &str = "http://www.w3.org/2002/07/owl#";
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// Namespaces whose terms the native engine may interpret. A term outside them is
/// an ordinary constant or relation with no engine law, so its facts are a model.
const INTERPRETED_NAMESPACES: [&str; 4] = [RDF_NS, RDFS_NS, OWL_NS, LOGIC_NS];

/// How an accounted predicate constrains the terms it relates.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Positions {
    /// Membership: the subject is any term; the object is a class or an accounted marker.
    Membership,
    /// Both positions name classes.
    Classes,
    /// Both positions name properties.
    Properties,
    /// A property and a class.
    PropertyClass,
    /// Two individuals related by equality, accounted only when they are identical.
    Equality,
    /// Two individuals related by explicit difference.
    Individuals,
    /// An annotation with no logical law.
    Annotation,
}

/// THE accounting list: every interpreted predicate whose law the completed native
/// closure satisfies, so that a completed closure without a clash is a model of the
/// units that use it. A predicate in an interpreted namespace that is not listed
/// here is a named drop. When in doubt a construct is not on this list.
const ACCOUNTED_PREDICATES: &[(&str, Positions, &str)] = &[
    (
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
        Positions::Membership,
        "membership fact; the object is checked against ACCOUNTED_MARKERS",
    ),
    (
        "https://blackcatinformatics.ca/logic/instanceOf",
        Positions::Membership,
        "membership fact; the object is checked against ACCOUNTED_MARKERS",
    ),
    (
        "https://blackcatinformatics.ca/logic/subClassOf",
        Positions::Classes,
        "Horn law closed by el:type-propagation and el:subClassOf-transitive",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#subClassOf",
        Positions::Classes,
        "Horn law closed by el:type-propagation and el:subClassOf-transitive",
    ),
    (
        "https://blackcatinformatics.ca/logic/equivalentClass",
        Positions::Classes,
        "Horn law closed by el:equivalentClass-fwd and el:equivalentClass-bwd",
    ),
    (
        "http://www.w3.org/2002/07/owl#equivalentClass",
        Positions::Classes,
        "Horn law closed by el:equivalentClass-fwd and el:equivalentClass-bwd",
    ),
    (
        "https://blackcatinformatics.ca/logic/disjointWith",
        Positions::Classes,
        "denial checked by dl:individual-clash and dl:unsatisfiable-class",
    ),
    (
        "http://www.w3.org/2002/07/owl#disjointWith",
        Positions::Classes,
        "denial checked by dl:individual-clash and dl:unsatisfiable-class",
    ),
    (
        "https://blackcatinformatics.ca/logic/subPropertyOf",
        Positions::Properties,
        "Horn law closed by subproperty propagation and el:subPropertyOf-transitive",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#subPropertyOf",
        Positions::Properties,
        "Horn law closed by subproperty propagation and el:subPropertyOf-transitive",
    ),
    (
        "https://blackcatinformatics.ca/logic/equivalentProperty",
        Positions::Properties,
        "Horn law closed by dl:equivalentProperty-subproperty",
    ),
    (
        "http://www.w3.org/2002/07/owl#equivalentProperty",
        Positions::Properties,
        "Horn law closed by dl:equivalentProperty-subproperty",
    ),
    (
        "https://blackcatinformatics.ca/logic/inverseOf",
        Positions::Properties,
        "Horn law closed by dl:inverseOf and dl:inverseOf-symmetry",
    ),
    (
        "http://www.w3.org/2002/07/owl#inverseOf",
        Positions::Properties,
        "Horn law closed by dl:inverseOf and dl:inverseOf-symmetry",
    ),
    (
        "https://blackcatinformatics.ca/logic/domain",
        Positions::PropertyClass,
        "Horn law closed by dl:domain",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#domain",
        Positions::PropertyClass,
        "Horn law closed by dl:domain",
    ),
    (
        "https://blackcatinformatics.ca/logic/range",
        Positions::PropertyClass,
        "Horn law closed by dl:range",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#range",
        Positions::PropertyClass,
        "Horn law closed by dl:range",
    ),
    (
        "https://blackcatinformatics.ca/logic/sameAs",
        Positions::Equality,
        "reflexive equality needs no substitution; any other equality is a drop",
    ),
    (
        "http://www.w3.org/2002/07/owl#sameAs",
        Positions::Equality,
        "reflexive equality needs no substitution; any other equality is a drop",
    ),
    (
        "https://blackcatinformatics.ca/logic/differentFrom",
        Positions::Individuals,
        "denial checked by dl:same-different-clash; without equality named terms stay distinct",
    ),
    (
        "http://www.w3.org/2002/07/owl#differentFrom",
        Positions::Individuals,
        "denial checked by dl:same-different-clash; without equality named terms stay distinct",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#label",
        Positions::Annotation,
        "annotation without a law",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#comment",
        Positions::Annotation,
        "annotation without a law",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#isDefinedBy",
        Positions::Annotation,
        "annotation without a law",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#seeAlso",
        Positions::Annotation,
        "annotation without a law",
    ),
];

/// Membership objects in an interpreted namespace that the accounting admits, with
/// the law the completed closure satisfies for each.
const ACCOUNTED_MARKERS: &[(&str, &str)] = &[
    (
        "http://www.w3.org/2002/07/owl#Class",
        "declaration without a law",
    ),
    (
        "http://www.w3.org/2000/01/rdf-schema#Class",
        "declaration without a law",
    ),
    (
        "https://blackcatinformatics.ca/logic/Class",
        "declaration without a law",
    ),
    (
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#Property",
        "declaration without a law",
    ),
    (
        "http://www.w3.org/2002/07/owl#ObjectProperty",
        "declaration without a law",
    ),
    (
        "http://www.w3.org/2002/07/owl#NamedIndividual",
        "declaration without a law",
    ),
    (
        "http://www.w3.org/2002/07/owl#Thing",
        "the universal class holds of every element",
    ),
    (
        "https://blackcatinformatics.ca/logic/Thing",
        "the universal class holds of every element",
    ),
    (
        "http://www.w3.org/2002/07/owl#Nothing",
        "denial checked by dl:nothing-membership",
    ),
    (
        "https://blackcatinformatics.ca/logic/Nothing",
        "denial checked by dl:nothing-membership",
    ),
    (
        "http://www.w3.org/2002/07/owl#TransitiveProperty",
        "Horn law closed by dl:transitive-property",
    ),
    (
        "https://blackcatinformatics.ca/logic/transitiveProperty",
        "Horn law closed by dl:transitive-property",
    ),
    (
        "http://www.w3.org/2002/07/owl#SymmetricProperty",
        "Horn law closed by dl:symmetric-property",
    ),
    (
        "https://blackcatinformatics.ca/logic/symmetricProperty",
        "Horn law closed by dl:symmetric-property",
    ),
    (
        "http://www.w3.org/2002/07/owl#IrreflexiveProperty",
        "denial checked by dl:irreflexive-property-clash",
    ),
    (
        "https://blackcatinformatics.ca/logic/irreflexiveProperty",
        "denial checked by dl:irreflexive-property-clash",
    ),
    (
        "http://www.w3.org/2002/07/owl#AsymmetricProperty",
        "denial checked by dl:asymmetric-property-clash",
    ),
    (
        "https://blackcatinformatics.ca/logic/asymmetricProperty",
        "denial checked by dl:asymmetric-property-clash",
    ),
    (
        "http://www.w3.org/2002/07/owl#FunctionalProperty",
        "equality-generating dl:functional-equality; every derived equality is checked",
    ),
    (
        "https://blackcatinformatics.ca/logic/functionalProperty",
        "equality-generating dl:functional-equality; every derived equality is checked",
    ),
    (
        "http://www.w3.org/2002/07/owl#InverseFunctionalProperty",
        "equality-generating dl:inverse-functional-equality; every derived equality is checked",
    ),
    (
        "https://blackcatinformatics.ca/logic/inverseFunctionalProperty",
        "equality-generating dl:inverse-functional-equality; every derived equality is checked",
    ),
];

/// Record-form property characteristics the native engine reads from the record
/// itself. The other sorts are read only in marker form.
pub(super) const ACCOUNTED_CHARACTERISTIC_RECORDS: &[(&str, &str)] = &[
    (
        "https://blackcatinformatics.ca/logic/functionalProperty",
        "read from the record by the functional characteristic schema",
    ),
    (
        "https://blackcatinformatics.ca/logic/inverseFunctionalProperty",
        "read from the record by the inverse-functional characteristic schema",
    ),
];

const UNIVERSAL_CLASSES: [&str; 2] = [
    "http://www.w3.org/2002/07/owl#Thing",
    "https://blackcatinformatics.ca/logic/Thing",
];
const EMPTY_CLASSES: [&str; 2] = [
    "http://www.w3.org/2002/07/owl#Nothing",
    "https://blackcatinformatics.ca/logic/Nothing",
];
const EQUALITY_PREDICATES: [&str; 2] = [
    "https://blackcatinformatics.ca/logic/sameAs",
    "http://www.w3.org/2002/07/owl#sameAs",
];

fn interpreted(iri: &str) -> bool {
    INTERPRETED_NAMESPACES
        .iter()
        .any(|namespace| iri.starts_with(namespace))
}

/// A term of an accounted unit, reduced to what accounting needs.
#[derive(Clone, Copy)]
pub(super) enum UnitTerm<'a> {
    Iri(&'a str),
    Variable,
    Other,
}

impl<'a> UnitTerm<'a> {
    pub(super) fn atomic(term: &'a gmeow_logic_compile::ir::AtomicTerm) -> Self {
        match term {
            gmeow_logic_compile::ir::AtomicTerm::Iri(iri) => Self::Iri(iri),
            gmeow_logic_compile::ir::AtomicTerm::Var(_) => Self::Variable,
            _ => Self::Other,
        }
    }

    fn formula(term: &'a gmeow_logic_compile::ir::Term) -> Self {
        match term {
            gmeow_logic_compile::ir::Term::Iri(iri) => Self::Iri(iri),
            gmeow_logic_compile::ir::Term::Var(_) => Self::Variable,
            _ => Self::Other,
        }
    }
}

/// Whether a class position holds a class the accounting admits in that role.
/// `universal_ok`/`empty_ok` admit the trivially satisfied and the denial readings.
fn class_accounted(term: UnitTerm<'_>, universal_ok: bool, empty_ok: bool) -> bool {
    match term {
        UnitTerm::Iri(iri) if UNIVERSAL_CLASSES.contains(&iri) => universal_ok,
        UnitTerm::Iri(iri) if EMPTY_CLASSES.contains(&iri) => empty_ok,
        UnitTerm::Iri(iri) => !interpreted(iri),
        UnitTerm::Variable | UnitTerm::Other => false,
    }
}

fn property_accounted(term: UnitTerm<'_>) -> bool {
    matches!(term, UnitTerm::Iri(iri) if !interpreted(iri))
}

/// Account one binary predication against THE list. `None` when accounted,
/// otherwise the reason it is not.
pub(super) fn account_predication(
    subject: UnitTerm<'_>,
    predicate: UnitTerm<'_>,
    object: UnitTerm<'_>,
) -> Option<String> {
    let UnitTerm::Iri(predicate) = predicate else {
        return Some("a variable or non-IRI relation has no accounted law".to_owned());
    };
    if !interpreted(predicate) {
        return None;
    }
    let Some((_, positions, _)) = ACCOUNTED_PREDICATES
        .iter()
        .find(|(iri, _, _)| *iri == predicate)
    else {
        return Some(format!(
            "interpreted predicate {predicate} is not on the accounting list"
        ));
    };
    let accounted = match positions {
        Positions::Membership => match object {
            UnitTerm::Iri(class) if interpreted(class) => {
                ACCOUNTED_MARKERS.iter().any(|(marker, _)| *marker == class)
            }
            UnitTerm::Iri(_) => true,
            UnitTerm::Variable | UnitTerm::Other => false,
        },
        Positions::Classes => match predicate.rsplit(['/', '#']).next() {
            Some("subClassOf") => {
                class_accounted(subject, false, true) && class_accounted(object, true, true)
            }
            _ => class_accounted(subject, false, false) && class_accounted(object, false, false),
        },
        Positions::Properties => property_accounted(subject) && property_accounted(object),
        Positions::PropertyClass => {
            property_accounted(subject) && class_accounted(object, true, false)
        }
        Positions::Equality => matches!(
            (subject, object),
            (UnitTerm::Iri(left), UnitTerm::Iri(right)) if left == right
        ),
        Positions::Individuals | Positions::Annotation => true,
    };
    (!accounted)
        .then(|| format!("{predicate} is accounted only for the positions on its list entry"))
}

/// Account every relation a formula uses. Horn lowering decides which atoms become
/// heads; every interpreted relation in the tree must therefore be accounted.
pub(super) fn account_formula(
    formula: &gmeow_logic_compile::ir::Formula,
    refusals: &mut Vec<String>,
) {
    use gmeow_logic_compile::ir::Formula;
    match formula {
        Formula::Atom { relation, args } => {
            let refusal = match args.as_slice() {
                [subject, object] => account_predication(
                    UnitTerm::formula(subject),
                    UnitTerm::formula(relation),
                    UnitTerm::formula(object),
                ),
                _ => match UnitTerm::formula(relation) {
                    UnitTerm::Iri(iri) if !interpreted(iri) => None,
                    _ => Some(
                        "a non-binary interpreted or variable relation has no accounted law"
                            .to_owned(),
                    ),
                },
            };
            refusals.extend(refusal);
        }
        Formula::Not(inner) => account_formula(inner, refusals),
        Formula::And(parts) | Formula::Or(parts) => {
            for part in parts {
                account_formula(part, refusals);
            }
        }
        Formula::Implies(left, right) | Formula::Iff(left, right) => {
            account_formula(left, refusals);
            account_formula(right, refusals);
        }
        Formula::Forall { body, .. } | Formula::Exists { body, .. } => {
            account_formula(body, refusals);
        }
    }
}

pub(super) fn account_axiom(axiom: &gmeow_logic_compile::ir::LogicAxiom) -> Option<String> {
    let predicate = if axiom.predicate.starts_with('?') {
        UnitTerm::Variable
    } else {
        UnitTerm::Iri(&axiom.predicate)
    };
    let subject = if axiom.subject.starts_with('?') {
        UnitTerm::Variable
    } else {
        UnitTerm::Iri(&axiom.subject)
    };
    account_predication(subject, predicate, UnitTerm::atomic(&axiom.obj))
}

/// Equality between two different terms, asserted or derived. The native engine has
/// no substitution, so a closure containing one is not known to be a model.
pub(super) fn equality_drops(inferred: &[crate::reason::InferredAxiom]) -> BTreeSet<DropRecord> {
    inferred
        .iter()
        .filter(|axiom| EQUALITY_PREDICATES.contains(&axiom.predicate.as_str()))
        .filter_map(|axiom| {
            let subject = axiom
                .subject
                .strip_prefix('<')
                .and_then(|inner| inner.strip_suffix('>'))
                .unwrap_or(&axiom.subject);
            match axiom.object.as_iri() {
                Some(object) if object == subject => None,
                object => Some(DropRecord::new(
                    ENGINE_NATIVE,
                    "EQUALITY_WITHOUT_SUBSTITUTION",
                    format!(
                        "{subject} = {} ({}); the native engine has no equality substitution",
                        object.map_or_else(|| format!("{:?}", axiom.object), ToOwned::to_owned),
                        if axiom.is_edb { "asserted" } else { "derived" }
                    ),
                )),
            }
        })
        .collect()
}
