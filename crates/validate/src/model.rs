// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Namespace IRI constants the validation lints need.
//!
//! PyO3-free. The GMEOW vocabulary namespace itself is NOT a constant here: it
//! is passed in from the Python `config.NAMESPACE` single-source-of-truth
//! (`https://blackcatinformatics.ca/gmeow/`) so the two never drift.

use oxigraph::model::NamedNodeRef;

/// RDF namespace constants (`http://www.w3.org/1999/02/22-rdf-syntax-ns#`).
pub mod rdf {
    use super::NamedNodeRef;

    /// `rdf:type`.
    pub const TYPE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
    /// `rdf:first` — the head cell of an RDF Collection.
    pub const FIRST: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/1999/02/22-rdf-syntax-ns#first");
    /// `rdf:rest` — the tail cell of an RDF Collection.
    pub const REST: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/1999/02/22-rdf-syntax-ns#rest");
    /// `rdf:nil` — the empty-list terminator of an RDF Collection.
    pub const NIL: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/1999/02/22-rdf-syntax-ns#nil");
}

/// RDFS namespace constants (`http://www.w3.org/2000/01/rdf-schema#`).
pub mod rdfs {
    use super::NamedNodeRef;

    /// `rdfs:label`.
    pub const LABEL: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2000/01/rdf-schema#label");
    /// `rdfs:comment`.
    pub const COMMENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2000/01/rdf-schema#comment");
    /// `rdfs:isDefinedBy`.
    pub const IS_DEFINED_BY: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2000/01/rdf-schema#isDefinedBy");
    /// `rdfs:subClassOf`.
    pub const SUB_CLASS_OF: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2000/01/rdf-schema#subClassOf");
    /// `rdfs:subPropertyOf`.
    pub const SUB_PROPERTY_OF: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2000/01/rdf-schema#subPropertyOf");
    /// `rdfs:Datatype`.
    pub const DATATYPE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2000/01/rdf-schema#Datatype");
    /// `rdfs:domain`.
    pub const DOMAIN: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2000/01/rdf-schema#domain");
    /// `rdfs:range`.
    pub const RANGE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2000/01/rdf-schema#range");
}

/// OWL namespace constants (`http://www.w3.org/2002/07/owl#`).
pub mod owl {
    use super::NamedNodeRef;

    /// `owl:sameAs` — the predicate the Principle 5 ban scans for.
    pub const SAME_AS: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#sameAs");
    /// `owl:Ontology`.
    pub const ONTOLOGY: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#Ontology");
    /// `owl:Class`.
    pub const CLASS: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#Class");
    /// `owl:ObjectProperty`.
    pub const OBJECT_PROPERTY: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#ObjectProperty");
    /// `owl:DatatypeProperty`.
    pub const DATATYPE_PROPERTY: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#DatatypeProperty");
    /// `owl:AnnotationProperty`.
    pub const ANNOTATION_PROPERTY: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#AnnotationProperty");
    /// `owl:FunctionalProperty`.
    pub const FUNCTIONAL_PROPERTY: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#FunctionalProperty");
    /// `owl:equivalentProperty`.
    pub const EQUIVALENT_PROPERTY: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#equivalentProperty");
    /// `owl:AllDisjointClasses`.
    pub const ALL_DISJOINT_CLASSES: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#AllDisjointClasses");
    /// `owl:members`.
    pub const MEMBERS: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2002/07/owl#members");
}

/// SKOS namespace constants (`http://www.w3.org/2004/02/skos/core#`).
pub mod skos {
    use super::NamedNodeRef;

    /// `skos:definition`.
    pub const DEFINITION: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2004/02/skos/core#definition");
    /// `skos:example`.
    pub const EXAMPLE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2004/02/skos/core#example");
}
