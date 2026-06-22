// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3-free engine for the gUFO/UFO reasoning invariants (#579).
//!
//! Ported byte-exact from `src/gmeow_tools/reasoning_lint.py`. The six OntoUML
//! anti-pattern checks (`exactly_one_stereotype`, `identity_overlap`,
//! `anti_rigidity_discipline`, `relator_mediation`, `coequal_facet_orthogonality`,
//! `frame_declaration_completeness`) run over an oxigraph [`Store`] built from the
//! merged ontology sources. The aggregator [`reasoning_invariants`] runs all six
//! and flattens their errors in the same order the Python does.
//!
//! The hard parts reproduced exactly:
//!
//! * the proper-ancestor transitive closure over `rdfs:subClassOf`
//!   ([`proper_ancestors`], mirroring rdflib `transitive_objects` minus self),
//! * the `owl:AllDisjointClasses` / `owl:members` RDF-Collection walk
//!   ([`all_disjoint_member_sets`], a manual `rdf:first`/`rdf:rest`/`rdf:nil`
//!   linked-list traversal since oxigraph has no Collection helper),
//! * the subPropertyOf/equivalentProperty property-bridge reachability DFS in
//!   [`coequal_facet_orthogonality`].
//!
//! Determinism: wherever the Python sorts (by `str`), this sorts the same way;
//! wherever the Python relied on graph-iteration order, the emitted output is
//! per-class (driven by the sorted [`gmeow_classes`]) or a counted aggregate, so
//! oxigraph's quad order never leaks into a diagnostic.
//!
//! Engine-core separation: this module imports no pyo3. The [`crate::py`]
//! bindings adapt [`reasoning_invariants`] to Python.

use std::collections::{BTreeSet, HashSet, VecDeque};

use oxigraph::model::{NamedNode, NamedOrBlankNode, Term};
use oxigraph::store::Store;

use crate::model::{owl, rdf, rdfs};

/// The OntoUML anti-pattern catalogue URL, cited in messages so failures
/// self-document (mirrors `_CATALOGUE`).
const CATALOGUE: &str = "https://ontouml.readthedocs.io/en/latest/anti-patterns/";

/// The gUFO namespace (`http://purl.org/nemo/gufo#`).
const GUFO_NS: &str = "http://purl.org/nemo/gufo#";

/// The canonical `logic:` namespace (`https://blackcatinformatics.ca/logic/`) —
/// the authoritative sort surface that subsumes gUFO (#663/#694). Slices migrate
/// their stereotype authoring from `gufo:` to `logic:`; this validator accepts
/// EITHER namespace so the per-slice migration can run incrementally with the
/// foundation-conformance gate green at every step (some slices migrated, others
/// not). The local name is identical across the two namespaces for every sort
/// except the perdurant down-projection renames (`gufo:EventType`→`logic:Event`,
/// `gufo:SituationType`→`logic:Situation`).
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// Build a gUFO-namespaced IRI string for a local name.
fn gufo(local: &str) -> String {
    format!("{GUFO_NS}{local}")
}

/// Build a `logic:`-namespaced IRI string for a local name.
fn logic(local: &str) -> String {
    format!("{LOGIC_NS}{local}")
}

/// Both namespaced IRIs for a sort whose local name is identical in `gufo:` and
/// `logic:` (every sort except the EventType/SituationType perdurant renames).
fn dual(local: &str) -> [String; 2] {
    [gufo(local), logic(local)]
}

/// Typed configuration for the reasoning invariants, supplied by the Python
/// caller from its single-source-of-truth constants (`config.NAMESPACE`).
#[derive(Debug, Clone)]
pub struct GufoConfig {
    /// The GMEOW vocabulary namespace (`config.NAMESPACE`).
    pub namespace: String,
}

impl GufoConfig {
    /// The `gmeow:TimeInterval` IRI — a relator's validity scope, never a relatum.
    fn time_interval(&self) -> String {
        format!("{}TimeInterval", self.namespace)
    }
}

/// The endurant-type stereotypes (`_ENDURANT_STEREOTYPES`), accepted in both the
/// `gufo:` and the canonical `logic:` namespace (#694).
fn endurant_stereotypes() -> Vec<String> {
    [
        "Kind",
        "SubKind",
        "Phase",
        "Role",
        "Category",
        "Mixin",
        "RoleMixin",
        "PhaseMixin",
    ]
    .into_iter()
    .flat_map(dual)
    .collect()
}

/// The perdurant stereotypes (`_PERDURANT_STEREOTYPES`). gUFO authors
/// `gufo:EventType`/`gufo:SituationType`; the canonical `logic:` form down-projects
/// them to `logic:Event`/`logic:Situation` (#694).
fn perdurant_stereotypes() -> Vec<String> {
    vec![
        gufo("EventType"),
        gufo("SituationType"),
        logic("Event"),
        logic("Situation"),
    ]
}

/// The abstract-individual stereotype (`_ABSTRACT_STEREOTYPES`), in both namespaces.
fn abstract_stereotypes() -> Vec<String> {
    Vec::from(dual("AbstractIndividualType"))
}

/// The full acceptable-stereotype set (`_META_CLASSES`).
fn meta_classes() -> HashSet<String> {
    endurant_stereotypes()
        .into_iter()
        .chain(perdurant_stereotypes())
        .chain(abstract_stereotypes())
        .collect()
}

/// Rigid sortals (`_RIGID_SORTALS`), in both `gufo:` and `logic:` namespaces.
fn rigid_sortals() -> HashSet<String> {
    dual("Kind").into_iter().chain(dual("SubKind")).collect()
}

/// Anti-rigid sortals (`_ANTI_RIGID_SORTALS`), in both namespaces.
fn anti_rigid_sortals() -> HashSet<String> {
    dual("Phase").into_iter().chain(dual("Role")).collect()
}

/// Anti-rigid / semi-rigid types a rigid sortal must never specialize
/// (`_ANTI_RIGID_TYPES`), in both namespaces.
fn anti_rigid_types() -> HashSet<String> {
    ["Phase", "Role", "PhaseMixin", "RoleMixin", "Mixin"]
        .into_iter()
        .flat_map(dual)
        .collect()
}

/// Whether an IRI is a bare GMEOW vocabulary term (not an instance sub-path)
/// (mirrors `_is_gmeow_class_iri`).
fn is_gmeow_class_iri(iri: &str, cfg: &GufoConfig) -> bool {
    if let Some(local) = iri.strip_prefix(&cfg.namespace) {
        !local.contains('/')
    } else {
        false
    }
}

/// Prefixed `gmeow:` / `gufo:` rendering for messages, full IRI otherwise
/// (mirrors `_local`).
fn local(iri: &str, cfg: &GufoConfig) -> String {
    if is_gmeow_class_iri(iri, cfg) {
        format!("gmeow:{}", &iri[cfg.namespace.len()..])
    } else if let Some(rest) = iri.strip_prefix(GUFO_NS) {
        format!("gufo:{rest}")
    } else if let Some(rest) = iri.strip_prefix(LOGIC_NS) {
        format!("logic:{rest}")
    } else {
        iri.to_owned()
    }
}

/// The GMEOW-namespaced `owl:Class` vocabulary terms, sorted by IRI for stable
/// output (mirrors `_gmeow_classes`).
fn gmeow_classes(store: &Store, cfg: &GufoConfig) -> Vec<String> {
    let mut classes: BTreeSet<String> = BTreeSet::new();
    for quad in store
        .quads_for_pattern(None, Some(rdf::TYPE), Some(owl::CLASS.into()), None)
        .flatten()
    {
        if let NamedOrBlankNode::NamedNode(n) = &quad.subject {
            let iri = n.as_str();
            if is_gmeow_class_iri(iri, cfg) {
                classes.insert(iri.to_owned());
            }
        }
    }
    classes.into_iter().collect()
}

/// Transitive `rdfs:subClassOf` super-classes of `cls`, excluding itself
/// (mirrors `_proper_ancestors` / rdflib `transitive_objects` minus self).
///
/// A BFS over the `subClassOf` edges, named-node objects only, reflexive closure
/// minus the start node — exactly what rdflib `transitive_objects` yields before
/// the Python comprehension drops `a == cls`.
fn proper_ancestors(store: &Store, cls: &str) -> HashSet<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(cls.to_owned());
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(cls.to_owned());
    while let Some(node) = queue.pop_front() {
        let subject = NamedNode::new_unchecked(&node);
        for quad in store
            .quads_for_pattern(
                Some((&subject).into()),
                Some(rdfs::SUB_CLASS_OF),
                None,
                None,
            )
            .flatten()
        {
            if let Term::NamedNode(parent) = &quad.object {
                let p = parent.as_str().to_owned();
                if visited.insert(p.clone()) {
                    queue.push_back(p.clone());
                }
                if p != cls {
                    seen.insert(p);
                }
            }
        }
    }
    seen
}

/// The gUFO meta-classes `cls` is punned as, via `rdf:type` (mirrors
/// `_stereotypes`). Only IRIs in `meta` are kept.
fn stereotypes(store: &Store, cls: &str, meta: &HashSet<String>) -> HashSet<String> {
    let subject = NamedNode::new_unchecked(cls);
    let mut out: HashSet<String> = HashSet::new();
    for quad in store
        .quads_for_pattern(Some((&subject).into()), Some(rdf::TYPE), None, None)
        .flatten()
    {
        if let Term::NamedNode(t) = &quad.object {
            let t = t.as_str();
            if meta.contains(t) {
                out.insert(t.to_owned());
            }
        }
    }
    out
}

/// Whether `cls` has `rdf:type cls_type` (a direct type probe).
fn has_type(store: &Store, cls: &str, cls_type: &str) -> bool {
    let subject = NamedNode::new_unchecked(cls);
    let object = NamedNode::new_unchecked(cls_type);
    store
        .quads_for_pattern(
            Some((&subject).into()),
            Some(rdf::TYPE),
            Some((&object).into()),
            None,
        )
        .next()
        .is_some()
}

/// Join `_local`-rendered IRIs with ", " in the given order.
fn join_local(iris: &[String], cfg: &GufoConfig) -> String {
    iris.iter()
        .map(|i| local(i, cfg))
        .collect::<Vec<_>>()
        .join(", ")
}

/// **exactly_one_stereotype** — every GMEOW class must be punned with exactly
/// one gUFO meta-class.
pub fn exactly_one_stereotype(store: &Store, cfg: &GufoConfig) -> Vec<String> {
    let meta = meta_classes();
    let mut problems: Vec<String> = Vec::new();
    for cls in gmeow_classes(store, cfg) {
        let st = stereotypes(store, &cls, &meta);
        if st.is_empty() {
            problems.push(format!(
                "{} carries no stereotype — pun it with exactly one of \
                 Kind/SubKind/Role/Phase/Category/Mixin/RoleMixin/PhaseMixin \
                 (Event/Situation for perdurants, or \
                 AbstractIndividualType for abstract individuals)",
                local(&cls, cfg)
            ));
        } else if st.len() > 1 {
            let mut names: Vec<String> = st.iter().map(|s| local(s, cfg)).collect();
            names.sort();
            problems.push(format!(
                "{} carries conflicting stereotypes ({}) — a class has \
                 exactly one stereotype",
                local(&cls, cfg),
                names.join(", ")
            ));
        }
    }
    problems
}

/// **identity_overlap (MixIden)** — a sortal inherits identity from exactly one
/// Kind; no Kind ⊑ Kind.
pub fn identity_overlap(store: &Store, cfg: &GufoConfig) -> Vec<String> {
    let meta = meta_classes();
    // A Kind in either namespace (gufo:Kind or the canonical logic:Kind, #694).
    let kinds: [String; 2] = dual("Kind");
    let is_kind = |s: &String| kinds.contains(s);
    let rigid = rigid_sortals();
    let anti_rigid = anti_rigid_sortals();
    let mut problems: Vec<String> = Vec::new();
    for cls in gmeow_classes(store, cfg) {
        let st = stereotypes(store, &cls, &meta);
        let ancestors = proper_ancestors(store, &cls);
        // kind_ancestors: ancestors that are themselves a Kind, sorted by str.
        let mut kind_ancestors: Vec<String> = ancestors
            .iter()
            .filter(|a| kinds.iter().any(|k| has_type(store, a, k)))
            .cloned()
            .collect();
        kind_ancestors.sort();

        if st.iter().any(is_kind) && !kind_ancestors.is_empty() {
            problems.push(format!(
                "{} is a Kind but specializes Kind(s) {} — identity \
                 conflict (OntoUML MixIden: every endurant instantiates exactly \
                 one Kind). See {}",
                local(&cls, cfg),
                join_local(&kind_ancestors, cfg),
                CATALOGUE
            ));
        }
        // A non-Kind sortal must trace to exactly one Kind (OntoUML MixIden).
        let is_sortal = st
            .iter()
            .any(|s| rigid.contains(s) || anti_rigid.contains(s));
        if is_sortal && !st.iter().any(is_kind) && kind_ancestors.len() != 1 {
            let names = if kind_ancestors.is_empty() {
                "none".to_owned()
            } else {
                join_local(&kind_ancestors, cfg)
            };
            problems.push(format!(
                "{} is a sortal but specializes {} Kind(s) ({}) — a sortal \
                 inherits identity from exactly one Kind (OntoUML MixIden). See {}",
                local(&cls, cfg),
                kind_ancestors.len(),
                names,
                CATALOGUE
            ));
        }
    }
    problems
}

/// **anti_rigidity_discipline (MixRig / FreeRole)** — anti-rigid sortals need a
/// rigid super; rigid types avoid anti-rigid ancestors.
pub fn anti_rigidity_discipline(store: &Store, cfg: &GufoConfig) -> Vec<String> {
    let meta = meta_classes();
    let rigid = rigid_sortals();
    let anti_rigid = anti_rigid_sortals();
    let anti_rigid_t = anti_rigid_types();
    let mut problems: Vec<String> = Vec::new();
    for cls in gmeow_classes(store, cfg) {
        let st = stereotypes(store, &cls, &meta);
        let ancestors = proper_ancestors(store, &cls);

        // Accumulate every ancestor's meta-class stereotypes.
        let mut ancestor_stereotypes: HashSet<String> = HashSet::new();
        for ancestor in &ancestors {
            for s in stereotypes(store, ancestor, &meta) {
                ancestor_stereotypes.insert(s);
            }
        }

        let is_anti_rigid = st.iter().any(|s| anti_rigid.contains(s));
        let ancestor_has_rigid = ancestor_stereotypes.iter().any(|s| rigid.contains(s));
        if is_anti_rigid && !ancestor_has_rigid {
            problems.push(format!(
                "{} is an anti-rigid sortal (Role/Phase) but specializes no rigid \
                 sortal — nowhere to inherit a principle of identity (OntoUML \
                 FreeRole). See {}",
                local(&cls, cfg),
                CATALOGUE
            ));
        }

        let is_rigid = st.iter().any(|s| rigid.contains(s));
        if is_rigid {
            // Name the offending ancestor class(es) and their anti-rigid stereotype.
            let mut bad_ancestors: Vec<String> = Vec::new();
            for ancestor in &ancestors {
                let bad: Vec<String> = stereotypes(store, ancestor, &meta)
                    .into_iter()
                    .filter(|s| anti_rigid_t.contains(s))
                    .collect();
                if !bad.is_empty() {
                    let mut labels: Vec<String> = bad.iter().map(|s| local(s, cfg)).collect();
                    labels.sort();
                    bad_ancestors.push(format!("{} ({})", local(ancestor, cfg), labels.join(", ")));
                }
            }
            if !bad_ancestors.is_empty() {
                bad_ancestors.sort();
                problems.push(format!(
                    "{} is a rigid sortal (Kind/SubKind) but specializes anti-rigid \
                     ancestor(s) {} — a rigid type cannot inherit contingent \
                     instantiation (OntoUML MixRig). See {}",
                    local(&cls, cfg),
                    bad_ancestors.join(", "),
                    CATALOGUE
                ));
            }
        }
    }
    problems
}

/// **relator_mediation (RelComp)** — every concrete `gufo:Relator` mediates at
/// least two relata.
pub fn relator_mediation(store: &Store, cfg: &GufoConfig) -> Vec<String> {
    // A Relator base in either namespace (gufo:Relator or canonical logic:Relator).
    let relators: [String; 2] = dual("Relator");
    let time_interval = cfg.time_interval();

    // The GMEOW object properties (graph iteration order; output is count-only).
    let mut gmeow_object_properties: Vec<String> = Vec::new();
    for quad in store
        .quads_for_pattern(
            None,
            Some(rdf::TYPE),
            Some(owl::OBJECT_PROPERTY.into()),
            None,
        )
        .flatten()
    {
        if let NamedOrBlankNode::NamedNode(n) = &quad.subject {
            let iri = n.as_str();
            if iri.starts_with(&cfg.namespace) {
                gmeow_object_properties.push(iri.to_owned());
            }
        }
    }

    // Precompute per-property (domain, range, functional) once — O(M) — so the
    // class loop below does not re-query the store for every (class × property)
    // pair (R8 / R9 hoist).
    struct PropInfo {
        domains: HashSet<String>,
        ranges: HashSet<String>,
        functional: bool,
    }
    let prop_infos: Vec<PropInfo> = gmeow_object_properties
        .iter()
        .map(|iri| PropInfo {
            domains: object_iris(store, iri, rdfs::DOMAIN),
            ranges: object_iris(store, iri, rdfs::RANGE),
            functional: is_functional(store, iri),
        })
        .collect();

    let mut problems: Vec<String> = Vec::new();
    for cls in gmeow_classes(store, cfg) {
        let ancestors = proper_ancestors(store, &cls);
        if !relators.iter().any(|r| ancestors.contains(r)) {
            continue;
        }
        // Concrete iff no GMEOW class specializes it.
        let has_gmeow_subclass = gmeow_subclasses(store, &cls)
            .iter()
            .any(|sub| sub != &cls && sub.starts_with(&cfg.namespace));
        if has_gmeow_subclass {
            continue; // abstract base — its concrete subtypes carry the mediations
        }
        // relator_terms = {cls} ∪ {GMEOW ancestors}.
        let mut relator_terms: HashSet<String> = HashSet::new();
        relator_terms.insert(cls.clone());
        for a in &ancestors {
            if a.starts_with(&cfg.namespace) {
                relator_terms.insert(a.clone());
            }
        }

        let mut ends = 0;
        for info in &prop_infos {
            let domain_hits = info.domains.intersection(&relator_terms).next().is_some();
            let range_hits = info.ranges.intersection(&relator_terms).next().is_some();
            let mut relata: HashSet<String> = HashSet::new();
            if domain_hits {
                relata.extend(info.ranges.iter().cloned());
            }
            if range_hits {
                relata.extend(info.domains.iter().cloned());
            }
            for t in &relator_terms {
                relata.remove(t);
            }
            relata.remove(&time_interval);
            if relata.is_empty() {
                continue;
            }
            ends += if info.functional { 1 } else { 2 };
        }
        if ends < 2 {
            problems.push(format!(
                "{} is a concrete Relator mediating only {} end(s) — a relator \
                 must mediate at least two (OntoUML RelComp). See {}",
                local(&cls, cfg),
                ends,
                CATALOGUE
            ));
        }
    }
    problems
}

/// Subjects of `(?, rdfs:subClassOf, cls)` (named-node subjects only) — the
/// classes that directly specialize `cls`.
fn gmeow_subclasses(store: &Store, cls: &str) -> HashSet<String> {
    let object = NamedNode::new_unchecked(cls);
    let mut out: HashSet<String> = HashSet::new();
    for quad in store
        .quads_for_pattern(None, Some(rdfs::SUB_CLASS_OF), Some((&object).into()), None)
        .flatten()
    {
        if let NamedOrBlankNode::NamedNode(n) = &quad.subject {
            out.insert(n.as_str().to_owned());
        }
    }
    out
}

/// Named-node object IRIs of `(subject_iri, predicate, ?)`.
fn object_iris(
    store: &Store,
    subject_iri: &str,
    predicate: oxigraph::model::NamedNodeRef,
) -> HashSet<String> {
    let subject = NamedNode::new_unchecked(subject_iri);
    let mut out: HashSet<String> = HashSet::new();
    for quad in store
        .quads_for_pattern(Some((&subject).into()), Some(predicate), None, None)
        .flatten()
    {
        if let Term::NamedNode(n) = &quad.object {
            out.insert(n.as_str().to_owned());
        }
    }
    out
}

/// Whether `prop` is declared an `owl:FunctionalProperty`.
fn is_functional(store: &Store, prop: &str) -> bool {
    has_type(store, prop, owl::FUNCTIONAL_PROPERTY.as_str())
}

/// Every `owl:AllDisjointClasses` axiom's member set (mirrors
/// `_all_disjoint_member_sets`). Walks the `owl:members` RDF Collection by hand.
fn all_disjoint_member_sets(store: &Store) -> Vec<HashSet<String>> {
    let mut sets: Vec<HashSet<String>> = Vec::new();
    for quad in store
        .quads_for_pattern(
            None,
            Some(rdf::TYPE),
            Some(owl::ALL_DISJOINT_CLASSES.into()),
            None,
        )
        .flatten()
    {
        let node = &quad.subject;
        for members_quad in store
            .quads_for_pattern(Some(node.as_ref()), Some(owl::MEMBERS), None, None)
            .flatten()
        {
            if let Term::NamedNode(_) | Term::BlankNode(_) = &members_quad.object {
                let head: NamedOrBlankNode = match &members_quad.object {
                    Term::NamedNode(n) => n.clone().into(),
                    Term::BlankNode(b) => b.clone().into(),
                    _ => continue,
                };
                sets.push(collection_members(store, &head));
            }
        }
    }
    sets
}

/// Walk an RDF Collection (`rdf:first`/`rdf:rest`/`rdf:nil` linked list) from
/// `head`, returning the IRI members (`isinstance(m, URIRef)` in the Python).
///
/// rdflib's `Collection` follows `rdf:rest` cells; this mirrors that, guarding
/// against a malformed cyclic list by tracking visited cells.
fn collection_members(store: &Store, head: &NamedOrBlankNode) -> HashSet<String> {
    let rdf_nil = owl_nil();
    let mut out: HashSet<String> = HashSet::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut cell: Option<NamedOrBlankNode> = Some(head.clone());
    while let Some(node) = cell {
        let key = match &node {
            NamedOrBlankNode::NamedNode(n) => n.as_str().to_owned(),
            NamedOrBlankNode::BlankNode(b) => format!("_:{}", b.as_str()),
        };
        if node == rdf_nil || !visited.insert(key) {
            break;
        }
        // rdf:first → a member (IRI only kept).
        if let Some(first) = store
            .quads_for_pattern(Some(node.as_ref()), Some(rdf::FIRST), None, None)
            .flatten()
            .next()
        {
            if let Term::NamedNode(m) = &first.object {
                out.insert(m.as_str().to_owned());
            }
        }
        // rdf:rest → the next cell.
        cell = store
            .quads_for_pattern(Some(node.as_ref()), Some(rdf::REST), None, None)
            .flatten()
            .next()
            .and_then(|q| match q.object {
                Term::NamedNode(n) => Some(NamedOrBlankNode::NamedNode(n)),
                Term::BlankNode(b) => Some(NamedOrBlankNode::BlankNode(b)),
                _ => None,
            });
    }
    out
}

/// `rdf:nil` as a [`NamedOrBlankNode`].
fn owl_nil() -> NamedOrBlankNode {
    NamedOrBlankNode::NamedNode(NamedNode::new_unchecked(rdf::NIL.as_str()))
}

/// **coequal_facet_orthogonality (P9 #281)** — co-equal facet axes stay
/// orthogonal.
pub fn coequal_facet_orthogonality(store: &Store, cfg: &GufoConfig) -> Vec<String> {
    let coequal = format!("{}coequalFacet", cfg.namespace);
    let coequal_node = NamedNode::new_unchecked(&coequal);

    // axes = sorted subjects of (?, coequalFacet, "true").
    let mut axes_set: BTreeSet<String> = BTreeSet::new();
    for quad in store
        .quads_for_pattern(None, Some(coequal_node.as_ref()), None, None)
        .flatten()
    {
        let is_true = matches!(&quad.object, Term::Literal(l) if l.value() == "true");
        if !is_true {
            continue;
        }
        if let NamedOrBlankNode::NamedNode(n) = &quad.subject {
            axes_set.insert(n.as_str().to_owned());
        }
    }
    let axes: Vec<String> = axes_set.into_iter().collect();
    if axes.is_empty() {
        return Vec::new();
    }

    let mut problems: Vec<String> = Vec::new();
    // ranges: axis -> its single range (only populated when exactly one range).
    let mut ranges: Vec<(String, String)> = Vec::new();
    for axis in &axes {
        let mut axis_ranges: Vec<String> =
            object_iris(store, axis, rdfs::RANGE).into_iter().collect();
        axis_ranges.sort();
        if axis_ranges.len() != 1 {
            problems.push(format!(
                "co-equal facet {} must have exactly one rdfs:range (found {}) — \
                 each axis owns its own value space",
                local(axis, cfg),
                axis_ranges.len()
            ));
            continue;
        }
        ranges.push((axis.clone(), axis_ranges[0].clone()));
        if is_functional(store, axis) {
            problems.push(format!(
                "co-equal facet {} is owl:FunctionalProperty — a locked single \
                 value contradicts co-equality (P9) and invites sameAs collapse (P5)",
                local(axis, cfg)
            ));
        }
    }

    // range_owners: range -> [axes], in axis-insertion order; iterated sorted by range.
    let mut range_owners: Vec<(String, Vec<String>)> = Vec::new();
    for (axis, rng) in &ranges {
        if let Some(entry) = range_owners.iter_mut().find(|(r, _)| r == rng) {
            entry.1.push(axis.clone());
        } else {
            range_owners.push((rng.clone(), vec![axis.clone()]));
        }
    }
    range_owners.sort_by(|a, b| a.0.cmp(&b.0));
    for (rng, owners) in &range_owners {
        if owners.len() > 1 {
            let names = join_local(owners, cfg);
            problems.push(format!(
                "co-equal facets {} share the range {} — axes collapsed into one \
                 value space",
                names,
                local(rng, cfg)
            ));
        }
    }

    // Bridge check over the transitive closure: subPropertyOf is directed,
    // equivalentProperty is symmetric.
    let bridged = bridged_pairs(store, &axes);
    for (a, b) in bridged {
        problems.push(format!(
            "co-equal facets {} and {} are bridged by a \
             subPropertyOf/equivalentProperty chain — one axis must never be \
             inferred from another",
            local(&a, cfg),
            local(&b, cfg)
        ));
    }

    // Jointly: every axis range must sit inside one owl:AllDisjointClasses axiom.
    let member_sets = all_disjoint_member_sets(store);
    let range_set: HashSet<String> = ranges.iter().map(|(_, r)| r.clone()).collect();
    if range_set.len() > 1 && !member_sets.iter().any(|s| range_set.is_subset(s)) {
        let mut names: Vec<String> = range_set.iter().map(|r| local(r, cfg)).collect();
        names.sort();
        problems.push(format!(
            "the co-equal facet ranges ({}) are not jointly declared in one \
             owl:AllDisjointClasses axiom — the orthogonality matrix is not \
             ELK-visible",
            names.join(", ")
        ));
    }
    problems
}

/// The bridged axis pairs `(a, b)` for `a` before `b` in the sorted `axes`,
/// where `b` is reachable from `a` or `a` from `b` over the
/// subPropertyOf/equivalentProperty adjacency (mirrors the Python double loop).
fn bridged_pairs(store: &Store, axes: &[String]) -> Vec<(String, String)> {
    // adjacency: directed subPropertyOf + symmetric equivalentProperty.
    use std::collections::HashMap;
    let mut adjacency: HashMap<String, HashSet<String>> = HashMap::new();
    for quad in store
        .quads_for_pattern(None, Some(rdfs::SUB_PROPERTY_OF), None, None)
        .flatten()
    {
        if let (NamedOrBlankNode::NamedNode(s), Term::NamedNode(o)) = (&quad.subject, &quad.object)
        {
            adjacency
                .entry(s.as_str().to_owned())
                .or_default()
                .insert(o.as_str().to_owned());
        }
    }
    for quad in store
        .quads_for_pattern(None, Some(owl::EQUIVALENT_PROPERTY), None, None)
        .flatten()
    {
        if let (NamedOrBlankNode::NamedNode(s), Term::NamedNode(o)) = (&quad.subject, &quad.object)
        {
            let (s, o) = (s.as_str().to_owned(), o.as_str().to_owned());
            adjacency.entry(s.clone()).or_default().insert(o.clone());
            adjacency.entry(o).or_default().insert(s);
        }
    }

    let reachable = |start: &str| -> HashSet<String> {
        let mut seen: HashSet<String> = HashSet::new();
        let mut stack: Vec<String> = vec![start.to_owned()];
        while let Some(node) = stack.pop() {
            if let Some(nexts) = adjacency.get(&node) {
                for nxt in nexts {
                    if seen.insert(nxt.clone()) {
                        stack.push(nxt.clone());
                    }
                }
            }
        }
        seen
    };

    // Precompute reachability for every axis once — O(K) traversals instead of
    // O(K²) — so the orthogonality check reads the precomputed sets (R10 hoist).
    let reach: Vec<HashSet<String>> = axes.iter().map(|a| reachable(a)).collect();

    let mut out: Vec<(String, String)> = Vec::new();
    for (i, a) in axes.iter().enumerate() {
        for (j, b) in axes[i + 1..].iter().enumerate() {
            if reach[i].contains(b) || reach[i + 1 + j].contains(a) {
                out.push((a.clone(), b.clone()));
            }
        }
    }
    out
}

/// **frame_declaration_completeness (P11 #283)** — frame-pointing property
/// carrier classes declare `gmeow:requiresFrame`.
pub fn frame_declaration_completeness(store: &Store, cfg: &GufoConfig) -> Vec<String> {
    let has_frame = format!("{}hasReferenceFrame", cfg.namespace);
    let requires = format!("{}requiresFrame", cfg.namespace);
    let requires_node = NamedNode::new_unchecked(&requires);

    // props = sorted transitive_subjects(subPropertyOf, has_frame) minus has_frame.
    let mut props: Vec<String> = transitive_subjects(store, rdfs::SUB_PROPERTY_OF, &has_frame)
        .into_iter()
        .filter(|p| p != &has_frame)
        .collect();
    props.sort();

    let mut problems: Vec<String> = Vec::new();
    for prop in &props {
        let mut domains: Vec<String> = object_iris(store, prop, rdfs::DOMAIN).into_iter().collect();
        domains.sort();
        for domain in &domains {
            // (domain, requires, prop) not in graph.
            let subject = NamedNode::new_unchecked(domain);
            let object = NamedNode::new_unchecked(prop);
            let present = store
                .quads_for_pattern(
                    Some((&subject).into()),
                    Some(requires_node.as_ref()),
                    Some((&object).into()),
                    None,
                )
                .next()
                .is_some();
            if !present {
                problems.push(format!(
                    "{} carries the frame-pointing property {} but declares no \
                     gmeow:requiresFrame for it — the frame-relativity shape would \
                     be missing (P11)",
                    local(domain, cfg),
                    local(prop, cfg)
                ));
            }
        }
    }
    problems
}

/// Reverse transitive closure: every subject reaching `target` over `predicate`
/// (mirrors rdflib `transitive_subjects`, reflexive — includes `target`).
fn transitive_subjects(
    store: &Store,
    predicate: oxigraph::model::NamedNodeRef,
    target: &str,
) -> HashSet<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    seen.insert(target.to_owned());
    queue.push_back(target.to_owned());
    while let Some(node) = queue.pop_front() {
        let object = NamedNode::new_unchecked(&node);
        for quad in store
            .quads_for_pattern(None, Some(predicate), Some((&object).into()), None)
            .flatten()
        {
            if let NamedOrBlankNode::NamedNode(s) = &quad.subject {
                let s = s.as_str().to_owned();
                if seen.insert(s.clone()) {
                    queue.push_back(s);
                }
            }
        }
    }
    seen
}

/// Run every UFO anti-pattern check; an empty list means the graph is clean
/// (mirrors `reasoning_invariants`). The six checks run in the same order, their
/// errors flattened.
pub fn reasoning_invariants(store: &Store, cfg: &GufoConfig) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    out.extend(exactly_one_stereotype(store, cfg));
    out.extend(identity_overlap(store, cfg));
    out.extend(anti_rigidity_discipline(store, cfg));
    out.extend(relator_mediation(store, cfg));
    out.extend(coequal_facet_orthogonality(store, cfg));
    out.extend(frame_declaration_completeness(store, cfg));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::io::{RdfFormat, RdfParser};

    const NS: &str = "https://blackcatinformatics.ca/gmeow/";

    fn cfg() -> GufoConfig {
        GufoConfig {
            namespace: NS.to_owned(),
        }
    }

    fn store_from(ttl: &str) -> Store {
        let store = Store::new().unwrap();
        for triple in RdfParser::from_format(RdfFormat::Turtle)
            .lenient()
            .for_reader(ttl.as_bytes())
        {
            store.insert(&triple.unwrap()).unwrap();
        }
        store
    }

    const PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix gufo: <http://purl.org/nemo/gufo#> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n";

    #[test]
    fn missing_stereotype_is_flagged() {
        let store = store_from(&format!("{PREFIXES}gmeow:Bare a owl:Class .\n"));
        let problems = exactly_one_stereotype(&store, &cfg());
        assert!(problems.iter().any(|p| p.contains("carries no stereotype")));
    }

    #[test]
    fn conflicting_stereotypes_are_flagged() {
        let store = store_from(&format!(
            "{PREFIXES}gmeow:TwoFaced a owl:Class , gufo:Kind , gufo:Role .\n"
        ));
        let problems = exactly_one_stereotype(&store, &cfg());
        assert!(problems
            .iter()
            .any(|p| p.contains("conflicting stereotypes")));
    }

    #[test]
    fn kind_under_kind_is_flagged_mixiden() {
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:Animal a owl:Class , gufo:Kind .\n\
             gmeow:Dog a owl:Class , gufo:Kind ; rdfs:subClassOf gmeow:Animal .\n"
        ));
        let problems = identity_overlap(&store, &cfg());
        assert!(problems
            .iter()
            .any(|p| p.contains("MixIden") && p.contains("gmeow:Dog")));
    }

    #[test]
    fn free_role_is_flagged() {
        let store = store_from(&format!(
            "{PREFIXES}gmeow:Wanderer a owl:Class , gufo:Role .\n"
        ));
        let problems = anti_rigidity_discipline(&store, &cfg());
        assert!(problems.iter().any(|p| p.contains("FreeRole")));
    }

    #[test]
    fn rigid_under_anti_rigid_is_flagged_mixrig() {
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:Student a owl:Class , gufo:Role .\n\
             gmeow:HonorsStudent a owl:Class , gufo:SubKind ; rdfs:subClassOf gmeow:Student .\n"
        ));
        let problems = anti_rigidity_discipline(&store, &cfg());
        assert!(problems.iter().any(|p| {
            p.contains("MixRig") && p.contains("gmeow:HonorsStudent") && p.contains("gmeow:Student")
        }));
    }

    #[test]
    fn under_mediated_relator_is_flagged_relcomp() {
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:LonelyBond a owl:Class , gufo:Kind ; rdfs:subClassOf gufo:Relator .\n\
             gmeow:bondParty a owl:ObjectProperty , owl:FunctionalProperty ;\n\
               rdfs:domain gmeow:LonelyBond ; rdfs:range gmeow:Person .\n"
        ));
        let problems = relator_mediation(&store, &cfg());
        assert!(problems
            .iter()
            .any(|p| p.contains("RelComp") && p.contains("gmeow:LonelyBond")));
    }

    #[test]
    fn well_formed_relator_passes() {
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:Bond a owl:Class , gufo:Kind ; rdfs:subClassOf gufo:Relator .\n\
             gmeow:bondLeft a owl:ObjectProperty , owl:FunctionalProperty ;\n\
               rdfs:domain gmeow:Bond ; rdfs:range gmeow:Person .\n\
             gmeow:bondRight a owl:ObjectProperty , owl:FunctionalProperty ;\n\
               rdfs:domain gmeow:Bond ; rdfs:range gmeow:Person .\n"
        ));
        assert!(relator_mediation(&store, &cfg()).is_empty());
    }

    #[test]
    fn abstract_relator_base_is_exempt() {
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:AbstractBond a owl:Class , gufo:Kind ; rdfs:subClassOf gufo:Relator .\n\
             gmeow:ConcreteBond a owl:Class , gufo:SubKind ; rdfs:subClassOf gmeow:AbstractBond .\n"
        ));
        assert!(!relator_mediation(&store, &cfg())
            .iter()
            .any(|p| p.contains("gmeow:AbstractBond")));
    }

    #[test]
    fn disjoint_collection_is_parsed() {
        // owl:AllDisjointClasses with a 2-member owl:members collection.
        let store = store_from(&format!(
            "{PREFIXES}\
             [] a owl:AllDisjointClasses ; owl:members ( gmeow:A gmeow:B ) .\n"
        ));
        let sets = all_disjoint_member_sets(&store);
        assert_eq!(sets.len(), 1);
        let want: HashSet<String> = [format!("{NS}A"), format!("{NS}B")].into_iter().collect();
        assert_eq!(sets[0], want);
    }

    #[test]
    fn coequal_bridge_is_flagged() {
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:axisA gmeow:coequalFacet true ; rdfs:range gmeow:RangeA .\n\
             gmeow:axisB gmeow:coequalFacet true ; rdfs:range gmeow:RangeB ;\n\
               rdfs:subPropertyOf gmeow:axisA .\n"
        ));
        let problems = coequal_facet_orthogonality(&store, &cfg());
        assert!(problems.iter().any(|p| p.contains("bridged")));
    }

    #[test]
    fn frame_completeness_is_flagged() {
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:pointsFrame rdfs:subPropertyOf gmeow:hasReferenceFrame ;\n\
               rdfs:domain gmeow:Carrier .\n"
        ));
        let problems = frame_declaration_completeness(&store, &cfg());
        assert!(problems
            .iter()
            .any(|p| p.contains("gmeow:Carrier") && p.contains("P11")));
    }

    #[test]
    fn clean_graph_has_no_problems() {
        let store = store_from(&format!(
            "{PREFIXES}gmeow:Animal a owl:Class , gufo:Kind .\n"
        ));
        assert!(reasoning_invariants(&store, &cfg()).is_empty());
    }

    // ── logic: stereotype acceptance (#694 owl/gUFO → logic: migration) ────────

    #[test]
    fn logic_kind_satisfies_stereotype_requirement() {
        // The canonical logic: form is accepted exactly as gufo: was — no
        // "carries no gUFO meta-class" for a class stereotyped a logic:Kind.
        let store = store_from(&format!(
            "{PREFIXES}gmeow:Animal a owl:Class , logic:Kind .\n"
        ));
        assert!(exactly_one_stereotype(&store, &cfg()).is_empty());
    }

    #[test]
    fn logic_perdurant_rename_is_accepted() {
        // gufo:EventType / gufo:SituationType down-project to logic:Event /
        // logic:Situation; both are valid perdurant stereotypes.
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:Wedding a owl:Class , logic:Event .\n\
             gmeow:Marriage a owl:Class , logic:Situation .\n"
        ));
        assert!(exactly_one_stereotype(&store, &cfg()).is_empty());
    }

    #[test]
    fn logic_sortal_under_logic_kind_passes_mixiden() {
        // A logic:SubKind that traces to exactly one logic:Kind is well-formed.
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:Animal a owl:Class , logic:Kind .\n\
             gmeow:Dog a owl:Class , logic:SubKind ; rdfs:subClassOf gmeow:Animal .\n"
        ));
        assert!(identity_overlap(&store, &cfg()).is_empty());
    }

    #[test]
    fn logic_relator_is_mediation_checked() {
        // An under-mediated logic:Relator is flagged exactly like a gufo:Relator.
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:LonelyBond a owl:Class , logic:Kind ; rdfs:subClassOf logic:Relator .\n\
             gmeow:bondParty a owl:ObjectProperty , owl:FunctionalProperty ;\n\
               rdfs:domain gmeow:LonelyBond ; rdfs:range gmeow:Person .\n"
        ));
        assert!(relator_mediation(&store, &cfg())
            .iter()
            .any(|p| p.contains("RelComp") && p.contains("gmeow:LonelyBond")));
    }

    #[test]
    fn mixed_namespace_double_stereotype_is_flagged() {
        // A class mid-migration carrying BOTH gufo:Kind and logic:Kind is two
        // stereotypes — the cardinality discipline still flags it.
        let store = store_from(&format!(
            "{PREFIXES}gmeow:Half a owl:Class , gufo:Kind , logic:Kind .\n"
        ));
        assert!(exactly_one_stereotype(&store, &cfg())
            .iter()
            .any(|p| p.contains("conflicting stereotypes")));
    }
}
