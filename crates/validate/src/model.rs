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
