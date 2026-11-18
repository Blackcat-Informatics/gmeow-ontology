// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact canonical and projected spellings of the native calculus vocabulary.

static CALCULUS_VOCABULARY: [(&str, &str); 52] = {
    macro_rules! owl {
        ($local:literal) => {
            (
                concat!("https://blackcatinformatics.ca/logic/", $local),
                concat!("http://www.w3.org/2002/07/owl#", $local),
            )
        };
    }
    // The property-characteristic markers are NOT a pure namespace swap: the canonical `logic:`
    // spelling is lower-camel (`logic:transitiveProperty`), the `owl:` view upper-camel
    // (`owl:TransitiveProperty`) — the exact map `adapter::OWL_CHARACTERISTIC_TO_LOGIC` and
    // `rdf::owl_for_char` use. A slice authors `?P a logic:transitiveProperty` once `owl:` is
    // retired as an authoring vocabulary, so the
    // reasoner must lower THAT onto the upper-camel spelling the fixed RL characteristic rules match.
    macro_rules! owl_char {
        ($logic_local:literal, $owl_local:literal) => {
            (
                concat!("https://blackcatinformatics.ca/logic/", $logic_local),
                concat!("http://www.w3.org/2002/07/owl#", $owl_local),
            )
        };
    }
    [
        // Anchors.
        (crate::LOGIC_SUB_CLASS_OF, crate::RDFS_SUB_CLASS_OF),
        (crate::LOGIC_SUB_PROPERTY_OF, crate::RDFS_SUB_PROPERTY_OF),
        // Property domain/range — canonical `logic:` lowered to the fixed `rdfs:` calculus
        // spelling the DL/RL domain-range rules match (a slice authors `?P logic:domain C`
        // / `?P logic:range C`, e.g. the `logic:` grounding slice's own reasoning axioms).
        (crate::LOGIC_DOMAIN, crate::RDFS_DOMAIN),
        (crate::LOGIC_RANGE, crate::RDFS_RANGE),
        owl!("equivalentClass"),
        // Class-expression body.
        owl!("Restriction"),
        owl!("onProperty"),
        owl!("someValuesFrom"),
        owl!("allValuesFrom"),
        owl!("hasValue"),
        owl!("onClass"),
        owl!("onDataRange"),
        owl!("onDatatype"),
        owl!("withRestrictions"),
        owl!("cardinality"),
        owl!("minCardinality"),
        owl!("maxCardinality"),
        owl!("qualifiedCardinality"),
        owl!("minQualifiedCardinality"),
        owl!("maxQualifiedCardinality"),
        // Typing markers (rdf:type objects the refuters read by name).
        owl!("Class"),
        owl!("ObjectProperty"),
        owl!("DatatypeProperty"),
        owl!("NamedIndividual"),
        owl!("AnnotationProperty"),
        owl!("Ontology"),
        owl!("Thing"),
        owl!("Nothing"),
        // Property-characteristic types — canonical lower-camel `logic:` → upper-camel `owl:` view.
        owl_char!("functionalProperty", "FunctionalProperty"),
        owl_char!("inverseFunctionalProperty", "InverseFunctionalProperty"),
        owl_char!("transitiveProperty", "TransitiveProperty"),
        owl_char!("symmetricProperty", "SymmetricProperty"),
        owl_char!("asymmetricProperty", "AsymmetricProperty"),
        owl_char!("reflexiveProperty", "ReflexiveProperty"),
        owl_char!("irreflexiveProperty", "IrreflexiveProperty"),
        // Class + property axiom vocabulary.
        owl!("disjointWith"),
        owl!("complementOf"),
        owl!("inverseOf"),
        owl!("unionOf"),
        owl!("oneOf"),
        owl!("intersectionOf"),
        owl!("disjointUnionOf"),
        owl!("sameAs"),
        owl!("differentFrom"),
        owl!("equivalentProperty"),
        owl!("propertyChainAxiom"),
        owl!("propertyDisjointWith"),
        owl!("hasKey"),
        owl!("members"),
        owl!("hasSelf"),
        owl!("AllDisjointClasses"),
        owl!("AllDisjointProperties"),
    ]
};

/// The ordered vocabulary pairs consumed by the native reasoning boundary.
#[must_use]
pub fn calculus_vocabulary() -> &'static [(&'static str, &'static str)] {
    &CALCULUS_VOCABULARY
}

/// The canonical symbol for either exact registered spelling. Data terms are
/// not rewritten by this lookup; callers choose an operator or marker role.
#[must_use]
pub fn canonical_calculus_symbol(iri: &str) -> Option<&'static str> {
    CALCULUS_VOCABULARY
        .iter()
        .find(|(canonical, projected)| *canonical == iri || *projected == iri)
        .map(|(canonical, _)| *canonical)
}

/// Source syntax owned by a schema operator, distinct from individual facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaPredicateRole {
    Signature,
    ClassField,
    ClassList,
    ValueList,
    PropertyList,
    FacetList,
    Members,
    DistinctMembers,
}

/// Exact native schema roles, including record fields outside the alias table.
#[must_use]
pub fn schema_predicate_role(iri: &str) -> Option<SchemaPredicateRole> {
    if iri == "http://www.w3.org/2002/07/owl#distinctMembers" {
        return Some(SchemaPredicateRole::DistinctMembers);
    }
    let canonical = canonical_calculus_symbol(iri)?;
    crate::owl_view_of_predicate(canonical)?;
    let local = canonical.strip_prefix(crate::LOGIC_NS)?;
    Some(match local {
        "sameAs" | "differentFrom" => return None,
        "unionOf" | "intersectionOf" | "disjointUnionOf" => SchemaPredicateRole::ClassList,
        "oneOf" => SchemaPredicateRole::ValueList,
        "hasKey" | "propertyChainAxiom" => SchemaPredicateRole::PropertyList,
        "withRestrictions" => SchemaPredicateRole::FacetList,
        "members" => SchemaPredicateRole::Members,
        "onProperty"
        | "onClass"
        | "onDataRange"
        | "onDatatype"
        | "someValuesFrom"
        | "allValuesFrom"
        | "hasValue"
        | "hasSelf"
        | "cardinality"
        | "minCardinality"
        | "maxCardinality"
        | "qualifiedCardinality"
        | "minQualifiedCardinality"
        | "maxQualifiedCardinality"
        | "complementOf" => SchemaPredicateRole::ClassField,
        _ => SchemaPredicateRole::Signature,
    })
}

/// A schema declaration marker. Ordinary class membership, NamedIndividual and
/// the universal class values Thing/Nothing remain individual assertions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaTypeRole {
    Class,
    ClassExpression,
    PropertySignature,
    PropertyCharacteristic,
    DisjointClasses,
    DisjointProperties,
    DistinctIndividuals,
}

#[must_use]
pub fn schema_type_role(iri: &str) -> Option<SchemaTypeRole> {
    if iri == "http://www.w3.org/2000/01/rdf-schema#Class" {
        return Some(SchemaTypeRole::Class);
    }
    if iri == "http://www.w3.org/2002/07/owl#AllDifferent" {
        return Some(SchemaTypeRole::DistinctIndividuals);
    }
    let canonical = canonical_calculus_symbol(iri)?;
    crate::owl_view_of_type_marker(canonical)?;
    Some(match canonical.strip_prefix(crate::LOGIC_NS)? {
        "Class" => SchemaTypeRole::Class,
        "Restriction" => SchemaTypeRole::ClassExpression,
        "ObjectProperty" | "DatatypeProperty" | "AnnotationProperty" => {
            SchemaTypeRole::PropertySignature
        }
        "functionalProperty"
        | "inverseFunctionalProperty"
        | "transitiveProperty"
        | "symmetricProperty"
        | "asymmetricProperty"
        | "reflexiveProperty"
        | "irreflexiveProperty" => SchemaTypeRole::PropertyCharacteristic,
        "AllDisjointClasses" => SchemaTypeRole::DisjointClasses,
        "AllDisjointProperties" => SchemaTypeRole::DisjointProperties,
        _ => return None,
    })
}
