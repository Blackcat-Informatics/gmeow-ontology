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
//! [`residue_constructs_for_surface`] is the ONE residue counter: it returns the
//! ungrounded, non-bridge [`Construct`] SET, and [`residue_for_surface`] /
//! [`residue`] are its `.len()` projection. There is deliberately no second
//! "keys-only" sibling — one predicate, one implementation.
//!
//! [`grounded_fraction`] is the other derived quantity callers want: the gate counts
//! [`residue`] (FullResidue, ungrounded and non-bridge), the advisory axis computes
//! [`grounded_fraction`] (Historical, grounded/total).
//!
//! Every enumerated construct carries a [`Witness`] — a RELOCATION-INVARIANT identity
//! anchored on its subject TERM IRI — so a residue cell can be compared across two
//! independently-built datasets (a merge-base measurement vs a working-tree one).
//! [`relocation_reasons`] then names, with machine-readable codes, the two ways residue
//! genuinely fails to be conserved when a construct moves between authoring surfaces.

use std::collections::{BTreeMap, BTreeSet};

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
use gmeow_ns::LOGIC_NS;
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

/// One enumerated hand-authored construct in a vocab's surface. PUBLIC (not
/// module-private) so the advisory `shape_migration_axis` can iterate the SAME
/// enumeration [`enumerate`] returns to emit its per-shape finding, and so the ratchet
/// gate's driver can carry each residue construct's [`Witness`] across a
/// base-vs-working comparison — instead of a second adapter re-deriving the same
/// information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Construct {
    /// A stable, deterministic key identifying the construct WITHIN ONE DATASET — an
    /// IRI, a blank-node-derived key, or (for a whole-triple key) a rendering of all
    /// three term positions. Never re-derived differently between two enumerations of
    /// the same dataset (dedup correctness depends on it).
    ///
    /// NOT comparable across two datasets: see [`Witness`] for the cross-dataset
    /// identity.
    pub key: String,
    /// Whether the construct carries a grounding back-reference (semantics depend on
    /// [`CountMode`]: presence-only under `Historical`, resolvable under
    /// `FullResidue`).
    pub grounded: bool,
    /// Whether the construct is a by-reference alignment/bridge link to an EXTERNAL
    /// upper-ontology/standard namespace (Principle 5) — exempt from the residue.
    /// Always `false` under `Historical` (no bridge subtraction existed pre-refactor).
    pub is_bridge: bool,
    /// The construct's RELOCATION-INVARIANT identity — see [`Witness`].
    pub witness: Witness,
}

/// A construct's cross-dataset identity, anchored on its SUBJECT TERM IRI.
///
/// WHY NOT [`Construct::key`]: [`term_key`] renders a blank node as
/// `_:{label}#{scope}`, and the scope id depends on **dataset construction order** —
/// which datasets were pushed into the builder, and in which order. The ratchet gate
/// builds its merge-base dataset (base bytes for one slice) and its working-tree
/// dataset (working files for one slice) in different orders, so a construct that did
/// not change at all can carry two different `key`s on the two sides. Keying a
/// relocation comparison on the full `s|p|o` key would therefore report phantom
/// churn.
///
/// Anchoring on the SUBJECT TERM IRI fixes that, and additionally makes a blank
/// *object* harmless: `«gmeow:X rdfs:subClassOf _:b0#3»` and the same triple
/// re-scoped to `_:b0#7` share the anchor `gmeow:X`. GMEOW mints every term into a
/// GLOBAL namespace (`crates/ns`), never a per-slice one, so a term IRI is already
/// invariant under moving the authoring file between slices.
///
/// For a construct whose SUBJECT is itself a blank node — the common case for `sh:`
/// residue, since [`enumerate_shape`] deliberately counts anonymous nested
/// `sh:property [ sh:path … ]` blocks — the anchor is the nearest NAMED ancestor
/// reached by walking `sh:property` / `sh:node` edges upward. When no named ancestor
/// exists the construct is [`Witness::NonRelocatable`]: FAIL-CLOSED, because there is
/// no evidence that would let a relocation-aware ratchet forgive it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Witness {
    /// The construct is anchored on this named term IRI — its own subject IRI, or the
    /// nearest `sh:property`/`sh:node` ancestor's IRI when the subject is blank.
    Anchored(String),
    /// No named anchor is reachable (a blank subject with no named
    /// `sh:property`/`sh:node` ancestor). The construct has NO relocation-invariant
    /// identity, so a relocation-aware ratchet must treat it as freshly authored.
    NonRelocatable,
}

impl Witness {
    /// The anchor term IRI, or `None` when the construct is
    /// [`Witness::NonRelocatable`].
    #[must_use]
    pub fn anchor(&self) -> Option<&str> {
        match self {
            Self::Anchored(iri) => Some(iri.as_str()),
            Self::NonRelocatable => None,
        }
    }

    /// Whether this construct has a relocation-invariant identity at all.
    #[must_use]
    pub fn is_relocatable(&self) -> bool {
        matches!(self, Self::Anchored(_))
    }
}

/// The `sh:property` / `sh:node` PARENT index used to anchor a blank-subject
/// construct on its nearest named ancestor. Collected ONCE per enumeration (two
/// predicate-bound scans) and consulted per construct, never re-queried inside the
/// per-construct loop.
struct AnchorIndex {
    /// nested shape node → the shape nodes that point at it via `sh:property`/`sh:node`.
    parents: BTreeMap<TermId, BTreeSet<TermId>>,
}

impl AnchorIndex {
    fn collect(ds: &RdfDataset) -> Self {
        let mut parents: BTreeMap<TermId, BTreeSet<TermId>> = BTreeMap::new();
        for pred_iri in [SH_PROPERTY, SH_NODE] {
            let Some(pred) = id(ds, pred_iri) else {
                continue;
            };
            for q in ds.quads_for_pattern(None, Some(pred), None, GraphMatch::Any) {
                parents.entry(q.o).or_default().insert(q.s);
            }
        }
        Self { parents }
    }

    /// The [`Witness`] for a construct whose subject term is `subject`.
    ///
    /// An IRI subject anchors on itself. A blank subject is walked upward level by
    /// level through the `sh:property`/`sh:node` parent edges; the FIRST level that
    /// contains any named ancestor decides, and among several the lexicographically
    /// smallest IRI is taken so the anchor is deterministic regardless of dataset
    /// construction order. A `visited` set bounds a cyclic (or diamond) shape graph.
    /// Exhausting the walk without reaching a named node is
    /// [`Witness::NonRelocatable`].
    fn witness(&self, ds: &RdfDataset, subject: TermId) -> Witness {
        if let TermRef::Iri(iri) = ds.resolve(subject) {
            return Witness::Anchored(iri.to_owned());
        }
        let mut visited: BTreeSet<TermId> = BTreeSet::new();
        visited.insert(subject);
        let mut frontier: Vec<TermId> = vec![subject];
        while !frontier.is_empty() {
            let mut named: BTreeSet<String> = BTreeSet::new();
            let mut next: BTreeSet<TermId> = BTreeSet::new();
            for node in &frontier {
                for parent in self.parents.get(node).into_iter().flatten() {
                    match ds.resolve(*parent) {
                        TermRef::Iri(iri) => {
                            named.insert(iri.to_owned());
                        }
                        _ => {
                            if visited.insert(*parent) {
                                next.insert(*parent);
                            }
                        }
                    }
                }
            }
            if let Some(anchor) = named.into_iter().next() {
                return Witness::Anchored(anchor);
            }
            frontier = next.into_iter().collect();
        }
        Witness::NonRelocatable
    }
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
/// [`residue`]/[`grounded_fraction`] quantities instead. PUBLIC so the ratchet gate's
/// driver can reach each construct's [`Witness`] without a second enumeration.
#[must_use]
pub fn enumerate(
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
    let anchors = AnchorIndex::collect(ds);
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
        let witness = anchors.witness(ds, q.s);
        out.push(Construct {
            key,
            grounded,
            is_bridge,
            witness,
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
                        // Historical scope is IRI-subjects-only, so the construct is
                        // always anchored on its own subject term IRI.
                        witness: Witness::Anchored(iri.clone()),
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

            let anchors = AnchorIndex::collect(ds);
            let mut out: Vec<Construct> = node_ids
                .into_iter()
                .map(|tid| {
                    let key = term_key(ds, tid);
                    let grounded = resolvable_grounding(ds, tid);
                    let witness = anchors.witness(ds, tid);
                    Construct {
                        key,
                        grounded,
                        is_bridge: false,
                        witness,
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
    let anchors = AnchorIndex::collect(ds);
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
        let witness = anchors.witness(ds, q.s);
        out.push(Construct {
            key,
            grounded,
            is_bridge,
            witness,
        });
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

/// The ungrounded residue of `vocab` over `ds`: the [`CountMode::FullResidue`]
/// constructs that are neither grounded nor an exempt by-reference bridge, in the
/// enumeration's deterministic key order.
///
/// This is THE residue counter. [`residue_for_surface`] and [`residue`] are its
/// `.len()` projection — there is deliberately no separate "count only" or "keys only"
/// implementation, so the gate, the seed, the debt report, and the relocation
/// accounting can never disagree about which constructs are in the residue.
#[must_use]
pub fn residue_constructs_for_surface(
    ds: &RdfDataset,
    vocab: &ProjectionVocabulary,
    surface_iri: &str,
) -> Vec<Construct> {
    let mut out = enumerate(ds, vocab, CountMode::FullResidue, surface_iri);
    out.retain(|c| !c.grounded && !c.is_bridge);
    out
}

/// The COUNT of [`residue_constructs_for_surface`] — the quantity the ratchet gate and
/// the seed both compare against a committed ceiling. A pure `.len()` projection of the
/// one construct set above.
#[must_use]
pub fn residue_for_surface(
    ds: &RdfDataset,
    vocab: &ProjectionVocabulary,
    surface_iri: &str,
) -> u64 {
    residue_constructs_for_surface(ds, vocab, surface_iri).len() as u64
}

/// The ungrounded residue measured AS IF on the vocabulary's own owner surface — the
/// owner-agnostic view where a validated correspondence cell is exemptible. Callers
/// that know the real surface (the gate/seed) use [`residue_for_surface`] so the
/// strict owner boundary is enforced; this wrapper is for owner-neutral contexts.
#[must_use]
pub fn residue(ds: &RdfDataset, vocab: &ProjectionVocabulary) -> u64 {
    residue_for_surface(ds, vocab, &vocab.owner)
}

// -----------------------------------------------------------------------------
// Relocation accounting: WHY residue is not conserved when a construct moves.
// -----------------------------------------------------------------------------

/// A machine-readable reason that a construct's residue membership is NOT conserved
/// when it moves from one authoring surface to another.
///
/// Residue is a function of `(dataset, surface_iri)`, not of the construct alone, and
/// two of that function's inputs genuinely change under relocation:
///
/// * [`is_bridge_exempt`] returns exempt IFF `surface_iri == vocab.owner` for a
///   grounding correspondence, so a construct crossing the owner boundary has residue
///   CREATED or DESTROYED with no authoring at all
///   ([`Self::ExemptionShiftOwnerBoundary`] / [`Self::BridgeExemptBothSides`]);
/// * [`resolvable_grounding`] requires the `logic:` axiom target to be in the SAME
///   per-slice dataset, so moving a construct away from the `logic:Formula` that
///   grounds it MANUFACTURES residue with no authoring
///   ([`Self::GroundingOrphaned`]).
///
/// These are computed from the two real datasets by [`relocation_reasons`], never
/// inferred from a count delta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RelocationReason {
    /// The construct's bridge exemption FLIPS between the source surface and the
    /// destination surface (the C1e owner boundary): exempt on exactly one of them, so
    /// relocating it alone creates or destroys residue.
    ExemptionShiftOwnerBoundary,
    /// The construct's anchor term resolves its `logic:formalizes` back-reference in
    /// the SOURCE dataset but not in the DESTINATION dataset — the grounding axiom
    /// stayed behind, so the relocated construct is ungrounded residue it never was
    /// before.
    GroundingOrphaned,
    /// The construct is an exempt by-reference bridge on BOTH surfaces — relocation is
    /// residue-neutral for it, and a ratchet must not book it as newly-authored debt.
    BridgeExemptBothSides,
}

impl RelocationReason {
    /// The stable, machine-readable code a consumer reports.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::ExemptionShiftOwnerBoundary => "exemption-shift-owner-boundary",
            Self::GroundingOrphaned => "grounding-orphaned",
            Self::BridgeExemptBothSides => "bridge-exempt-both-sides",
        }
    }
}

/// Explain, per [`Witness`] ANCHOR, why `vocab`'s residue over `source` fails to be
/// conserved when its constructs are attributed to `destination_surface_iri` in the
/// `destination` dataset.
///
/// Three real measurements, no inference:
///
/// 1. `source` enumerated at `source_surface_iri` — the bytes where they sat;
/// 2. `source` enumerated at `destination_surface_iri` — the SAME bytes measured AS IF
///    they already sat at the destination surface (the surface-normalized base
///    measurement). Identical dataset ⇒ identical [`Construct::key`]s, so the ONLY
///    field that can differ is [`Construct::is_bridge`] — exactly the owner-boundary
///    exemption shift;
/// 3. `destination` — where the construct now lives, consulted for whether the anchor
///    term's grounding survived the move.
///
/// The result is keyed on the relocation-invariant anchor IRI. A
/// [`Witness::NonRelocatable`] construct is deliberately ABSENT from the map: it has no
/// cross-dataset identity, so it carries no relocation warrant (fail-closed).
#[must_use]
pub fn relocation_reasons(
    source: &RdfDataset,
    source_surface_iri: &str,
    destination: &RdfDataset,
    destination_surface_iri: &str,
    vocab: &ProjectionVocabulary,
) -> BTreeMap<String, BTreeSet<RelocationReason>> {
    let at_source = enumerate(source, vocab, CountMode::FullResidue, source_surface_iri);
    // Same dataset, destination surface: the surface-normalized measurement.
    let normalized: BTreeMap<String, bool> = enumerate(
        source,
        vocab,
        CountMode::FullResidue,
        destination_surface_iri,
    )
    .into_iter()
    .map(|c| (c.key, c.is_bridge))
    .collect();

    // Whether an anchor term resolves its grounding back-reference in a given dataset —
    // the exact predicate [`resolvable_grounding`] applies to residue membership,
    // evaluated on the relocation-invariant anchor rather than on a blank-node key that
    // cannot cross a dataset boundary.
    let grounded_in =
        |ds: &RdfDataset, iri: &str| id(ds, iri).is_some_and(|term| resolvable_grounding(ds, term));

    let mut out: BTreeMap<String, BTreeSet<RelocationReason>> = BTreeMap::new();
    for c in &at_source {
        let Some(anchor) = c.witness.anchor() else {
            continue; // NonRelocatable: no cross-dataset identity, no warrant.
        };
        let mut reasons: BTreeSet<RelocationReason> = BTreeSet::new();
        // The normalized enumeration is over the same dataset, so every key is present;
        // fall back to the source verdict rather than invent a shift if it somehow is not.
        let normalized_bridge = normalized.get(&c.key).copied().unwrap_or(c.is_bridge);
        if c.is_bridge != normalized_bridge {
            reasons.insert(RelocationReason::ExemptionShiftOwnerBoundary);
        } else if c.is_bridge {
            reasons.insert(RelocationReason::BridgeExemptBothSides);
        }
        // Grounding is a property of the per-slice dataset, not of the surface IRI:
        // consult the destination dataset the construct actually moved into.
        if id(destination, anchor).is_some()
            && grounded_in(source, anchor)
            && !grounded_in(destination, anchor)
        {
            reasons.insert(RelocationReason::GroundingOrphaned);
        }
        if !reasons.is_empty() {
            out.entry(anchor.to_owned()).or_default().extend(reasons);
        }
    }
    out
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

#[path = "counting.tests.rs"]
#[cfg(test)]
mod tests;
