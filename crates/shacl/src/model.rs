// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! Namespace IRI constants for SHACL, RDF, RDFS, and XSD.
//!
//! All constants are `oxigraph::model::NamedNodeRef<'static>` so they can be
//! used directly in oxigraph quad patterns without allocation.

use oxigraph::model::NamedNodeRef;

/// SHACL namespace constants (`http://www.w3.org/ns/shacl#`).
pub mod sh {
    use super::NamedNodeRef;

    pub const CONFORMS: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#conforms");

    pub const VALIDATION_REPORT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#ValidationReport");

    pub const VALIDATION_RESULT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#ValidationResult");

    pub const RESULT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#result");

    pub const FOCUS_NODE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#focusNode");

    pub const RESULT_PATH: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#resultPath");

    pub const VALUE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#value");

    pub const RESULT_SEVERITY: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#resultSeverity");

    pub const RESULT_MESSAGE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#resultMessage");

    pub const SOURCE_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#sourceConstraintComponent");

    pub const SOURCE_SHAPE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#sourceShape");

    pub const VIOLATION: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#Violation");

    pub const WARNING: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#Warning");

    pub const INFO: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#Info");
}

/// RDF namespace constants (`http://www.w3.org/1999/02/22-rdf-syntax-ns#`).
pub mod rdf {
    use super::NamedNodeRef;

    pub const TYPE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");

    pub const FIRST: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/1999/02/22-rdf-syntax-ns#first");

    pub const REST: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/1999/02/22-rdf-syntax-ns#rest");

    pub const NIL: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/1999/02/22-rdf-syntax-ns#nil");
}

/// RDFS namespace base string.
pub mod rdfs {
    pub const BASE: &str = "http://www.w3.org/2000/01/rdf-schema#";
}

/// XSD namespace base string.
pub mod xsd {
    pub const BASE: &str = "http://www.w3.org/2001/XMLSchema#";

    pub const BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

    pub const STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
}
