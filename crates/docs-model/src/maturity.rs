// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The documentation-maturity Formal Concept Analysis (Galois) machinery — the
//! deterministic, reasoner-free closure over the abstract "documented record
//! *covers* dimension" incidence.
//!
//! The named maturity tiers ([`MaturityAnchor`]) are distinguished INTENTS inside
//! the Formal-Concept lattice of that incidence: each anchor is the set of
//! [`Dimension`] attributes it requires, and a record EARNS the largest anchor
//! whose intent is a subset of the record's covered-dimension set (the projected
//! floor, [`earned_maturity`]). The maturity ORDER between anchors is derived from
//! intent inclusion (`Minimal ⊆ Basic ⊆ Full ⊆ Maximal`), never a tuned numeric
//! threshold; the only numeric on the surface is the bounded fraction
//! [`coverage_fraction`] ∈ `[0, 1]`.
//!
//! This module is deliberately PURE: it operates over the abstract [`Dimension`]
//! / [`MaturityAnchor`] types with NO dependency on the `DocsModel`. It is the
//! single Rust source of the anchor intents ([`anchor_table`]); the later
//! coverage→RDF projection wires the model's
//! per-term coverage into a [`DimSet`] and reuses this table.
//!
//! # Synchronization contract
//!
//! [`anchor_table`] is the Rust twin of `slices/core/documentation/module.ttl`'s
//! `gmeow:maturityRequiresDimension` intents. The two MUST stay in lockstep: a
//! change to an anchor's intent in the TTL requires the same change here (and the
//! slice's structural cells `saAnchorIntentsNest` / `saNoOrphanDimension` guard
//! the TTL side, while the module tests guard this side).

use std::collections::BTreeSet;

/// A documentation-coverage dimension — one attribute of the Formal-Concept
/// incidence. Each variant is a DETERMINISTIC structural predicate (a
/// present/absent fact of a documented record), never a tuned threshold, which is
/// what keeps every maturity axis objective. The declaration order is the stable
/// dimension order and matches `gmeow:DocCoverageDimension`'s seed list in
/// `module.ttl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dimension {
    /// `skos:definition` / `rdfs:comment` present and non-blank.
    Definition,
    /// `rdfs:label` present and non-blank.
    Label,
    /// At least one of `gmeow:useWhen` / `avoidWhen` / `howToUse` present.
    UsageAdvice,
    /// At least one `skos:example` present.
    Example,
    /// At least one `skos:scopeNote` present.
    ScopeNote,
    /// The term is the subject of at least one external alignment / linkage.
    Alignment,
    /// The term is referenced by BOTH a well-formed and a counter-example fixture.
    FixturePair,
    /// The term is exercised by a competency question carrying a rationale.
    CompetencyRationale,
    /// The term is demonstrated by a worked instance under `examples/`.
    WorkedInstance,
    /// The term carries a loss-kind evidence node (a projection-loss ledger row).
    LossLedgerRow,
    /// The term participates in the cross-vocabulary linkage / mapping coverage.
    LinkageCoverage,
    /// The slice's `docs.md` design-set table marks the term's realized state.
    RealizedState,
    /// The full advice coat: useWhen ∧ avoidWhen ∧ howToUse ∧ graphBoxRole.
    AnnotationCoat,
    /// The owning slice's `docs.md` opens with a thesis sentence.
    ThesisSentence,
    /// The term's carrier strings are present in every supported language.
    TranslationCoverage,
    /// The term is reached by at least one structural or competency test.
    TestReach,
    /// The rationale names no test artifact (a name-membership test).
    ProvenanceHonesty,
    /// The prose-quality structural conjunction (three-NOTs ∧ worked triple ∧
    /// distinct usage coat ∧ distinct rationale).
    ProseQuality,
    /// The MAXIMAL-only Principle-17 loss refinement: every projection-loss
    /// judgment for the term is sound-or-stronger in the `logic:PreservationKind`
    /// ordering. Distinct from [`Dimension::LossLedgerRow`] (the FULL-tier presence
    /// bit) so the presence and the judgment gate never collide on one attribute.
    LossJudgmentSound,
}

impl Dimension {
    /// Every dimension in stable declaration order — the canonical order the
    /// projection and the burn-down readout use.
    pub const ALL: [Dimension; 19] = [
        Dimension::Definition,
        Dimension::Label,
        Dimension::UsageAdvice,
        Dimension::Example,
        Dimension::ScopeNote,
        Dimension::Alignment,
        Dimension::FixturePair,
        Dimension::CompetencyRationale,
        Dimension::WorkedInstance,
        Dimension::LossLedgerRow,
        Dimension::LinkageCoverage,
        Dimension::RealizedState,
        Dimension::AnnotationCoat,
        Dimension::ThesisSentence,
        Dimension::TranslationCoverage,
        Dimension::TestReach,
        Dimension::ProvenanceHonesty,
        Dimension::ProseQuality,
        Dimension::LossJudgmentSound,
    ];

    /// The dimension whose TBox `gmeow:dim*` individual carries `local`, if any —
    /// the inverse of [`Dimension::local_name`]. Used to lift a dimension local
    /// name read back from the emitted `graph/documentation` incidence into the
    /// typed enum (the health page reads projected facts, then names them).
    pub fn from_local(local: &str) -> Option<Dimension> {
        Dimension::ALL
            .iter()
            .copied()
            .find(|d| d.local_name() == local)
    }

    /// The `gmeow:` local name of the dimension's TBox individual in
    /// `module.ttl` — the join key that ties this Rust twin to the ontology.
    pub fn local_name(self) -> &'static str {
        match self {
            Dimension::Definition => "dimDefinition",
            Dimension::Label => "dimLabel",
            Dimension::UsageAdvice => "dimUsageAdvice",
            Dimension::Example => "dimExample",
            Dimension::ScopeNote => "dimScopeNote",
            Dimension::Alignment => "dimAlignment",
            Dimension::FixturePair => "dimFixturePair",
            Dimension::CompetencyRationale => "dimCompetencyRationale",
            Dimension::WorkedInstance => "dimWorkedInstance",
            Dimension::LossLedgerRow => "dimLossLedgerRow",
            Dimension::LinkageCoverage => "dimLinkageCoverage",
            Dimension::RealizedState => "dimRealizedState",
            Dimension::AnnotationCoat => "dimAnnotationCoat",
            Dimension::ThesisSentence => "dimThesisSentence",
            Dimension::TranslationCoverage => "dimTranslationCoverage",
            Dimension::TestReach => "dimTestReach",
            Dimension::ProvenanceHonesty => "dimProvenanceHonesty",
            Dimension::ProseQuality => "dimProseQuality",
            Dimension::LossJudgmentSound => "dimLossJudgmentSound",
        }
    }
}

/// A set of coverage dimensions — a record's covered-dimension set (its concept
/// intent), or an anchor's required-dimension set. A [`BTreeSet`] so membership
/// and subset tests are exact and iteration is deterministic.
pub type DimSet = BTreeSet<Dimension>;

/// Build a [`DimSet`] from a slice of dimensions.
pub fn dim_set(dims: &[Dimension]) -> DimSet {
    dims.iter().copied().collect()
}

/// A named documentation-maturity anchor — a distinguished intent in the
/// Formal-Concept lattice. The variants' rank order (`Minimal < Basic < Full <
/// Maximal`) is the derived maturity order; it is guaranteed to agree with intent
/// inclusion because the intents are authored to nest (checked in the module tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MaturityAnchor {
    /// A term is named and defined.
    Minimal,
    /// The six-dimension core coat.
    Basic,
    /// The core coat plus proof-carrying evidence and honest realized state.
    Full,
    /// The full intent plus every remaining structural dimension.
    Maximal,
}

impl MaturityAnchor {
    /// Every anchor from lowest to highest rank.
    pub const ALL: [MaturityAnchor; 4] = [
        MaturityAnchor::Minimal,
        MaturityAnchor::Basic,
        MaturityAnchor::Full,
        MaturityAnchor::Maximal,
    ];

    /// The anchor's position in the nesting order — the tie-break key for
    /// [`earned_maturity`] (the largest rank wins).
    pub fn rank(self) -> u8 {
        match self {
            MaturityAnchor::Minimal => 0,
            MaturityAnchor::Basic => 1,
            MaturityAnchor::Full => 2,
            MaturityAnchor::Maximal => 3,
        }
    }

    /// The `gmeow:` local name of the anchor's TBox individual in `module.ttl`.
    pub fn local_name(self) -> &'static str {
        match self {
            MaturityAnchor::Minimal => "docMaturityMinimal",
            MaturityAnchor::Basic => "docMaturityBasic",
            MaturityAnchor::Full => "docMaturityFull",
            MaturityAnchor::Maximal => "docMaturityMaximal",
        }
    }

    /// The anchor whose `gmeow:docMaturity*` individual carries `local`, if any —
    /// the inverse of [`MaturityAnchor::local_name`], lifting an anchor read back
    /// from the emitted `gmeow:docEarnedMaturity` / `gmeow:sliceDocMaturity`
    /// incidence into the typed enum.
    pub fn from_local(local: &str) -> Option<MaturityAnchor> {
        MaturityAnchor::ALL
            .iter()
            .copied()
            .find(|a| a.local_name() == local)
    }

    /// The next anchor up the derived ladder (`Minimal → Basic → Full → Maximal`),
    /// or `None` at the top. The target of the health page's gap-to-next-tier
    /// burn-down: the dimensions in `self.next()`'s intent a record does not yet
    /// cover are exactly what stands between it and the next tier.
    pub fn next(self) -> Option<MaturityAnchor> {
        MaturityAnchor::ALL
            .iter()
            .copied()
            .find(|a| a.rank() == self.rank() + 1)
    }

    /// The anchor's INTENT — the set of dimensions it requires. This is the single
    /// Rust source of the intents and the twin of `module.ttl`'s
    /// `gmeow:maturityRequiresDimension`; the intents nest by construction
    /// (`Minimal ⊆ Basic ⊆ Full ⊆ Maximal`).
    pub fn intent(self) -> DimSet {
        use Dimension::*;
        let minimal = [Definition, Label];
        let basic_extra = [UsageAdvice, Example, ScopeNote, Alignment];
        let full_extra = [
            FixturePair,
            CompetencyRationale,
            WorkedInstance,
            LossLedgerRow,
            LinkageCoverage,
            RealizedState,
        ];
        let maximal_extra = [
            AnnotationCoat,
            ThesisSentence,
            TranslationCoverage,
            TestReach,
            ProvenanceHonesty,
            ProseQuality,
            LossJudgmentSound,
        ];
        let mut set: DimSet = minimal.iter().copied().collect();
        if self.rank() >= MaturityAnchor::Basic.rank() {
            set.extend(basic_extra);
        }
        if self.rank() >= MaturityAnchor::Full.rank() {
            set.extend(full_extra);
        }
        if self.rank() >= MaturityAnchor::Maximal.rank() {
            set.extend(maximal_extra);
        }
        set
    }
}

/// The canonical anchor table — each anchor paired with its intent, in ascending
/// rank order. The single Rust source of the intents that the coverage→RDF
/// projection reuses; the twin of `module.ttl`'s `gmeow:maturityRequiresDimension`.
pub fn anchor_table() -> Vec<(MaturityAnchor, DimSet)> {
    MaturityAnchor::ALL
        .iter()
        .map(|&a| (a, a.intent()))
        .collect()
}

/// The projected floor: the LARGEST anchor whose required set is a subset of
/// `covered`. Deterministic; ties broken by the nesting rank (the highest rank
/// wins). Returns `None` when no anchor's intent is satisfied (not even the
/// minimal one).
pub fn earned_maturity(
    covered: &DimSet,
    anchors: &[(MaturityAnchor, DimSet)],
) -> Option<MaturityAnchor> {
    anchors
        .iter()
        .filter(|(_, intent)| intent.is_subset(covered))
        .map(|(anchor, _)| *anchor)
        .max_by_key(|anchor| anchor.rank())
}

/// The headline gate predicate — `true` when the CLAIMED maturity exceeds the
/// EARNED floor (`asserted ⊄ earned`), i.e. the slice asserts a tier its coverage
/// does not support. With nested intents, `asserted`'s intent ⊆ covered exactly
/// when `earned`'s rank ≥ `asserted`'s rank, so this is intent-inclusion by the
/// derived order: a violation is `earned` absent, or `earned` ranked below
/// `asserted`.
pub fn asserted_exceeds_earned(asserted: MaturityAnchor, earned: Option<MaturityAnchor>) -> bool {
    match earned {
        None => true,
        Some(e) => asserted.rank() > e.rank(),
    }
}

/// The bounded coverage fraction — `|covered ∩ intent| / |intent|`, a value in the
/// closed interval `[0, 1]`. An empty intent yields `1.0` (vacuously fully
/// covered). A bounded fraction by construction, never an unbounded ratio, so it
/// can never be tuned to a target.
pub fn coverage_fraction(covered: &DimSet, intent: &DimSet) -> f64 {
    if intent.is_empty() {
        return 1.0;
    }
    let hit = intent.iter().filter(|d| covered.contains(d)).count();
    hit as f64 / intent.len() as f64
}

#[path = "maturity.tests.rs"]
#[cfg(test)]
mod tests;
