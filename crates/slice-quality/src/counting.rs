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
/// SHACL `sh:property` predicate — its OBJECT is a (usually anonymous) property
/// shape carrying constraint obligations, counted as a construct so that adding a
/// nested obligation block to an existing shape is not free.
pub const SH_PROPERTY: &str = "http://www.w3.org/ns/shacl#property";
/// SHACL `sh:node` predicate — its OBJECT is a nested node shape, counted for the
/// same reason as `sh:property`.
pub const SH_NODE: &str = "http://www.w3.org/ns/shacl#node";
/// The `logic:formalizes` back-reference predicate — a construct naming the
/// `logic:` axiom it was derived from. This is the SOLE grounding back-reference
/// predicate: `logic:grounds` was a phantom (defined nowhere) and is gone.
pub const LOGIC_FORMALIZES: &str = "https://blackcatinformatics.ca/logic/formalizes";
/// The `logic:` core namespace — every guarded vocab's `subsumed_by` witness, and
/// (per [`resolvable_grounding`]) the namespace an appropriately-typed grounding
/// target's `rdf:type` must fall in.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
/// `owl:AllDisjointClasses` — the one non-`logic:`-namespaced grounding-target type
/// (a named disjointness axiom a shape may formalize).
const OWL_ALL_DISJOINT_CLASSES: &str = "http://www.w3.org/2002/07/owl#AllDisjointClasses";
const GM_TO_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/toPredicate";
const GM_TO_CLASS: &str = "https://blackcatinformatics.ca/gmeow/toClass";
const GM_EDOAL_TARGET: &str = "https://blackcatinformatics.ca/gmeow/edoalTarget";
const LOGIC_TARGET_ENDPOINT: &str = "https://blackcatinformatics.ca/logic/targetEndpoint";

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

/// Every object of `pred_iri` that is an IRI or blank node (the nested shape a
/// `sh:property`/`sh:node` obligation points at). Literal objects are skipped —
/// only a shape node is a construct.
fn objects_of(ds: &RdfDataset, pred_iri: &str) -> Vec<TermId> {
    let Some(pred_id) = id(ds, pred_iri) else {
        return Vec::new();
    };
    ds.quads_for_pattern(None, Some(pred_id), None, GraphMatch::Any)
        .filter(|q| matches!(ds.resolve(q.o), TermRef::Iri(_) | TermRef::Blank { .. }))
        .map(|q| q.o)
        .collect()
}

/// RESOLVABLE grounding: `subject` carries a `logic:formalizes` back-reference to a
/// target that is an APPROPRIATELY TYPED grounding construct — one whose `rdf:type`
/// is a `logic:` axiom class (`logic:Formula`/`logic:Rule`/`logic:*Assertion`, i.e.
/// any type in the `logic:` namespace) or the named `owl:AllDisjointClasses`
/// disjointness axiom. Merely being the subject of *some* triple no longer grounds a
/// construct: a back-reference to an untyped or non-axiom target (a rubber-stamp to a
/// domain term, or to nowhere) does NOT ground, so a migration cannot be faked. The
/// direction follows the `logic:formalizes` contract (the projection construct is the
/// subject, the axiom it re-encodes is the object); the phantom `logic:grounds` — a
/// predicate the ontology never defined — is gone.
fn resolvable_grounding(ds: &RdfDataset, subject: TermId) -> bool {
    let (Some(formalizes_p), Some(type_p)) = (id(ds, LOGIC_FORMALIZES), id(ds, RDF_TYPE)) else {
        return false;
    };
    for target_iri in all_iris(ds, subject, formalizes_p) {
        let Some(target_id) = id(ds, &target_iri) else {
            continue; // the target IRI never appears in ds at all → dangling
        };
        for type_iri in all_iris(ds, target_id, type_p) {
            if type_iri.starts_with(LOGIC_NS) || type_iri == OWL_ALL_DISJOINT_CLASSES {
                return true;
            }
        }
    }
    false
}

/// The native alignment cells' base triples, split for the residue enumeration. The
/// canonical reader is `O(reifiers)`, so this is collected ONCE per enumeration and
/// consulted per construct — never re-read inside the quad loop.
struct AlignmentBridges {
    /// Every native alignment cell base triple `(s, p, o)` — a first-class RDF-1.2
    /// correspondence record (identified by its `gmeow:sssomFile` reifier), not
    /// hand-authored second-source residue.
    all: BTreeSet<(TermId, TermId, TermId)>,
    /// The subset carrying a complete grounding envelope
    /// (`is_native_validated_grounding_term_cell`): exempt ONLY on the vocabulary's
    /// owner surface (the C1e owner boundary).
    grounding: BTreeSet<(TermId, TermId, TermId)>,
}

impl AlignmentBridges {
    fn collect(ds: &RdfDataset) -> Self {
        let mut all = BTreeSet::new();
        let mut grounding = BTreeSet::new();
        for cell in crate::grounding::native_alignment_cells(ds) {
            let (Some(s), Some(p), Some(o)) = (
                id(ds, &cell.subject),
                id(ds, &cell.predicate),
                id(ds, &cell.obj),
            ) else {
                continue;
            };
            all.insert((s, p, o));
            if crate::grounding::is_native_validated_grounding_term_cell(&cell) {
                grounding.insert((s, p, o));
            }
        }
        Self { all, grounding }
    }
}

/// Whether the enumerated triple `(subject, predicate, object)` is a by-reference
/// alignment/bridge link exempt from the ungrounded residue on `surface_iri`. Three
/// frontends are recognized:
///
/// * a NATIVE grounding correspondence (complete envelope on the reifier) — exempt
///   ONLY on the vocabulary's owner surface (C1e: an external grounding term has one
///   authoring home);
/// * any OTHER native alignment cell base triple with at least one EXTERNAL endpoint —
///   a first-class correspondence record, exempt on every surface (an internal
///   `gmeow`-to-`gmeow` cell stays in the residue as a genuine second source);
/// * the node-authored `ProjectionMapping` frontend (not migrated) — its flat
///   `logic:targetEndpoint`, or the sole `toClass`/`toPredicate`/`edoalTarget` binding
///   target of a validated mapping, on the owner surface with an external object.
fn is_bridge_exempt(
    ds: &RdfDataset,
    subject: TermId,
    predicate: TermId,
    object: TermId,
    surface_iri: &str,
    vocab: &ProjectionVocabulary,
    bridges: &AlignmentBridges,
) -> bool {
    let external =
        |t: TermId| matches!(ds.resolve(t), TermRef::Iri(iri) if !iri.starts_with(GMEOW));
    let triple = (subject, predicate, object);
    if bridges.grounding.contains(&triple) {
        return surface_iri == vocab.owner;
    }
    // An ordinary (non-grounding) native alignment cell is a first-class correspondence
    // record only where the counted surface is the STRUCTURAL rdfs taxonomy: a domain
    // slice aligning its term to/from an external class via `rdfs:subClassOf` is a
    // correspondence, not hand-authored TBox. For a TYPED-axiom foundational vocabulary
    // (gUFO/BFO/…) only a COMPLETE grounding correspondence is exempt — an incomplete
    // cell targeting it stays in the residue as an unwarranted grounding.
    if vocab.count_kind == CountKind::StructuralAxiom
        && bridges.all.contains(&triple)
        && (external(subject) || external(object))
    {
        return true;
    }
    // The node-authored ProjectionMapping frontend is exempt only on the owner surface
    // when its target edge points at an external object.
    if surface_iri != vocab.owner || !external(object) {
        return false;
    }
    let Some(predicate_iri) = (match ds.resolve(predicate) {
        TermRef::Iri(iri) => Some(iri),
        _ => None,
    }) else {
        return false;
    };
    if predicate_iri == LOGIC_TARGET_ENDPOINT {
        return match ds.resolve(subject) {
            TermRef::Iri(cell) => crate::grounding::is_validated_grounding_correspondence(ds, cell),
            _ => false,
        };
    }
    [GM_TO_PREDICATE, GM_TO_CLASS, GM_EDOAL_TARGET].contains(&predicate_iri)
        && crate::grounding::validated_projection_owner(ds, subject).is_some()
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
    surface_iri: &str,
) -> Vec<Construct> {
    match vocab.count_kind {
        CountKind::Shape => enumerate_shape(ds, mode),
        CountKind::TypedAxiom => enumerate_typed_axiom(ds, vocab, mode, surface_iri),
        CountKind::StructuralAxiom => enumerate_structural_axiom(ds, vocab, mode, surface_iri),
        CountKind::NonRdfSurface => Vec::new(),
    }
}

/// `CountKind::StructuralAxiom` enumeration — RDFS's minimum useful structural set.
/// Counts distinct triples whose PREDICATE IRI is in `vocab.counted_predicates` (e.g.
/// `rdfs:subClassOf`/`subPropertyOf`/`domain`/`range`), so annotation predicates
/// (`rdfs:label`/`comment`/`isDefinedBy`/`seeAlso`) never count and OWL is untouched.
/// Only meaningful under [`CountMode::FullResidue`]; [`CountMode::Historical`] returns
/// empty (the legacy axis never scored structural-axiom vocabs).
fn enumerate_structural_axiom(
    ds: &RdfDataset,
    vocab: &ProjectionVocabulary,
    mode: CountMode,
    surface_iri: &str,
) -> Vec<Construct> {
    if mode == CountMode::Historical {
        return Vec::new();
    }
    let mut seen: BTreeSet<(TermId, TermId, TermId)> = BTreeSet::new();
    let mut out = Vec::new();
    let bridges = AlignmentBridges::collect(ds);
    for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
        let TermRef::Iri(p_iri) = ds.resolve(q.p) else {
            continue;
        };
        if !vocab.counted_predicates.iter().any(|cp| cp == p_iri) {
            continue;
        }
        if !seen.insert((q.s, q.p, q.o)) {
            continue;
        }
        let key = format!(
            "{}|{}|{}",
            term_key(ds, q.s),
            term_key(ds, q.p),
            term_key(ds, q.o)
        );
        let grounded = resolvable_grounding(ds, q.s);
        // A structural axiom is exempt only when it is the target edge of a native
        // alignment correspondence. Raw hand-authored structural triples never are.
        let is_bridge = is_bridge_exempt(ds, q.s, q.p, q.o, surface_iri, vocab, &bridges);
        out.push(Construct {
            key,
            grounded,
            is_bridge,
        });
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
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
            // The OBJECT of sh:property / sh:node is a nested obligation-carrying
            // shape — count it too, so bolting an extra sh:property block onto an
            // existing shape is not a free constraint (docs §2 "count obligations,
            // not only shape roots").
            node_ids.extend(objects_of(ds, SH_PROPERTY));
            node_ids.extend(objects_of(ds, SH_NODE));
            // Dedup on the interned `TermId` itself — one dataset resolves each
            // distinct term to exactly one `TermId`, so this is equivalent to the
            // former dedup-by-formatted-key but skips formatting every candidate
            // just to throw most of the strings away.
            node_ids.sort();
            node_ids.dedup();

            let mut out: Vec<Construct> = node_ids
                .into_iter()
                .map(|tid| {
                    let key = term_key(ds, tid);
                    let grounded = resolvable_grounding(ds, tid);
                    Construct {
                        key,
                        grounded,
                        is_bridge: false,
                    }
                })
                .collect();
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
    surface_iri: &str,
) -> Vec<Construct> {
    if mode == CountMode::Historical {
        return Vec::new();
    }
    let mut seen: BTreeSet<(TermId, TermId, TermId)> = BTreeSet::new();
    let mut out = Vec::new();
    let bridges = AlignmentBridges::collect(ds);
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
        // Dedup on the (s, p, o) `TermId` triple BEFORE formatting the string key —
        // most candidate quads are duplicates across graphs/surfaces, so this skips
        // formatting the ones we're about to discard anyway.
        if !seen.insert((q.s, q.p, q.o)) {
            continue;
        }
        let key = format!(
            "{}|{}|{}",
            term_key(ds, q.s),
            term_key(ds, q.p),
            term_key(ds, q.o)
        );
        let grounded = resolvable_grounding(ds, q.s);
        // A by-reference bridge is exempt when this is the base triple of a native
        // alignment correspondence (a grounding correspondence on its owner surface, or
        // any other external-facing alignment cell) or the target edge of a validated
        // ProjectionMapping. A raw hand-authored rdfs/owl edge remains second-source
        // residue. C1e strict owner boundary: the grounding-correspondence exemption
        // applies ONLY on the vocabulary's OWNER surface.
        let is_bridge = is_bridge_exempt(ds, q.s, q.p, q.o, surface_iri, vocab, &bridges);
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
pub fn residue_for_surface(
    ds: &RdfDataset,
    vocab: &ProjectionVocabulary,
    surface_iri: &str,
) -> u64 {
    enumerate(ds, vocab, CountMode::FullResidue, surface_iri)
        .iter()
        .filter(|c| !c.grounded && !c.is_bridge)
        .count() as u64
}

/// The ungrounded residue measured AS IF on the vocabulary's own owner surface — the
/// owner-agnostic view where a validated correspondence cell is exemptible. Callers
/// that know the real surface (the gate/seed) use [`residue_for_surface`] so the
/// strict owner boundary is enforced; this wrapper is for owner-neutral contexts.
#[must_use]
pub fn residue(ds: &RdfDataset, vocab: &ProjectionVocabulary) -> u64 {
    residue_for_surface(ds, vocab, &vocab.owner)
}

/// The grounded fraction over [`CountMode::Historical`] enumeration — legacy
/// `grounded / authored` semantics, `1.0` when there is nothing authored. This is the
/// quantity the advisory `shape_migration_axis` measures.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn grounded_fraction(ds: &RdfDataset, vocab: &ProjectionVocabulary) -> f64 {
    let constructs = enumerate(ds, vocab, CountMode::Historical, &vocab.owner);
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
        owner: LOGIC_NS.to_owned(),
        count_kind: CountKind::Shape,
        default_ceiling: 0,
        preservation: "SoundUnderApproximation".to_owned(),
        alignment_predicates: Vec::new(),
        counted_predicates: Vec::new(),
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
            owner: LOGIC_NS.to_owned(),
            count_kind: CountKind::TypedAxiom,
            default_ceiling: 0,
            preservation: "SoundUnderApproximation".to_owned(),
            alignment_predicates: vec![
                "http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned(),
                "http://www.w3.org/2002/07/owl#equivalentClass".to_owned(),
            ],
            counted_predicates: Vec::new(),
        }
    }

    #[test]
    fn grounded_shape_not_counted_in_residue() {
        // The grounding target is a real logic: axiom construct (logic:Formula is in
        // the logic: namespace), so the shape is grounded and subtracted.
        let ds = ds_of(
            r#"
            gmeow:S a sh:NodeShape ; logic:formalizes logic:disjointGoals .
            logic:disjointGoals a logic:Formula .
            "#,
        );
        assert_eq!(residue(&ds, &shacl_vocab()), 0);
    }

    #[test]
    fn back_ref_to_non_axiom_target_still_counts() {
        // logic:formalizes points at a target typed owl:Class — a plain class
        // declaration, NOT a logic: axiom / owl:AllDisjointClasses. Under the tightened
        // typed-grounding contract this does NOT ground the shape, so it is counted.
        let ds = ds_of(
            r#"
            gmeow:S a sh:NodeShape ; logic:formalizes gmeow:Goal .
            gmeow:Goal a owl:Class .
            "#,
        );
        assert_eq!(residue(&ds, &shacl_vocab()), 1);
    }

    #[test]
    fn back_ref_to_named_disjointness_axiom_grounds() {
        // A shape may formalize a named owl:AllDisjointClasses axiom (the one
        // non-logic:-namespaced grounding-target type) — grounded, not counted.
        let ds = ds_of(
            r#"
            gmeow:S a sh:NodeShape ; logic:formalizes gmeow:identityDisjointness .
            gmeow:identityDisjointness a owl:AllDisjointClasses .
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
    fn raw_external_bridge_now_counts_not_exempt() {
        let ds = ds_of(
            r#"
            gmeow:X rdfs:subClassOf gufo:Kind .
            gmeow:X gufo:mediates gmeow:Y .
            "#,
        );
        let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#");
        // A raw rdfs:subClassOf to an external gufo object is NO LONGER an exempt
        // bridge (it is not a validated gmeow:TermEquivalence cell), so it counts; the
        // gufo:mediates triple counts too → residue 2.
        assert_eq!(residue(&ds, &vocab), 2);
    }

    #[test]
    fn validated_correspondence_cell_is_exempt() {
        // A native RDF-1.2 grounding correspondence: the envelope rides the reifier, so
        // the only external-facing flat triple is the asserted match base triple, and it
        // is subtracted as a by-reference bridge on the owner surface.
        let ds = ds_of(
            r#"
            gmeow:MyKind skos:exactMatch gufo:Kind {|
                a logic:GroundingCorrespondence ;
                gmeow:sssomFile "grounding.sssom.tsv" ;
                gmeow:justification gmeow:ManualMappingCuration ;
                logic:sourceEndpoint gmeow:MyKind ;
                logic:targetEndpoint gufo:Kind ;
                logic:morphismClass logic:WellBehavedLens ;
                logic:morphismKind logic:InstitutionMorphism ;
                logic:preservationKind logic:SoundUnderApproximation
            |} .
            "#,
        );
        let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#");
        assert_eq!(residue(&ds, &vocab), 0);
    }

    #[test]
    fn validated_cell_on_non_owner_surface_still_counts() {
        // The SAME validated cell that is exempt on the owner surface counts when
        // measured on a non-owner surface — strict owner boundary (C1e). The single
        // asserted match base triple is the one external-facing flat triple.
        let ds = ds_of(
            r#"
            gmeow:MyKind skos:exactMatch gufo:Kind {|
                a logic:GroundingCorrespondence ;
                gmeow:sssomFile "grounding.sssom.tsv" ;
                gmeow:justification gmeow:ManualMappingCuration ;
                logic:sourceEndpoint gmeow:MyKind ;
                logic:targetEndpoint gufo:Kind ;
                logic:morphismClass logic:WellBehavedLens ;
                logic:morphismKind logic:InstitutionMorphism ;
                logic:preservationKind logic:SoundUnderApproximation
            |} .
            "#,
        );
        let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#"); // owner = LOGIC_NS
        assert_eq!(residue_for_surface(&ds, &vocab, LOGIC_NS), 0); // on owner: exempt
        assert_eq!(
            residue_for_surface(
                &ds,
                &vocab,
                "https://blackcatinformatics.ca/gmeow/slices/kernel"
            ),
            1 // the match base triple counts off the owner surface
        );
    }

    #[test]
    fn grounding_cell_without_justification_is_not_exempt() {
        // A native grounding cell missing its warrant (no gmeow:justification) is an
        // incomplete grounding correspondence; targeting a TYPED-axiom foundational
        // vocabulary, its match base triple stays in the residue.
        let ds = ds_of(
            r#"
            gmeow:MyKind skos:exactMatch gufo:Kind {|
                a logic:GroundingCorrespondence ;
                gmeow:sssomFile "grounding.sssom.tsv" ;
                logic:sourceEndpoint gmeow:MyKind ;
                logic:targetEndpoint gufo:Kind ;
                logic:morphismClass logic:WellBehavedLens ;
                logic:morphismKind logic:InstitutionMorphism ;
                logic:preservationKind logic:SoundUnderApproximation
            |} .
            "#,
        );
        let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#");
        assert_eq!(residue(&ds, &vocab), 1);
    }

    #[test]
    fn ordinary_alignment_to_typed_vocab_stays_in_residue() {
        // A bare native alignment cell (no grounding envelope) to a TYPED-axiom
        // foundational vocabulary is not a warranted grounding correspondence; its match
        // base triple counts, never opening the owner boundary.
        let ds = ds_of(
            r#"
            gmeow:MyKind skos:exactMatch gufo:Kind {|
                gmeow:sssomFile "ordinary.sssom.tsv" ;
                gmeow:justification gmeow:ManualMappingCuration
            |} .
            "#,
        );
        let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#");
        assert_eq!(residue(&ds, &vocab), 1);
    }

    #[test]
    fn structural_domain_alignment_cell_is_exempt() {
        // A domain slice aligning an external class into the gmeow taxonomy via a native
        // rdfs:subClassOf cell is a first-class correspondence record, not hand-authored
        // second-source rdfs — subtracted from the STRUCTURAL residue on any surface.
        let ds = ds_of(
            r#"
            gufo:Kind rdfs:subClassOf gmeow:MyKind {|
                gmeow:sssomFile "classes.sssom.tsv" ;
                gmeow:justification gmeow:ManualMappingCuration
            |} .
            "#,
        );
        let mut vocab = alignment_vocab("rdfs", "http://www.w3.org/2000/01/rdf-schema#");
        vocab.count_kind = CountKind::StructuralAxiom;
        vocab.counted_predicates =
            vec!["http://www.w3.org/2000/01/rdf-schema#subClassOf".to_owned()];
        assert_eq!(
            residue_for_surface(
                &ds,
                &vocab,
                "https://blackcatinformatics.ca/gmeow/slices/documents"
            ),
            0
        );
    }

    #[test]
    fn single_binding_grounding_projection_target_is_exempt() {
        let ds = ds_of(
            r#"
            gmeow:mapKind a gmeow:ProjectionMapping, logic:GroundingCorrespondence ;
                gmeow:hasMappingPattern [ gmeow:anchor "s" ] ;
                gmeow:hasBinding [
                    gmeow:profile "gufo" ;
                    gmeow:relation "=" ;
                    gmeow:toClass gufo:Kind
                ] ;
                gmeow:justification gmeow:ManualMappingCuration ;
                logic:sourceEndpoint gmeow:MyKind ;
                logic:targetEndpoint gufo:Kind ;
                logic:morphismClass logic:WellBehavedLens ;
                logic:morphismKind logic:InstitutionMorphism ;
                logic:preservationKind logic:SoundUnderApproximation .
            "#,
        );
        let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#");
        assert_eq!(residue(&ds, &vocab), 0);
    }

    #[test]
    fn multi_binding_grounding_projection_does_not_open_the_boundary() {
        let ds = ds_of(
            r#"
            gmeow:mapKind a gmeow:ProjectionMapping, logic:GroundingCorrespondence ;
                gmeow:hasMappingPattern [ gmeow:anchor "s" ] ;
                gmeow:hasBinding
                    [ gmeow:profile "gufo" ; gmeow:relation "=" ; gmeow:toClass gufo:Kind ],
                    [ gmeow:profile "gufo-2" ; gmeow:relation "=" ; gmeow:toClass gufo:Category ] ;
                gmeow:justification gmeow:ManualMappingCuration ;
                logic:sourceEndpoint gmeow:MyKind ;
                logic:targetEndpoint gufo:Kind ;
                logic:morphismClass logic:WellBehavedLens ;
                logic:morphismKind logic:InstitutionMorphism ;
                logic:preservationKind logic:SoundUnderApproximation .
            "#,
        );
        let vocab = alignment_vocab("gufo", "https://w3id.org/gufo#");
        assert_eq!(residue(&ds, &vocab), 3);
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
        let constructs = enumerate(&ds, &shacl_vocab(), CountMode::Historical, "");
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
            owner: LOGIC_NS.to_owned(),
            count_kind: CountKind::TypedAxiom,
            default_ceiling: 0,
            preservation: "SoundUnderApproximation".to_owned(),
            alignment_predicates: Vec::new(),
            counted_predicates: Vec::new(),
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
            owner: LOGIC_NS.to_owned(),
            count_kind: CountKind::NonRdfSurface,
            default_ceiling: 0,
            preservation: "SoundUnderApproximation".to_owned(),
            alignment_predicates: Vec::new(),
            counted_predicates: Vec::new(),
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
