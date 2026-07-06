// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `Grade` bilattice and the single gate policy morphism.
//!
//! A diagnostic's *grade* is not a scalar severity. It is a point in a
//! **bilattice** — two independent bounded orderings over the same carrier:
//!
//! * The **truth ordering** `⊑_t` — "how bad, and does it block?" It is the
//!   product of three chains: [`Severity`] (Info ⊑ Note ⊑ Warning ⊑ Error),
//!   [`Standpoint`] (Advisory ⊑ Perspectival ⊑ Binding), and the [`Blocking`]
//!   projection of the finding [`FindingCategory`] (Coherent ⊑ Blocking). Gate
//!   fatality is the principal up-set of this ordering, computed by the single
//!   monotone morphism [`gate`]. When two witnesses land on the same ledger
//!   anchor, their severities and standpoints merge by the `⊑_t`-**join** — the
//!   *least upper bound* — so the surviving grade is independent of the order in
//!   which the parallel scheduler folded them. That order-independence is the
//!   whole reason the merge is a lattice join and not a "last writer wins".
//!
//! * The **knowledge ordering** `⊑_k` — "what does the evidence, taken together,
//!   say?" It is the four-valued [`Belnap`] lattice
//!   (Neither ⊑ {Supported, Opposed} ⊑ Both). A finding that asserts a defect is
//!   *Supported*; a finding that asserts the apparent defect is actually coherent
//!   (a permitted epistemic conflict) is *Opposed*. Two witnesses that disagree
//!   `⊑_k`-join to **Both** — a *glut* — which is detected and surfaced, never
//!   silently collapsed to one side. Absence of any witness is *Neither* — a gap,
//!   which is distinct from a glut. The two are never conflated ("never collapse
//!   Belnap").
//!
//! The gate reads **only** the truth axis; a knowledge glut or gap never gates.

use serde::{Deserialize, Serialize};

pub use crate::model::{FindingCategory, Severity};

/// A bounded lattice: a partial order with a least element [`BOTTOM`], a greatest
/// element [`TOP`], and total binary [`join`] (least upper bound) and [`meet`]
/// (greatest lower bound). Every impl in this module is verified *exhaustively*
/// against the lattice laws in the tests, which is possible — and stronger than
/// sampled property testing — because every carrier here is a small finite set.
///
/// [`BOTTOM`]: BoundedLattice::BOTTOM
/// [`TOP`]: BoundedLattice::TOP
/// [`join`]: BoundedLattice::join
/// [`meet`]: BoundedLattice::meet
pub trait BoundedLattice: Copy + Eq {
    const BOTTOM: Self;
    const TOP: Self;
    /// Least upper bound.
    fn join(self, other: Self) -> Self;
    /// Greatest lower bound.
    fn meet(self, other: Self) -> Self;
    /// The reflexive partial order induced by the lattice: `a ⊑ b` iff
    /// `a.join(b) == b` (equivalently `a.meet(b) == a`).
    fn leq(self, other: Self) -> bool {
        self.join(other) == other
    }
}

// --- Severity as a truth chain ------------------------------------------------

impl Severity {
    /// Position on the truth chain, `Info = 0 .. Error = 3` (bottom-up). This is
    /// the *inverse* of [`Severity`]'s report `sort_rank` (which puts the loudest
    /// finding first): on the lattice the loudest severity is the TOP.
    fn truth_rank(self) -> u8 {
        match self {
            Self::Info => 0,
            Self::Note => 1,
            Self::Warning => 2,
            Self::Error => 3,
        }
    }

    /// Every severity, bottom-to-top — drives the exhaustive lattice-law tests.
    pub const ALL: [Severity; 4] = [
        Severity::Info,
        Severity::Note,
        Severity::Warning,
        Severity::Error,
    ];
}

impl BoundedLattice for Severity {
    const BOTTOM: Self = Severity::Info;
    const TOP: Self = Severity::Error;
    fn join(self, other: Self) -> Self {
        if self.truth_rank() >= other.truth_rank() {
            self
        } else {
            other
        }
    }
    fn meet(self, other: Self) -> Self {
        if self.truth_rank() <= other.truth_rank() {
            self
        } else {
            other
        }
    }
}

// --- Standpoint: the gating-strength chain ------------------------------------

/// The gating *strength* a finding is asserted from — a chain
/// Advisory ⊑ Perspectival ⊑ Binding. It grounds `logic:StandpointContextAxis`
/// and the `gmeow:sharpens` standpoint poset. Only a **Binding** standpoint can
/// contribute to gate fatality; an **Advisory** finding never gates, whatever its
/// severity — that "never gate" guarantee falls out of the up-set construction,
/// it is not a special case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Standpoint {
    Advisory,
    Perspectival,
    Binding,
}

impl Standpoint {
    fn rank(self) -> u8 {
        match self {
            Self::Advisory => 0,
            Self::Perspectival => 1,
            Self::Binding => 2,
        }
    }
    pub const ALL: [Standpoint; 3] = [
        Standpoint::Advisory,
        Standpoint::Perspectival,
        Standpoint::Binding,
    ];
}

impl BoundedLattice for Standpoint {
    const BOTTOM: Self = Standpoint::Advisory;
    const TOP: Self = Standpoint::Binding;
    fn join(self, other: Self) -> Self {
        if self.rank() >= other.rank() {
            self
        } else {
            other
        }
    }
    fn meet(self, other: Self) -> Self {
        if self.rank() <= other.rank() {
            self
        } else {
            other
        }
    }
}

// --- Blocking: the gating projection of a category ----------------------------

/// The only part of a [`FindingCategory`] that touches gating: whether the kind
/// of finding is *Blocking* (a real failure) or *Coherent* (surfaced but never a
/// failure). Coherent ⊑ Blocking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Blocking {
    Coherent,
    Blocking,
}

impl Blocking {
    pub const ALL: [Blocking; 2] = [Blocking::Coherent, Blocking::Blocking];
}

impl BoundedLattice for Blocking {
    const BOTTOM: Self = Blocking::Coherent;
    const TOP: Self = Blocking::Blocking;
    fn join(self, other: Self) -> Self {
        if self == Blocking::Blocking || other == Blocking::Blocking {
            Blocking::Blocking
        } else {
            Blocking::Coherent
        }
    }
    fn meet(self, other: Self) -> Self {
        if self == Blocking::Coherent || other == Blocking::Coherent {
            Blocking::Coherent
        } else {
            Blocking::Blocking
        }
    }
}

impl FindingCategory {
    /// The gating projection of a category. Exactly the three *failure* kinds are
    /// Blocking; the other five — including [`PermittedEpistemicConflict`] — are
    /// Coherent and can never contribute to gate fatality.
    ///
    /// [`PermittedEpistemicConflict`]: FindingCategory::PermittedEpistemicConflict
    pub fn blocking(self) -> Blocking {
        match self {
            Self::DataShapeViolation
            | Self::ModelingDisciplineViolation
            | Self::ContradictionWitness => Blocking::Blocking,
            Self::PermittedEpistemicConflict
            | Self::UnsupportedSemanticFeature
            | Self::IncompleteCheck
            | Self::ProjectionLoss
            | Self::PolicyWarning => Blocking::Coherent,
        }
    }

    /// The Belnap coherence *polarity* this kind of finding asserts. A witnessed
    /// contradiction is evidence a defect is present (*Supported*); a permitted
    /// epistemic conflict is evidence the apparent contradiction is coherent
    /// (*Opposed*). Every other kind takes no stance on coherence (*Neither*).
    pub fn polarity(self) -> Belnap {
        match self {
            Self::DataShapeViolation
            | Self::ModelingDisciplineViolation
            | Self::ContradictionWitness => Belnap::Supported,
            Self::PermittedEpistemicConflict => Belnap::Opposed,
            Self::UnsupportedSemanticFeature
            | Self::IncompleteCheck
            | Self::ProjectionLoss
            | Self::PolicyWarning => Belnap::Neither,
        }
    }

    /// A deterministic total order on categories, used only to break ties when two
    /// equally-blocking categories merge — so the merge is commutative.
    fn merge_rank(self) -> u8 {
        match self {
            Self::DataShapeViolation => 0,
            Self::ModelingDisciplineViolation => 1,
            Self::ContradictionWitness => 2,
            Self::PermittedEpistemicConflict => 3,
            Self::UnsupportedSemanticFeature => 4,
            Self::IncompleteCheck => 5,
            Self::ProjectionLoss => 6,
            Self::PolicyWarning => 7,
        }
    }

    pub const ALL: [FindingCategory; 8] = [
        FindingCategory::DataShapeViolation,
        FindingCategory::ModelingDisciplineViolation,
        FindingCategory::ContradictionWitness,
        FindingCategory::PermittedEpistemicConflict,
        FindingCategory::UnsupportedSemanticFeature,
        FindingCategory::IncompleteCheck,
        FindingCategory::ProjectionLoss,
        FindingCategory::PolicyWarning,
    ];
}

// --- Belnap: the knowledge (information) axis ---------------------------------

/// The four-valued Belnap knowledge lattice, `⊑_k`. It is the diamond
/// Neither ⊑ {Supported, Opposed} ⊑ Both: *Neither* is a gap (no evidence),
/// *Both* is a glut (contradictory evidence), and Supported/Opposed are
/// incomparable one-sided evidence. The `⊑_k`-**join** combines evidence, so two
/// witnesses that disagree join to *Both*. Gaps and gluts are distinct values and
/// are never collapsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Belnap {
    Neither,
    Supported,
    Opposed,
    Both,
}

impl Belnap {
    /// Whether this value is a glut (contradictory evidence).
    pub fn is_glut(self) -> bool {
        self == Belnap::Both
    }
    pub const ALL: [Belnap; 4] = [
        Belnap::Neither,
        Belnap::Supported,
        Belnap::Opposed,
        Belnap::Both,
    ];
}

impl BoundedLattice for Belnap {
    const BOTTOM: Self = Belnap::Neither;
    const TOP: Self = Belnap::Both;
    fn join(self, other: Self) -> Self {
        use Belnap::*;
        match (self, other) {
            (a, b) if a == b => a,
            (Neither, x) | (x, Neither) => x,
            (Both, _) | (_, Both) => Both,
            // Supported and Opposed are incomparable: combining them is a glut.
            (Supported, Opposed) | (Opposed, Supported) => Both,
            // remaining same-value cases handled by the first arm.
            _ => unreachable!(),
        }
    }
    fn meet(self, other: Self) -> Self {
        use Belnap::*;
        match (self, other) {
            (a, b) if a == b => a,
            (Both, x) | (x, Both) => x,
            (Neither, _) | (_, Neither) => Neither,
            // Supported and Opposed share only the bottom.
            (Supported, Opposed) | (Opposed, Supported) => Neither,
            _ => unreachable!(),
        }
    }
}

// --- Grade: the bilattice point -----------------------------------------------

/// A diagnostic grade: a point in the `(Severity × FindingCategory × Standpoint)`
/// bilattice. `category` is the full payload kind; its gating contribution is the
/// [`Blocking`] projection and its evidential contribution is the [`Belnap`]
/// polarity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Grade {
    pub severity: Severity,
    pub category: FindingCategory,
    pub standpoint: Standpoint,
}

/// The result of merging two grades at one ledger anchor: the `⊑_t`-joined
/// [`Grade`] together with the `⊑_k`-joined [`Belnap`] knowledge value. A
/// `knowledge` of [`Belnap::Both`] flags a *glut* — the two witnesses disagreed
/// about coherence and neither was dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GradeMerge {
    pub grade: Grade,
    pub knowledge: Belnap,
}

impl GradeMerge {
    /// Whether the merge surfaced contradictory evidence.
    pub fn is_glut(self) -> bool {
        self.knowledge.is_glut()
    }
}

impl Grade {
    pub const fn new(
        severity: Severity,
        category: FindingCategory,
        standpoint: Standpoint,
    ) -> Self {
        Grade {
            severity,
            category,
            standpoint,
        }
    }

    /// The truth-axis triple `(severity, blocking(category), standpoint)`.
    fn truth(self) -> (Severity, Blocking, Standpoint) {
        (self.severity, self.category.blocking(), self.standpoint)
    }

    /// The reflexive truth ordering `⊑_t` — componentwise on the truth triple.
    pub fn leq_truth(self, other: Self) -> bool {
        let (s0, b0, p0) = self.truth();
        let (s1, b1, p1) = other.truth();
        s0.leq(s1) && b0.leq(b1) && p0.leq(p1)
    }

    /// Merge two grades landing on the same anchor. Severity and standpoint take
    /// the `⊑_t`-join (least upper bound) so the outcome is independent of merge
    /// order; the category is the more-blocking of the two, ties broken by a fixed
    /// total order so the choice is deterministic; and the knowledge value is the
    /// `⊑_k`-join of the two coherence polarities, exposing a glut when the
    /// witnesses disagree. This is the operation the hash-consed ledger applies on
    /// a fingerprint collision.
    pub fn merge(self, other: Self) -> GradeMerge {
        let category = self.join_category(other);
        GradeMerge {
            grade: Grade {
                severity: self.severity.join(other.severity),
                category,
                standpoint: self.standpoint.join(other.standpoint),
            },
            knowledge: self.category.polarity().join(other.category.polarity()),
        }
    }

    /// The merged payload category: the more-blocking of the two, ties broken by
    /// the fixed [`FindingCategory::merge_rank`] total order (lower rank wins) so
    /// the choice is commutative and associative.
    fn join_category(self, other: Self) -> FindingCategory {
        let (a, b) = (self.category, other.category);
        match a.blocking().join(b.blocking()) {
            // If exactly one is blocking, take it; if both or neither, tie-break.
            Blocking::Blocking if a.blocking() != b.blocking() => {
                if a.blocking() == Blocking::Blocking {
                    a
                } else {
                    b
                }
            }
            _ => {
                if a.merge_rank() <= b.merge_rank() {
                    a
                } else {
                    b
                }
            }
        }
    }
}

// --- The gate policy morphism -------------------------------------------------

/// The verdict of the gate: a two-element chain, Collected ⊑ Fatal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GateVerdict {
    Collected,
    Fatal,
}

impl GateVerdict {
    pub const ALL: [GateVerdict; 2] = [GateVerdict::Collected, GateVerdict::Fatal];
}

impl BoundedLattice for GateVerdict {
    const BOTTOM: Self = GateVerdict::Collected;
    const TOP: Self = GateVerdict::Fatal;
    fn join(self, other: Self) -> Self {
        if self == GateVerdict::Fatal || other == GateVerdict::Fatal {
            GateVerdict::Fatal
        } else {
            GateVerdict::Collected
        }
    }
    fn meet(self, other: Self) -> Self {
        if self == GateVerdict::Collected || other == GateVerdict::Collected {
            GateVerdict::Collected
        } else {
            GateVerdict::Fatal
        }
    }
}

/// **The** gate policy morphism: the single place fatality is decided.
///
/// It is the [`meet`] (logical AND) of three monotone axis-maps into
/// [`GateVerdict`] — a finding is Fatal iff its severity is `Error`, its category
/// is Blocking, *and* its standpoint is Binding. Being a meet of monotone maps it
/// is itself monotone over `⊑_t`, so the Fatal region is exactly the principal
/// up-set `↑(Error, Blocking, Binding)`. Two theorems follow *by construction*,
/// not by special-casing: any Advisory-standpoint finding and any Coherent-category
/// finding (which includes every [`FindingCategory::PermittedEpistemicConflict`])
/// is structurally unable to reach Fatal.
///
/// [`meet`]: BoundedLattice::meet
pub fn gate(grade: Grade) -> GateVerdict {
    let sev = if grade.severity == Severity::Error {
        GateVerdict::Fatal
    } else {
        GateVerdict::Collected
    };
    let cat = if grade.category.blocking() == Blocking::Blocking {
        GateVerdict::Fatal
    } else {
        GateVerdict::Collected
    };
    let standpoint = if grade.standpoint == Standpoint::Binding {
        GateVerdict::Fatal
    } else {
        GateVerdict::Collected
    };
    sev.meet(cat).meet(standpoint)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exhaustively check the bounded-lattice laws over a finite carrier. This is
    /// a proof over the whole domain, not a sampled property test.
    fn assert_lattice_laws<L: BoundedLattice + std::fmt::Debug>(all: &[L]) {
        for &a in all {
            // Idempotence.
            assert_eq!(a.join(a), a, "join idempotence");
            assert_eq!(a.meet(a), a, "meet idempotence");
            // Identities with BOTTOM/TOP.
            assert_eq!(a.join(L::BOTTOM), a, "join bottom identity");
            assert_eq!(a.meet(L::TOP), a, "meet top identity");
            assert_eq!(a.join(L::TOP), L::TOP, "join top absorbs");
            assert_eq!(a.meet(L::BOTTOM), L::BOTTOM, "meet bottom absorbs");
            for &b in all {
                // Commutativity.
                assert_eq!(a.join(b), b.join(a), "join commutative");
                assert_eq!(a.meet(b), b.meet(a), "meet commutative");
                // Absorption.
                assert_eq!(a.join(a.meet(b)), a, "absorption join/meet");
                assert_eq!(a.meet(a.join(b)), a, "absorption meet/join");
                // leq consistency: a ⊑ b  ⇔  join == b  ⇔  meet == a.
                assert_eq!(a.leq(b), a.join(b) == b, "leq via join");
                assert_eq!(a.leq(b), a.meet(b) == a, "leq via meet");
                for &c in all {
                    // Associativity.
                    assert_eq!(a.join(b).join(c), a.join(b.join(c)), "join associative");
                    assert_eq!(a.meet(b).meet(c), a.meet(b.meet(c)), "meet associative");
                }
            }
        }
    }

    #[test]
    fn truth_axis_lattices_obey_the_laws() {
        assert_lattice_laws(&Severity::ALL);
        assert_lattice_laws(&Standpoint::ALL);
        assert_lattice_laws(&Blocking::ALL);
        assert_lattice_laws(&GateVerdict::ALL);
    }

    #[test]
    fn knowledge_axis_belnap_obeys_the_laws() {
        assert_lattice_laws(&Belnap::ALL);
    }

    /// Every grade in the finite bilattice, for exhaustive gate/merge tests.
    fn all_grades() -> Vec<Grade> {
        let mut out = Vec::new();
        for &s in &Severity::ALL {
            for &c in &FindingCategory::ALL {
                for &p in &Standpoint::ALL {
                    out.push(Grade::new(s, c, p));
                }
            }
        }
        out
    }

    #[test]
    fn gate_is_monotone_over_the_truth_order() {
        // g1 ⊑_t g2  ⇒  gate(g1) ⊑ gate(g2), over every ordered pair.
        for &g1 in &all_grades() {
            for &g2 in &all_grades() {
                if g1.leq_truth(g2) {
                    assert!(
                        gate(g1).leq(gate(g2)),
                        "gate not monotone: {g1:?} ⊑_t {g2:?} but gate {:?} ⋢ {:?}",
                        gate(g1),
                        gate(g2)
                    );
                }
            }
        }
    }

    #[test]
    fn advisory_and_permitted_conflict_never_gate() {
        for &g in &all_grades() {
            if g.standpoint == Standpoint::Advisory {
                assert_eq!(
                    gate(g),
                    GateVerdict::Collected,
                    "advisory must not gate: {g:?}"
                );
            }
            if g.category == FindingCategory::PermittedEpistemicConflict {
                assert_eq!(
                    gate(g),
                    GateVerdict::Collected,
                    "permitted epistemic conflict must not gate: {g:?}"
                );
            }
        }
    }

    #[test]
    fn gate_fatal_exactly_on_the_principal_up_set() {
        for &g in &all_grades() {
            let expected = g.severity == Severity::Error
                && g.category.blocking() == Blocking::Blocking
                && g.standpoint == Standpoint::Binding;
            assert_eq!(
                gate(g) == GateVerdict::Fatal,
                expected,
                "gate fatal region mismatch at {g:?}"
            );
        }
    }

    #[test]
    fn merge_is_order_independent() {
        // merge(a,b).grade == merge(b,a).grade for every pair — hash-cons cannot
        // depend on which shard folded first. Knowledge join is commutative too.
        for &a in &all_grades() {
            for &b in &all_grades() {
                let ab = a.merge(b);
                let ba = b.merge(a);
                assert_eq!(ab.grade.severity, ba.grade.severity, "severity merge order");
                assert_eq!(
                    ab.grade.standpoint, ba.grade.standpoint,
                    "standpoint merge order"
                );
                assert_eq!(ab.grade.category, ba.grade.category, "category merge order");
                assert_eq!(ab.knowledge, ba.knowledge, "knowledge merge order");
                // Severity/standpoint are the joins of the inputs.
                assert_eq!(ab.grade.severity, a.severity.join(b.severity));
                assert_eq!(ab.grade.standpoint, a.standpoint.join(b.standpoint));
            }
        }
    }

    #[test]
    fn contradictory_pair_merges_to_a_glut_not_to_either_side() {
        let witness = Grade::new(
            Severity::Error,
            FindingCategory::ContradictionWitness,
            Standpoint::Binding,
        );
        let permitted = Grade::new(
            Severity::Note,
            FindingCategory::PermittedEpistemicConflict,
            Standpoint::Perspectival,
        );
        let merged = witness.merge(permitted);
        assert!(
            merged.is_glut(),
            "disagreeing witnesses must produce a glut"
        );
        assert_eq!(merged.knowledge, Belnap::Both);
        // The glut is not either input's polarity taken alone.
        assert_ne!(merged.knowledge, witness.category.polarity());
        assert_ne!(merged.knowledge, permitted.category.polarity());
    }

    #[test]
    fn agreeing_witnesses_do_not_glut() {
        let a = Grade::new(
            Severity::Error,
            FindingCategory::DataShapeViolation,
            Standpoint::Binding,
        );
        let b = Grade::new(
            Severity::Warning,
            FindingCategory::ContradictionWitness,
            Standpoint::Perspectival,
        );
        // Both assert a defect (Supported); their join stays Supported, no glut.
        assert!(!a.merge(b).is_glut());
        assert_eq!(a.merge(b).knowledge, Belnap::Supported);
    }

    #[test]
    fn unknown_severity_token_is_a_hard_fail() {
        // F2: parsing must reject an unknown token, never silently default.
        assert!(Severity::parse("bogus").is_err());
        assert!(Severity::parse("").is_err());
        // Known aliases still resolve (behavior preserved).
        assert_eq!(Severity::parse("fatal").unwrap(), Severity::Error);
        assert_eq!(Severity::parse("warn").unwrap(), Severity::Warning);
    }
}
