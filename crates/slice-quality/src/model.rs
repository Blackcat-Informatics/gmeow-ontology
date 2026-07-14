// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The data model of the slice-quality rubric and its computed grades.
//!
//! Everything here mirrors the ontology-resident rubric authored in
//! `slices/core/slice-quality-rubric/module.ttl`. The Rust side carries only the
//! shape; the axes, tiers, thresholds, advice text, and scopes are loaded from
//! that slice (see [`crate::rubric`]). Grades form a bounded lattice: the roll-up
//! tier is the **unweighted meet** of the per-axis grades (see [`crate::lattice`]).

use std::cmp::Ordering;

/// The GMEOW namespace prefix every rubric IRI shares.
pub const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

/// One rung of the quality ladder — a `gmeow:QualityTier` individual.
///
/// Tiers are totally ordered by [`Tier::rank`]; the ordering is what makes the
/// ladder a lattice. Two tiers are compared by rank alone, and a stable lexical
/// tie-break on the IRI keeps sort order deterministic even if two tiers ever
/// shared a rank (a case the structural gate forbids, but the code must not rely
/// on derived `Ord` over a floating field).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tier {
    /// The full tier IRI (e.g. `…/tierGrounded`).
    pub iri: String,
    /// The human label (`rdfs:label`).
    pub label: String,
    /// The integer rank giving the ladder its total order.
    pub rank: i64,
}

impl Tier {
    /// A stable, deterministic sort key: rank first, IRI as tie-break. Never
    /// derive `Ord` on the struct — the label must not influence order.
    #[must_use]
    pub fn sort_key(&self) -> (i64, &str) {
        (self.rank, self.iri.as_str())
    }
}

impl PartialOrd for Tier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Tier {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

/// The read-breadth coeffect an axis is granted — how much graph its primitive
/// may consult. Advice is single-slice at every scope; this bounds reads only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextScope {
    /// Only the target slice's own module, examples, and tests.
    SliceLocal,
    /// The target slice plus its declared dependency closure.
    DepsClosure,
    /// The whole merged, reasoned closure.
    MergedClosure,
}

impl ContextScope {
    /// Resolve a `gmeow:AxisContextScope` individual's local name.
    #[must_use]
    pub fn from_local(local: &str) -> Option<Self> {
        match local {
            "scopeSliceLocal" => Some(Self::SliceLocal),
            "scopeDepsClosure" => Some(Self::DepsClosure),
            "scopeMergedClosure" => Some(Self::MergedClosure),
            _ => None,
        }
    }
}

/// A per-tier score floor for an axis — a `gmeow:AxisThreshold` individual.
#[derive(Debug, Clone, PartialEq)]
pub struct Threshold {
    /// The tier this floor unlocks.
    pub tier_iri: String,
    /// The minimum normalized score (0.0–1.0) required to earn `tier_iri`.
    pub floor: f64,
}

/// One measured quality axis — a `gmeow:QualityAxis` individual with its bindings.
#[derive(Debug, Clone)]
pub struct Axis {
    /// The full axis IRI.
    pub iri: String,
    /// The human label.
    pub label: String,
    /// The measurement-primitive key (`gmeow:axisProducer`) — resolved to a
    /// closed Rust primitive; an unknown key is a hard fail, never a silent skip.
    pub producer: String,
    /// The quality dimension the score is emitted under (`gmeow:axisDimension`).
    pub dimension_iri: String,
    /// The per-tier floors, sorted ascending by floor.
    pub thresholds: Vec<Threshold>,
    /// The advice-ranking weight (`gmeow:axisWeight`) — ranking only, never the meet.
    pub weight: f64,
    /// The read-breadth coeffect (`gmeow:axisContextScope`).
    pub scope: ContextScope,
    /// The opinionated uplift advice (`gmeow:axisAdviceTemplate`).
    pub advice: String,
}

/// A dated, self-cleaning exemption — a `gmeow:AxisExemption` individual.
#[derive(Debug, Clone)]
pub struct Exemption {
    /// The exemption IRI.
    pub iri: String,
    /// The axis it covers.
    pub axis_iri: String,
    /// The doctrine-anchored reason.
    pub reason: String,
    /// The date it was minted (ISO `xsd:date` lexical form).
    pub date: String,
    /// The Rust symbol whose resolution in-repo makes the exemption stale.
    pub producer: String,
}

/// A committed per-slice, per-axis measured-score floor — a
/// `gmeow:AxisFloorCommitment` individual. The gate enforces it as a raise-only
/// ratchet: the slice's measured axis score may rise above `floor` but never fall
/// below it. A missing slice, axis, or value is a hard fail, never a silent skip.
#[derive(Debug, Clone, PartialEq)]
pub struct AxisFloorCommitment {
    /// The slice IRI the floor is committed against (`gmeow:floorSlice`).
    pub slice: String,
    /// The axis IRI whose measured score is floored (`gmeow:floorAxis`).
    pub axis: String,
    /// The committed minimum normalized score (0.0–1.0) at full f64 precision.
    pub floor: f64,
}

/// A committed per-slice roll-up tier floor — a `gmeow:SliceTierFloor` individual.
/// The gate enforces it as a raise-only ratchet over the lattice order: the slice's
/// declared roll-up tier may never fall below `tier`.
#[derive(Debug, Clone, PartialEq)]
pub struct SliceTierFloorCommitment {
    /// The slice IRI the floor is committed against (`gmeow:floorSlice`).
    pub slice: String,
    /// The tier IRI below which the slice's roll-up may not fall (`gmeow:floorTier`).
    pub tier: String,
}

/// The floor-free measurement standard SCORING reads: the tier ladder and the
/// quality axes (with their producers, dimensions, thresholds, weights, scopes, and
/// advice). This is EVERYTHING the lattice scorer ([`crate::lattice::assess`] /
/// `grade_axis` / `meet`) and the axis primitives consult — never a governance
/// floor. Splitting it out of [`Rubric`] gives scoring a floor-free projection
/// (interface segregation): a scorer cannot reach a committed floor, only measure.
#[derive(Debug, Clone, Default)]
pub struct MeasurementStandard {
    /// The tier ladder, sorted ascending by rank.
    pub tiers: Vec<Tier>,
    /// The quality axes, sorted by IRI for deterministic iteration.
    pub axes: Vec<Axis>,
}

impl MeasurementStandard {
    /// The floor tier (least rank), if the ladder is non-empty.
    #[must_use]
    pub fn bottom_tier(&self) -> Option<&Tier> {
        self.tiers.iter().min()
    }

    /// Look up a tier by IRI.
    #[must_use]
    pub fn tier(&self, iri: &str) -> Option<&Tier> {
        self.tiers.iter().find(|t| t.iri == iri)
    }
}

/// The governance data the RATCHET GATE reads: the dated axis exemptions and the two
/// committed floor sets (`gmeow:AxisFloorCommitment` measured-score floors and
/// `gmeow:SliceTierFloor` roll-up tier floors). SCORING never reads any of these —
/// they gate a measured score, they never produce one.
#[derive(Debug, Clone, Default)]
pub struct GovernanceFloors {
    /// The dated exemptions.
    pub exemptions: Vec<Exemption>,
    /// The committed per-slice, per-axis measured-score floors, sorted by IRI.
    pub commitments: Vec<AxisFloorCommitment>,
    /// The committed per-slice roll-up tier floors, sorted by IRI.
    pub tier_floors: Vec<SliceTierFloorCommitment>,
    /// The guarded projection-vocabulary set the ratchet gate counts residue
    /// against — `gmeow:ProjectionVocabulary` individuals, sorted by prefix. Loaded
    /// by a later change; empty until the ontology-resident loader lands.
    pub vocabularies: Vec<ProjectionVocabulary>,
    /// The committed per-(slice, vocabulary) residue ceilings —
    /// `gmeow:ProjectionCeilingCommitment` individuals, sorted by IRI. Loaded by a
    /// later change; empty until the ontology-resident loader lands.
    pub ceilings: Vec<ProjectionCeilingCommitment>,
}

/// How a guarded [`ProjectionVocabulary`]'s hand-authored constructs are recognized
/// in a slice's TTL surface — a `gmeow:vocabularyCountKind` individual. Drives which
/// enumeration [`crate::counting::enumerate`] runs for that vocab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountKind {
    /// SHACL-shaped: counted by structural role (typed `sh:NodeShape`/`sh:PropertyShape`
    /// plus subjects of `sh:path`/`sh:sparql`/`sh:rule`, so an anonymous nested property
    /// shape is caught, not just a typed top-level shape).
    Shape,
    /// A typed-axiom projection vocabulary (gUFO, FnO, BFO, DOLCE, EDOAL, SSSOM):
    /// counted by distinct triples whose predicate or object IRI falls in the vocab's
    /// namespace(s).
    TypedAxiom,
    /// A non-RDF surface (Datalog, Prolog, N3): structurally 0 in TTL slices. These
    /// registry entries are documentary-only — never enforced, never counted.
    NonRdfSurface,
}

impl CountKind {
    /// Resolve a `gmeow:vocabularyCountKind` individual's local name.
    #[must_use]
    pub fn from_local(local: &str) -> Option<Self> {
        match local {
            "countKindShape" => Some(Self::Shape),
            "countKindTypedAxiom" => Some(Self::TypedAxiom),
            "countKindNonRdfSurface" => Some(Self::NonRdfSurface),
            _ => None,
        }
    }

    /// The `gmeow:vocabularyCountKind` individual's local name this variant round-trips
    /// to — the exact inverse of [`Self::from_local`].
    #[must_use]
    pub fn as_local(&self) -> &'static str {
        match self {
            Self::Shape => "countKindShape",
            Self::TypedAxiom => "countKindTypedAxiom",
            Self::NonRdfSurface => "countKindNonRdfSurface",
        }
    }
}

/// A guarded `logic:`-subsumable projection vocabulary — a `gmeow:ProjectionVocabulary`
/// individual. Per Principle 17, OWL, SHACL, gUFO, BFO, DOLCE, and the alignment stack
/// (SSSOM, EDOAL, FnO) are generated lossy projections of `logic:`; each guarded vocab
/// here names one such projection surface, how its hand-authored constructs are
/// recognized ([`CountKind`]), and the by-reference bridge predicates that are exempt
/// from the ratchet (Principle 5, foundational alignment is "MORE is always BETTER").
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionVocabulary {
    /// The short vocabulary prefix (`sh`, `gufo`, `bfo`, `dul`, `fno`, `edoal`, `sssom`,
    /// `datalog`, `prolog`, `n3`) — the ceiling-commitment join key.
    pub prefix: String,
    /// The vocabulary's IRI namespace prefix(es). A `Vec` because a vocab may be
    /// authored under aliased namespaces (DOLCE's `dul`/`dolce-lite` variants) — any
    /// IRI starting with ANY listed namespace counts as belonging to this vocab.
    pub namespaces: Vec<String>,
    /// The `logic:` core IRI this vocabulary is a generated lossy projection of
    /// (`gmeow:vocabularySubsumedBy`) — the Principle 17 subsumption witness.
    pub subsumed_by: String,
    /// How authored constructs are recognized in this vocab's surface.
    pub count_kind: CountKind,
    /// The ceiling a slice with no explicit [`ProjectionCeilingCommitment`] for this
    /// vocab is held to (`gmeow:vocabularyDefaultCeiling`) — `0` for every guarded
    /// vocab, so a slice's first ungrounded use of a previously-absent vocab reds the
    /// gate instead of silently passing.
    pub default_ceiling: u64,
    /// The `logic:PreservationKind` local name this projection carries in the loss
    /// ledger (`gmeow:vocabularyPreservation`).
    pub preservation: String,
    /// The by-reference alignment/bridge predicate IRIs this vocab exempts from the
    /// residue when the triple's object resolves to an EXTERNAL (non-`gmeow:`)
    /// namespace (`gmeow:vocabularyAlignmentPredicate`) — e.g. `skos:*Match`,
    /// `rdfs:seeAlso`, `owl:equivalentClass`, `rdf:type`, `rdfs:subClassOf`.
    pub alignment_predicates: Vec<String>,
}

/// A committed per-(slice, vocabulary) ungrounded-residue ceiling — a
/// `gmeow:ProjectionCeilingCommitment` individual. The gate enforces it as a
/// lower-only ratchet, the inverse polarity of [`AxisFloorCommitment`]: the slice's
/// measured residue for `vocab_prefix` may never exceed `count`, and `count` itself
/// may only fall (or be removed) across a base-vs-working comparison, never rise.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionCeilingCommitment {
    /// The slice IRI the ceiling is committed against (`gmeow:ceilingSlice`).
    pub slice: String,
    /// The vocabulary prefix the ceiling caps (`gmeow:ceilingVocabulary`), joining
    /// against [`ProjectionVocabulary::prefix`].
    pub vocab_prefix: String,
    /// The committed maximum ungrounded-residue count (`gmeow:ceilingCount`).
    pub count: u64,
}

/// The whole rubric loaded from the slice: the floor-free measurement `standard`
/// scoring reads and the governance `floors` the ratchet gate reads. The two
/// concerns are segregated so a scorer is handed only [`MeasurementStandard`], never
/// a path to a committed floor.
#[derive(Debug, Clone, Default)]
pub struct Rubric {
    /// The measurement standard scoring reads (tier ladder + axes).
    pub standard: MeasurementStandard,
    /// The governance floors the ratchet gate reads (exemptions + committed floors).
    pub floors: GovernanceFloors,
}

/// The grade one axis earned on a slice: its measured score and the resulting tier.
#[derive(Debug, Clone)]
pub struct AxisGrade {
    /// The axis IRI.
    pub axis_iri: String,
    /// The normalized measured score in 0.0–1.0.
    pub score: f64,
    /// The tier the score earns (the highest tier whose floor the score meets),
    /// or the bottom tier when it meets no floor.
    pub tier: Tier,
}

/// The full assessment of a slice: the per-axis grade vector (the primary object)
/// plus the roll-up tier (its lossy meet projection).
#[derive(Debug, Clone)]
pub struct SliceAssessment {
    /// The slice IRI (or path key) under assessment.
    pub slice: String,
    /// The per-axis grades — the primary object, never discarded.
    pub grades: Vec<AxisGrade>,
    /// The roll-up tier: the unweighted lattice meet of the axis grades.
    pub rollup: Tier,
}
