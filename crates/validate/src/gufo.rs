// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3-free engine for the gUFO/UFO reasoning invariants.
//!
//! The OntoUML anti-pattern checks (`exactly_one_stereotype`, `identity_overlap`,
//! `anti_rigidity_discipline`, `relator_mediation`, `coequal_facet_orthogonality`,
//! `frame_declaration_completeness`) run over a native [`purrdf::RdfDataset`]
//! built from the merged ontology sources, querying through the indexed
//! [`purrdf::DatasetView::quads_for_pattern`]. The aggregator
//! [`reasoning_invariants`] runs the five PRODUCTION checks and flattens their errors.
//! `relator_mediation` (RelComp) is now enforced natively over the whole ontology by the
//! foundation lowering and is retained here only as the regression oracle that native
//! enforcement is validated against — it is not in the production aggregate.
//!
//! The hard parts reproduced exactly:
//!
//! * the proper-ancestor transitive closure over `rdfs:subClassOf`
//!   ([`proper_ancestors`], mirroring rdflib `transitive_objects` minus self),
//! * the `owl:AllDisjointClasses` / `owl:members` RDF-Collection walk
//!   ([`all_disjoint_member_sets`], a manual `rdf:first`/`rdf:rest`/`rdf:nil`
//!   linked-list traversal over term ids since the IR has no Collection helper),
//! * the subPropertyOf/equivalentProperty property-bridge reachability DFS in
//!   [`coequal_facet_orthogonality`].
//!
//! Graph handling: the legacy pipeline flattened named graphs into the default
//! graph, so these read across all graphs with [`purrdf::GraphMatch::Any`].
//!
//! Determinism: wherever the Python sorts (by `str`), this sorts the same way;
//! wherever the Python relied on graph-iteration order, the emitted output is
//! per-class (driven by the sorted [`gmeow_classes`]) or a counted aggregate, so
//! the dataset's quad order never leaks into a diagnostic.
//!
//! Engine-core separation: this module is pure Rust with no binding surface.

use std::collections::{BTreeSet, HashSet, VecDeque};

use gmeow_errors::model::{Finding, Location, Severity};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue};

use crate::model::{owl, rdf, rdfs};

/// Resolve an IRI value to its dataset-local [`TermId`], if interned.
#[inline]
fn iri_id(ds: &RdfDataset, iri: &str) -> Option<TermId> {
    ds.term_id_by_value(&TermValue::iri(iri))
}

/// The OntoUML anti-pattern catalogue URL, cited in messages so failures
/// self-document (mirrors `_CATALOGUE`).
const CATALOGUE: &str = "https://ontouml.readthedocs.io/en/latest/anti-patterns/";

/// The gUFO namespace (`http://purl.org/nemo/gufo#`).
const GUFO_NS: &str = "http://purl.org/nemo/gufo#";

/// The canonical `logic:` namespace (`https://blackcatinformatics.ca/logic/`) —
/// the authoritative sort surface that subsumes gUFO. Slices migrate
/// their stereotype authoring from `gufo:` to `logic:`; this validator accepts
/// EITHER namespace so the per-slice migration can run incrementally with the
/// foundation-conformance gate green at every step (some slices migrated, others
/// not). The local name is identical across the two namespaces for every sort
/// except the perdurant down-projection renames (`gufo:EventType`→`logic:Event`,
/// `gufo:SituationType`→`logic:Situation`).
use gmeow_ns::LOGIC_NS;

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
/// `gufo:` and the canonical `logic:` namespace.
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
/// them to `logic:Event`/`logic:Situation`.
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
fn gmeow_classes(ds: &RdfDataset, cfg: &GufoConfig) -> Vec<String> {
    let mut classes: BTreeSet<String> = BTreeSet::new();
    let Some(type_id) = iri_id(ds, rdf::TYPE) else {
        return Vec::new();
    };
    // A class is typed in the canonical `logic:Class`; its generated OWL view uses
    // `owl:Class`. Iterate BOTH markers so a re-authored slice is not read as
    // class-less after the `owl:`→`logic:` surface flip.
    for class_id in [owl::CLASS, gmeow_ns::LOGIC_CLASS]
        .into_iter()
        .filter_map(|m| iri_id(ds, m))
    {
        for q in ds.quads_for_pattern(None, Some(type_id), Some(class_id), GraphMatch::Any) {
            if let TermRef::Iri(iri) = ds.resolve(q.s)
                && is_gmeow_class_iri(iri, cfg)
            {
                classes.insert(iri.to_owned());
            }
        }
    }
    classes.into_iter().collect()
}

/// Transitive subsumption super-classes of `cls`, excluding itself (mirrors
/// `_proper_ancestors` / rdflib `transitive_objects` minus self).
///
/// A BFS over the subsumption edges — BOTH `rdfs:subClassOf` AND the canonical
/// `logic:subClassOf` it is a Principle-17 projection of — named-node objects only,
/// reflexive closure minus the start node. Traversing `logic:subClassOf` too is what
/// lets a slice ground its SubKind→Kind edge as `logic:subClassOf` (zero ungrounded
/// rdfs residue) while the OntoUML identity checks still trace it to its Kind.
///
/// `pub(crate)`: also the Tier-1 consumer path's subclass-closure injection
/// ([`crate::data_validate`]) reuses this exact BFS over the bundle's ontology dataset,
/// so the `sh:targetClass` shortcut edges it synthesizes trace the SAME subsumption
/// lattice (both `rdfs:subClassOf` and `logic:subClassOf`) these OntoUML checks do.
pub(crate) fn proper_ancestors(ds: &RdfDataset, cls: &str) -> HashSet<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(cls.to_owned());
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(cls.to_owned());
    let subclass_ids: Vec<_> = gmeow_ns::SUB_CLASS_OF
        .iter()
        .filter_map(|p| iri_id(ds, p))
        .collect();
    if subclass_ids.is_empty() {
        return seen;
    }
    while let Some(node) = queue.pop_front() {
        let Some(subject_id) = iri_id(ds, &node) else {
            continue;
        };
        for subclass_id in &subclass_ids {
            for q in
                ds.quads_for_pattern(Some(subject_id), Some(*subclass_id), None, GraphMatch::Any)
            {
                if let TermRef::Iri(parent) = ds.resolve(q.o) {
                    let p = parent.to_owned();
                    if visited.insert(p.clone()) {
                        queue.push_back(p.clone());
                    }
                    if p != cls {
                        seen.insert(p);
                    }
                }
            }
        }
    }
    seen
}

/// The gUFO meta-classes `cls` is punned as, via `rdf:type` (mirrors
/// `_stereotypes`). Only IRIs in `meta` are kept.
fn stereotypes(ds: &RdfDataset, cls: &str, meta: &HashSet<String>) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    let (Some(subject_id), Some(type_id)) = (iri_id(ds, cls), iri_id(ds, rdf::TYPE)) else {
        return out;
    };
    for q in ds.quads_for_pattern(Some(subject_id), Some(type_id), None, GraphMatch::Any) {
        if let TermRef::Iri(t) = ds.resolve(q.o)
            && meta.contains(t)
        {
            out.insert(t.to_owned());
        }
    }
    out
}

/// Whether `cls` has `rdf:type cls_type` (a direct type probe).
fn has_type(ds: &RdfDataset, cls: &str, cls_type: &str) -> bool {
    let (Some(subject_id), Some(type_id), Some(object_id)) =
        (iri_id(ds, cls), iri_id(ds, rdf::TYPE), iri_id(ds, cls_type))
    else {
        return false;
    };
    ds.quads_for_pattern(
        Some(subject_id),
        Some(type_id),
        Some(object_id),
        GraphMatch::Any,
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
pub fn exactly_one_stereotype(ds: &RdfDataset, cfg: &GufoConfig) -> Vec<Finding> {
    let meta = meta_classes();
    let mut problems: Vec<Finding> = Vec::new();
    for cls in gmeow_classes(ds, cfg) {
        let st = stereotypes(ds, &cls, &meta);
        if st.is_empty() {
            let message = format!(
                "{} carries no stereotype — pun it with exactly one of \
                 Kind/SubKind/Role/Phase/Category/Mixin/RoleMixin/PhaseMixin \
                 (Event/Situation for perdurants, or \
                 AbstractIndividualType for abstract individuals)",
                local(&cls, cfg)
            );
            let mut f = Finding::new(
                Severity::Error,
                crate::codes::DISCIPLINE_STEREOTYPE,
                message,
            );
            f.add_location(Location {
                logical: Some(cls.clone()),
                ..Location::default()
            });
            problems.push(f);
        } else if st.len() > 1 {
            let mut names: Vec<String> = st.iter().map(|s| local(s, cfg)).collect();
            names.sort();
            let message = format!(
                "{} carries conflicting stereotypes ({}) — a class has \
                 exactly one stereotype",
                local(&cls, cfg),
                names.join(", ")
            );
            let mut f = Finding::new(
                Severity::Error,
                crate::codes::DISCIPLINE_STEREOTYPE,
                message,
            );
            f.add_location(Location {
                logical: Some(cls.clone()),
                ..Location::default()
            });
            problems.push(f);
        }
    }
    problems
}

/// **identity_overlap (MixIden)** — a sortal inherits identity from exactly one
/// Kind; no Kind ⊑ Kind.
pub fn identity_overlap(ds: &RdfDataset, cfg: &GufoConfig) -> Vec<Finding> {
    let meta = meta_classes();
    // A Kind in either namespace (gufo:Kind or the canonical logic:Kind).
    let kinds: [String; 2] = dual("Kind");
    let is_kind = |s: &String| kinds.contains(s);
    let rigid = rigid_sortals();
    let anti_rigid = anti_rigid_sortals();
    let mut problems: Vec<Finding> = Vec::new();
    for cls in gmeow_classes(ds, cfg) {
        let st = stereotypes(ds, &cls, &meta);
        let ancestors = proper_ancestors(ds, &cls);
        // kind_ancestors: ancestors that are themselves a Kind, sorted by str.
        let mut kind_ancestors: Vec<String> = ancestors
            .iter()
            .filter(|a| kinds.iter().any(|k| has_type(ds, a, k)))
            .cloned()
            .collect();
        kind_ancestors.sort();

        if st.iter().any(is_kind) && !kind_ancestors.is_empty() {
            let message = format!(
                "{} is a Kind but specializes Kind(s) {} — identity \
                 conflict (OntoUML MixIden: every endurant instantiates exactly \
                 one Kind). See {}",
                local(&cls, cfg),
                join_local(&kind_ancestors, cfg),
                CATALOGUE
            );
            let mut f = Finding::new(
                Severity::Error,
                crate::codes::DISCIPLINE_IDENTITY_OVERLAP,
                message,
            );
            f.add_location(Location {
                logical: Some(cls.clone()),
                ..Location::default()
            });
            problems.push(f);
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
            let message = format!(
                "{} is a sortal but specializes {} Kind(s) ({}) — a sortal \
                 inherits identity from exactly one Kind (OntoUML MixIden). See {}",
                local(&cls, cfg),
                kind_ancestors.len(),
                names,
                CATALOGUE
            );
            let mut f = Finding::new(
                Severity::Error,
                crate::codes::DISCIPLINE_IDENTITY_OVERLAP,
                message,
            );
            f.add_location(Location {
                logical: Some(cls.clone()),
                ..Location::default()
            });
            problems.push(f);
        }
    }
    problems
}

/// **anti_rigidity_discipline (MixRig / FreeRole)** — anti-rigid sortals need a
/// rigid super; rigid types avoid anti-rigid ancestors.
pub fn anti_rigidity_discipline(ds: &RdfDataset, cfg: &GufoConfig) -> Vec<Finding> {
    let meta = meta_classes();
    let rigid = rigid_sortals();
    let anti_rigid = anti_rigid_sortals();
    let anti_rigid_t = anti_rigid_types();
    let mut problems: Vec<Finding> = Vec::new();
    for cls in gmeow_classes(ds, cfg) {
        let st = stereotypes(ds, &cls, &meta);
        let ancestors = proper_ancestors(ds, &cls);

        // Accumulate every ancestor's meta-class stereotypes.
        let mut ancestor_stereotypes: HashSet<String> = HashSet::new();
        for ancestor in &ancestors {
            for s in stereotypes(ds, ancestor, &meta) {
                ancestor_stereotypes.insert(s);
            }
        }

        let is_anti_rigid = st.iter().any(|s| anti_rigid.contains(s));
        let ancestor_has_rigid = ancestor_stereotypes.iter().any(|s| rigid.contains(s));
        if is_anti_rigid && !ancestor_has_rigid {
            let message = format!(
                "{} is an anti-rigid sortal (Role/Phase) but specializes no rigid \
                 sortal — nowhere to inherit a principle of identity (OntoUML \
                 FreeRole). See {}",
                local(&cls, cfg),
                CATALOGUE
            );
            let mut f = Finding::new(
                Severity::Error,
                crate::codes::DISCIPLINE_ANTI_RIGIDITY,
                message,
            );
            f.add_location(Location {
                logical: Some(cls.clone()),
                ..Location::default()
            });
            problems.push(f);
        }

        let is_rigid = st.iter().any(|s| rigid.contains(s));
        if is_rigid {
            // Name the offending ancestor class(es) and their anti-rigid stereotype.
            let mut bad_ancestors: Vec<String> = Vec::new();
            for ancestor in &ancestors {
                let bad: Vec<String> = stereotypes(ds, ancestor, &meta)
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
                let message = format!(
                    "{} is a rigid sortal (Kind/SubKind) but specializes anti-rigid \
                     ancestor(s) {} — a rigid type cannot inherit contingent \
                     instantiation (OntoUML MixRig). See {}",
                    local(&cls, cfg),
                    bad_ancestors.join(", "),
                    CATALOGUE
                );
                let mut f = Finding::new(
                    Severity::Error,
                    crate::codes::DISCIPLINE_ANTI_RIGIDITY,
                    message,
                );
                f.add_location(Location {
                    logical: Some(cls.clone()),
                    ..Location::default()
                });
                problems.push(f);
            }
        }
    }
    problems
}

/// **relator_mediation (RelComp)** — every concrete `gufo:Relator` mediates at
/// least two relata.
pub fn relator_mediation(ds: &RdfDataset, cfg: &GufoConfig) -> Vec<Finding> {
    // A Relator base in either namespace (gufo:Relator or canonical logic:Relator).
    let relators: [String; 2] = dual("Relator");
    let time_interval = cfg.time_interval();

    // The GMEOW object properties (graph iteration order; output is count-only).
    let mut gmeow_object_properties: Vec<String> = Vec::new();
    if let Some(type_id) = iri_id(ds, rdf::TYPE) {
        // Both the canonical `logic:ObjectProperty` and its generated `owl:` view.
        for obj_prop_id in [owl::OBJECT_PROPERTY, gmeow_ns::LOGIC_OBJECT_PROPERTY]
            .into_iter()
            .filter_map(|m| iri_id(ds, m))
        {
            for q in ds.quads_for_pattern(None, Some(type_id), Some(obj_prop_id), GraphMatch::Any) {
                if let TermRef::Iri(iri) = ds.resolve(q.s)
                    && iri.starts_with(&cfg.namespace)
                {
                    gmeow_object_properties.push(iri.to_owned());
                }
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
            domains: object_iris(ds, iri, rdfs::DOMAIN),
            ranges: object_iris(ds, iri, rdfs::RANGE),
            functional: is_functional(ds, iri),
        })
        .collect();

    let mut problems: Vec<Finding> = Vec::new();
    for cls in gmeow_classes(ds, cfg) {
        let ancestors = proper_ancestors(ds, &cls);
        if !relators.iter().any(|r| ancestors.contains(r)) {
            continue;
        }
        // Concrete iff no GMEOW class specializes it.
        let has_gmeow_subclass = gmeow_subclasses(ds, &cls)
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
            let message = format!(
                "{} is a concrete Relator mediating only {} end(s) — a relator \
                 must mediate at least two (OntoUML RelComp). See {}",
                local(&cls, cfg),
                ends,
                CATALOGUE
            );
            let mut f = Finding::new(
                Severity::Error,
                crate::codes::DISCIPLINE_RELATOR_MEDIATION,
                message,
            );
            f.add_location(Location {
                logical: Some(cls.clone()),
                ..Location::default()
            });
            problems.push(f);
        }
    }
    problems
}

/// Subjects of `(?, rdfs:subClassOf | logic:subClassOf, cls)` (named-node subjects
/// only) — the classes that directly specialize `cls` over either the projected
/// `rdfs:subClassOf` or the canonical `logic:subClassOf` subsumption edge.
fn gmeow_subclasses(ds: &RdfDataset, cls: &str) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    let Some(object_id) = iri_id(ds, cls) else {
        return out;
    };
    for pred in gmeow_ns::SUB_CLASS_OF {
        let Some(subclass_id) = iri_id(ds, pred) else {
            continue;
        };
        for q in ds.quads_for_pattern(None, Some(subclass_id), Some(object_id), GraphMatch::Any) {
            if let TermRef::Iri(n) = ds.resolve(q.s) {
                out.insert(n.to_owned());
            }
        }
    }
    out
}

/// Named-node object IRIs of `(subject_iri, predicate_iri, ?)`.
fn object_iris(ds: &RdfDataset, subject_iri: &str, predicate_iri: &str) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    let (Some(subject_id), Some(predicate_id)) =
        (iri_id(ds, subject_iri), iri_id(ds, predicate_iri))
    else {
        return out;
    };
    for q in ds.quads_for_pattern(Some(subject_id), Some(predicate_id), None, GraphMatch::Any) {
        if let TermRef::Iri(n) = ds.resolve(q.o) {
            out.insert(n.to_owned());
        }
    }
    out
}

/// Whether `prop` is declared functional — in the canonical
/// `logic:functionalProperty` or its generated `owl:FunctionalProperty` view.
fn is_functional(ds: &RdfDataset, prop: &str) -> bool {
    has_type(ds, prop, &logic("functionalProperty")) || has_type(ds, prop, owl::FUNCTIONAL_PROPERTY)
}

/// Every `owl:AllDisjointClasses` axiom's member set (mirrors
/// `_all_disjoint_member_sets`). Walks the `owl:members` RDF Collection by hand.
fn all_disjoint_member_sets(ds: &RdfDataset) -> Vec<HashSet<String>> {
    let mut sets: Vec<HashSet<String>> = Vec::new();
    let (Some(type_id), Some(adc_id), Some(members_id)) = (
        iri_id(ds, rdf::TYPE),
        iri_id(ds, owl::ALL_DISJOINT_CLASSES),
        iri_id(ds, owl::MEMBERS),
    ) else {
        return sets;
    };
    for q in ds.quads_for_pattern(None, Some(type_id), Some(adc_id), GraphMatch::Any) {
        let node = q.s;
        for members_q in ds.quads_for_pattern(Some(node), Some(members_id), None, GraphMatch::Any) {
            // The collection head is a named or blank node; a literal/triple head is
            // skipped (matching the legacy `NamedNode | BlankNode` guard).
            if matches!(
                ds.resolve(members_q.o),
                TermRef::Iri(_) | TermRef::Blank { .. }
            ) {
                sets.push(collection_members(ds, members_q.o));
            }
        }
    }
    sets
}

/// Walk an RDF Collection (`rdf:first`/`rdf:rest`/`rdf:nil` linked list) from the
/// `head` term id, returning the IRI members (`isinstance(m, URIRef)` in the Python).
///
/// rdflib's `Collection` follows `rdf:rest` cells; this mirrors that, guarding
/// against a malformed cyclic list by tracking visited cells (by term id).
fn collection_members(ds: &RdfDataset, head: TermId) -> HashSet<String> {
    let rdf_nil = iri_id(ds, rdf::NIL);
    let (Some(first_id), Some(rest_id)) = (iri_id(ds, rdf::FIRST), iri_id(ds, rdf::REST)) else {
        return HashSet::new();
    };
    let mut out: HashSet<String> = HashSet::new();
    let mut visited: HashSet<TermId> = HashSet::new();
    let mut cell: Option<TermId> = Some(head);
    while let Some(node) = cell {
        if Some(node) == rdf_nil || !visited.insert(node) {
            break;
        }
        // rdf:first → a member (IRI only kept).
        if let Some(first) = ds
            .quads_for_pattern(Some(node), Some(first_id), None, GraphMatch::Any)
            .next()
            && let TermRef::Iri(m) = ds.resolve(first.o)
        {
            out.insert(m.to_owned());
        }
        // rdf:rest → the next cell (named or blank node only).
        cell = ds
            .quads_for_pattern(Some(node), Some(rest_id), None, GraphMatch::Any)
            .next()
            .filter(|q| matches!(ds.resolve(q.o), TermRef::Iri(_) | TermRef::Blank { .. }))
            .map(|q| q.o);
    }
    out
}

/// **coequal_facet_orthogonality (P9)** — co-equal facet axes stay
/// orthogonal.
pub fn coequal_facet_orthogonality(ds: &RdfDataset, cfg: &GufoConfig) -> Vec<Finding> {
    let coequal = format!("{}coequalFacet", cfg.namespace);

    // axes = sorted subjects of (?, coequalFacet, "true").
    let mut axes_set: BTreeSet<String> = BTreeSet::new();
    if let Some(coequal_id) = iri_id(ds, &coequal) {
        for q in ds.quads_for_pattern(None, Some(coequal_id), None, GraphMatch::Any) {
            let is_true =
                matches!(ds.resolve(q.o), TermRef::Literal { lexical, .. } if lexical == "true");
            if !is_true {
                continue;
            }
            if let TermRef::Iri(n) = ds.resolve(q.s) {
                axes_set.insert(n.to_owned());
            }
        }
    }
    let axes: Vec<String> = axes_set.into_iter().collect();
    if axes.is_empty() {
        return Vec::new();
    }

    let mut problems: Vec<Finding> = Vec::new();
    // ranges: axis -> its single range (only populated when exactly one range).
    let mut ranges: Vec<(String, String)> = Vec::new();
    for axis in &axes {
        let mut axis_ranges: Vec<String> = object_iris(ds, axis, rdfs::RANGE).into_iter().collect();
        axis_ranges.sort();
        if axis_ranges.len() != 1 {
            let message = format!(
                "co-equal facet {} must have exactly one rdfs:range (found {}) — \
                 each axis owns its own value space",
                local(axis, cfg),
                axis_ranges.len()
            );
            let mut f = Finding::new(
                Severity::Error,
                crate::codes::DISCIPLINE_COEQUAL_ORTHOGONALITY,
                message,
            );
            f.add_location(Location {
                logical: Some(axis.clone()),
                ..Location::default()
            });
            problems.push(f);
            continue;
        }
        ranges.push((axis.clone(), axis_ranges[0].clone()));
        if is_functional(ds, axis) {
            let message = format!(
                "co-equal facet {} is owl:FunctionalProperty — a locked single \
                 value contradicts co-equality (P9) and invites sameAs collapse (P5)",
                local(axis, cfg)
            );
            let mut f = Finding::new(
                Severity::Error,
                crate::codes::DISCIPLINE_COEQUAL_ORTHOGONALITY,
                message,
            );
            f.add_location(Location {
                logical: Some(axis.clone()),
                ..Location::default()
            });
            problems.push(f);
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
            let message = format!(
                "co-equal facets {} share the range {} — axes collapsed into one \
                 value space",
                names,
                local(rng, cfg)
            );
            // No single focus node — multiple axes involved; emit without location.
            problems.push(Finding::new(
                Severity::Error,
                crate::codes::DISCIPLINE_COEQUAL_ORTHOGONALITY,
                message,
            ));
        }
    }

    // Bridge check over the transitive closure: subPropertyOf is directed,
    // equivalentProperty is symmetric.
    let bridged = bridged_pairs(ds, &axes);
    for (a, b) in bridged {
        let message = format!(
            "co-equal facets {} and {} are bridged by a \
             subPropertyOf/equivalentProperty chain — one axis must never be \
             inferred from another",
            local(&a, cfg),
            local(&b, cfg)
        );
        // Two nodes involved — attach the first (a) as the focus node.
        let mut f = Finding::new(
            Severity::Error,
            crate::codes::DISCIPLINE_COEQUAL_ORTHOGONALITY,
            message,
        );
        f.add_location(Location {
            logical: Some(a.clone()),
            ..Location::default()
        });
        problems.push(f);
    }

    // Jointly: every axis range must sit inside one owl:AllDisjointClasses axiom.
    let member_sets = all_disjoint_member_sets(ds);
    let range_set: HashSet<String> = ranges.iter().map(|(_, r)| r.clone()).collect();
    if range_set.len() > 1 && !member_sets.iter().any(|s| range_set.is_subset(s)) {
        let mut names: Vec<String> = range_set.iter().map(|r| local(r, cfg)).collect();
        names.sort();
        let message = format!(
            "the co-equal facet ranges ({}) are not jointly declared in one \
             owl:AllDisjointClasses axiom — the orthogonality matrix is not \
             visible to the OWL 2 DL reasoner",
            names.join(", ")
        );
        // No single focus node — the axiom is missing; emit without location.
        problems.push(Finding::new(
            Severity::Error,
            crate::codes::DISCIPLINE_COEQUAL_ORTHOGONALITY,
            message,
        ));
    }
    problems
}

/// The bridged axis pairs `(a, b)` for `a` before `b` in the sorted `axes`,
/// where `b` is reachable from `a` or `a` from `b` over the
/// subPropertyOf/equivalentProperty adjacency (mirrors the Python double loop).
fn bridged_pairs(ds: &RdfDataset, axes: &[String]) -> Vec<(String, String)> {
    // adjacency: directed subPropertyOf (both the canonical `logic:subPropertyOf`
    // edge and its `rdfs:` projection — gmeow_ns::SUB_PROPERTY_OF doctrine;
    // crates/ns/src/lib.rs:106-166) + symmetric equivalentProperty.
    use std::collections::HashMap;
    let mut adjacency: HashMap<String, HashSet<String>> = HashMap::new();
    for pred in gmeow_ns::SUB_PROPERTY_OF {
        let Some(subprop_id) = iri_id(ds, pred) else {
            continue;
        };
        for q in ds.quads_for_pattern(None, Some(subprop_id), None, GraphMatch::Any) {
            if let (TermRef::Iri(s), TermRef::Iri(o)) = (ds.resolve(q.s), ds.resolve(q.o)) {
                adjacency
                    .entry(s.to_owned())
                    .or_default()
                    .insert(o.to_owned());
            }
        }
    }
    // Property equivalence is authored in the canonical `logic:equivalentProperty`;
    // its generated OWL view is `owl:equivalentProperty`. Fold BOTH into adjacency.
    for equiv_id in [
        logic("equivalentProperty"),
        owl::EQUIVALENT_PROPERTY.to_owned(),
    ]
    .into_iter()
    .filter_map(|m| iri_id(ds, &m))
    {
        for q in ds.quads_for_pattern(None, Some(equiv_id), None, GraphMatch::Any) {
            if let (TermRef::Iri(s), TermRef::Iri(o)) = (ds.resolve(q.s), ds.resolve(q.o)) {
                let (s, o) = (s.to_owned(), o.to_owned());
                adjacency.entry(s.clone()).or_default().insert(o.clone());
                adjacency.entry(o).or_default().insert(s);
            }
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

/// **frame_declaration_completeness (P11)** — frame-pointing property
/// carrier classes declare `gmeow:requiresFrame`.
pub fn frame_declaration_completeness(ds: &RdfDataset, cfg: &GufoConfig) -> Vec<Finding> {
    let has_frame = format!("{}hasReferenceFrame", cfg.namespace);
    let requires = format!("{}requiresFrame", cfg.namespace);
    let requires_id = iri_id(ds, &requires);

    // props = sorted union of transitive_subjects(subPropertyOf, has_frame) over BOTH
    // the canonical `logic:subPropertyOf` edge and its `rdfs:` projection
    // (gmeow_ns::SUB_PROPERTY_OF doctrine; crates/ns/src/lib.rs:106-166), minus
    // has_frame itself — a frame-pointing property re-authored to `logic:subPropertyOf`
    // must still be found.
    let mut props_set: HashSet<String> = HashSet::new();
    for pred in gmeow_ns::SUB_PROPERTY_OF {
        props_set.extend(transitive_subjects(ds, pred, &has_frame));
    }
    let mut props: Vec<String> = props_set.into_iter().filter(|p| p != &has_frame).collect();
    props.sort();

    let mut problems: Vec<Finding> = Vec::new();
    for prop in &props {
        let mut domains: Vec<String> = object_iris(ds, prop, rdfs::DOMAIN).into_iter().collect();
        domains.sort();
        for domain in &domains {
            // (domain, requires, prop) not in graph.
            let present = match (requires_id, iri_id(ds, domain), iri_id(ds, prop)) {
                (Some(p_id), Some(s_id), Some(o_id)) => ds
                    .quads_for_pattern(Some(s_id), Some(p_id), Some(o_id), GraphMatch::Any)
                    .next()
                    .is_some(),
                _ => false,
            };
            if !present {
                let message = format!(
                    "{} carries the frame-pointing property {} but declares no \
                     gmeow:requiresFrame for it — the frame-relativity shape would \
                     be missing (P11)",
                    local(domain, cfg),
                    local(prop, cfg)
                );
                let mut f = Finding::new(
                    Severity::Error,
                    crate::codes::DISCIPLINE_FRAME_COMPLETENESS,
                    message,
                );
                f.add_location(Location {
                    logical: Some(domain.clone()),
                    ..Location::default()
                });
                problems.push(f);
            }
        }
    }
    problems
}

/// Reverse transitive closure: every subject reaching `target` over `predicate`
/// (mirrors rdflib `transitive_subjects`, reflexive — includes `target`).
fn transitive_subjects(ds: &RdfDataset, predicate_iri: &str, target: &str) -> HashSet<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    seen.insert(target.to_owned());
    queue.push_back(target.to_owned());
    let Some(predicate_id) = iri_id(ds, predicate_iri) else {
        return seen;
    };
    while let Some(node) = queue.pop_front() {
        let Some(object_id) = iri_id(ds, &node) else {
            continue;
        };
        for q in ds.quads_for_pattern(None, Some(predicate_id), Some(object_id), GraphMatch::Any) {
            if let TermRef::Iri(s) = ds.resolve(q.s) {
                let s = s.to_owned();
                if seen.insert(s.clone()) {
                    queue.push_back(s);
                }
            }
        }
    }
    seen
}

/// Run every production UFO anti-pattern check; an empty list means the graph is clean.
///
/// The relator-mediation discipline (RelComp) is deliberately NOT run here. It is now
/// enforced natively over the WHOLE ontology by the foundation lowering — the canonical
/// `logic:` enforcement mechanism, proved by the `whole_bundle_relcomp_gate` coherence-gate
/// teeth test. The [`relator_mediation`] check below is retained as the regression ORACLE
/// the native lowering is validated against (via its own unit tests and the projection
/// conformance case), not as a second production enforcer — one canonical enforcer, no
/// divergence between two mediation readings.
pub fn reasoning_findings(ds: &RdfDataset, cfg: &GufoConfig) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    out.extend(exactly_one_stereotype(ds, cfg));
    out.extend(identity_overlap(ds, cfg));
    out.extend(anti_rigidity_discipline(ds, cfg));
    out.extend(coequal_facet_orthogonality(ds, cfg));
    out.extend(frame_declaration_completeness(ds, cfg));
    out
}

/// Run every UFO anti-pattern check; an empty list means the graph is clean
/// (mirrors `reasoning_invariants`). The six checks run in the same order, their
/// errors flattened.
///
/// This is a pure projection of [`reasoning_findings`] to preserve the string
/// interface for all existing callers (Python bindings, reasoning-parity tests).
pub fn reasoning_invariants(ds: &RdfDataset, cfg: &GufoConfig) -> Vec<String> {
    reasoning_findings(ds, cfg)
        .into_iter()
        .map(|f| f.message)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::parse_dataset;
    use std::sync::Arc;

    const NS: &str = "https://blackcatinformatics.ca/gmeow/";

    fn cfg() -> GufoConfig {
        GufoConfig {
            namespace: NS.to_owned(),
        }
    }

    fn store_from(ttl: &str) -> Arc<RdfDataset> {
        parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap()
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
        assert!(
            problems
                .iter()
                .any(|p| p.message.contains("carries no stereotype"))
        );
    }

    #[test]
    fn conflicting_stereotypes_are_flagged() {
        let store = store_from(&format!(
            "{PREFIXES}gmeow:TwoFaced a owl:Class , gufo:Kind , gufo:Role .\n"
        ));
        let problems = exactly_one_stereotype(&store, &cfg());
        assert!(
            problems
                .iter()
                .any(|p| p.message.contains("conflicting stereotypes"))
        );
    }

    #[test]
    fn kind_under_kind_is_flagged_mixiden() {
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:Animal a owl:Class , gufo:Kind .\n\
             gmeow:Dog a owl:Class , gufo:Kind ; rdfs:subClassOf gmeow:Animal .\n"
        ));
        let problems = identity_overlap(&store, &cfg());
        assert!(
            problems
                .iter()
                .any(|p| p.message.contains("MixIden") && p.message.contains("gmeow:Dog"))
        );
    }

    #[test]
    fn free_role_is_flagged() {
        let store = store_from(&format!(
            "{PREFIXES}gmeow:Wanderer a owl:Class , gufo:Role .\n"
        ));
        let problems = anti_rigidity_discipline(&store, &cfg());
        assert!(problems.iter().any(|p| p.message.contains("FreeRole")));
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
            p.message.contains("MixRig")
                && p.message.contains("gmeow:HonorsStudent")
                && p.message.contains("gmeow:Student")
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
        assert!(
            problems
                .iter()
                .any(|p| p.message.contains("RelComp") && p.message.contains("gmeow:LonelyBond"))
        );
    }

    #[test]
    fn relator_finding_has_discipline_code_and_location() {
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:LonelyBond a owl:Class , gufo:Kind ; rdfs:subClassOf gufo:Relator .\n\
             gmeow:bondParty a owl:ObjectProperty , owl:FunctionalProperty ;\n\
               rdfs:domain gmeow:LonelyBond ; rdfs:range gmeow:Person .\n"
        ));
        let problems = relator_mediation(&store, &cfg());
        let finding = problems
            .iter()
            .find(|p| p.message.contains("gmeow:LonelyBond"))
            .expect("under-mediated relator finding must be present");
        assert_eq!(finding.code, "discipline/relator-mediation");
        assert!(
            finding.locations.iter().any(|loc| loc
                .logical
                .as_deref()
                .is_some_and(|l| l.contains("LonelyBond"))),
            "finding must carry a logical location for LonelyBond"
        );
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
        assert!(
            !relator_mediation(&store, &cfg())
                .iter()
                .any(|p| p.message.contains("gmeow:AbstractBond"))
        );
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
        assert!(problems.iter().any(|p| p.message.contains("bridged")));
    }

    #[test]
    fn frame_completeness_is_flagged() {
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:pointsFrame rdfs:subPropertyOf gmeow:hasReferenceFrame ;\n\
               rdfs:domain gmeow:Carrier .\n"
        ));
        let problems = frame_declaration_completeness(&store, &cfg());
        assert!(
            problems
                .iter()
                .any(|p| p.message.contains("gmeow:Carrier") && p.message.contains("P11"))
        );
    }

    #[test]
    fn clean_graph_has_no_problems() {
        let store = store_from(&format!(
            "{PREFIXES}gmeow:Animal a owl:Class , gufo:Kind .\n"
        ));
        assert!(reasoning_invariants(&store, &cfg()).is_empty());
    }

    // ── logic: stereotype acceptance (owl/gUFO → logic: migration) ────────

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
        assert!(
            relator_mediation(&store, &cfg())
                .iter()
                .any(|p| p.message.contains("RelComp") && p.message.contains("gmeow:LonelyBond"))
        );
    }

    /// The subsumption traversals see a CANONICAL `logic:subClassOf` edge — the
    /// property the shared [`gmeow_ns::SUB_CLASS_OF`] definition guarantees, pinned
    /// here so a future narrowing of it re-reds this crate too.
    ///
    /// `proper_ancestors` (via `identity_overlap`) and `gmeow_subclasses` (via
    /// `relator_mediation`) must both trace an edge authored with NO `rdfs:`
    /// spelling anywhere; an `rdfs:`-only read would report both fixtures clean by
    /// simply not seeing the hierarchy.
    #[test]
    fn canonical_logic_subclass_edges_are_traversed() {
        // MixIden: two logic:Kind ancestors reached ONLY over `logic:subClassOf`.
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:Animal a owl:Class , logic:Kind .\n\
             gmeow:Machine a owl:Class , logic:Kind .\n\
             gmeow:Cyborg a owl:Class , logic:SubKind ;\n\
               logic:subClassOf gmeow:Animal , gmeow:Machine .\n"
        ));
        assert!(
            identity_overlap(&store, &cfg())
                .iter()
                .any(|p| p.message.contains("MixIden") && p.message.contains("gmeow:Cyborg")),
            "proper_ancestors must traverse the canonical subsumption edge"
        );

        // RelComp: a two-level chain — AbstractBond specializes logic:Relator, and
        // LonelyBond specializes AbstractBond — with EVERY edge authored only over
        // the canonical `logic:subClassOf` spelling (no `rdfs:` anywhere). This
        // pins `gmeow_subclasses` (not just `proper_ancestors`): a mutation to
        // RDFS-only `gmeow_subclasses` REDS this exact fixture, because AbstractBond
        // would then look concrete (no subclass found) and wrongly earn its own
        // RelComp finding instead of being skipped as the abstract base. (Verified:
        // reverting `gmeow_subclasses` to `[gmeow_ns::RDFS_SUB_CLASS_OF]` alone kept
        // the ORIGINAL single-level fixture green — LonelyBond had no subclasses
        // either way, so it never exercised `gmeow_subclasses` at all — which is
        // exactly why this fixture was extended to a second level.)
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:AbstractBond a owl:Class , logic:Kind ; logic:subClassOf logic:Relator .\n\
             gmeow:LonelyBond a owl:Class , logic:Kind ; logic:subClassOf gmeow:AbstractBond .\n\
             gmeow:bondParty a owl:ObjectProperty , owl:FunctionalProperty ;\n\
               rdfs:domain gmeow:LonelyBond ; rdfs:range gmeow:Person .\n"
        ));

        let abstract_bond = format!("{NS}AbstractBond");
        let lonely_bond = format!("{NS}LonelyBond");
        assert!(
            gmeow_subclasses(&store, &abstract_bond).contains(&lonely_bond),
            "gmeow_subclasses must match LonelyBond as a subclass of AbstractBond over \
             the canonical logic:subClassOf edge"
        );

        let findings = relator_mediation(&store, &cfg());
        assert!(
            findings
                .iter()
                .any(|p| p.message.contains("RelComp") && p.message.contains("gmeow:LonelyBond")),
            "the concrete child LonelyBond must get the RelComp finding: {findings:?}"
        );
        assert!(
            !findings
                .iter()
                .any(|p| p.message.contains("gmeow:AbstractBond")),
            "the abstract base AbstractBond must NOT get its own finding — its concrete \
             subtype carries the mediation: {findings:?}"
        );
    }

    #[test]
    fn mixed_namespace_double_stereotype_is_flagged() {
        // A class mid-migration carrying BOTH gufo:Kind and logic:Kind is two
        // stereotypes — the cardinality discipline still flags it.
        let store = store_from(&format!(
            "{PREFIXES}gmeow:Half a owl:Class , gufo:Kind , logic:Kind .\n"
        ));
        assert!(
            exactly_one_stereotype(&store, &cfg())
                .iter()
                .any(|p| p.message.contains("conflicting stereotypes"))
        );
    }
}
