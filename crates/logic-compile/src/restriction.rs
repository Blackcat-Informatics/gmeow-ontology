// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared OWL/`logic:` restriction skolemizer — the single lift routine behind the
//! `isSupersetOf` round-trip.
//!
//! An OWL class-expression restriction (`C rdfs:subClassOf [ owl:onProperty P ;
//! owl:someValuesFrom D ]`) and the equivalent `logic:`-authored form must normalize
//! to the SAME flat [`crate::ir::LogicAxiom`] set, so the IR-isomorphism gate
//! (`adapter::assert_ir_isomorphic`) passes.  The anonymous restriction node is the
//! only obstacle: a raw blank-node label is per-parse and would never collide across
//! the two authoring surfaces.
//!
//! The fix mirrors the covering projection: mint a **deterministic, content-addressed
//! IRI** `logic:restriction/<sha256_12(content_key)>` for the restriction node, where
//! `content_key` is a canonical function of the restriction's *meaning only*
//! (`onProperty` + its constraints).  Both the `owl:` adapter and the `logic:`
//! front-end run THIS one routine (parameterized only by the source vocabulary), so
//! identical meaning ⇒ identical skolem IRI ⇒ identical axiom set.
//!
//! Every lifted restriction becomes flat `(subject, predicate, obj)` triples in the
//! program's `axioms` — never a new collection — so a restriction-free program's
//! axiom vector and content key are byte-identical (the append-only discipline).

use std::collections::BTreeSet;

use purrdf::RdfDataset;

use crate::frontend::{Diagnostic, Severity};
use crate::graphutil::{
    default_graph_quads, objects, sha256_12, subject_str, subjects_with, term_is_literal, term_str,
    value, Node, Subject,
};
use crate::ir::LOGIC_NAMESPACE;

/// The `\u{0}` field separator that pins the restriction `content_key` (matches the
/// IR `sort_key` separator so the byte form is consistent across the compiler).
const SEP: char = '\u{0}';

const OWL_NS: &str = "http://www.w3.org/2002/07/owl#";
const RDFS_NS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The `logic:` local name of the restriction class (`_:r rdf:type logic:Restriction`).
pub(crate) const RESTRICTION_CLASS_LOCAL: &str = "Restriction";
/// The `logic:` local name of the property slot (`_:r logic:onProperty P`).
pub(crate) const ON_PROPERTY_LOCAL: &str = "onProperty";
/// The `logic:` local name of the enumeration class (`_:e rdf:type logic:Enumeration`).
pub(crate) const ENUMERATION_CLASS_LOCAL: &str = "Enumeration";
/// The `logic:` local name of the enumeration membership predicate (`_:e logic:oneOf m`).
pub(crate) const ONE_OF_LOCAL: &str = "oneOf";

const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

/// The single-valued restriction *constraint* predicates handled by the lift, as
/// local names (shared verbatim by the `owl:` and `logic:` namespaces).  A restriction
/// node carries `onProperty` plus one or more of these:
///
/// * value constraints — `someValuesFrom`, `allValuesFrom`, `hasValue`;
/// * unqualified cardinality — `minCardinality`, `maxCardinality`, `cardinality`;
/// * qualified cardinality — `qualifiedCardinality`, `qualifiedMinCardinality`,
///   `qualifiedMaxCardinality`, each paired with `onClass` (a class filler) or
///   `onDataRange` (a datatype filler).
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
    "qualifiedMinCardinality",
    "qualifiedMaxCardinality",
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
    "qualifiedMinCardinality",
    "qualifiedMaxCardinality",
];

/// The source vocabulary a [`skolemize_restrictions`] pass reads.  The constraint /
/// property-slot / type local names are shared between `owl:` and `logic:`; only the
/// namespace and the two anchoring predicates (`subClassOf` / `equivalentClass`)
/// differ between the legacy-OWL surface and the canonical `logic:` surface.
pub(crate) struct RestrictionVocab {
    /// Namespace of the restriction predicates + class (`owl:` or `logic:`).
    ns: String,
    /// The class→restriction anchor (`rdfs:subClassOf` for OWL, `logic:subClassOf` for logic).
    sub_class_of: String,
    /// The equivalence anchor (`owl:equivalentClass` for OWL, `logic:equivalentClass` for logic).
    equivalent_class: String,
}

impl RestrictionVocab {
    /// The legacy-OWL source vocabulary (the adapter path).
    pub(crate) fn owl() -> Self {
        Self {
            ns: OWL_NS.to_owned(),
            sub_class_of: format!("{RDFS_NS}subClassOf"),
            equivalent_class: format!("{OWL_NS}equivalentClass"),
        }
    }

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
/// set).  `owl:oneOf` / cardinality families extend this in later construct tasks.
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
        let Some(on_property) = on_properties.first().map(term_str) else {
            diagnostics.push(warn(
                "MALFORMED_RESTRICTION",
                format!(
                    "restriction node {node_label:?} is typed a restriction but has no \
                     onProperty; skipped"
                ),
                Some(node_label),
            ));
            continue;
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

/// Walk an `rdf:first`/`rdf:rest`/`rdf:nil` list from `head`, returning its members in
/// order.  A malformed / cyclic list terminates at the first missing `rdf:rest` or on
/// revisiting a node (the visited guard keeps a corrupt input from looping).
fn rdf_list(store: &RdfDataset, head: &Node) -> Vec<Node> {
    let first = crate::graphutil::nn(RDF_FIRST);
    let rest = crate::graphutil::nn(RDF_REST);
    let mut out: Vec<Node> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut cursor = head.clone();
    while let Some(node) = crate::graphutil::term_as_subject(&cursor) {
        if subject_str(&node) == RDF_NIL || !seen.insert(subject_str(&node)) {
            break;
        }
        if let Some(item) = value(store, &node, &first) {
            out.push(item);
        }
        match value(store, &node, &rest) {
            Some(next) => cursor = next,
            None => break,
        }
    }
    out
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
        let members = rdf_list(store, &list_head);
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
}
