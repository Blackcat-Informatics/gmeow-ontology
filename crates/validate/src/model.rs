// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Namespace IRI constants the validation lints need.
//!
//! PyO3-free. The GMEOW vocabulary namespace itself is NOT a constant here: it
//! is passed in from the Python `config.NAMESPACE` single-source-of-truth
//! (`https://blackcatinformatics.ca/gmeow/`) so the two never drift.

/// RDF namespace constants (`http://www.w3.org/1999/02/22-rdf-syntax-ns#`).
pub mod rdf {

    /// `rdf:type`.
    pub const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    /// `rdf:first` — the head cell of an RDF Collection.
    pub const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
    /// `rdf:rest` — the tail cell of an RDF Collection.
    pub const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
    /// `rdf:nil` — the empty-list terminator of an RDF Collection.
    pub const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
}

/// RDFS namespace constants (`http://www.w3.org/2000/01/rdf-schema#`).
pub mod rdfs {

    /// `rdfs:label`.
    pub const LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
    /// `rdfs:comment`.
    pub const COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
    /// `rdfs:isDefinedBy`.
    pub const IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
    /// `rdfs:subClassOf`.
    pub const SUB_CLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    /// `rdfs:subPropertyOf`.
    pub const SUB_PROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
    /// `rdfs:Datatype`.
    pub const DATATYPE: &str = "http://www.w3.org/2000/01/rdf-schema#Datatype";
    /// `rdfs:domain`.
    pub const DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
    /// `rdfs:range`.
    pub const RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
}

/// GMEOW `logic:` namespace constants (`https://blackcatinformatics.ca/logic/`).
///
/// The `logic:` core is the CANONICAL subsumption vocabulary in this codebase
/// (`rdfs:subClassOf` is a Principle-17 lossy projection of it), so any structural
/// traversal that must see the full subsumption lattice — the OntoUML sortal
/// identity checks especially — reads `logic:subClassOf` alongside `rdfs:subClassOf`.
pub mod logic {

    /// `logic:subClassOf` — the canonical subsumption edge (grounded twin of `rdfs:subClassOf`).
    pub const SUB_CLASS_OF: &str = "https://blackcatinformatics.ca/logic/subClassOf";
}

/// OWL namespace constants (`http://www.w3.org/2002/07/owl#`).
pub mod owl {

    /// `owl:sameAs` — the predicate the Principle 5 ban scans for.
    pub const SAME_AS: &str = "http://www.w3.org/2002/07/owl#sameAs";
    /// `owl:Ontology`.
    pub const ONTOLOGY: &str = "http://www.w3.org/2002/07/owl#Ontology";
    /// `owl:Class`.
    pub const CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
    /// `owl:ObjectProperty`.
    pub const OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
    /// `owl:DatatypeProperty`.
    pub const DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
    /// `owl:AnnotationProperty`.
    pub const ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";
    /// `owl:FunctionalProperty`.
    pub const FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
    /// `owl:equivalentProperty`.
    pub const EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
    /// `owl:AllDisjointClasses`.
    pub const ALL_DISJOINT_CLASSES: &str = "http://www.w3.org/2002/07/owl#AllDisjointClasses";
    /// `owl:members`.
    pub const MEMBERS: &str = "http://www.w3.org/2002/07/owl#members";
}

/// SKOS namespace constants (`http://www.w3.org/2004/02/skos/core#`).
pub mod skos {

    /// `skos:definition`.
    pub const DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
    /// `skos:example`.
    pub const EXAMPLE: &str = "http://www.w3.org/2004/02/skos/core#example";
}
