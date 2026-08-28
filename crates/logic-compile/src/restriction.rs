// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `logic:` restriction skolemizer — the single lift routine behind the
//! `isSupersetOf` round-trip.
//!
//! A `logic:`-authored class-expression restriction (`C logic:subClassOf
//! [ logic:onProperty P ; logic:someValuesFrom D ]`) must normalize to a flat
//! [`crate::ir::LogicAxiom`] set that survives the OWL projection and reparse
//! round-trip (the IR-isomorphism gate `adapter::assert_ir_isomorphic`).  The anonymous
//! restriction node is the only obstacle: a raw blank-node label is per-parse and would
//! never collide across a project → reparse cycle.
//!
//! The fix mirrors the covering projection: mint a **deterministic, content-addressed
//! IRI** `logic:restriction/<sha256_12(content_key)>` for the restriction node, where
//! `content_key` is a canonical function of the restriction's *meaning only*
//! (`onProperty` + its constraints), so identical meaning ⇒ identical skolem IRI ⇒
//! identical axiom set (structure sharing + a stable round-trip).
//!
//! Every lifted restriction becomes flat `(subject, predicate, obj)` triples in the
//! program's `axioms` — never a new collection — so a restriction-free program's
//! axiom vector and content key are byte-identical (the append-only discipline).

use std::collections::{BTreeMap, BTreeSet};

use purrdf::RdfDataset;

use crate::frontend::{Diagnostic, Severity};
use crate::graphutil::{
    Node, Subject, default_graph_quads, objects, sha256_12, subject_str, subjects_with,
    term_is_literal, term_str, value,
};
use crate::ir::LOGIC_NAMESPACE;

/// The `\u{0}` field separator that pins the restriction `content_key` (matches the
/// IR `sort_key` separator so the byte form is consistent across the compiler).
const SEP: char = '\u{0}';

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The XSD namespace the constraining facets ([`FACET_LOCALS`]) live under.  Facets are
/// `xsd:`-namespaced on BOTH the `owl:` and the `logic:` authoring surfaces (unlike the
/// property-restriction constraints, whose local names are shared between the two
/// namespaces), so they are keyed and emitted on their FULL `xsd:` IRI.
const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema#";
/// The `rdfs:Datatype` type IRI a datatype-restriction (datarange) node carries.
const RDFS_DATATYPE: &str = "http://www.w3.org/2000/01/rdf-schema#Datatype";

/// The `logic:` local name of the restriction class (`_:r rdf:type logic:Restriction`).
pub(crate) const RESTRICTION_CLASS_LOCAL: &str = "Restriction";
/// The `logic:` local name of the property slot (`_:r logic:onProperty P`).
pub(crate) const ON_PROPERTY_LOCAL: &str = "onProperty";
/// The `logic:` local name of the enumeration class (`_:e rdf:type logic:Enumeration`).
pub(crate) const ENUMERATION_CLASS_LOCAL: &str = "Enumeration";
/// The `logic:` local name of the enumeration membership predicate (`_:e logic:oneOf m`).
pub(crate) const ONE_OF_LOCAL: &str = "oneOf";
/// The `logic:` local name of the datarange class (`_:d rdf:type logic:Datarange`) —
/// the lifted IR type of an `owl:withRestrictions` datatype restriction.
pub(crate) const DATARANGE_CLASS_LOCAL: &str = "Datarange";
/// The `logic:`/`owl:` local name of the base-datatype slot (`_:d <ns>onDatatype D`).
pub(crate) const ON_DATATYPE_LOCAL: &str = "onDatatype";
/// The `logic:`/`owl:` local name of the facet-list slot (`_:d <ns>withRestrictions ( … )`).
pub(crate) const WITH_RESTRICTIONS_LOCAL: &str = "withRestrictions";

/// The XSD constraining-facet local names a datatype restriction may carry, one per
/// facet cell in its `withRestrictions` list (`[ xsd:minInclusive "0.0"^^xsd:decimal ]`).
/// These are `xsd:`-namespaced verbatim on both authoring surfaces, so they are keyed and
/// re-emitted on their full `xsd:` IRI (see [`XSD_NS`]).
pub(crate) const FACET_LOCALS: &[&str] = &[
    "minInclusive",
    "maxInclusive",
    "minExclusive",
    "maxExclusive",
    "minLength",
    "maxLength",
    "length",
    "pattern",
    "langRange",
    "totalDigits",
    "fractionDigits",
];

const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

/// The single-valued restriction *constraint* predicates handled by the lift, as
/// local names (shared verbatim by the `owl:` and `logic:` namespaces).  A restriction
/// node carries `onProperty` plus one or more of these:
///
/// * value constraints — `someValuesFrom`, `allValuesFrom`, `hasValue`;
/// * unqualified cardinality — `minCardinality`, `maxCardinality`, `cardinality`;
/// * qualified cardinality — `qualifiedCardinality`, `minQualifiedCardinality`,
///   `maxQualifiedCardinality` (the OWL 2 standard local names), each paired with
///   `onClass` (a class filler) or `onDataRange` (a datatype filler).
///
/// Each is a single object (IRI filler or literal count), so the generic
/// [`collect_constraints`] walk lifts them uniformly.  `owl:oneOf` is a multi-valued
/// class enumeration (no `onProperty`) and is out of the property-restriction surface.
pub(crate) const CONSTRAINT_LOCALS: &[&str] = &[
    "someValuesFrom",
    "allValuesFrom",
    "hasValue",
    "minCardinality",
    "maxCardinality",
    "cardinality",
    "qualifiedCardinality",
    "minQualifiedCardinality",
    "maxQualifiedCardinality",
    "onClass",
    "onDataRange",
];

/// The cardinality-count constraint locals — their filler is a non-negative integer,
/// which the OWL projection re-emits as an `xsd:nonNegativeInteger`-typed literal.
pub(crate) const CARDINALITY_LOCALS: &[&str] = &[
    "minCardinality",
    "maxCardinality",
    "cardinality",
    "qualifiedCardinality",
    "minQualifiedCardinality",
    "maxQualifiedCardinality",
];

/// The source vocabulary a [`skolemize_restrictions`] pass reads — the canonical
/// `logic:` surface.  The constraint / property-slot / type local names are shared
/// verbatim with the OWL projection vocabulary; the namespace and the two anchoring
/// predicates (`subClassOf` / `equivalentClass`) are carried here so the skolemizer
/// stays parameterized over them.
pub(crate) struct RestrictionVocab {
    /// Namespace of the restriction predicates + class (`logic:`).
    ns: String,
    /// The class→restriction anchor (`logic:subClassOf`).
    sub_class_of: String,
    /// The equivalence anchor (`logic:equivalentClass`).
    equivalent_class: String,
}

impl RestrictionVocab {
    /// The canonical `logic:` source vocabulary (the front-end path).
    pub(crate) fn logic() -> Self {
        Self {
            ns: LOGIC_NAMESPACE.to_owned(),
            sub_class_of: format!("{LOGIC_NAMESPACE}subClassOf"),
            equivalent_class: format!("{LOGIC_NAMESPACE}equivalentClass"),
        }
    }

    fn iri(&self, local: &str) -> String {
        format!("{}{local}", self.ns)
    }
}

/// A flat lifted triple, ready to become a [`crate::ir::LogicAxiom`] by either caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiftedTriple {
    pub subject: String,
    pub predicate: String,
    pub obj: String,
    pub obj_is_literal: bool,
}

fn logic(local: &str) -> String {
    format!("{LOGIC_NAMESPACE}{local}")
}

/// A collected single constraint on a restriction node.
struct Constraint {
    /// The shared local name (e.g. `someValuesFrom`); emitted as `logic:<local>`.
    local: String,
    /// The filler / value (IRI or literal lexical form).
    value: String,
    /// Whether `value` is a literal (`owl:hasValue` of a data value).
    is_literal: bool,
    /// Whether the filler is itself an anonymous node (a nested class / datatype
    /// expression, e.g. `someValuesFrom [ owl:unionOf … ]` or a `withRestrictions`
    /// datarange).  Such fillers have no stable identity here and take the restriction
    /// to the fail-soft disclosure path — the documented anonymous-nested boundary.
    is_blank: bool,
}

/// The mint key contribution of a constraint: `<local>=<value>[|lit]`.  Sorting these
/// (in [`content_key`]) makes a multi-constraint node's id order-independent.
fn constraint_key(c: &Constraint) -> String {
    if c.is_literal {
        format!("{}={}|lit", c.local, c.value)
    } else {
        format!("{}={}", c.local, c.value)
    }
}

/// The frozen restriction `content_key` — a canonical function of meaning only.
///
/// Format (do NOT reorder — it pins the skolem IRI, so any change re-mints every
/// restriction and diverges the OWL/`logic:` isomorphism): `onProperty=<P>` then, for
/// each constraint sorted by its [`constraint_key`], `␀<local>=<value>[|lit]`.  The
/// subject class is deliberately EXCLUDED, so two classes bearing an identical
/// restriction share one node.
fn content_key(on_property: &str, constraints: &[Constraint]) -> String {
    let mut keys: Vec<String> = constraints.iter().map(constraint_key).collect();
    keys.sort();
    let mut base = format!("onProperty={on_property}");
    for k in keys {
        base.push(SEP);
        base.push_str(&k);
    }
    base
}

/// The deterministic skolem IRI for a restriction with the given `content_key`.
fn skolem_iri(content_key: &str) -> String {
    format!("{LOGIC_NAMESPACE}restriction/{}", sha256_12(content_key))
}

/// Every subject that is a restriction node under `vocab`: it either carries the
/// property slot (`<ns>onProperty`) or is typed `<ns>Restriction`.  Returned as the
/// set of subject *labels* (`subject_str`) so the generic extractors can cheaply skip
/// restriction-internal triples, plus the resolved [`Subject`]s for querying.
fn restriction_nodes(store: &RdfDataset, vocab: &RestrictionVocab) -> Vec<Subject> {
    let on_property = vocab.iri(ON_PROPERTY_LOCAL);
    let restriction_ty = vocab.iri(RESTRICTION_CLASS_LOCAL);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<Subject> = Vec::new();
    for q in default_graph_quads(store) {
        let is_node = q.predicate.as_str() == on_property
            || (q.predicate.as_str() == RDF_TYPE && term_str(&q.object) == restriction_ty);
        if is_node && seen.insert(subject_str(&q.subject)) {
            out.push(q.subject.clone());
        }
    }
    out
}

/// The set of restriction-node labels (for the generic extractors' skip filter).
pub(crate) fn restriction_node_labels(
    store: &RdfDataset,
    vocab: &RestrictionVocab,
) -> BTreeSet<String> {
    restriction_nodes(store, vocab)
        .iter()
        .map(subject_str)
        .collect()
}

/// Collect a restriction node's single-valued constraints (the [`CONSTRAINT_LOCALS`]
/// set).  Cardinality-count constraints are part of [`CONSTRAINT_LOCALS`] and lift
/// here; `owl:oneOf` class enumerations are a separate multi-valued construct handled
/// by [`skolemize_enumerations`].
fn collect_constraints(
    store: &RdfDataset,
    node: &Subject,
    vocab: &RestrictionVocab,
) -> Vec<Constraint> {
    let mut constraints: Vec<Constraint> = Vec::new();
    for local in CONSTRAINT_LOCALS {
        let pred = crate::graphutil::nn(&vocab.iri(local));
        for obj in objects(store, node, &pred) {
            constraints.push(Constraint {
                local: (*local).to_owned(),
                value: term_str(&obj),
                is_literal: term_is_literal(&obj),
                is_blank: crate::graphutil::term_is_blank(&obj),
            });
        }
    }
    constraints
}

/// Lift every OWL/`logic:` restriction under `vocab` into flat skolem-keyed
/// [`LiftedTriple`]s, appending a surfaced [`Diagnostic`] for any malformed node
/// (typed a restriction but missing `onProperty` or carrying no constraint) so nothing
/// is silently lost.  A well-formed restriction contributes: the `logic:subClassOf` /
/// `logic:equivalentClass` anchor edge(s) redirected to the skolem node, the
/// `rdf:type logic:Restriction` typing, the `logic:onProperty` slot, and one axiom per
/// constraint.
pub(crate) fn skolemize_restrictions(
    store: &RdfDataset,
    vocab: &RestrictionVocab,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<LiftedTriple> {
    let on_property_pred = crate::graphutil::nn(&vocab.iri(ON_PROPERTY_LOCAL));
    let sub_class_of_pred = crate::graphutil::nn(&vocab.sub_class_of);
    let equivalent_class_pred = crate::graphutil::nn(&vocab.equivalent_class);
    let mut out: Vec<LiftedTriple> = Vec::new();

    for node in restriction_nodes(store, vocab) {
        let node_label = subject_str(&node);
        let on_properties = objects(store, &node, &on_property_pred);
        let on_property = match on_properties.as_slice() {
            // No onProperty is authored-input malformedness: disclose and skip.
            [] => {
                diagnostics.push(warn(
                    "MALFORMED_RESTRICTION",
                    format!(
                        "restriction node {node_label:?} is typed a restriction but has no \
                         onProperty; skipped"
                    ),
                    Some(node_label),
                ));
                continue;
            }
            // Two or more onProperty values is a wiring contradiction, not a
            // disclosable malformedness — pick-first would silently drop a slot, so
            // hard-fail (the No-optionality invariant).
            [_, _, ..] => panic!(
                "restriction node {node_label:?} has {} onProperty values (a \
                 single-property restriction is required)",
                on_properties.len()
            ),
            [only] => term_str(only),
        };
        let constraints = collect_constraints(store, &node, vocab);
        if constraints.is_empty() {
            diagnostics.push(warn(
                "MALFORMED_RESTRICTION",
                format!(
                    "restriction node {node_label:?} on {on_property:?} has no recognized \
                     value/cardinality constraint; skipped"
                ),
                Some(node_label),
            ));
            continue;
        }
        // A filler that is itself an anonymous class / datatype expression has no stable
        // identity to lift — disclose and skip the whole restriction (the documented
        // anonymous-nested-filler boundary; nested class expressions are the covering /
        // datarange machinery's concern, not the property-restriction lift).
        if constraints.iter().any(|c| c.is_blank) {
            diagnostics.push(warn(
                "UNSUPPORTED_NESTED_RESTRICTION",
                format!(
                    "restriction node {node_label:?} on {on_property:?} has an anonymous \
                     class/datatype filler (nested class expression); not lifted"
                ),
                Some(node_label),
            ));
            continue;
        }

        let skolem = skolem_iri(&content_key(&on_property, &constraints));

        // Restriction internals.
        out.push(LiftedTriple {
            subject: skolem.clone(),
            predicate: RDF_TYPE.to_owned(),
            obj: logic(RESTRICTION_CLASS_LOCAL),
            obj_is_literal: false,
        });
        out.push(LiftedTriple {
            subject: skolem.clone(),
            predicate: logic(ON_PROPERTY_LOCAL),
            obj: on_property.clone(),
            obj_is_literal: false,
        });
        for c in &constraints {
            out.push(LiftedTriple {
                subject: skolem.clone(),
                predicate: logic(&c.local),
                obj: c.value.clone(),
                obj_is_literal: c.is_literal,
            });
        }

        // Anchor edges (class → restriction), redirected from the blank/IRI node to
        // the skolem node.  A restriction with no anchor is a free-floating class
        // expression (not authored today); it still lifts its internals.
        let node_term = subject_as_object(&node);
        for anchor in subjects_with(store, &sub_class_of_pred, &node_term) {
            out.push(LiftedTriple {
                subject: subject_str(&anchor),
                predicate: logic("subClassOf"),
                obj: skolem.clone(),
                obj_is_literal: false,
            });
        }
        for anchor in subjects_with(store, &equivalent_class_pred, &node_term) {
            out.push(LiftedTriple {
                subject: subject_str(&anchor),
                predicate: logic("equivalentClass"),
                obj: skolem.clone(),
                obj_is_literal: false,
            });
        }
    }

    out
}

// --------------------------------------------------------------------------- //
// Class enumerations (owl:oneOf)
// --------------------------------------------------------------------------- //

/// The outcome of walking an `rdf:first`/`rdf:rest`/`rdf:nil` list.
enum ListWalk {
    /// A well-formed list: every cell carried `rdf:first` and the walk reached `rdf:nil`.
    Complete(Vec<Node>),
    /// A corrupt list — the walk hit the named defect before reaching `rdf:nil`.
    Malformed(&'static str),
}

/// Walk an `rdf:first`/`rdf:rest`/`rdf:nil` list from `head`, returning its members in
/// order when the list is well-formed, or the defect that broke the walk.  A cell with
/// no `rdf:first` (a hole), a cell with no `rdf:rest` (not nil-terminated), a non-resource
/// cell, and a cycle (a revisited node) are each surfaced as [`ListWalk::Malformed`]
/// rather than silently truncating the member set — the caller discloses and skips.
fn rdf_list(store: &RdfDataset, head: &Node) -> ListWalk {
    let first = crate::graphutil::nn(RDF_FIRST);
    let rest = crate::graphutil::nn(RDF_REST);
    let mut out: Vec<Node> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut cursor = head.clone();
    loop {
        let Some(node) = crate::graphutil::term_as_subject(&cursor) else {
            return ListWalk::Malformed("a list cell is a literal, not a resource");
        };
        if subject_str(&node) == RDF_NIL {
            return ListWalk::Complete(out);
        }
        if !seen.insert(subject_str(&node)) {
            return ListWalk::Malformed("list is cyclic");
        }
        match value(store, &node, &first) {
            Some(item) => out.push(item),
            None => return ListWalk::Malformed("a list cell has no rdf:first"),
        }
        match value(store, &node, &rest) {
            Some(next) => cursor = next,
            None => return ListWalk::Malformed("list is not nil-terminated"),
        }
    }
}

/// Every ANONYMOUS class enumeration under `vocab`: a blank subject carrying
/// `<ns>oneOf` (the `[ owl:oneOf ( … ) ]` form anchored via `equivalentClass` /
/// `subClassOf`).  Scoped to blank nodes because such a node holds ONLY enumeration
/// internals, so the generic extractors can skip it wholesale (as they do restriction
/// nodes) without dropping unrelated axioms.  A NAMED class carrying `oneOf` may also
/// carry ordinary domain axioms, so it stays on the fail-soft disclosure path — the
/// same anonymous-vs-named boundary the nested-filler case draws.
fn enumeration_nodes(store: &RdfDataset, vocab: &RestrictionVocab) -> Vec<Subject> {
    let one_of = vocab.iri(ONE_OF_LOCAL);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<Subject> = Vec::new();
    for q in default_graph_quads(store) {
        if q.predicate.as_str() == one_of
            && matches!(q.subject, Subject::Blank { .. })
            && seen.insert(subject_str(&q.subject))
        {
            out.push(q.subject.clone());
        }
    }
    out
}

/// The set of enumeration-node labels (for the generic extractors' skip filter).
pub(crate) fn enumeration_node_labels(
    store: &RdfDataset,
    vocab: &RestrictionVocab,
) -> BTreeSet<String> {
    enumeration_nodes(store, vocab)
        .iter()
        .map(subject_str)
        .collect()
}

/// Lift every `owl:oneOf` class enumeration under `vocab` into flat `logic:oneOf`
/// axioms on a stable node — a named class keeps its own IRI; an anonymous enumeration
/// is content-addressed as `logic:enumeration/<hash>` (members sorted + deduped, so the
/// id is order-independent and owl:/logic: authoring collide).  Members are emitted as
/// individual `logic:oneOf` axioms (no RDF-list reification in the IR).
///
/// Enumerations are a CLOSED-world construct the `logic:` layer treats as a projection
/// artifact (gmeow's own slices never author them — that policy is enforced elsewhere);
/// the adapter lifts them only so external OWL round-trips faithfully.
pub(crate) fn skolemize_enumerations(
    store: &RdfDataset,
    vocab: &RestrictionVocab,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<LiftedTriple> {
    let one_of_pred = crate::graphutil::nn(&vocab.iri(ONE_OF_LOCAL));
    let sub_class_of_pred = crate::graphutil::nn(&vocab.sub_class_of);
    let equivalent_class_pred = crate::graphutil::nn(&vocab.equivalent_class);
    let mut out: Vec<LiftedTriple> = Vec::new();

    for node in enumeration_nodes(store, vocab) {
        let node_label = subject_str(&node);
        let Some(list_head) = value(store, &node, &one_of_pred) else {
            continue;
        };
        let members = match rdf_list(store, &list_head) {
            ListWalk::Complete(members) => members,
            ListWalk::Malformed(why) => {
                diagnostics.push(warn(
                    "MALFORMED_ENUMERATION",
                    format!("enumeration {node_label:?} has a corrupt oneOf list ({why}); skipped"),
                    Some(node_label),
                ));
                continue;
            }
        };
        if members.is_empty() {
            diagnostics.push(warn(
                "MALFORMED_ENUMERATION",
                format!("enumeration {node_label:?} has an empty oneOf list; skipped"),
                Some(node_label),
            ));
            continue;
        }
        // Anonymous members have no stable identity — disclose and skip (a oneOf of
        // blank nodes is not a well-formed nominal enumeration).
        if members.iter().any(crate::graphutil::term_is_blank) {
            diagnostics.push(warn(
                "UNSUPPORTED_NESTED_ENUMERATION",
                format!("enumeration {node_label:?} has an anonymous member; not lifted"),
                Some(node_label),
            ));
            continue;
        }
        // (value, is_literal) members, sorted + deduped for a stable content key.
        let mut mem: Vec<(String, bool)> = members
            .iter()
            .map(|m| (term_str(m), term_is_literal(m)))
            .collect();
        mem.sort();
        mem.dedup();

        // Content-address the anonymous enumeration (members only → order-independent,
        // and owl:/logic: authoring collide on the same node).
        let enum_node = format!(
            "{LOGIC_NAMESPACE}enumeration/{}",
            sha256_12(&enumeration_content_key(&mem))
        );

        out.push(LiftedTriple {
            subject: enum_node.clone(),
            predicate: RDF_TYPE.to_owned(),
            obj: logic(ENUMERATION_CLASS_LOCAL),
            obj_is_literal: false,
        });
        for (member, is_literal) in &mem {
            out.push(LiftedTriple {
                subject: enum_node.clone(),
                predicate: logic(ONE_OF_LOCAL),
                obj: member.clone(),
                obj_is_literal: *is_literal,
            });
        }

        // Redirect the enumeration's anchor edges to the skolem node.
        let node_term = subject_as_object(&node);
        for anchor in subjects_with(store, &sub_class_of_pred, &node_term) {
            out.push(LiftedTriple {
                subject: subject_str(&anchor),
                predicate: logic("subClassOf"),
                obj: enum_node.clone(),
                obj_is_literal: false,
            });
        }
        for anchor in subjects_with(store, &equivalent_class_pred, &node_term) {
            out.push(LiftedTriple {
                subject: subject_str(&anchor),
                predicate: logic("equivalentClass"),
                obj: enum_node.clone(),
                obj_is_literal: false,
            });
        }
    }

    out
}

/// The frozen enumeration content key — `oneOf=<m1>[|lit],<m2>[|lit],…` over the sorted,
/// deduped member list.  Do NOT reorder: it pins the anonymous-enumeration skolem IRI.
fn enumeration_content_key(members: &[(String, bool)]) -> String {
    let parts: Vec<String> = members
        .iter()
        .map(|(m, lit)| if *lit { format!("{m}|lit") } else { m.clone() })
        .collect();
    format!("oneOf={}", parts.join(","))
}

// --------------------------------------------------------------------------- //
// Datatype restrictions (owl:withRestrictions dataranges)
// --------------------------------------------------------------------------- //

/// A single constraining facet lifted off a datarange's `withRestrictions` list: the
/// full `xsd:` facet IRI and its literal value (`xsd:minInclusive "0.0"`).  A facet value
/// is always a literal, so no `is_literal` flag is carried.
struct Facet {
    /// The full `xsd:` constraining-facet IRI (e.g. `…XMLSchema#minInclusive`).
    iri: String,
    /// The facet value's literal lexical form.
    value: String,
}

/// The mint-key contribution of a facet: `<facetIRI>=<value>|lit`.  Facet values are
/// always literals, so the `|lit` tag is unconditional — it keeps a facet key distinct in
/// shape from an `onDatatype=<IRI>` key.
fn facet_key(f: &Facet) -> String {
    format!("{}={}|lit", f.iri, f.value)
}

/// The frozen datarange `content_key` — a canonical function of meaning only.
///
/// Format (do NOT reorder — it pins the skolem IRI): `onDatatype=<D>` then, for each facet
/// sorted by its [`facet_key`], `␀<facetIRI>=<value>|lit`.  Two identical dataranges (and
/// an `owl:`- and `logic:`-authored twin) collapse to one node.
fn datarange_content_key(on_datatype: &str, facets: &[Facet]) -> String {
    let mut keys: Vec<String> = facets.iter().map(facet_key).collect();
    keys.sort();
    let mut base = format!("onDatatype={on_datatype}");
    for k in keys {
        base.push(SEP);
        base.push_str(&k);
    }
    base
}

/// The deterministic skolem IRI for a datarange with the given `content_key`.
fn datarange_skolem_iri(content_key: &str) -> String {
    format!("{LOGIC_NAMESPACE}datarange/{}", sha256_12(content_key))
}

/// Every datatype-restriction (datarange) node under `vocab`: a subject that is
/// `rdf:type rdfs:Datatype` AND carries `<ns>withRestrictions`.  The `withRestrictions`
/// requirement is load-bearing — it keeps a plain `rdfs:Datatype` *declaration* (no facet
/// list) off the lift path.  Such a node holds only datarange internals, so the generic
/// extractors skip it wholesale via [`datarange_node_labels`].
fn datarange_nodes(store: &RdfDataset, vocab: &RestrictionVocab) -> Vec<Subject> {
    let with_restrictions = vocab.iri(WITH_RESTRICTIONS_LOCAL);
    let mut typed: BTreeSet<String> = BTreeSet::new();
    let mut has_facets: BTreeSet<String> = BTreeSet::new();
    let mut subjects: BTreeMap<String, Subject> = BTreeMap::new();
    for q in default_graph_quads(store) {
        let label = subject_str(&q.subject);
        if q.predicate.as_str() == RDF_TYPE && term_str(&q.object) == RDFS_DATATYPE {
            typed.insert(label.clone());
            subjects.entry(label).or_insert_with(|| q.subject.clone());
        } else if q.predicate.as_str() == with_restrictions {
            has_facets.insert(label.clone());
            subjects.entry(label).or_insert_with(|| q.subject.clone());
        }
    }
    typed
        .intersection(&has_facets)
        .filter_map(|label| subjects.get(label).cloned())
        .collect()
}

/// The set of datarange-node labels (for the generic extractors' skip filter).
pub(crate) fn datarange_node_labels(
    store: &RdfDataset,
    vocab: &RestrictionVocab,
) -> BTreeSet<String> {
    datarange_nodes(store, vocab)
        .iter()
        .map(subject_str)
        .collect()
}

/// Collect the constraining facets carried by a single facet cell (`[ xsd:minInclusive
/// "0.0" ]`).  A well-formed cell carries exactly one facet triple; every recognized
/// [`FACET_LOCALS`] predicate on it is gathered here.
fn collect_cell_facets(store: &RdfDataset, cell: &Subject) -> Vec<(String, Node)> {
    let mut out: Vec<(String, Node)> = Vec::new();
    for local in FACET_LOCALS {
        let iri = format!("{XSD_NS}{local}");
        let pred = crate::graphutil::nn(&iri);
        for obj in objects(store, cell, &pred) {
            out.push((iri.clone(), obj));
        }
    }
    out
}

/// Walk a datarange's `withRestrictions` list into its ordered facets, or `None` (with a
/// surfaced [`Diagnostic`]) when a cell is malformed — a literal cell, a cell with no
/// facet triple, or a facet whose value is not a literal.  Whole-datarange skip on any
/// defect (never a silent partial lift), mirroring the enumeration list discipline.
fn collect_datarange_facets(
    store: &RdfDataset,
    cells: &[Node],
    node_label: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<Facet>> {
    let mut facets: Vec<Facet> = Vec::new();
    for cell in cells {
        let Some(cell_subject) = crate::graphutil::term_as_subject(cell) else {
            diagnostics.push(warn(
                "MALFORMED_DATARANGE",
                format!("datarange {node_label:?} has a facet cell that is a literal, not a resource; skipped"),
                Some(node_label.to_owned()),
            ));
            return None;
        };
        let cell_facets = collect_cell_facets(store, &cell_subject);
        if cell_facets.is_empty() {
            diagnostics.push(warn(
                "MALFORMED_DATARANGE",
                format!(
                    "datarange {node_label:?} has a facet cell with no recognized constraining \
                     facet; skipped"
                ),
                Some(node_label.to_owned()),
            ));
            return None;
        }
        for (iri, obj) in cell_facets {
            if !term_is_literal(&obj) {
                diagnostics.push(warn(
                    "MALFORMED_DATARANGE",
                    format!(
                        "datarange {node_label:?} facet {iri:?} has a non-literal value; skipped"
                    ),
                    Some(node_label.to_owned()),
                ));
                return None;
            }
            facets.push(Facet {
                iri,
                value: term_str(&obj),
            });
        }
    }
    Some(facets)
}

/// Lift every `owl:`/`logic:` datatype restriction (`[ a rdfs:Datatype ; owl:onDatatype D ;
/// owl:withRestrictions ( … ) ]`) under `vocab` into flat skolem-keyed [`LiftedTriple`]s,
/// mirroring [`skolemize_enumerations`].  A well-formed datarange contributes: the
/// `logic:subClassOf` / `logic:equivalentClass` anchor edge(s) redirected to the skolem
/// node, the `rdf:type logic:Datarange` typing, the `logic:onDatatype` base slot, and one
/// axiom per facet keyed on its full `xsd:` IRI (`obj_is_literal = true`).
///
/// Every malformedness (a corrupt facet list, a missing `onDatatype`, a facet cell with no
/// facet triple, a non-literal facet value) is disclosed and the datarange skipped whole;
/// a nested/anonymous `onDatatype` is disclosed as the documented anonymous-nested
/// boundary — nothing is silently lost.
pub(crate) fn skolemize_dataranges(
    store: &RdfDataset,
    vocab: &RestrictionVocab,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<LiftedTriple> {
    let on_datatype_pred = crate::graphutil::nn(&vocab.iri(ON_DATATYPE_LOCAL));
    let with_restrictions_pred = crate::graphutil::nn(&vocab.iri(WITH_RESTRICTIONS_LOCAL));
    let sub_class_of_pred = crate::graphutil::nn(&vocab.sub_class_of);
    let equivalent_class_pred = crate::graphutil::nn(&vocab.equivalent_class);
    let mut out: Vec<LiftedTriple> = Vec::new();

    for node in datarange_nodes(store, vocab) {
        let node_label = subject_str(&node);
        let on_datatypes = objects(store, &node, &on_datatype_pred);
        let on_datatype_term = match on_datatypes.as_slice() {
            // No onDatatype is authored-input malformedness: disclose and skip.
            [] => {
                diagnostics.push(warn(
                    "MALFORMED_DATARANGE",
                    format!(
                        "datarange {node_label:?} carries withRestrictions but has no \
                         onDatatype; skipped"
                    ),
                    Some(node_label),
                ));
                continue;
            }
            // Two or more base datatypes is a wiring contradiction — pick-first would
            // silently drop one, so hard-fail (the No-optionality invariant), exactly as
            // the multi-onProperty restriction case does.
            [_, _, ..] => panic!(
                "datarange node {node_label:?} has {} onDatatype values (a single base \
                 datatype is required)",
                on_datatypes.len()
            ),
            [only] => only.clone(),
        };
        // A nested/anonymous base datatype has no stable identity to lift — disclose and
        // skip (the documented anonymous-nested boundary, mirroring the restriction
        // nested-filler case).
        if crate::graphutil::term_is_blank(&on_datatype_term) {
            diagnostics.push(warn(
                "UNSUPPORTED_NESTED_DATARANGE",
                format!(
                    "datarange {node_label:?} has an anonymous onDatatype (nested datatype \
                     expression); not lifted"
                ),
                Some(node_label),
            ));
            continue;
        }
        let on_datatype = term_str(&on_datatype_term);

        let Some(list_head) = value(store, &node, &with_restrictions_pred) else {
            continue;
        };
        let cells = match rdf_list(store, &list_head) {
            ListWalk::Complete(cells) => cells,
            ListWalk::Malformed(why) => {
                diagnostics.push(warn(
                    "MALFORMED_DATARANGE",
                    format!(
                        "datarange {node_label:?} has a corrupt withRestrictions list ({why}); \
                         skipped"
                    ),
                    Some(node_label),
                ));
                continue;
            }
        };
        if cells.is_empty() {
            diagnostics.push(warn(
                "MALFORMED_DATARANGE",
                format!("datarange {node_label:?} has an empty withRestrictions list; skipped"),
                Some(node_label),
            ));
            continue;
        }
        let Some(facets) = collect_datarange_facets(store, &cells, &node_label, diagnostics) else {
            continue;
        };

        let skolem = datarange_skolem_iri(&datarange_content_key(&on_datatype, &facets));

        // Datarange internals.
        out.push(LiftedTriple {
            subject: skolem.clone(),
            predicate: RDF_TYPE.to_owned(),
            obj: logic(DATARANGE_CLASS_LOCAL),
            obj_is_literal: false,
        });
        out.push(LiftedTriple {
            subject: skolem.clone(),
            predicate: logic(ON_DATATYPE_LOCAL),
            obj: on_datatype.clone(),
            obj_is_literal: false,
        });
        for f in &facets {
            out.push(LiftedTriple {
                subject: skolem.clone(),
                // Facets keep their full xsd: IRI on both surfaces (unlike the shared-local
                // restriction constraints), so emit the facet IRI verbatim.
                predicate: f.iri.clone(),
                obj: f.value.clone(),
                obj_is_literal: true,
            });
        }

        // Redirect the datarange's anchor edges to the skolem node.
        let node_term = subject_as_object(&node);
        for anchor in subjects_with(store, &sub_class_of_pred, &node_term) {
            out.push(LiftedTriple {
                subject: subject_str(&anchor),
                predicate: logic("subClassOf"),
                obj: skolem.clone(),
                obj_is_literal: false,
            });
        }
        for anchor in subjects_with(store, &equivalent_class_pred, &node_term) {
            out.push(LiftedTriple {
                subject: subject_str(&anchor),
                predicate: logic("equivalentClass"),
                obj: skolem.clone(),
                obj_is_literal: false,
            });
        }
    }

    out
}

/// View a subject node as an object [`Node`] for a `subjects_with(pred, object)` query.
fn subject_as_object(s: &Subject) -> Node {
    match s {
        Subject::Iri(iri) => Node::Iri(iri.clone()),
        Subject::Blank { label, scope } => Node::Blank {
            label: label.clone(),
            scope: *scope,
        },
    }
}

fn warn(code: &str, message: String, subject: Option<String>) -> Diagnostic {
    Diagnostic {
        severity: Severity::Warning,
        code: code.to_owned(),
        message,
        subject,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(local: &str, value: &str, is_literal: bool) -> Constraint {
        Constraint {
            local: local.to_owned(),
            value: value.to_owned(),
            is_literal,
            is_blank: false,
        }
    }

    #[test]
    fn content_key_is_order_independent() {
        // The mint key sorts constraints, so declaration order cannot change the id.
        let a = content_key(
            "P",
            &[c("someValuesFrom", "C", false), c("hasValue", "V", false)],
        );
        let b = content_key(
            "P",
            &[c("hasValue", "V", false), c("someValuesFrom", "C", false)],
        );
        assert_eq!(a, b);
        assert_eq!(skolem_iri(&a), skolem_iri(&b));
    }

    #[test]
    fn content_key_distinguishes_literal_from_iri_filler() {
        // An IRI filler and a literal filler with the same lexical form must NOT collide.
        let iri = content_key("P", &[c("hasValue", "Red", false)]);
        let lit = content_key("P", &[c("hasValue", "Red", true)]);
        assert_ne!(iri, lit);
        assert_ne!(skolem_iri(&iri), skolem_iri(&lit));
    }

    #[test]
    fn content_key_excludes_subject_class() {
        // The key is a function of onProperty + constraints only — no subject — so two
        // classes bearing the same restriction share one skolem node.
        let k = content_key("P", &[c("someValuesFrom", "C", false)]);
        assert!(k.starts_with("onProperty=P"));
        assert!(!k.contains("subClassOf"));
    }

    fn f(facet_local: &str, value: &str) -> Facet {
        Facet {
            iri: format!("{XSD_NS}{facet_local}"),
            value: value.to_owned(),
        }
    }

    #[test]
    fn datarange_content_key_is_order_independent() {
        // The mint key sorts facets, so authored facet order cannot change the id.
        let a = datarange_content_key(
            "http://www.w3.org/2001/XMLSchema#decimal",
            &[f("minInclusive", "0.0"), f("maxInclusive", "1.0")],
        );
        let b = datarange_content_key(
            "http://www.w3.org/2001/XMLSchema#decimal",
            &[f("maxInclusive", "1.0"), f("minInclusive", "0.0")],
        );
        assert_eq!(a, b);
        assert_eq!(datarange_skolem_iri(&a), datarange_skolem_iri(&b));
    }

    #[test]
    fn datarange_content_key_distinguishes_datatype_and_facets() {
        // A different base datatype, a different facet IRI, and a different facet value
        // must each mint a distinct node.
        let base = datarange_content_key(
            "http://www.w3.org/2001/XMLSchema#decimal",
            &[f("minInclusive", "0.0")],
        );
        let other_dt = datarange_content_key(
            "http://www.w3.org/2001/XMLSchema#integer",
            &[f("minInclusive", "0.0")],
        );
        let other_facet = datarange_content_key(
            "http://www.w3.org/2001/XMLSchema#decimal",
            &[f("minExclusive", "0.0")],
        );
        let other_value = datarange_content_key(
            "http://www.w3.org/2001/XMLSchema#decimal",
            &[f("minInclusive", "0.5")],
        );
        assert!(base.starts_with("onDatatype=http://www.w3.org/2001/XMLSchema#decimal"));
        assert_ne!(base, other_dt);
        assert_ne!(base, other_facet);
        assert_ne!(base, other_value);
        assert_ne!(datarange_skolem_iri(&base), datarange_skolem_iri(&other_dt));
    }
}
