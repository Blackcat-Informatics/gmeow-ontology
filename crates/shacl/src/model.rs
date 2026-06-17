// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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

    // ── Shape type terms ───────────────────────────────────────────────────────

    pub const NODE_SHAPE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#NodeShape");

    pub const PROPERTY_SHAPE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#PropertyShape");

    // ── Target predicates ──────────────────────────────────────────────────────

    pub const TARGET_CLASS: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#targetClass");

    pub const TARGET_SUBJECTS_OF: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#targetSubjectsOf");

    pub const TARGET_OBJECTS_OF: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#targetObjectsOf");

    pub const TARGET_NODE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#targetNode");

    // ── Property shape plumbing ────────────────────────────────────────────────

    pub const PROPERTY: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#property");

    pub const PATH: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#path");

    pub const INVERSE_PATH: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#inversePath");

    // ── Supported path forms that are NOT modelled (hard-fail set) ────────────

    pub const ALTERNATIVE_PATH: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#alternativePath");

    pub const ZERO_OR_MORE_PATH: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#zeroOrMorePath");

    pub const ONE_OR_MORE_PATH: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#oneOrMorePath");

    pub const ZERO_OR_ONE_PATH: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#zeroOrOnePath");

    // ── Constraint predicates (supported) ─────────────────────────────────────

    pub const CLASS: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#class");

    pub const DATATYPE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#datatype");

    pub const NODE_KIND: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#nodeKind");

    pub const MIN_COUNT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#minCount");

    pub const MAX_COUNT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#maxCount");

    pub const IN: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#in");

    pub const HAS_VALUE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#hasValue");

    pub const PATTERN: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#pattern");

    pub const FLAGS: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#flags");

    pub const MIN_LENGTH: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#minLength");

    pub const UNIQUE_LANG: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#uniqueLang");

    pub const MIN_INCLUSIVE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#minInclusive");

    pub const MAX_INCLUSIVE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#maxInclusive");

    pub const AND: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#and");

    pub const OR: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#or");

    pub const XONE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#xone");

    pub const NODE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#node");

    pub const REIFIER_SHAPE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#reifierShape");

    pub const REIFICATION_REQUIRED: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#reificationRequired");

    // ── Shape metadata (benign, not constraints) ───────────────────────────────

    pub const SEVERITY: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#severity");

    pub const MESSAGE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#message");

    pub const DEACTIVATED: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#deactivated");

    pub const NAME: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#name");

    pub const DESCRIPTION: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#description");

    pub const ORDER: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#order");

    pub const GROUP: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#group");

    // ── sh:nodeKind value IRIs ─────────────────────────────────────────────────

    pub const IRI: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#IRI");

    pub const BLANK_NODE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#BlankNode");

    pub const LITERAL: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#Literal");

    pub const BLANK_NODE_OR_IRI: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#BlankNodeOrIRI");

    pub const BLANK_NODE_OR_LITERAL: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#BlankNodeOrLiteral");

    pub const IRI_OR_LITERAL: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#IRIOrLiteral");

    // ── Unsupported constraint predicates (hard-fail set) ─────────────────────

    pub const SPARQL: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#sparql");

    pub const TARGET: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#target");

    pub const QUALIFIED_VALUE_SHAPE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#qualifiedValueShape");

    pub const QUALIFIED_MIN_COUNT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#qualifiedMinCount");

    pub const QUALIFIED_MAX_COUNT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#qualifiedMaxCount");

    pub const LESS_THAN: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#lessThan");

    pub const LESS_THAN_OR_EQUALS: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#lessThanOrEquals");

    pub const EQUALS: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#equals");

    pub const DISJOINT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#disjoint");

    pub const NOT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#not");

    pub const CLOSED: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#closed");

    pub const IGNORED_PROPERTIES: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#ignoredProperties");

    pub const LANGUAGE_IN: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#languageIn");

    pub const MAX_LENGTH: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#maxLength");

    pub const MIN_EXCLUSIVE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#minExclusive");

    pub const MAX_EXCLUSIVE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#maxExclusive");

    pub const SELECT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#select");

    // ── SHACL-AF prefix declarations (sh:prefixes / sh:declare) ───────────────

    pub const PREFIXES: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#prefixes");

    pub const DECLARE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#declare");

    pub const PREFIX: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#prefix");

    pub const NAMESPACE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#namespace");

    pub const SPARQL_CONSTRAINT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#SPARQLConstraint");

    pub const SPARQL_TARGET: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#SPARQLTarget");

    pub const SPARQL_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#SPARQLConstraintComponent");

    // ── Constraint component IRIs (sh:*ConstraintComponent) ──────────────────

    pub const MIN_COUNT_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#MinCountConstraintComponent");

    pub const MAX_COUNT_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#MaxCountConstraintComponent");

    pub const CLASS_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#ClassConstraintComponent");

    pub const DATATYPE_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#DatatypeConstraintComponent");

    pub const NODE_KIND_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#NodeKindConstraintComponent");

    pub const IN_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#InConstraintComponent");

    pub const HAS_VALUE_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#HasValueConstraintComponent");

    pub const PATTERN_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#PatternConstraintComponent");

    pub const MIN_LENGTH_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#MinLengthConstraintComponent");

    pub const UNIQUE_LANG_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#UniqueLangConstraintComponent");

    pub const MIN_INCLUSIVE_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#MinInclusiveConstraintComponent");

    pub const MAX_INCLUSIVE_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#MaxInclusiveConstraintComponent");

    pub const MIN_EXCLUSIVE_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#MinExclusiveConstraintComponent");

    pub const MAX_EXCLUSIVE_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#MaxExclusiveConstraintComponent");

    pub const AND_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#AndConstraintComponent");

    pub const OR_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#OrConstraintComponent");

    pub const XONE_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#XoneConstraintComponent");

    pub const NODE_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#NodeConstraintComponent");

    pub const REIFIER_SHAPE_CONSTRAINT_COMPONENT: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/ns/shacl#ReifierShapeConstraintComponent");
}

/// GMEOW namespace constants (`https://blackcatinformatics.ca/gmeow/`).
pub mod gmeow {
    use super::NamedNodeRef;

    pub const GRAPH_BOX_ROLE: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("https://blackcatinformatics.ca/gmeow/graphBoxRole");

    pub const BOX_ABOX: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("https://blackcatinformatics.ca/gmeow/boxABox");

    pub const BOX_TBOX: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("https://blackcatinformatics.ca/gmeow/boxTBox");

    pub const BOX_RBOX: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("https://blackcatinformatics.ca/gmeow/boxRBox");

    pub const BOX_CBOX: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("https://blackcatinformatics.ca/gmeow/boxCBox");

    pub const BOX_CONFIG_BOX: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("https://blackcatinformatics.ca/gmeow/boxConfigBox");
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

    pub const REIFIES: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies");
}

/// RDFS namespace constants.
pub mod rdfs {
    use super::NamedNodeRef;

    pub const BASE: &str = "http://www.w3.org/2000/01/rdf-schema#";

    pub const CLASS: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2000/01/rdf-schema#Class");

    pub const SUB_CLASS_OF: NamedNodeRef<'static> =
        NamedNodeRef::new_unchecked("http://www.w3.org/2000/01/rdf-schema#subClassOf");
}

/// XSD namespace base string.
pub mod xsd {
    pub const BASE: &str = "http://www.w3.org/2001/XMLSchema#";

    pub const BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

    pub const STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

    pub const INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
}
