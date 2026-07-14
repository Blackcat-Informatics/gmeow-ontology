// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The shared ungrounded-residue enumerator — the single count semantics fed to the
//! seed, the ratchet gate, AND the advisory `axisShapeMigration` axis, so the three
//! can never silently diverge on "what is a construct" or "what counts as grounded."
//!
//! Per Principle 17, OWL, SHACL, gUFO, BFO, DOLCE, and the alignment stack (SSSOM,
//! EDOAL, FnO) are generated lossy projections of `logic:`. A hand-authored
//! construct in one of those surfaces that carries no back-reference to the `logic:`
//! axiom it was derived from is a second source of truth — the *ungrounded residue*
//! this module measures.
//!
//! [`enumerate`] is the one enumeration primitive, parameterized by [`CountMode`]:
//! - [`CountMode::FullResidue`] is the gate's scope — every slice-owned surface,
//!   structural-role construct detection, RESOLVABLE grounding (a dangling
//!   `logic:formalizes`/`logic:grounds` does not ground), and the external-object-only
//!   by-reference bridge carve-out.
//! - [`CountMode::Historical`] pins every one of those dimensions to the *legacy*
//!   `shape_migration_axis` behaviour — presence-only grounding, no bridge
//!   subtraction, typed-shape-only detection — so the advisory axis's measured
//!   scores stay bit-identical across the refactor. It exists ONLY so that axis can
//!   share this implementation instead of duplicating it; it is not a second, looser
//!   gate semantics.
//!
//! [`residue`] and [`grounded_fraction`] are the two derived quantities callers
//! actually want: the gate counts [`residue`] (FullResidue, ungrounded and
//! non-bridge), the axis computes [`grounded_fraction`] (Historical, grounded/total).

use std::collections::BTreeSet;

use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef};

use crate::graph::{self, RDF_TYPE, all_iris, id, instances_of};
use crate::model::{CountKind, GMEOW, ProjectionVocabulary};

/// SHACL node-shape class IRI.
pub const SH_NODESHAPE: &str = "http://www.w3.org/ns/shacl#NodeShape";
/// SHACL property-shape class IRI.
pub const SH_PROPERTYSHAPE: &str = "http://www.w3.org/ns/shacl#PropertyShape";
/// SHACL `sh:path` predicate — present on every property shape, including an
/// anonymous one nested under `sh:property [ … ]` that carries no `rdf:type` triple
/// of its own (the structural-role blind spot [`CountMode::FullResidue`] closes).
pub const SH_PATH: &str = "http://www.w3.org/ns/shacl#path";
/// SHACL `sh:sparql` predicate — a SPARQL-based constraint component.
pub const SH_SPARQL: &str = "http://www.w3.org/ns/shacl#sparql";
/// SHACL `sh:rule` predicate — a SHACL rule.
pub const SH_RULE: &str = "http://www.w3.org/ns/shacl#rule";
/// The `logic:formalizes` back-reference predicate — a construct naming the
/// `logic:` axiom it was derived from.
pub const LOGIC_FORMALIZES: &str = "https://blackcatinformatics.ca/logic/formalizes";
/// The `logic:grounds` back-reference predicate — the inverse-direction sibling of
/// `logic:formalizes` some producers author instead.
pub const LOGIC_GROUNDS: &str = "https://blackcatinformatics.ca/logic/grounds";
/// The `logic:` core namespace — every guarded vocab's `subsumed_by` witness.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// The enumeration scope [`enumerate`] runs at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountMode {
    /// Legacy `shape_migration_axis` semantics, pinned bit-for-bit: typed-shape-only
    /// construct detection, presence-only grounding, no bridge subtraction. Used
    /// ONLY by the advisory axis so its measured scores never move under this
    /// refactor.
    Historical,
    /// The gate's full scope: structural-role construct detection, resolvable
    /// grounding, and the external-object-only by-reference bridge carve-out.
    FullResidue,
}

/// One enumerated hand-authored construct in a vocab's surface. Crate-visible (not
/// module-private) so the advisory `shape_migration_axis` can iterate the SAME
/// enumeration [`enumerate`] returns to emit its per-shape finding, instead of a
/// second adapter re-deriving the same information.
pub(crate) struct Construct {
    /// A stable, deterministic key identifying the construct — an IRI, a
    /// blank-node-derived key, or (for a whole-triple key) a rendering of all three
    /// term positions. Never re-derived differently between two enumerations of the
    /// same dataset (dedup correctness depends on it).
    pub(crate) key: String,
    /// Whether the construct carries a grounding back-reference (semantics depend on
    /// [`CountMode`]: presence-only under `Historical`, resolvable under
    /// `FullResidue`).
    pub(crate) grounded: bool,
    /// Whether the construct is a by-reference alignment/bridge link to an EXTERNAL
    /// upper-ontology/standard namespace (Principle 5) — exempt from the residue.
    /// Always `false` under `Historical` (no bridge subtraction existed pre-refactor).
    pub(crate) is_bridge: bool,
}

/// A stable key for a resolved term: the IRI itself, a blank-node key derived from
/// its label and scope (so two enumerations of the SAME frozen dataset agree, since
/// blank labels are stable within one `RdfDataset`), or a quoted literal lexical.
fn term_key(ds: &RdfDataset, term: TermId) -> String {
    match ds.resolve(term) {
        TermRef::Iri(iri) => iri.to_owned(),
        TermRef::Blank { label, scope } => format!("_:{label}#{}", scope.0),
        TermRef::Literal { lexical, .. } => format!("\"{lexical}\""),
        TermRef::Triple { s, p, o } => format!(
            "«{} {} {}»",
            term_key(ds, s),
            term_key(ds, p),
            term_key(ds, o)
        ),
    }
}

/// Every subject typed `class_iri` (IRI OR blank-node subjects) — the structural-role
/// sibling of [`graph::instances_of`], which yields IRI subjects only.
fn typed_subjects(ds: &RdfDataset, class_iri: &str) -> Vec<TermId> {
    let (Some(type_p), Some(class_id)) = (id(ds, RDF_TYPE), id(ds, class_iri)) else {
        return Vec::new();
    };
    ds.quads_for_pattern(None, Some(type_p), Some(class_id), GraphMatch::Any)
        .map(|q| q.s)
        .collect()
}

/// Every subject of `pred_iri`, for any object (IRI OR blank-node subjects).
fn subjects_of(ds: &RdfDataset, pred_iri: &str) -> Vec<TermId> {
    let Some(pred_id) = id(ds, pred_iri) else {
        return Vec::new();
    };
    ds.quads_for_pattern(None, Some(pred_id), None, GraphMatch::Any)
        .map(|q| q.s)
        .collect()
}

/// RESOLVABLE grounding: `subject` carries a `logic:formalizes`/`logic:grounds`
/// back-reference AND the referenced object IRI appears as the subject of at least
/// one triple in `ds`. A dangling back-reference (the object is never itself a
/// subject) does NOT ground — otherwise a migration could be faked with a
/// rubber-stamped triple to nowhere (back-ref integrity).
fn resolvable_grounding(ds: &RdfDataset, subject: TermId) -> bool {
    for pred_iri in [LOGIC_FORMALIZES, LOGIC_GROUNDS] {
        let Some(pred_id) = id(ds, pred_iri) else {
            continue;
        };
        for target_iri in all_iris(ds, subject, pred_id) {
            let Some(target_id) = id(ds, &target_iri) else {
                continue; // never appears as any term in ds at all → dangling
            };
            let resolves = ds
                .quads_for_pattern(Some(target_id), None, None, GraphMatch::Any)
                .next()
                .is_some();
            if resolves {
                return true;
            }
        }
    }
    false
}

/// PRESENCE-ONLY grounding: `subject` merely carries a `logic:formalizes`
/// back-reference, with no resolvability check. This is the legacy
/// `shape_migration_axis` criterion (a dangling back-ref grounds exactly as it did
/// before this refactor) — used ONLY under [`CountMode::Historical`].
fn presence_grounding(ds: &RdfDataset, subject: TermId) -> bool {
    id(ds, LOGIC_FORMALIZES).is_some_and(|p| graph::has_any(ds, subject, p))
}

/// Enumerate `vocab`'s hand-authored constructs in `ds` at the given [`CountMode`].
/// The single primitive the gate (full residue), the seed (full residue), and the
/// advisory axis (historical) all share — the "what is a construct" and "what counts
/// as grounded" decisions are made exactly once. Crate-visible: [`crate::axes`]'s
/// `shape_migration_axis` calls this directly at [`CountMode::Historical`] to
/// reproduce its per-shape advisories; external callers use the derived
/// [`residue`]/[`grounded_fraction`] quantities instead.
#[must_use]
pub(crate) fn enumerate(
    ds: &RdfDataset,
    vocab: &ProjectionVocabulary,
    mode: CountMode,
) -> Vec<Construct> {
    match vocab.count_kind {
        CountKind::Shape => enumerate_shape(ds, mode),
        CountKind::TypedAxiom => enumerate_typed_axiom(ds, vocab, mode),
        CountKind::NonRdfSurface => Vec::new(),
    }
}

/// `CountKind::Shape` enumeration.
fn enumerate_shape(ds: &RdfDataset, mode: CountMode) -> Vec<Construct> {
    match mode {
        CountMode::Historical => {
            // Reproduces the pre-refactor `shape_migration_axis` EXACTLY: typed
            // `sh:NodeShape`/`sh:PropertyShape` IRI subjects only, presence-only
            // grounding, no bridge subtraction.
            let mut authored: Vec<String> = instances_of(ds, SH_NODESHAPE);
            authored.extend(instances_of(ds, SH_PROPERTYSHAPE));
            authored.sort();
            authored.dedup();
            authored
                .into_iter()
                .map(|iri| {
                    let grounded = id(ds, &iri).is_some_and(|s| presence_grounding(ds, s));
                    Construct {
                        key: iri,
                        grounded,
                        is_bridge: false,
                    }
                })
                .collect()
        }
        CountMode::FullResidue => {
            // Structural role, not merely `instances_of`: a node counts if it is
            // typed sh:NodeShape/sh:PropertyShape, OR is the subject of sh:path
            // (catches an anonymous nested `sh:property [ sh:path … ]` block), OR is
            // the subject of sh:sparql/sh:rule. IRI and blank-node subjects both
            // count; dedup by a stable node key.
            let mut node_ids: Vec<TermId> = Vec::new();
            node_ids.extend(typed_subjects(ds, SH_NODESHAPE));
            node_ids.extend(typed_subjects(ds, SH_PROPERTYSHAPE));
            node_ids.extend(subjects_of(ds, SH_PATH));
            node_ids.extend(subjects_of(ds, SH_SPARQL));
            node_ids.extend(subjects_of(ds, SH_RULE));

            let mut seen: BTreeSet<String> = BTreeSet::new();
            let mut out = Vec::new();
            for tid in node_ids {
                let key = term_key(ds, tid);
                if !seen.insert(key.clone()) {
                    continue;
                }
                let grounded = resolvable_grounding(ds, tid);
                out.push(Construct {
                    key,
                    grounded,
                    is_bridge: false,
                });
            }
            out.sort_by(|a, b| a.key.cmp(&b.key));
            out
        }
    }
}

/// `CountKind::TypedAxiom` enumeration — only meaningful under [`CountMode::FullResidue`];
/// [`CountMode::Historical`] returns empty (the legacy axis never scored typed-axiom
/// vocabs).
fn enumerate_typed_axiom(
    ds: &RdfDataset,
    vocab: &ProjectionVocabulary,
    mode: CountMode,
) -> Vec<Construct> {
    if mode == CountMode::Historical {
        return Vec::new();
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();
    for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
        let p_iri = match ds.resolve(q.p) {
            TermRef::Iri(iri) => Some(iri),
            _ => None,
        };
        let o_iri = match ds.resolve(q.o) {
            TermRef::Iri(iri) => Some(iri),
            _ => None,
        };
        let in_vocab_ns = |candidate: &str| {
            vocab
                .namespaces
                .iter()
                .any(|ns| candidate.starts_with(ns.as_str()))
        };
        let matches_vocab = p_iri.is_some_and(in_vocab_ns) || o_iri.is_some_and(in_vocab_ns);
        if !matches_vocab {
            continue;
        }
        let key = format!(
            "{}|{}|{}",
            term_key(ds, q.s),
            term_key(ds, q.p),
            term_key(ds, q.o)
        );
        if !seen.insert(key.clone()) {
            continue;
        }
        let grounded = resolvable_grounding(ds, q.s);
        // A by-reference bridge: the predicate is one of the vocab's declared
        // alignment predicates AND the object is an IRI in an EXTERNAL namespace
        // (never `gmeow:`). An internal gmeow↔gmeow object under the SAME predicate
        // (e.g. `gmeow:X owl:equivalentClass gmeow:Y`) is a genuine second source of
        // truth and stays in the residue — the external-object condition applies to
        // EVERY carve-out predicate, no exceptions.
        let is_bridge = p_iri.is_some_and(|p| vocab.alignment_predicates.iter().any(|ap| ap == p))
            && o_iri.is_some_and(|o| !o.starts_with(GMEOW));
        out.push(Construct {
            key,
            grounded,
            is_bridge,
        });
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

/// The ungrounded residue of `vocab` over `ds`: the count of [`CountMode::FullResidue`]
/// constructs that are neither grounded nor an exempt by-reference bridge. This is
/// the quantity the ratchet gate and the seed both count — never diverging, because
/// both call this function.
#[must_use]
pub fn residue(ds: &RdfDataset, vocab: &ProjectionVocabulary) -> u64 {
    enumerate(ds, vocab, CountMode::FullResidue)
        .iter()
        .filter(|c| !c.grounded && !c.is_bridge)
        .count() as u64
}

/// The grounded fraction over [`CountMode::Historical`] enumeration — legacy
/// `grounded / authored` semantics, `1.0` when there is nothing authored. This is the
/// quantity the advisory `shape_migration_axis` measures.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn grounded_fraction(ds: &RdfDataset, vocab: &ProjectionVocabulary) -> f64 {
    let constructs = enumerate(ds, vocab, CountMode::Historical);
    if constructs.is_empty() {
        return 1.0;
    }
    let grounded = constructs.iter().filter(|c| c.grounded).count();
    grounded as f64 / constructs.len() as f64
}

/// The canonical SHACL descriptor the advisory `shape_migration_axis` measures
/// against: SHACL's `sh:` namespace, `Shape`-kind construct detection, subsumed by
/// `logic:` per Principle 17 (validation shapes are a `SoundUnder` approximation of
/// the `logic:` obligation they were derived from), no default ceiling and no
/// alignment predicates (the axis reads no ceiling and performs no bridge
/// subtraction — those fields matter only to the ratchet gate's descriptor).
#[must_use]
pub fn shacl_vocab() -> ProjectionVocabulary {
    ProjectionVocabulary {
        prefix: "sh".to_owned(),
        namespaces: vec!["http://www.w3.org/ns/shacl#".to_owned()],
        subsumed_by: LOGIC_NS.to_owned(),
        count_kind: CountKind::Shape,
        default_ceiling: 0,
        preservation: "SoundUnderApproximation".to_owned(),
        alignment_predicates: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ds_of(ttl: &str) -> std::sync::Arc<RdfDataset> {
        let full = format!(
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             @prefix gufo: <https://w3id.org/gufo#> .\n\
             {ttl}"
        );
        purrdf::parse_dataset(full.as_bytes(), "text/turtle", None)
            .expect("test fixture parses as valid Turtle")
    }

    fn alignment_vocab(prefix: &str, ns: &str) -> ProjectionVocabulary {
        ProjectionVocabulary {
            prefix: prefix.to_owned(),
            namespaces: vec![ns.to_owned()],
            subsumed_by: LOGIC_NS.to_owned(),
            count_kind: CountKind::TypedAxiom,
            default_ceiling: 0,
            preservation: "SoundUnderApproximation".to_owned(),
            alignment_predicates: vec![
                "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned(),
                "http://www.w3.org/2002/07/owl#equivalentClass".to_owned(),
            ],
        }
    }

    #[test]
    fn grounded_shape_not_counted_in_residue() {
        let ds = ds_of(
            r#"
            gmeow:S a sh:NodeShape ; logic:formalizes logic:Obligation .
            logic:Obligation a owl:Class .
            "#,
        );
        assert_eq!(residue(&ds, &shacl_vocab()), 0);
    }

    #[test]
    fn ungrounded_shape_counted_in_residue() {
        let ds = ds_of(
            r#"
            gmeow:S a sh:NodeShape .
            "#,
        );
        assert_eq!(residue(&ds, &shacl_vocab()), 1);
    }

    #[test]
    fn dangling_back_ref_still_counts() {
        // logic:formalizes points at logic:Nowhere, which never appears as a subject
        // of any triple in the dataset — a rubber-stamp, not a real grounding.
        let ds = ds_of(
            r#"
            gmeow:S a sh:NodeShape ; logic:formalizes logic:Nowhere .
            "#,
        );
        assert_eq!(residue(&ds, &shacl_vocab()), 1);
    }

    #[test]
    fn declarative_owl_rdfs_axiom_not_counted_for_guarded_vocab() {
        let ds = ds_of(
            r#"
            gmeow:Widget a owl:Class ; rdfs:subClassOf gmeow:Thing .
            gmeow:Thing a owl:Class .
            "#,
        );
        let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#");
        // Neither triple mentions the gufo namespace at all, so the gUFO-guarded
        // residue over this dataset is 0 — owl/rdfs declarative axioms are simply
        // outside the vocab's own namespace, never a `gufo`-kind construct.
        assert_eq!(residue(&ds, &vocab), 0);
    }

    #[test]
    fn external_gufo_bridge_not_counted_but_internal_gufo_predicate_is() {
        let ds = ds_of(
            r#"
            gmeow:X rdfs:subClassOf gufo:Kind .
            gmeow:X gufo:mediates gmeow:Y .
            "#,
        );
        let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#");
        // gmeow:X rdfs:subClassOf gufo:Kind: predicate is an alignment predicate AND
        // the object (gufo:Kind) is external → bridge, exempt.
        // gmeow:X gufo:mediates gmeow:Y: predicate is in the gufo namespace itself
        // (not merely an alignment predicate on an external object) and the object is
        // internal (gmeow:Y) → stays in residue.
        assert_eq!(residue(&ds, &vocab), 1);
    }

    #[test]
    fn internal_equivalent_class_stays_in_residue() {
        let ds = ds_of(
            r#"
            gmeow:X owl:equivalentClass gmeow:Y .
            "#,
        );
        // Use a vocab whose namespace is the gmeow namespace itself and whose
        // alignment predicates include owl:equivalentClass: the triple matches the
        // vocab (object is in-namespace) but the object is INTERNAL, so the bridge
        // carve-out does not apply — a genuine second-source-of-truth axiom.
        let vocab = alignment_vocab("gmeow-internal", GMEOW);
        assert_eq!(residue(&ds, &vocab), 1);
    }

    #[test]
    fn anonymous_nested_property_shape_counted_in_full_residue() {
        let ds = ds_of(
            r#"
            gmeow:S a sh:NodeShape ;
                sh:property [ sh:path gmeow:p ; sh:minCount 1 ] .
            "#,
        );
        // Two constructs: the named gmeow:S NodeShape, and the anonymous blank-node
        // property shape (caught via its sh:path subject, not via rdf:type).
        assert_eq!(residue(&ds, &shacl_vocab()), 2);
    }

    #[test]
    fn anonymous_nested_property_shape_absent_from_historical_scope() {
        // The legacy axis scope (typed shapes only) does NOT see the anonymous
        // blank-node property shape — only the named sh:NodeShape.
        let ds = ds_of(
            r#"
            gmeow:S a sh:NodeShape ;
                sh:property [ sh:path gmeow:p ; sh:minCount 1 ] .
            "#,
        );
        let constructs = enumerate(&ds, &shacl_vocab(), CountMode::Historical);
        assert_eq!(constructs.len(), 1);
        assert_eq!(constructs[0].key, format!("{GMEOW}S"));
    }

    #[test]
    fn aliased_namespace_construct_counted() {
        let ds = ds_of(
            r#"
            gmeow:X gufo:mediates gmeow:Y .
            "#,
        );
        let vocab = ProjectionVocabulary {
            prefix: "gufo".to_owned(),
            namespaces: vec![
                "https://w3id.org/gufo#".to_owned(),
                "http://gufo.example.org/aliased#".to_owned(),
            ],
            subsumed_by: LOGIC_NS.to_owned(),
            count_kind: CountKind::TypedAxiom,
            default_ceiling: 0,
            preservation: "SoundUnderApproximation".to_owned(),
            alignment_predicates: Vec::new(),
        };
        assert_eq!(residue(&ds, &vocab), 1);
    }

    #[test]
    fn non_rdf_surface_vocab_is_structurally_zero() {
        let ds = ds_of(
            r#"
            gmeow:S a sh:NodeShape .
            "#,
        );
        let vocab = ProjectionVocabulary {
            prefix: "datalog".to_owned(),
            namespaces: vec!["https://blackcatinformatics.ca/datalog/".to_owned()],
            subsumed_by: LOGIC_NS.to_owned(),
            count_kind: CountKind::NonRdfSurface,
            default_ceiling: 0,
            preservation: "SoundUnderApproximation".to_owned(),
            alignment_predicates: Vec::new(),
        };
        assert_eq!(residue(&ds, &vocab), 0);
    }

    #[test]
    fn grounded_fraction_matches_legacy_grounded_over_authored() {
        let ds = ds_of(
            r#"
            gmeow:A a sh:NodeShape ; logic:formalizes logic:Obligation .
            gmeow:B a sh:PropertyShape .
            logic:Obligation a owl:Class .
            "#,
        );
        // 1 of 2 typed shapes carries logic:formalizes → 0.5.
        assert!((grounded_fraction(&ds, &shacl_vocab()) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn grounded_fraction_is_one_when_nothing_authored() {
        let ds = ds_of("gmeow:Unrelated a owl:Class .");
        assert!((grounded_fraction(&ds, &shacl_vocab()) - 1.0).abs() < f64::EPSILON);
    }
}
