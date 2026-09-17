// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW's registered ontology term namespaces, and the single vocabulary /
//! profile constructor every consumer builds from them.
//!
//! purrdf is a namespace-neutral toolkit: its slice catalog + ownership analyzer,
//! its slice emitters, the SHACL→JSON-Schema keying, and the JSON-LD-star
//! statement-metadata downcast all take a namespace/vocab from the CONSUMER
//! rather than baking in any ontology. GMEOW is that consumer, and it mints
//! ontology terms into **four** namespaces, not one.
//!
//! ## Why one declaration site, and why this low
//!
//! [`purrdf::SliceVocab`] distinguishes the *framework* namespace (which mints
//! `gmeow:Slice`, `gmeow:sliceTier`, `gmeow:sliceDependsOn`) from the *owned term
//! namespaces* (the namespaces a corpus's slices mint ontology terms into). A
//! namespace GMEOW mints into but never declares is invisible to ownership
//! analysis: `rdfs:isDefinedBy` claims and typed vocabulary terms whose subject
//! lies there are dropped, so every reference to those terms resolves to no
//! owning slice and contributes no dependency edge. Nothing reports this — the
//! analysis simply says the slice has no dependents and nothing depends on it.
//!
//! The `math` slice mints its entire vocabulary into `math:` and nothing into
//! `gmeow:`, so declaring only the framework namespace made every dependency on
//! `math` uncomputable. That is exactly the failure a duplicated constructor
//! hides: six call sites each said `for_namespace(GMEOW_NS)` and each was wrong
//! in the same invisible way. There is now one constructor, and it is stated
//! once — [`TERM_NAMESPACES`].
//!
//! This crate depends on `purrdf` and on no first-party crate at all, because its
//! consumers (`gmeow-validate`, `gmeow-docs`, `gmeow-slice-brief`,
//! `gmeow-pipeline`, `gmeow-dev-cli`) sit at four different heights in the crate
//! layering. A shared constructor placed in any of them would be a layering
//! inversion for the others.
//!
//! ```
//! let vocab = gmeow_ns::gmeow_slice_vocab();
//! assert!(vocab.owns_term("https://blackcatinformatics.ca/gmeow/Slice"));
//! assert!(vocab.owns_term("https://blackcatinformatics.ca/math/Quantity"));
//! assert!(!vocab.owns_term("http://www.w3.org/2002/07/owl#Class"));
//! ```

// The named-GRAPH IRIs a bundle reader addresses, alongside the term NAMESPACES it mints
// into. They belong to the same question — "what is the ONE spelling of this GMEOW IRI?" —
// and this crate is the answer to it. They used to live in `gmeow-bundle-view`, which meant
// a consumer that merely selects on a graph had to link the whole bundle read side;
// `gmeow_bundle_view::graph_iris` now re-exports this module, so every existing reference
// is unchanged and there is still exactly one definition site.
pub mod graph_iris;

use purrdf::{Namespaces, OntologyProfile, SliceVocab};

/// GMEOW's canonical ontology namespace (trailing `/` for term concatenation).
/// This is also the slice-FRAMEWORK namespace: `gmeow:Slice`, `gmeow:sliceTier`,
/// `gmeow:sliceDependsOn` and the analysis-graph terms are minted here.
pub const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// GMEOW's logic-core namespace — the canonical reasoning language.
pub const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// GMEOW's language grounding namespace (the semiotic grounding layer, peer of
/// `logic:` and `math:`; the grounding order is `logic:` < `lang:` < `math:`).
pub const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";

/// GMEOW's mathematics grounding namespace, peer of `logic:` and `lang:`.
pub const MATH_NS: &str = "https://blackcatinformatics.ca/math/";

/// **The** registered term namespaces: every namespace a GMEOW slice is allowed
/// to mint ontology terms into.
///
/// This is the single authority. `crates/validate`'s authoring gate asserts no
/// `module.ttl` / `shapes.ttl` mints a term subject outside this set, and
/// [`gmeow_slice_vocab`] hands the identical set to purrdf's ownership analyzer,
/// so "a slice may mint here" and "the analyzer can see terms minted here" are
/// the same fact rather than two facts that can drift apart.
pub const TERM_NAMESPACES: [&str; 4] = [GMEOW_NS, LOGIC_NS, LANG_NS, MATH_NS];

/// The IRI authority prefix every GMEOW-minted IRI shares.
///
/// This is the test for "GMEOW minted this IRI" as opposed to "GMEOW is
/// describing someone else's term": a `dcterms:` or `skos:` IRI redeclared in a
/// module is a foreign term GMEOW does not own, while an IRI under this authority
/// is GMEOW's own even when its namespace is not (yet) registered. Exactly that
/// second case is the invisible-slice defect, so it is what the authoring gate
/// keys on.
///
/// Every entry in [`TERM_NAMESPACES`] starts with this prefix; a registered
/// namespace that did not would mean GMEOW mints under a second authority, and
/// the unit tests below refuse it.
pub const GMEOW_AUTHORITY: &str = "https://blackcatinformatics.ca/";

/// The CURIE prefix each registered term namespace is bound to, in the order
/// emitters declare them.
pub const TERM_NAMESPACE_PREFIXES: [(&str, &str); 4] = [
    ("gmeow", GMEOW_NS),
    ("logic", LOGIC_NS),
    ("lang", LANG_NS),
    ("math", MATH_NS),
];

/// The registered term namespace `iri` lies in, or `None` if it lies in none.
///
/// The longest match wins, so a namespace nested inside another resolves to the
/// more specific one rather than to whichever happens to be tested first.
#[must_use]
pub fn registered_term_namespace(iri: &str) -> Option<&'static str> {
    TERM_NAMESPACES
        .into_iter()
        .filter(|ns| iri.starts_with(ns))
        .max_by_key(|ns| ns.len())
}

// ── Subsumption edges ───────────────────────────────────────────────────────

/// `logic:subClassOf` — **the** class-subsumption edge.
pub const LOGIC_SUB_CLASS_OF: &str = "https://blackcatinformatics.ca/logic/subClassOf";

/// `logic:subPropertyOf` — **the** property-subsumption edge.
pub const LOGIC_SUB_PROPERTY_OF: &str = "https://blackcatinformatics.ca/logic/subPropertyOf";

/// `rdfs:subClassOf` — the RDFS projection of [`LOGIC_SUB_CLASS_OF`].
pub const RDFS_SUB_CLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

/// `rdfs:subPropertyOf` — the RDFS projection of [`LOGIC_SUB_PROPERTY_OF`].
pub const RDFS_SUB_PROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";

/// Both spellings of the CLASS-subsumption edge, canonical first.
pub const SUB_CLASS_OF: [&str; 2] = [LOGIC_SUB_CLASS_OF, RDFS_SUB_CLASS_OF];

/// Both spellings of the PROPERTY-subsumption edge, canonical first.
pub const SUB_PROPERTY_OF: [&str; 2] = [LOGIC_SUB_PROPERTY_OF, RDFS_SUB_PROPERTY_OF];

// ── Property domain/range edges ─────────────────────────────────────────────

/// `logic:domain` — **the** canonical property-domain edge (declared in the
/// `logic:` slice; the reasoner lowers it to [`RDFS_DOMAIN`], exactly as
/// [`LOGIC_SUB_CLASS_OF`] lowers to [`RDFS_SUB_CLASS_OF`]).
pub const LOGIC_DOMAIN: &str = "https://blackcatinformatics.ca/logic/domain";

/// `logic:range` — **the** canonical property-range edge.
pub const LOGIC_RANGE: &str = "https://blackcatinformatics.ca/logic/range";

/// `rdfs:domain` — the RDFS projection of [`LOGIC_DOMAIN`].
pub const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";

/// `rdfs:range` — the RDFS projection of [`LOGIC_RANGE`].
pub const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";

// ── Canonical construct typing markers ──────────────────────────────────────
//
// The rdf:type objects a reader recognizes a term's kind by. Each pairs the
// canonical `logic:` spelling with its generated W3C OWL projection (Principle
// 17), exactly as [`LOGIC_SUB_CLASS_OF`] pairs with [`RDFS_SUB_CLASS_OF`]. The
// grounding correspondence law backing each pair ships in the `logic:` slice's
// `graph/correspondence-laws` corpus, and the reasoner's calculus-vocabulary
// lowering carries the same map at the EDB boundary.

/// `owl:` — the W3C OWL namespace. Every `OWL_*` projection constant below is
/// [`OWL_NS`] concatenated with the term's local name; a reader lowering a
/// canonical `logic:` marker to its OWL view reuses this rather than re-spelling
/// the literal (Principle 17: OWL is a generated projection, never authored, so
/// it is deliberately NOT one of the [`TERM_NAMESPACES`] a slice may mint into).
pub const OWL_NS: &str = "http://www.w3.org/2002/07/owl#";

/// `logic:Class` — the canonical class typing marker.
pub const LOGIC_CLASS: &str = "https://blackcatinformatics.ca/logic/Class";
/// `owl:Class` — the generated OWL projection of [`LOGIC_CLASS`].
pub const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";

/// `logic:ObjectProperty` — the canonical object-property typing marker.
pub const LOGIC_OBJECT_PROPERTY: &str = "https://blackcatinformatics.ca/logic/ObjectProperty";
/// `owl:ObjectProperty` — the generated OWL projection of [`LOGIC_OBJECT_PROPERTY`].
pub const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";

/// `logic:DatatypeProperty` — the canonical datatype-property typing marker.
pub const LOGIC_DATATYPE_PROPERTY: &str = "https://blackcatinformatics.ca/logic/DatatypeProperty";
/// `owl:DatatypeProperty` — the generated OWL projection of [`LOGIC_DATATYPE_PROPERTY`].
pub const OWL_DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";

/// `logic:AnnotationProperty` — the canonical annotation-property typing marker.
pub const LOGIC_ANNOTATION_PROPERTY: &str =
    "https://blackcatinformatics.ca/logic/AnnotationProperty";
/// `owl:AnnotationProperty` — the generated OWL projection of [`LOGIC_ANNOTATION_PROPERTY`].
pub const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";

/// `logic:NamedIndividual` — the canonical named-individual typing marker.
pub const LOGIC_NAMED_INDIVIDUAL: &str = "https://blackcatinformatics.ca/logic/NamedIndividual";
/// `owl:NamedIndividual` — the generated OWL projection of [`LOGIC_NAMED_INDIVIDUAL`].
pub const OWL_NAMED_INDIVIDUAL: &str = "http://www.w3.org/2002/07/owl#NamedIndividual";

/// `logic:Ontology` — the canonical ontology-header typing marker.
pub const LOGIC_ONTOLOGY: &str = "https://blackcatinformatics.ca/logic/Ontology";
/// `owl:Ontology` — the generated OWL projection of [`LOGIC_ONTOLOGY`].
pub const OWL_ONTOLOGY: &str = "http://www.w3.org/2002/07/owl#Ontology";

/// `logic:Thing` — the canonical universal (top) class.
pub const LOGIC_THING: &str = "https://blackcatinformatics.ca/logic/Thing";
/// `owl:Thing` — the generated OWL projection of [`LOGIC_THING`].
pub const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";

/// `logic:Nothing` — the canonical empty (bottom) class.
pub const LOGIC_NOTHING: &str = "https://blackcatinformatics.ca/logic/Nothing";
/// `owl:Nothing` — the generated OWL projection of [`LOGIC_NOTHING`].
pub const OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";

/// `logic:Restriction` — the canonical class-expression restriction marker.
pub const LOGIC_RESTRICTION: &str = "https://blackcatinformatics.ca/logic/Restriction";
/// `owl:Restriction` — the generated OWL projection of [`LOGIC_RESTRICTION`].
pub const OWL_RESTRICTION: &str = "http://www.w3.org/2002/07/owl#Restriction";

// ── Canonical header / annotation predicates ─────────────────────────────────
//
// The ontology-header and per-term lifecycle predicates. Each pairs the canonical
// `logic:` spelling authored in a slice `module.ttl` header with its generated W3C
// OWL projection, exactly as the typing markers above. A reader that scans the
// AUTHORED / canonical store (a slice header, the compiled `gmeow.gts` canonical
// graph) sees the `logic:` spelling; the generated OWL view uses the `owl:` one.
// Recognizing both is a canonical-plus-projection read, never a compat shim.

/// `logic:versionInfo` — the canonical version-annotation predicate.
pub const LOGIC_VERSION_INFO: &str = "https://blackcatinformatics.ca/logic/versionInfo";
/// `owl:versionInfo` — the generated OWL projection of [`LOGIC_VERSION_INFO`].
pub const OWL_VERSION_INFO: &str = "http://www.w3.org/2002/07/owl#versionInfo";
/// Both spellings of the version-annotation predicate, canonical first.
pub const VERSION_INFO: [&str; 2] = [LOGIC_VERSION_INFO, OWL_VERSION_INFO];

/// `logic:imports` — the canonical ontology-import predicate.
pub const LOGIC_IMPORTS: &str = "https://blackcatinformatics.ca/logic/imports";
/// `owl:imports` — the generated OWL projection of [`LOGIC_IMPORTS`].
pub const OWL_IMPORTS: &str = "http://www.w3.org/2002/07/owl#imports";
/// Both spellings of the ontology-import predicate, canonical first.
pub const IMPORTS: [&str; 2] = [LOGIC_IMPORTS, OWL_IMPORTS];

/// `logic:deprecated` — the canonical term-deprecation predicate.
pub const LOGIC_DEPRECATED: &str = "https://blackcatinformatics.ca/logic/deprecated";
/// `owl:deprecated` — the generated OWL projection of [`LOGIC_DEPRECATED`].
pub const OWL_DEPRECATED: &str = "http://www.w3.org/2002/07/owl#deprecated";
/// Both spellings of the term-deprecation predicate, canonical first.
pub const DEPRECATED: [&str; 2] = [LOGIC_DEPRECATED, OWL_DEPRECATED];

/// The `owl:` view spelling of a canonical `logic:` `rdf:type` marker — a typing marker OR a
/// property-characteristic type — or `None` when `iri` is neither.
///
/// This is the read-side counterpart of the constant pairs above: a consumer that classifies a
/// term's kind by scanning its AUTHORED `rdf:type` sees the canonical `logic:` spelling after the
/// `owl:`→`logic:` surface flip, but every such classifier (EDOAL entity-kind, correspondence
/// soundness, the mapping transform, slice peerage, …) matches against the `owl:` constants the
/// generated view uses. Without lowering the authored type first the term's kind reads as
/// indeterminate — a hard fail in the mapping lowering, a silent inert check elsewhere.
///
/// The property characteristics are NOT a pure namespace swap: the canonical spelling is lower-camel
/// (`logic:transitiveProperty`) and the `owl:` view upper-camel (`owl:TransitiveProperty`).
#[must_use]
pub fn owl_view_of_type_marker(iri: &str) -> Option<&'static str> {
    Some(match iri {
        LOGIC_CLASS => OWL_CLASS,
        LOGIC_OBJECT_PROPERTY => OWL_OBJECT_PROPERTY,
        LOGIC_DATATYPE_PROPERTY => OWL_DATATYPE_PROPERTY,
        LOGIC_ANNOTATION_PROPERTY => OWL_ANNOTATION_PROPERTY,
        LOGIC_NAMED_INDIVIDUAL => OWL_NAMED_INDIVIDUAL,
        LOGIC_ONTOLOGY => OWL_ONTOLOGY,
        LOGIC_THING => OWL_THING,
        LOGIC_NOTHING => OWL_NOTHING,
        LOGIC_RESTRICTION => OWL_RESTRICTION,
        "https://blackcatinformatics.ca/logic/transitiveProperty" => {
            "http://www.w3.org/2002/07/owl#TransitiveProperty"
        }
        "https://blackcatinformatics.ca/logic/symmetricProperty" => {
            "http://www.w3.org/2002/07/owl#SymmetricProperty"
        }
        "https://blackcatinformatics.ca/logic/functionalProperty" => {
            "http://www.w3.org/2002/07/owl#FunctionalProperty"
        }
        "https://blackcatinformatics.ca/logic/inverseFunctionalProperty" => {
            "http://www.w3.org/2002/07/owl#InverseFunctionalProperty"
        }
        "https://blackcatinformatics.ca/logic/reflexiveProperty" => {
            "http://www.w3.org/2002/07/owl#ReflexiveProperty"
        }
        "https://blackcatinformatics.ca/logic/asymmetricProperty" => {
            "http://www.w3.org/2002/07/owl#AsymmetricProperty"
        }
        "https://blackcatinformatics.ca/logic/irreflexiveProperty" => {
            "http://www.w3.org/2002/07/owl#IrreflexiveProperty"
        }
        // Axiom-annotation and disjointness AXIOM types (used in `rdf:type` object
        // position, exactly like the markers above). Their grounding-correspondence
        // laws ship in the `logic:` slice's `graph/correspondence-laws` corpus
        // (`logic:corrAllDisjointClassesGrounding` / `…Properties…` / `…AxiomGrounding`),
        // each an exact structural rename (logic:Isomorphism / ExactPreservation).
        "https://blackcatinformatics.ca/logic/AllDisjointClasses" => {
            "http://www.w3.org/2002/07/owl#AllDisjointClasses"
        }
        "https://blackcatinformatics.ca/logic/AllDisjointProperties" => {
            "http://www.w3.org/2002/07/owl#AllDisjointProperties"
        }
        "https://blackcatinformatics.ca/logic/Axiom" => "http://www.w3.org/2002/07/owl#Axiom",
        _ => return None,
    })
}

/// `true` when `iri` is a predicate whose OBJECT is a CLASS (a subsumption edge, a
/// domain/range, or a class-expression restriction filler), in EITHER the canonical
/// `logic:` spelling OR its already-projected `owl:`/`rdfs:` spelling.
///
/// A consume-time OWL/RDFS view lowers the universal class-identity markers
/// `logic:Thing` / `logic:Nothing` to `owl:Thing` / `owl:Nothing` ONLY when they sit in
/// one of these class-valued object positions. The both-spelling coverage matters
/// because a slice may author the edge in the projected surface already
/// (`rdfs:range logic:Thing`) — a mixed spelling whose `logic:Thing` filler still needs
/// the `owl:Thing` view. A NON-class predicate (a `logic:GroundingCorrespondence`'s
/// `logic:sourceEndpoint logic:Thing`, which NAMES the term as data) is deliberately
/// excluded, so the view never mints a second endpoint value.
#[must_use]
pub fn is_class_position_predicate(iri: &str) -> bool {
    matches!(
        iri,
        LOGIC_SUB_CLASS_OF
            | RDFS_SUB_CLASS_OF
            | LOGIC_DOMAIN
            | RDFS_DOMAIN
            | LOGIC_RANGE
            | RDFS_RANGE
            | "https://blackcatinformatics.ca/logic/onClass"
            | "http://www.w3.org/2002/07/owl#onClass"
            | "https://blackcatinformatics.ca/logic/someValuesFrom"
            | "http://www.w3.org/2002/07/owl#someValuesFrom"
            | "https://blackcatinformatics.ca/logic/allValuesFrom"
            | "http://www.w3.org/2002/07/owl#allValuesFrom"
            | "https://blackcatinformatics.ca/logic/equivalentClass"
            | "http://www.w3.org/2002/07/owl#equivalentClass"
            | "https://blackcatinformatics.ca/logic/disjointWith"
            | "http://www.w3.org/2002/07/owl#disjointWith"
            | "https://blackcatinformatics.ca/logic/complementOf"
            | "http://www.w3.org/2002/07/owl#complementOf"
    )
}

/// The OWL/RDFS **view** spelling of a canonical `logic:` PREDICATE (an axiom,
/// class-expression, restriction, cardinality, individual-identity, or
/// ontology-header edge), or `None` when `iri` is not a projected `logic:`
/// predicate.
///
/// This is the predicate-position counterpart of [`owl_view_of_type_marker`]
/// (which lowers `rdf:type` OBJECTS). Every entry here mirrors ONE
/// `logic:GroundingCorrespondence` in the `logic:` slice's
/// `graph/correspondence-laws` corpus (`logic:sourceEndpoint` → `logic:targetEndpoint`),
/// each an exact structural rename under a `logic:InstitutionMorphism` carrying
/// `logic:ExactPreservation`. The subsumption and domain/range edges project onto
/// the **RDFS** surface (`rdfs:subClassOf`, `rdfs:domain`, …) per their
/// correspondence laws; every other edge projects onto the **OWL** surface.
///
/// Like the subsumption projection, a consumer that materializes this view KEEPS
/// the canonical `logic:` edge and ADDS the projected one — it never rewrites the
/// authored edge away (Principle 17: the OWL/RDFS surface is a generated view of
/// the canonical `logic:` truth, not a replacement for it).
#[must_use]
pub fn owl_view_of_predicate(iri: &str) -> Option<&'static str> {
    Some(match iri {
        // Subsumption + domain/range → RDFS surface.
        LOGIC_SUB_CLASS_OF => RDFS_SUB_CLASS_OF,
        LOGIC_SUB_PROPERTY_OF => RDFS_SUB_PROPERTY_OF,
        LOGIC_DOMAIN => RDFS_DOMAIN,
        LOGIC_RANGE => RDFS_RANGE,
        // Ontology-header / lifecycle → OWL surface (constants already paired above).
        LOGIC_VERSION_INFO => OWL_VERSION_INFO,
        LOGIC_IMPORTS => OWL_IMPORTS,
        LOGIC_DEPRECATED => OWL_DEPRECATED,
        // Class-expression / axiom edges → OWL surface.
        "https://blackcatinformatics.ca/logic/equivalentClass" => {
            "http://www.w3.org/2002/07/owl#equivalentClass"
        }
        "https://blackcatinformatics.ca/logic/equivalentProperty" => {
            "http://www.w3.org/2002/07/owl#equivalentProperty"
        }
        "https://blackcatinformatics.ca/logic/disjointWith" => {
            "http://www.w3.org/2002/07/owl#disjointWith"
        }
        "https://blackcatinformatics.ca/logic/propertyDisjointWith" => {
            "http://www.w3.org/2002/07/owl#propertyDisjointWith"
        }
        "https://blackcatinformatics.ca/logic/disjointUnionOf" => {
            "http://www.w3.org/2002/07/owl#disjointUnionOf"
        }
        "https://blackcatinformatics.ca/logic/inverseOf" => {
            "http://www.w3.org/2002/07/owl#inverseOf"
        }
        "https://blackcatinformatics.ca/logic/complementOf" => {
            "http://www.w3.org/2002/07/owl#complementOf"
        }
        "https://blackcatinformatics.ca/logic/unionOf" => "http://www.w3.org/2002/07/owl#unionOf",
        "https://blackcatinformatics.ca/logic/intersectionOf" => {
            "http://www.w3.org/2002/07/owl#intersectionOf"
        }
        "https://blackcatinformatics.ca/logic/oneOf" => "http://www.w3.org/2002/07/owl#oneOf",
        "https://blackcatinformatics.ca/logic/members" => "http://www.w3.org/2002/07/owl#members",
        "https://blackcatinformatics.ca/logic/hasKey" => "http://www.w3.org/2002/07/owl#hasKey",
        "https://blackcatinformatics.ca/logic/propertyChainAxiom" => {
            "http://www.w3.org/2002/07/owl#propertyChainAxiom"
        }
        // Individual identity → OWL surface.
        "https://blackcatinformatics.ca/logic/sameAs" => "http://www.w3.org/2002/07/owl#sameAs",
        "https://blackcatinformatics.ca/logic/differentFrom" => {
            "http://www.w3.org/2002/07/owl#differentFrom"
        }
        // Property-restriction edges → OWL surface.
        "https://blackcatinformatics.ca/logic/onProperty" => {
            "http://www.w3.org/2002/07/owl#onProperty"
        }
        "https://blackcatinformatics.ca/logic/onClass" => "http://www.w3.org/2002/07/owl#onClass",
        "https://blackcatinformatics.ca/logic/onDataRange" => {
            "http://www.w3.org/2002/07/owl#onDataRange"
        }
        "https://blackcatinformatics.ca/logic/onDatatype" => {
            "http://www.w3.org/2002/07/owl#onDatatype"
        }
        "https://blackcatinformatics.ca/logic/withRestrictions" => {
            "http://www.w3.org/2002/07/owl#withRestrictions"
        }
        "https://blackcatinformatics.ca/logic/someValuesFrom" => {
            "http://www.w3.org/2002/07/owl#someValuesFrom"
        }
        "https://blackcatinformatics.ca/logic/allValuesFrom" => {
            "http://www.w3.org/2002/07/owl#allValuesFrom"
        }
        "https://blackcatinformatics.ca/logic/hasValue" => "http://www.w3.org/2002/07/owl#hasValue",
        "https://blackcatinformatics.ca/logic/hasSelf" => "http://www.w3.org/2002/07/owl#hasSelf",
        // Cardinality family → OWL surface.
        "https://blackcatinformatics.ca/logic/cardinality" => {
            "http://www.w3.org/2002/07/owl#cardinality"
        }
        "https://blackcatinformatics.ca/logic/minCardinality" => {
            "http://www.w3.org/2002/07/owl#minCardinality"
        }
        "https://blackcatinformatics.ca/logic/maxCardinality" => {
            "http://www.w3.org/2002/07/owl#maxCardinality"
        }
        "https://blackcatinformatics.ca/logic/qualifiedCardinality" => {
            "http://www.w3.org/2002/07/owl#qualifiedCardinality"
        }
        "https://blackcatinformatics.ca/logic/minQualifiedCardinality" => {
            "http://www.w3.org/2002/07/owl#minQualifiedCardinality"
        }
        "https://blackcatinformatics.ca/logic/maxQualifiedCardinality" => {
            "http://www.w3.org/2002/07/owl#maxQualifiedCardinality"
        }
        _ => return None,
    })
}

/// [`owl_view_of_type_marker`] as a total function: the `owl:` view spelling of a canonical
/// `logic:` `rdf:type` marker, or `iri` returned unchanged (an already-`owl:` term, a domain
/// class, or a gUFO sort passes through).
#[must_use]
pub fn to_owl_view(iri: &str) -> &str {
    owl_view_of_type_marker(iri).unwrap_or(iri)
}

/// **The** definition of "this triple asserts a subsumption edge": every predicate
/// a reader must scan to see the whole authored taxonomy, in a fixed order —
/// canonical class, projected class, canonical property, projected property.
///
/// ## Why two spellings, and why that is not a compat shim
///
/// Principle 17 makes `logic:` the CANONICAL vocabulary and `rdfs:` one of its
/// lossy projections, so a reader of the *authored* surface has to accept both:
/// the canonical spelling because it is the truth, and the projected spelling
/// because parts of the corpus have not been re-authored into the canonical one
/// yet. Reading only `rdfs:` blinds a consumer to every re-authored term (it sees
/// an empty taxonomy and reports no error); reading only `logic:` blinds it to
/// every term not yet converted. This is a canonical-plus-projection read, not a
/// backwards-compatibility arm.
///
/// ## Terminal condition — when the `rdfs:` arm is deleted
///
/// The `rdfs:` entries are TRANSITIONAL RESIDUE. They are live only while the
/// `rdfs` projection ceilings in the slice-quality ratchet are nonzero, i.e. only
/// while some slice still authors a raw `rdfs:subClassOf` / `rdfs:subPropertyOf`
/// edge. The ratchet drives those ceilings monotonically down; when they reach 0
/// corpus-wide, no authored surface spells a subsumption edge in `rdfs:` any more,
/// [`RDFS_SUB_CLASS_OF`] / [`RDFS_SUB_PROPERTY_OF`] become dead code here, and
/// this accessor collapses to the two `logic:` entries. Deleting the `rdfs:` arm
/// is then a required cleanup, not an optional one.
///
/// ```
/// let preds = gmeow_ns::subsumption_predicates();
/// assert_eq!(preds[0], "https://blackcatinformatics.ca/logic/subClassOf");
/// assert_eq!(preds.len(), 4);
/// ```
#[must_use]
pub fn subsumption_predicates() -> [&'static str; 4] {
    [
        SUB_CLASS_OF[0],
        SUB_CLASS_OF[1],
        SUB_PROPERTY_OF[0],
        SUB_PROPERTY_OF[1],
    ]
}

/// GMEOW's single ontology profile: the `gmeow:` primary namespace plus the
/// authored `logic:`, `lang:`, and `math:` prefixes. purrdf's builtins
/// (xsd/rdf/rdfs/owl/sh) are always available on top of these, so the profile only
/// carries GMEOW's own vocab.
#[must_use]
pub fn gmeow_profile() -> OntologyProfile {
    OntologyProfile::for_namespace(GMEOW_NS)
        .with_prefix("gmeow")
        .with_prefixes(
            TERM_NAMESPACE_PREFIXES
                .into_iter()
                .map(|(prefix, ns)| (prefix.to_owned(), ns.to_owned()))
                .collect(),
        )
}

/// **The** slice vocabulary: prefix `gmeow`, framework namespace [`GMEOW_NS`],
/// and all four [`TERM_NAMESPACES`] declared as owned term namespaces.
///
/// Every `SliceCatalog::discover` / `OwnershipAnalyzer` construction in the
/// workspace passes this. Constructing a `SliceVocab` any other way re-opens the
/// invisible-namespace hole described at the module level.
#[must_use]
pub fn gmeow_slice_vocab() -> SliceVocab {
    gmeow_profile()
        .slice_vocab()
        .with_term_namespaces(TERM_NAMESPACES)
}

/// The SHACL→JSON-Schema keying namespaces (GMEOW primary + authored prefixes).
///
/// Construction cannot fail: the `gmeow` primary prefix is always declared by
/// [`gmeow_profile`].
///
/// # Panics
///
/// Never in practice — the expectation documents an invariant of
/// [`gmeow_profile`], not a runtime condition.
#[must_use]
pub fn gmeow_json_schema_namespaces() -> Namespaces {
    gmeow_profile()
        .namespaces()
        .expect("gmeow primary prefix is declared in gmeow_profile")
}

#[path = "lib.tests.rs"]
#[cfg(test)]
mod tests;
