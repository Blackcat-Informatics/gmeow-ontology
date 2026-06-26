// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native Rust evaluator for the OntoUML *foundation* disciplines (issue #636).
//!
//! This module is the **canonical** native evaluator for the OntoUML *foundation*
//! disciplines.  (The Python foundation oracle that preceded it —
//! `logic_foundation.py` plus the `enable_naf` chase path of
//! `logic_materialize.py` — was retired in #636/#497.)  It lowers five OntoUML
//! structural disciplines into a small stratified Datalog program with
//! negation-as-failure and inequality guards, runs that program *per world* as a
//! semi-naive chase, and then applies two cross-world post-passes (positive
//! cross-world rigidity and the anti-rigidity witness policy).
//!
//! # Canonical evaluation contract
//!
//! The materialized quad *set* alone is not enough: the explanation goldens are
//! content-addressed by **derivation IRIs**, and a derivation IRI is
//! `mint_derivation_id(rule_iri, sorted(source_reifiers))`.  For a quad derivable
//! by more than one rule firing, this crate records the **first** firing under its
//! evaluation order (first-wins dedup).  The following ordering constraints are
//! this crate's normative contract:
//!
//! 1. **Stratum order** — the foundation rules are partitioned into the same five
//!    strata the certifier's `stratify` produces (helpers/markers before the
//!    NAF-dependent helpers before the violation rules), so a negated atom is only
//!    checked once the predicate it negates is at fixpoint.
//! 2. **Rule order within a stratum** — the canonical order
//!    `LogicProgram.__post_init__` sorts rules into (by each rule's `_sort_key`:
//!    head, then body, then distinct pairs).  The rule tables in [`STRATA`] are
//!    written out in exactly that order.
//! 3. **Body-binding enumeration order** — the join walks facts in *insertion
//!    order* (the Python `fact_index` is insertion-ordered), so this evaluator
//!    stores facts in a `Vec` and iterates it in order.
//! 4. **First-wins dedup** — a quad whose `(s.n3(), p.n3(), o.n3())` key already
//!    exists is dropped, keeping the first derivation's provenance.
//!
//! The provenance recipe itself is reused verbatim from [`crate::provenance`]
//! (`mint_reifier` / `mint_derivation_id`), which is already golden-pinned to the
//! Python oracle.
//!
//! # No-optionality
//!
//! An unknown anti-rigidity policy is a hard error ([`AntiRigidityPolicy::from_str`]
//! returns `Err`).  A malformed inequality guard (an unbound guard variable) is a
//! hard error.  There is no silent default and no degraded fallback.

use std::collections::HashMap;
use std::collections::HashSet;

use oxigraph::model::{NamedNode, Term};
use rayon::prelude::*;

use crate::provenance::{mint_derivation_id, mint_reifier};
use crate::store::WorldStore;

// ── Namespace + vocabulary constants ───────────────────────────────────────────

/// The `logic:` vocabulary namespace — term IRIs are `LOGIC_NS + local`.
/// Matches `gmeow_tools.config.LOGIC_NAMESPACE`.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// The `rdf:type` predicate IRI (string form), matching `logic_foundation._RDF_TYPE`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Sentinel rule IRI stamped on asserted (input) quads (`logic:assert`).
pub const ASSERT_RULE_IRI: &str = "https://blackcatinformatics.ca/logic/assert";

/// Rule IRI stamped on every in-world foundation rule firing.  The foundation
/// rules carry no `scope.provenance`, so the Python chase stamps them all with
/// `logic:rule/anonymous` (see `_chase_world`).
const ANON_RULE_IRI: &str = "https://blackcatinformatics.ca/logic/rule/anonymous";

/// Rule IRI for the cross-world rigidity closure pass (`logic:rule/cross-world-rigidity`).
const RIGIDITY_RULE_IRI: &str = "https://blackcatinformatics.ca/logic/rule/cross-world-rigidity";

/// Rule IRI for the anti-rigidity witness pass (`logic:rule/anti-rigidity-witness`).
const ANTI_RIGIDITY_RULE_IRI: &str =
    "https://blackcatinformatics.ca/logic/rule/anti-rigidity-witness";

/// The semantic-profile IRI stamped on every emitted quad — the only profile the
/// v1 oracle applies.  Matches `py.rs::ASSERTED_PROFILE` and the Python
/// `_LOGIC_NS + str(SemanticProfileId.POSITIVE_HORN)`.
const PROFILE_IRI: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";

/// Budget status stamped on every quad — this evaluator runs to full fixpoint with
/// no budget ceiling, so every quad is `"ok"` (matching the unbounded oracle path).
const BUDGET_OK: &str = "ok";

/// Rigid sortals (supply / inherit a principle of identity).  Mirrors
/// `logic_foundation._RIGID_SORTALS`; the **primary** rigid-type path.
const RIGID_SORTALS: [&str; 2] = ["Kind", "SubKind"];

/// Anti-rigid sortals (classify instances only contingently).  Mirrors
/// `logic_foundation._ANTI_RIGID_SORTALS`.
const ANTI_RIGID_SORTALS: [&str; 2] = ["Phase", "Role"];

/// Marker the schema may carry to declare a type rigid explicitly (honoured in
/// addition to the stereotype-derived path).  Mirrors
/// `logic_foundation._P_RIGIDLY_APPLIES_TO`.
const RIGIDLY_APPLIES_TO: &str = "https://blackcatinformatics.ca/logic/rigidlyAppliesTo";

// ── Anti-rigidity witness policy ────────────────────────────────────────────────

/// The closed three-valued anti-rigidity witness policy (LOGIC-SEMANTICS.md
/// §Anti-rigidity needs a witness policy).  Mirrors the Python
/// `_ANTI_RIGIDITY_POLICIES` enum.  An unknown value is a HARD FAILURE — these are
/// a closed enum, not feature flags, and there is no silent default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntiRigidityPolicy {
    /// `witness-obligation` (default) — emit one `(x, logic:dischargeObligation, T)`
    /// per anti-rigid instantiation: a standing obligation, NOT a violation.
    WitnessObligation,
    /// `schema-only` — emit nothing at the instance level (the type-level verdicts
    /// are the whole story).
    SchemaOnly,
    /// `witness-required` — strict: emit `(x, logic:witnessRequiredViolation, T)`
    /// unless a materialized counter-world discharges the obligation.
    WitnessRequired,
}

impl AntiRigidityPolicy {
    /// Parse a policy string.  Unknown values are a hard error (no silent default).
    ///
    /// # Errors
    ///
    /// Returns `Err` for any string outside the closed enum.
    // Named `from_str` deliberately (the PyO3 seam + issue #636 spec call it by this
    // name); the fallible `String`-error signature does not match `std::str::FromStr`.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Result<Self, String> {
        match value {
            "witness-obligation" => Ok(Self::WitnessObligation),
            "schema-only" => Ok(Self::SchemaOnly),
            "witness-required" => Ok(Self::WitnessRequired),
            other => Err(format!(
                "Unknown anti_rigidity_policy {other:?}; must be one of \
                 [\"schema-only\", \"witness-obligation\", \"witness-required\"]"
            )),
        }
    }
}

// ── Rule IR (atoms / rules as data) ─────────────────────────────────────────────

/// A body/head term: either a `?var` reference or a constant IRI.
#[derive(Debug, Clone, Copy)]
enum TermPat {
    /// A variable, e.g. `?C`.  The `&str` is the variable name including the `?`.
    Var(&'static str),
    /// A constant IRI (the full IRI string).
    Const(&'static str),
}

/// A single Datalog atom (all terms are IRIs or variables — never literals, which
/// the foundation lowering never emits).
#[derive(Debug, Clone, Copy)]
struct Atom {
    subject: TermPat,
    predicate: TermPat,
    object: TermPat,
    /// `true` iff this is a negation-as-failure body literal.
    negated: bool,
}

/// A Datalog rule: a head atom, an ordered body, and inequality guards.
#[derive(Debug, Clone, Copy)]
struct Rule {
    head: Atom,
    /// The body atoms, in the canonical sorted order the Python `LogicRule`
    /// produces (so first-wins provenance matches by construction).
    body: &'static [Atom],
    /// Inequality guards `(?A, ?B)` — the rule fires only when both bind to
    /// distinct N3 forms.  Mirrors `LogicRule.distinct_pairs`.
    distinct_pairs: &'static [(&'static str, &'static str)],
}

// ── Small term constructors (compile-time) ──────────────────────────────────────

const fn var(name: &'static str) -> TermPat {
    TermPat::Var(name)
}

/// A `logic:`-namespaced constant.  The IRI strings are written out in full so the
/// table is a literal mirror of the Python rule IRIs (no runtime concatenation).
macro_rules! logic_iri {
    ($local:literal) => {
        concat!("https://blackcatinformatics.ca/logic/", $local)
    };
}

const fn pos(subject: TermPat, predicate: TermPat, object: TermPat) -> Atom {
    Atom {
        subject,
        predicate,
        object,
        negated: false,
    }
}

const fn neg(subject: TermPat, predicate: TermPat, object: TermPat) -> Atom {
    Atom {
        subject,
        predicate,
        object,
        negated: true,
    }
}

const NO_GUARD: &[(&str, &str)] = &[];

// ── Rule-table macros ───────────────────────────────────────────────────────────
//
// These expand to `Rule` table entries so the long rule tables below read as close
// to the Python generators as possible.  They must precede their use sites.

/// `hasMetaClass(?C, logic:M) :- ?C rdf:type logic:M`.
macro_rules! meta_rule {
    ($m:literal) => {
        Rule {
            head: pos(
                var("?C"),
                TermPat::Const(logic_iri!("hasMetaClass")),
                TermPat::Const(logic_iri!($m)),
            ),
            body: &[pos(
                var("?C"),
                TermPat::Const(RDF_TYPE),
                TermPat::Const(logic_iri!($m)),
            )],
            distinct_pairs: NO_GUARD,
        }
    };
}

/// `<head>(?C, ?C) :- hasMetaClass(?C, logic:M)` — a per-class stereotype-family marker.
macro_rules! sortal_marker {
    ($head:literal, $m:literal) => {
        Rule {
            head: pos(var("?C"), TermPat::Const(logic_iri!($head)), var("?C")),
            body: &[pos(
                var("?C"),
                TermPat::Const(logic_iri!("hasMetaClass")),
                TermPat::Const(logic_iri!($m)),
            )],
            distinct_pairs: NO_GUARD,
        }
    };
}

/// `<head>(?C, ?C) :- hasMetaClass(?A, logic:M), subClassOfT(?C, ?A)` — an ancestor marker.
macro_rules! ancestor_marker {
    ($head:literal, $m:literal) => {
        Rule {
            head: pos(var("?C"), TermPat::Const(logic_iri!($head)), var("?C")),
            body: &[
                pos(
                    var("?A"),
                    TermPat::Const(logic_iri!("hasMetaClass")),
                    TermPat::Const(logic_iri!($m)),
                ),
                pos(
                    var("?C"),
                    TermPat::Const(logic_iri!("subClassOfT")),
                    var("?A"),
                ),
            ],
            distinct_pairs: NO_GUARD,
        }
    };
}

// ── The stratified foundation program (data) ────────────────────────────────────
//
// These five strata reproduce, exactly, the partition the certifier's `stratify`
// produces for `foundation_rules()` (helpers before NAF-dependent helpers before
// violations), and the rule order WITHIN each stratum is the canonical
// `LogicProgram` sort order (by each rule's `_sort_key`).  The body of every rule
// is likewise in canonical sorted order.  This was captured from the live Python
// oracle (issue #636) and is the parity anchor: chasing these strata in order with
// first-wins dedup yields the same derivation IRIs as the oracle.

// Stratum 0 is empty (no rule's head predicate lands in the lowest SCC layer, which
// holds only the EDB predicates rdf:type/subClassOf/mediates).
const STRATUM_0: &[Rule] = &[];

const STRATUM_1: &[Rule] = &[
    // hasLogicSubclass(?C, ?C) :- subClassOf(?X, ?C)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("hasLogicSubclass")),
            var("?C"),
        ),
        body: &[pos(
            var("?X"),
            TermPat::Const(logic_iri!("subClassOf")),
            var("?C"),
        )],
        distinct_pairs: NO_GUARD,
    },
    // hasMetaClass(?C, logic:M) :- ?C rdf:type logic:M   (one per meta-class,
    // alphabetical by M to match the canonical sort)
    meta_rule!("Category"),
    meta_rule!("Event"),
    meta_rule!("Kind"),
    meta_rule!("Mixin"),
    meta_rule!("Phase"),
    meta_rule!("PhaseMixin"),
    meta_rule!("Relator"),
    meta_rule!("Role"),
    meta_rule!("RoleMixin"),
    meta_rule!("Situation"),
    meta_rule!("SubKind"),
    // hasTwoMediatedRelata(?C, ?C) :- mediates(?C, ?R1), mediates(?C, ?R2), ?R1 != ?R2
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("hasTwoMediatedRelata")),
            var("?C"),
        ),
        body: &[
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("mediates")),
                var("?R1"),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("mediates")),
                var("?R2"),
            ),
        ],
        distinct_pairs: &[("?R1", "?R2")],
    },
    // subClassOfT(?C, ?A) :- subClassOfT(?B, ?A), subClassOf(?C, ?B)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("subClassOfT")),
            var("?A"),
        ),
        body: &[
            pos(
                var("?B"),
                TermPat::Const(logic_iri!("subClassOfT")),
                var("?A"),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("subClassOf")),
                var("?B"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // subClassOfT(?C, ?A) :- subClassOf(?C, ?A)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("subClassOfT")),
            var("?A"),
        ),
        body: &[pos(
            var("?C"),
            TermPat::Const(logic_iri!("subClassOf")),
            var("?A"),
        )],
        distinct_pairs: NO_GUARD,
    },
    // ── Typed/contextual mereology + holon kernel (issue #704, C1) ──────────────────
    // Positive prerequisites: overlap, the supplementation-profile marker, and the
    // unary holon projection.  All depend only on the asserted (EDB) relations
    // logic:properPartOf and logic:underMereologyProfile, so they are inert on inputs
    // that carry neither — the pre-#704 foundation goldens are unaffected.
    //
    // overlaps(?A, ?B) :- properPartOf(?Z, ?A), properPartOf(?Z, ?B)
    Rule {
        head: pos(var("?A"), TermPat::Const(logic_iri!("overlaps")), var("?B")),
        body: &[
            pos(
                var("?Z"),
                TermPat::Const(logic_iri!("properPartOf")),
                var("?A"),
            ),
            pos(
                var("?Z"),
                TermPat::Const(logic_iri!("properPartOf")),
                var("?B"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // overlaps(?A, ?B) :- properPartOf(?A, ?B)   (a proper part overlaps its whole)
    Rule {
        head: pos(var("?A"), TermPat::Const(logic_iri!("overlaps")), var("?B")),
        body: &[pos(
            var("?A"),
            TermPat::Const(logic_iri!("properPartOf")),
            var("?B"),
        )],
        distinct_pairs: NO_GUARD,
    },
    // overlaps(?A, ?B) :- properPartOf(?B, ?A)   (symmetric mirror of the above)
    Rule {
        head: pos(var("?A"), TermPat::Const(logic_iri!("overlaps")), var("?B")),
        body: &[pos(
            var("?B"),
            TermPat::Const(logic_iri!("properPartOf")),
            var("?A"),
        )],
        distinct_pairs: NO_GUARD,
    },
    // supplementationScoped(?X, ?X) :- underMereologyProfile(?X, ?M)
    // Arms the weak-supplementation rule only for wholes declared under a profile;
    // parthood is profiled, not universal (LOGIC-FOUNDATION.md §mereology+holons).
    Rule {
        head: pos(
            var("?X"),
            TermPat::Const(logic_iri!("supplementationScoped")),
            var("?X"),
        ),
        body: &[pos(
            var("?X"),
            TermPat::Const(logic_iri!("underMereologyProfile")),
            var("?M"),
        )],
        distinct_pairs: NO_GUARD,
    },
    // isHolon(?X, ?X) :- properPartOf(?P, ?X), properPartOf(?X, ?W)
    // The lossy unary projection of logic:HolonicPosition: an entity that is both a
    // proper part of some whole and itself has a proper part.  Roots and leaves do not.
    Rule {
        head: pos(var("?X"), TermPat::Const(logic_iri!("isHolon")), var("?X")),
        body: &[
            pos(
                var("?P"),
                TermPat::Const(logic_iri!("properPartOf")),
                var("?X"),
            ),
            pos(
                var("?X"),
                TermPat::Const(logic_iri!("properPartOf")),
                var("?W"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // ── Holonic emergence: aggregate reduction (issue #705, C2) ──────────────────────
    // The positive, derivation-grounded verdict marker.  Under the assessment's declared
    // logic:ReductionTheory, the whole bears a property the theory's logic:reductionBasis
    // carries AND a proper part also bears it, so the property reduces to the parts — a
    // genuine part-reconstruction, not a default.  Inert on inputs with no
    // logic:EmergenceAssessment.  (LOGIC-FOUNDATION.md §mereology+holons.)
    //
    // aggregateAssessed(?A, ?A) :- assessmentWhole(?A, ?W), assessmentProperty(?A, ?Pv),
    //     assessmentReductionTheory(?A, ?T), reductionBasis(?T, ?Pv), bearsProperty(?W, ?Pv),
    //     properPartOf(?Part, ?W), bearsProperty(?Part, ?Pv)
    Rule {
        head: pos(
            var("?A"),
            TermPat::Const(logic_iri!("aggregateAssessed")),
            var("?A"),
        ),
        body: &[
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("assessmentWhole")),
                var("?W"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("assessmentProperty")),
                var("?Pv"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("assessmentReductionTheory")),
                var("?T"),
            ),
            pos(
                var("?T"),
                TermPat::Const(logic_iri!("reductionBasis")),
                var("?Pv"),
            ),
            pos(
                var("?W"),
                TermPat::Const(logic_iri!("bearsProperty")),
                var("?Pv"),
            ),
            pos(
                var("?Part"),
                TermPat::Const(logic_iri!("properPartOf")),
                var("?W"),
            ),
            pos(
                var("?Part"),
                TermPat::Const(logic_iri!("bearsProperty")),
                var("?Pv"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // ── Holonic governance: override marker (issue #706, C3) ─────────────────────────
    // The positive, derivation-grounded face of downward constraint, mirroring the C2
    // aggregate marker.  A logic:DownwardConstraint is OVERRIDDEN when it names an
    // override token (constraintOverride) and the constrained target actually bears
    // that token (bearsProperty) — the join on the SAME ?Ov is what gates this, so only
    // the DECLARED override can fire it, never an unrelated property assertion.  It
    // settles in stratum 1 so the binding rule's NAF over it (stratum 3) is stratified.
    //
    // overriddenConstraint(?C, ?C) :- constraintWhole(?C, ?W), constraintTarget(?C, ?P),
    //     constraintOverride(?C, ?Ov), bearsProperty(?P, ?Ov)
    //
    // The constraintWhole(?C, ?W) pattern binds ?W without reusing it on purpose: it is a
    // well-formedness existence guard kept symmetric with the binding (stratum 3) and unknown
    // (stratum 4) rules, which both require a declared whole.  Without it a whole-less constraint
    // could be Overridden yet never Binding/Unknown, breaking the closed-trichotomy contract —
    // so a malformed constraint correctly receives no verdict.
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("overriddenConstraint")),
            var("?C"),
        ),
        body: &[
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("constraintWhole")),
                var("?W"),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("constraintTarget")),
                var("?P"),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("constraintOverride")),
                var("?Ov"),
            ),
            pos(
                var("?P"),
                TermPat::Const(logic_iri!("bearsProperty")),
                var("?Ov"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // ── Holonic agency: the two co-equal tendency markers (issue #707, C4) ────────────
    // Koestler's Janus-faced holon carries a self-assertive (autonomy-as-a-whole) and an
    // integrative (subordination-as-a-part) tendency.  These are CO-EQUAL vantage facets
    // (Principle 9): the two markers are built by IDENTICAL rules — a holon evidences a
    // tendency when its declared logic:HolonicAgencyProfile carries a basis value (the
    // selfAssertiveBasis / integrativeBasis twin) and the holon bears that value — so
    // neither face is privileged in the vocabulary or the firing order.  Both settle in
    // stratum 1 so the pathology NAF (stratum 3) and the unknown NAF (stratum 4) are
    // stratified.  Inert on inputs with no logic:AgencyAssessment.  Agency is a DECLARED
    // profile a holarchy adopts, not a universal rule (#775).  (LOGIC-FOUNDATION.md
    // §mereology+holons.)
    //
    // selfAssertive(?A, ?A) :- agencyHolon(?A, ?H), agencyProfile(?A, ?Pr),
    //     selfAssertiveBasis(?Pr, ?V), bearsProperty(?H, ?V)
    Rule {
        head: pos(
            var("?A"),
            TermPat::Const(logic_iri!("selfAssertive")),
            var("?A"),
        ),
        body: &[
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("agencyHolon")),
                var("?H"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("agencyProfile")),
                var("?Pr"),
            ),
            pos(
                var("?Pr"),
                TermPat::Const(logic_iri!("selfAssertiveBasis")),
                var("?V"),
            ),
            pos(
                var("?H"),
                TermPat::Const(logic_iri!("bearsProperty")),
                var("?V"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // integrative(?A, ?A) :- agencyHolon(?A, ?H), agencyProfile(?A, ?Pr),
    //     integrativeBasis(?Pr, ?V), bearsProperty(?H, ?V)
    Rule {
        head: pos(
            var("?A"),
            TermPat::Const(logic_iri!("integrative")),
            var("?A"),
        ),
        body: &[
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("agencyHolon")),
                var("?H"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("agencyProfile")),
                var("?Pr"),
            ),
            pos(
                var("?Pr"),
                TermPat::Const(logic_iri!("integrativeBasis")),
                var("?V"),
            ),
            pos(
                var("?H"),
                TermPat::Const(logic_iri!("bearsProperty")),
                var("?V"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // ── Holonic level coherence: position presence marker (issue #708, C5) ───────────
    // A holon's logic:holonicLevel (mereological compositional depth) is READ OFF its
    // logic:HolonicPosition — the canonical relational construct of which logic:Holon
    // and a level are lossy projections (see logic:Holon / logic:holonicLevel defs).
    // The level itself is a literal (xsd:nonNegativeInteger), and the foundation chase
    // is all-IRI (no literal facts), so coherence is keyed on the IRI-valued canonical
    // construct: a holon "is levelled" exactly when it occupies a logic:HolonicPosition,
    // i.e. some position reifier P has logic:positionEntity(P, ?X).  Without a position
    // there is no path along which a depth could be measured, so the level is incoherent.
    //
    // NON-CONFLATION: this marker is fed ONLY by logic:positionEntity (the mereological
    // position axis).  logic:instanceOf / logic:orderedType (the HiLog deep-instantiation
    // order — the type tower) do NOT feed it; an entity high in the instantiation tower
    // but occupying no holonic position still fails the stratum-4 NAF and is charged,
    // because instantiation order is not a mereological level.  Settles in stratum 1 so
    // the stratum-4 NAF is stratified.  (LOGIC-FOUNDATION.md §mereology+holons.)
    //
    // hasHolonicPosition(?X, ?X) :- positionEntity(?P, ?X)
    Rule {
        head: pos(
            var("?X"),
            TermPat::Const(logic_iri!("hasHolonicPosition")),
            var("?X"),
        ),
        body: &[pos(
            var("?P"),
            TermPat::Const(logic_iri!("positionEntity")),
            var("?X"),
        )],
        distinct_pairs: NO_GUARD,
    },
];

const STRATUM_2: &[Rule] = &[
    // antiRigidSortalClass(?C, ?C) :- hasMetaClass(?C, logic:Phase)
    sortal_marker!("antiRigidSortalClass", "Phase"),
    // antiRigidSortalClass(?C, ?C) :- hasMetaClass(?C, logic:Role)
    sortal_marker!("antiRigidSortalClass", "Role"),
    // hasAntiRigidAncestor(?C, ?C) :- hasMetaClass(?A, logic:M), subClassOfT(?C, ?A)
    ancestor_marker!("hasAntiRigidAncestor", "Mixin"),
    ancestor_marker!("hasAntiRigidAncestor", "Phase"),
    ancestor_marker!("hasAntiRigidAncestor", "PhaseMixin"),
    ancestor_marker!("hasAntiRigidAncestor", "Role"),
    ancestor_marker!("hasAntiRigidAncestor", "RoleMixin"),
    // hasRigidAncestor(?C, ?C) :- hasMetaClass(?A, logic:M), subClassOfT(?C, ?A)
    ancestor_marker!("hasRigidAncestor", "Kind"),
    ancestor_marker!("hasRigidAncestor", "SubKind"),
    // hasSomeStereotype(?C, ?C) :- hasMetaClass(?C, ?M)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("hasSomeStereotype")),
            var("?C"),
        ),
        body: &[pos(
            var("?C"),
            TermPat::Const(logic_iri!("hasMetaClass")),
            var("?M"),
        )],
        distinct_pairs: NO_GUARD,
    },
    // hasSortalStereotype(?C, ?C) :- hasMetaClass(?C, logic:M)   (alphabetical M)
    sortal_marker!("hasSortalStereotype", "Kind"),
    sortal_marker!("hasSortalStereotype", "Phase"),
    sortal_marker!("hasSortalStereotype", "Role"),
    sortal_marker!("hasSortalStereotype", "SubKind"),
    // isClass(?C, ?C) :- hasMetaClass(?C, ?M)
    Rule {
        head: pos(var("?C"), TermPat::Const(logic_iri!("isClass")), var("?C")),
        body: &[pos(
            var("?C"),
            TermPat::Const(logic_iri!("hasMetaClass")),
            var("?M"),
        )],
        distinct_pairs: NO_GUARD,
    },
    // isClass(?C, ?C) :- subClassOf(?C, ?X)
    Rule {
        head: pos(var("?C"), TermPat::Const(logic_iri!("isClass")), var("?C")),
        body: &[pos(
            var("?C"),
            TermPat::Const(logic_iri!("subClassOf")),
            var("?X"),
        )],
        distinct_pairs: NO_GUARD,
    },
    // isRelatorClass(?C, ?C) :- hasMetaClass(?A, logic:Relator), subClassOfT(?C, ?A)
    ancestor_marker!("isRelatorClass", "Relator"),
    // isRelatorClass(?C, ?C) :- hasMetaClass(?C, logic:Relator)
    sortal_marker!("isRelatorClass", "Relator"),
    // kindAncestor(?C, ?A) :- hasMetaClass(?A, logic:Kind), subClassOfT(?C, ?A)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("kindAncestor")),
            var("?A"),
        ),
        body: &[
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("hasMetaClass")),
                TermPat::Const(logic_iri!("Kind")),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("subClassOfT")),
                var("?A"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // rigidSortalClass(?C, ?C) :- hasMetaClass(?C, logic:M)   (alphabetical M)
    sortal_marker!("rigidSortalClass", "Kind"),
    sortal_marker!("rigidSortalClass", "SubKind"),
    // isClass(?X, ?X) :- subClassOf(?C, ?X)
    Rule {
        head: pos(var("?X"), TermPat::Const(logic_iri!("isClass")), var("?X")),
        body: &[pos(
            var("?C"),
            TermPat::Const(logic_iri!("subClassOf")),
            var("?X"),
        )],
        distinct_pairs: NO_GUARD,
    },
    // ── Holonic emergence: aggregate verdict projection (issue #705, C2) ─────────────
    // assessmentVerdict(?A, logic:Aggregate) :- aggregateAssessed(?A, ?A)
    Rule {
        head: pos(
            var("?A"),
            TermPat::Const(logic_iri!("assessmentVerdict")),
            TermPat::Const(logic_iri!("Aggregate")),
        ),
        body: &[pos(
            var("?A"),
            TermPat::Const(logic_iri!("aggregateAssessed")),
            var("?A"),
        )],
        distinct_pairs: NO_GUARD,
    },
    // ── Holonic governance: overridden verdict projection (issue #706, C3) ───────────
    // constraintVerdict(?C, logic:ConstraintOverridden) :- overriddenConstraint(?C, ?C)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("constraintVerdict")),
            TermPat::Const(logic_iri!("ConstraintOverridden")),
        ),
        body: &[pos(
            var("?C"),
            TermPat::Const(logic_iri!("overriddenConstraint")),
            var("?C"),
        )],
        distinct_pairs: NO_GUARD,
    },
    // ── Holonic agency: integral verdict projection (issue #707, C4) ─────────────────
    // The positive, derivation-grounded verdict, mirroring the C2 Aggregate and C3
    // Overridden projections: a holon is INTEGRAL when BOTH co-equal tendency markers
    // hold — it asserts itself as a whole AND subordinates itself as a part.  The
    // agencyHolon/agencyProfile atoms re-bind the well-formedness existence guard kept
    // symmetric across all four verdict rules, so a malformed assessment (no holon or no
    // profile) provably receives no verdict.  Both markers settle in stratum 1, so this
    // pure-positive rule sits correctly in stratum 2.
    //
    // agencyVerdict(?A, logic:HolonIntegral) :- agencyHolon(?A, ?H), agencyProfile(?A, ?Pr),
    //     selfAssertive(?A, ?A), integrative(?A, ?A)
    Rule {
        head: pos(
            var("?A"),
            TermPat::Const(logic_iri!("agencyVerdict")),
            TermPat::Const(logic_iri!("HolonIntegral")),
        ),
        body: &[
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("agencyHolon")),
                var("?H"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("agencyProfile")),
                var("?Pr"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("selfAssertive")),
                var("?A"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("integrative")),
                var("?A"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
];

const STRATUM_3: &[Rule] = &[
    // concreteRelator(?C, ?C) :- NOT hasLogicSubclass(?C, ?C), isRelatorClass(?C, ?C)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("concreteRelator")),
            var("?C"),
        ),
        body: &[
            neg(
                var("?C"),
                TermPat::Const(logic_iri!("hasLogicSubclass")),
                var("?C"),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("isRelatorClass")),
                var("?C"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // hasKindAncestor(?C, ?C) :- kindAncestor(?C, ?A)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("hasKindAncestor")),
            var("?C"),
        ),
        body: &[pos(
            var("?C"),
            TermPat::Const(logic_iri!("kindAncestor")),
            var("?A"),
        )],
        distinct_pairs: NO_GUARD,
    },
    // isNonKindSortal(?C, ?C) :- NOT hasMetaClass(?C, logic:Kind), hasSortalStereotype(?C, ?C)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("isNonKindSortal")),
            var("?C"),
        ),
        body: &[
            neg(
                var("?C"),
                TermPat::Const(logic_iri!("hasMetaClass")),
                TermPat::Const(logic_iri!("Kind")),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("hasSortalStereotype")),
                var("?C"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // ── Mereology NAF helpers (issue #704, C1) ──────────────────────────────────────
    // Disjointness is the negation of overlap, scoped to co-parts of a common whole so
    // the NAF body is range-restricted (overlaps must be settled in a lower stratum).
    //
    // disjoint(?P, ?P2) :- properPartOf(?P, ?X), properPartOf(?P2, ?X), NOT overlaps(?P, ?P2)
    Rule {
        head: pos(
            var("?P"),
            TermPat::Const(logic_iri!("disjoint")),
            var("?P2"),
        ),
        body: &[
            neg(
                var("?P"),
                TermPat::Const(logic_iri!("overlaps")),
                var("?P2"),
            ),
            pos(
                var("?P"),
                TermPat::Const(logic_iri!("properPartOf")),
                var("?X"),
            ),
            pos(
                var("?P2"),
                TermPat::Const(logic_iri!("properPartOf")),
                var("?X"),
            ),
        ],
        distinct_pairs: &[("?P", "?P2")],
    },
    // hasDisjointCopart(?X, ?P) :- properPartOf(?P, ?X), properPartOf(?P2, ?X), NOT overlaps(?P, ?P2)
    Rule {
        head: pos(
            var("?X"),
            TermPat::Const(logic_iri!("hasDisjointCopart")),
            var("?P"),
        ),
        body: &[
            neg(
                var("?P"),
                TermPat::Const(logic_iri!("overlaps")),
                var("?P2"),
            ),
            pos(
                var("?P"),
                TermPat::Const(logic_iri!("properPartOf")),
                var("?X"),
            ),
            pos(
                var("?P2"),
                TermPat::Const(logic_iri!("properPartOf")),
                var("?X"),
            ),
        ],
        distinct_pairs: &[("?P", "?P2")],
    },
    // ── Holonic emergence: emergent verdict (issue #705, C2) ─────────────────────────
    // Emergent by negation-as-failure over the aggregate reduction, WHILE the assessment
    // still binds a declared logic:ReductionTheory (?T) — so the verdict is theory-relative,
    // never a bare "unflagged" default, and failure-to-derive is not irreducibility.
    // aggregateAssessed settles in stratum 1, so the NAF is stratified; ?A/?T/?Pv/?W are all
    // positively bound, so the rule is DL-safe.
    //
    // emergentAssessed(?A, ?A) :- assessmentWhole(?A, ?W), assessmentProperty(?A, ?Pv),
    //     assessmentReductionTheory(?A, ?T), bearsProperty(?W, ?Pv), NOT aggregateAssessed(?A, ?A)
    Rule {
        head: pos(
            var("?A"),
            TermPat::Const(logic_iri!("emergentAssessed")),
            var("?A"),
        ),
        body: &[
            neg(
                var("?A"),
                TermPat::Const(logic_iri!("aggregateAssessed")),
                var("?A"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("assessmentWhole")),
                var("?W"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("assessmentProperty")),
                var("?Pv"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("assessmentReductionTheory")),
                var("?T"),
            ),
            pos(
                var("?W"),
                TermPat::Const(logic_iri!("bearsProperty")),
                var("?Pv"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // assessmentVerdict(?A, logic:Emergent) :- emergentAssessed(?A, ?A)
    Rule {
        head: pos(
            var("?A"),
            TermPat::Const(logic_iri!("assessmentVerdict")),
            TermPat::Const(logic_iri!("Emergent")),
        ),
        body: &[pos(
            var("?A"),
            TermPat::Const(logic_iri!("emergentAssessed")),
            var("?A"),
        )],
        distinct_pairs: NO_GUARD,
    },
    // ── Holonic governance: binding marker (issue #706, C3) ──────────────────────────
    // A downward constraint BINDS its named target by negation-as-failure over the
    // override derivation, WHILE the constraint still binds a declared logic:GovernanceRegime
    // (?R) whose activationBasis carries the constrained state (?S) — so the verdict is
    // regime-relative, never a bare "unconstrained" default.  NON-TRANSITIVE by default
    // (#775): the constraint is read only for the explicitly named ?P (a proper part of
    // ?W); there is no rule cascading it to ?P's own sub-parts.  overriddenConstraint
    // settles in stratum 1, so the NAF is stratified; ?C/?W/?P/?S/?R are all positively
    // bound, so the rule is DL-safe.
    //
    // bindingConstraint(?C, ?C) :- constraintWhole(?C, ?W), constraintTarget(?C, ?P),
    //     constraintState(?C, ?S), constraintRegime(?C, ?R), properPartOf(?P, ?W),
    //     activationBasis(?R, ?S), NOT overriddenConstraint(?C, ?C)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("bindingConstraint")),
            var("?C"),
        ),
        body: &[
            neg(
                var("?C"),
                TermPat::Const(logic_iri!("overriddenConstraint")),
                var("?C"),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("constraintWhole")),
                var("?W"),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("constraintTarget")),
                var("?P"),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("constraintState")),
                var("?S"),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("constraintRegime")),
                var("?R"),
            ),
            pos(
                var("?P"),
                TermPat::Const(logic_iri!("properPartOf")),
                var("?W"),
            ),
            pos(
                var("?R"),
                TermPat::Const(logic_iri!("activationBasis")),
                var("?S"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // constraintVerdict(?C, logic:ConstraintBinding) :- bindingConstraint(?C, ?C)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("constraintVerdict")),
            TermPat::Const(logic_iri!("ConstraintBinding")),
        ),
        body: &[pos(
            var("?C"),
            TermPat::Const(logic_iri!("bindingConstraint")),
            var("?C"),
        )],
        distinct_pairs: NO_GUARD,
    },
    // ── Holonic agency: the two co-equal pathology verdicts (issue #707, C4) ──────────
    // Koestler's two pathologies, each the collapse of ONE tendency, reached by
    // negation-as-failure over the corresponding stratum-1 marker WHILE the opposite
    // marker still holds — so each verdict is profile-relative, never a bare default.
    // The two rules are MIRROR IMAGES settling in the same stratum, so the duality is
    // genuinely co-equal rather than one tendency defaulting to the other.  selfAssertive
    // and integrative settle in stratum 1, so the NAF is stratified; ?A/?H/?Pr are all
    // positively bound, so each rule is DL-safe.  Every verdict rule re-binds the
    // agencyHolon/agencyProfile well-formedness guard.
    //
    // agencyVerdict(?A, logic:AutonomyDeficient) :- agencyHolon(?A, ?H), agencyProfile(?A, ?Pr),
    //     integrative(?A, ?A), NOT selfAssertive(?A, ?A)
    // The first pathology — a "part" with no autonomy: it integrates but does not assert itself.
    Rule {
        head: pos(
            var("?A"),
            TermPat::Const(logic_iri!("agencyVerdict")),
            TermPat::Const(logic_iri!("AutonomyDeficient")),
        ),
        body: &[
            neg(
                var("?A"),
                TermPat::Const(logic_iri!("selfAssertive")),
                var("?A"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("agencyHolon")),
                var("?H"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("agencyProfile")),
                var("?Pr"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("integrative")),
                var("?A"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // agencyVerdict(?A, logic:IntegrationDeficient) :- agencyHolon(?A, ?H), agencyProfile(?A, ?Pr),
    //     selfAssertive(?A, ?A), NOT integrative(?A, ?A)
    // The second pathology — a "whole" refusing to integrate: it asserts itself but does not subordinate.
    Rule {
        head: pos(
            var("?A"),
            TermPat::Const(logic_iri!("agencyVerdict")),
            TermPat::Const(logic_iri!("IntegrationDeficient")),
        ),
        body: &[
            neg(
                var("?A"),
                TermPat::Const(logic_iri!("integrative")),
                var("?A"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("agencyHolon")),
                var("?H"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("agencyProfile")),
                var("?Pr"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("selfAssertive")),
                var("?A"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
];

const STRATUM_4: &[Rule] = &[
    // violation(?C, FreeRole) :- antiRigidSortalClass(?C, ?C), NOT hasRigidAncestor(?C, ?C)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("violation")),
            TermPat::Const(logic_iri!("FreeRole")),
        ),
        body: &[
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("antiRigidSortalClass")),
                var("?C"),
            ),
            neg(
                var("?C"),
                TermPat::Const(logic_iri!("hasRigidAncestor")),
                var("?C"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // violation(?C, MixIden) :- NOT hasKindAncestor(?C, ?C), isNonKindSortal(?C, ?C)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("violation")),
            TermPat::Const(logic_iri!("MixIden")),
        ),
        body: &[
            neg(
                var("?C"),
                TermPat::Const(logic_iri!("hasKindAncestor")),
                var("?C"),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("isNonKindSortal")),
                var("?C"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // violation(?C, MixIden) :- hasMetaClass(?C, logic:Kind), kindAncestor(?C, ?A)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("violation")),
            TermPat::Const(logic_iri!("MixIden")),
        ),
        body: &[
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("hasMetaClass")),
                TermPat::Const(logic_iri!("Kind")),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("kindAncestor")),
                var("?A"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // violation(?C, MixIden) :- isNonKindSortal(?C, ?C), kindAncestor(?C, ?A1),
    //                           kindAncestor(?C, ?A2), ?A1 != ?A2
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("violation")),
            TermPat::Const(logic_iri!("MixIden")),
        ),
        body: &[
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("isNonKindSortal")),
                var("?C"),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("kindAncestor")),
                var("?A1"),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("kindAncestor")),
                var("?A2"),
            ),
        ],
        distinct_pairs: &[("?A1", "?A2")],
    },
    // violation(?C, MixRig) :- hasAntiRigidAncestor(?C, ?C), rigidSortalClass(?C, ?C)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("violation")),
            TermPat::Const(logic_iri!("MixRig")),
        ),
        body: &[
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("hasAntiRigidAncestor")),
                var("?C"),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("rigidSortalClass")),
                var("?C"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // violation(?C, RelComp) :- concreteRelator(?C, ?C), NOT hasTwoMediatedRelata(?C, ?C)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("violation")),
            TermPat::Const(logic_iri!("RelComp")),
        ),
        body: &[
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("concreteRelator")),
                var("?C"),
            ),
            neg(
                var("?C"),
                TermPat::Const(logic_iri!("hasTwoMediatedRelata")),
                var("?C"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // violation(?C, StereotypeCardinality) :- hasMetaClass(?C, ?M1),
    //                                         hasMetaClass(?C, ?M2), ?M1 != ?M2
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("violation")),
            TermPat::Const(logic_iri!("StereotypeCardinality")),
        ),
        body: &[
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("hasMetaClass")),
                var("?M1"),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("hasMetaClass")),
                var("?M2"),
            ),
        ],
        distinct_pairs: &[("?M1", "?M2")],
    },
    // violation(?C, StereotypeCardinality) :- NOT hasSomeStereotype(?C, ?C), isClass(?C, ?C)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("violation")),
            TermPat::Const(logic_iri!("StereotypeCardinality")),
        ),
        body: &[
            neg(
                var("?C"),
                TermPat::Const(logic_iri!("hasSomeStereotype")),
                var("?C"),
            ),
            pos(var("?C"), TermPat::Const(logic_iri!("isClass")), var("?C")),
        ],
        distinct_pairs: NO_GUARD,
    },
    // ── Weak supplementation (issue #704, C1) ───────────────────────────────────────
    // A profile-scoped MereologyConstraint (NOT an OntoUML Discipline): a whole with a
    // proper part must have another proper part disjoint from the first.  Fires only
    // for wholes armed by supplementationScoped (declared under a logic:MereologyProfile).
    //
    // violation(?X, WeakSupplementation) :- properPartOf(?P, ?X),
    //     NOT hasDisjointCopart(?X, ?P), supplementationScoped(?X, ?X)
    Rule {
        head: pos(
            var("?X"),
            TermPat::Const(logic_iri!("violation")),
            TermPat::Const(logic_iri!("WeakSupplementation")),
        ),
        body: &[
            pos(
                var("?P"),
                TermPat::Const(logic_iri!("properPartOf")),
                var("?X"),
            ),
            neg(
                var("?X"),
                TermPat::Const(logic_iri!("hasDisjointCopart")),
                var("?P"),
            ),
            pos(
                var("?X"),
                TermPat::Const(logic_iri!("supplementationScoped")),
                var("?X"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // ── Holonic level coherence: incoherence violation (issue #708, C5) ─────────────────
    // PROFILE-SCOPED, exactly like weak supplementation (and per #775 profile-relativity):
    // a holon (isHolon — both a proper part of some whole AND itself has a proper part) is
    // charged with this coherence violation ONLY when it is declared under a mereology
    // profile (underMereologyProfile) yet occupies NO logic:HolonicPosition.  A holon
    // outside any logic:MereologyProfile is NEVER charged — parthood is profiled, not
    // universal, and a holonic level is path-relative (a min/max band), optional outside a
    // profile.  logic:holonicLevel is a literal read off the holon's logic:HolonicPosition;
    // the foundation chase is all-IRI, so coherence is keyed on the IRI-valued canonical
    // construct (hasHolonicPosition) rather than the literal level: a profiled holon with no
    // position has no path along which a depth could be measured, so its level is incoherent.
    // The NAF target hasHolonicPosition settles in stratum 1 (armed by logic:positionEntity),
    // isHolon also settles in stratum 1, and underMereologyProfile is an asserted EDB
    // relation, so the negation is stratified — this mirrors the weak-supplementation
    // violation exactly.  ?X is positively bound by isHolon(?X, ?X) and
    // underMereologyProfile(?X, ?M), so the rule is DL-safe.
    //
    // CRITICAL NON-CONFLATION: logic:instanceOf / logic:orderedType (HiLog deep-instantiation
    // order — the type tower) do NOT feed hasHolonicPosition — the two axes are orthogonal.
    // A profiled holon high in the instantiation tower but occupying no holonic position
    // still fires this violation, because mereological compositional depth (read off a
    // logic:HolonicPosition in a whole/part DAG) and instantiation-tower order must not be
    // conflated.  (LOGIC-FOUNDATION.md §mereology+holons.)
    //
    // violation(?X, logic:HolonicLevelIncoherence) :- isHolon(?X, ?X),
    //     underMereologyProfile(?X, ?M), NOT hasHolonicPosition(?X, ?X)
    Rule {
        head: pos(
            var("?X"),
            TermPat::Const(logic_iri!("violation")),
            TermPat::Const(logic_iri!("HolonicLevelIncoherence")),
        ),
        body: &[
            pos(var("?X"), TermPat::Const(logic_iri!("isHolon")), var("?X")),
            pos(
                var("?X"),
                TermPat::Const(logic_iri!("underMereologyProfile")),
                var("?M"),
            ),
            neg(
                var("?X"),
                TermPat::Const(logic_iri!("hasHolonicPosition")),
                var("?X"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // ── Holonic emergence: unknown verdict (issue #705, C2) ──────────────────────────
    // ME9's first-class third value: the whole bears the property under assessment, but
    // neither an aggregate reduction nor a theory-relative emergence verdict is derivable
    // (the assessment declares no logic:assessmentReductionTheory, so emergentAssessed
    // cannot fire either), so the reducibility question cannot be posed.  Both NAF targets
    // (aggregateAssessed S1, emergentAssessed S3) are settled below stratum 4, so the
    // negation is stratified; ?A/?W/?Pv are positively bound, so the rule is DL-safe.
    //
    // assessmentVerdict(?A, logic:EmergenceUnknown) :- assessmentWhole(?A, ?W),
    //     assessmentProperty(?A, ?Pv), bearsProperty(?W, ?Pv),
    //     NOT aggregateAssessed(?A, ?A), NOT emergentAssessed(?A, ?A)
    Rule {
        head: pos(
            var("?A"),
            TermPat::Const(logic_iri!("assessmentVerdict")),
            TermPat::Const(logic_iri!("EmergenceUnknown")),
        ),
        body: &[
            neg(
                var("?A"),
                TermPat::Const(logic_iri!("aggregateAssessed")),
                var("?A"),
            ),
            neg(
                var("?A"),
                TermPat::Const(logic_iri!("emergentAssessed")),
                var("?A"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("assessmentWhole")),
                var("?W"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("assessmentProperty")),
                var("?Pv"),
            ),
            pos(
                var("?W"),
                TermPat::Const(logic_iri!("bearsProperty")),
                var("?Pv"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // ── Holonic governance: unknown verdict (issue #706, C3) ─────────────────────────
    // The first-class third value: the constraint names a target that is a proper part
    // of the governing whole, but neither an override-defeat nor a regime-relative
    // binding is derivable (no constraintRegime activates the state), so the binding
    // question cannot be posed — an un-activated constraint is never silently read as
    // binding.  Both NAF targets (overriddenConstraint S1, bindingConstraint S3) are
    // settled below stratum 4, so the negation is stratified; ?C/?W/?P are positively
    // bound, so the rule is DL-safe.
    //
    // constraintVerdict(?C, logic:ConstraintUnknown) :- constraintWhole(?C, ?W),
    //     constraintTarget(?C, ?P), properPartOf(?P, ?W),
    //     NOT overriddenConstraint(?C, ?C), NOT bindingConstraint(?C, ?C)
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("constraintVerdict")),
            TermPat::Const(logic_iri!("ConstraintUnknown")),
        ),
        body: &[
            neg(
                var("?C"),
                TermPat::Const(logic_iri!("overriddenConstraint")),
                var("?C"),
            ),
            neg(
                var("?C"),
                TermPat::Const(logic_iri!("bindingConstraint")),
                var("?C"),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("constraintWhole")),
                var("?W"),
            ),
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("constraintTarget")),
                var("?P"),
            ),
            pos(
                var("?P"),
                TermPat::Const(logic_iri!("properPartOf")),
                var("?W"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // ── Holonic agency: unknown verdict (issue #707, C4) ─────────────────────────────
    // The first-class fourth value: the assessment names a holon and a profile, but the
    // holon evidences NEITHER tendency — neither the self-assertive nor the integrative
    // marker can fire — so the integrity question has no positive footing.  This subsumes
    // the "cannot pose the question" case: it fires both when the holon bears no basis
    // value and when the profile declares no basis at all (no marker can derive), so a
    // basis-free profile is unknown, not deficient.  Both NAF targets (selfAssertive,
    // integrative) settle in stratum 1, below stratum 4, so the negation is stratified;
    // ?A/?H/?Pr are positively bound, so the rule is DL-safe.  agencyHolon/agencyProfile
    // are the well-formedness existence guard symmetric with the other three verdict rules.
    //
    // agencyVerdict(?A, logic:AgencyUnknown) :- agencyHolon(?A, ?H), agencyProfile(?A, ?Pr),
    //     NOT selfAssertive(?A, ?A), NOT integrative(?A, ?A)
    Rule {
        head: pos(
            var("?A"),
            TermPat::Const(logic_iri!("agencyVerdict")),
            TermPat::Const(logic_iri!("AgencyUnknown")),
        ),
        body: &[
            neg(
                var("?A"),
                TermPat::Const(logic_iri!("selfAssertive")),
                var("?A"),
            ),
            neg(
                var("?A"),
                TermPat::Const(logic_iri!("integrative")),
                var("?A"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("agencyHolon")),
                var("?H"),
            ),
            pos(
                var("?A"),
                TermPat::Const(logic_iri!("agencyProfile")),
                var("?Pr"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
];

/// The five strata, low-to-high.  Each is chased to fixpoint before the next so a
/// negated atom is only checked once the predicate it negates has settled.
const STRATA: [&[Rule]; 5] = [STRATUM_0, STRATUM_1, STRATUM_2, STRATUM_3, STRATUM_4];

// ── Output quad type ────────────────────────────────────────────────────────────

/// A materialized quad with the full seam provenance contract.
///
/// `object` is in canonical N3 form (`<iri>` for an IRI) — matching the Python
/// `DerivedQuad.obj` and the materializer's object canonicalisation, so reifier and
/// derivation IRIs are byte-identical.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundationQuad {
    /// The world IRI (named-graph component).
    pub graph: String,
    /// The subject IRI.
    pub subject: String,
    /// The predicate IRI.
    pub predicate: String,
    /// The object term in N3 form (`<iri>`).
    pub object: String,
    /// The firing rule IRI (`logic:assert`, `logic:rule/anonymous`, or a pass IRI).
    pub rule_iri: String,
    /// The reifier IRIs of the antecedent quads consumed by the firing.
    pub source_quad_ids: Vec<String>,
    /// The content-addressed derivation IRI.
    pub derivation_id: String,
}

// ── Internal fact representation ─────────────────────────────────────────────────
//
// A fact is a fully-ground `(subject_iri, predicate_iri, object_iri)`.  Every
// foundation term is an IRI, so we store bare IRI strings; N3 (`<iri>`) is computed
// on demand only where a serialized form is needed.  The dedup key is the triple of
// bare IRIs, matching the Python `fact_index` key under the same first-wins order.

/// A ground fact (all IRIs).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Fact {
    subject: String,
    predicate: String,
    object: String,
}

impl Fact {
    /// The dedup key `(s, p, o)` of bare IRIs — mirrors the Python `fact_index` key.
    /// The angle-bracket (N3) form is not needed: the key is purely internal
    /// (membership dedup + provenance source recovery), and the wrap/strip round
    /// trip only added allocations, so the bare IRIs are stored directly.
    fn key(&self) -> (String, String, String) {
        (
            self.subject.clone(),
            self.predicate.clone(),
            self.object.clone(),
        )
    }
}

/// N3 form of an IRI: `<iri>`.
fn n3(iri: &str) -> String {
    format!("<{iri}>")
}

/// Reifier IRI for an all-IRI fact, via the golden-pinned recipe.
fn fact_reifier(fact: &Fact) -> Result<String, String> {
    let s = Term::NamedNode(
        NamedNode::new(&fact.subject).map_err(|e| format!("invalid subject IRI: {e}"))?,
    );
    let p = NamedNode::new(&fact.predicate).map_err(|e| format!("invalid predicate IRI: {e}"))?;
    let o = Term::NamedNode(
        NamedNode::new(&fact.object).map_err(|e| format!("invalid object IRI: {e}"))?,
    );
    mint_reifier(&s, &p, &o)
}

/// The content-addressed reifier IRI of a materialized [`FoundationQuad`].
///
/// `subject`/`predicate` are bare IRIs and `object` is already in canonical N3 form
/// (`<iri>`), so this delegates to the byte-identical
/// [`crate::provenance::reifier_from_strings`] recipe (the same one the explanation
/// engine and `mint_reifier` agree on). This is the fact's persistent, runtime-id
/// independent identity — the key under which the derivation graph records it.
///
/// # Errors
///
/// Never fails today (the inputs are validated IRIs + an N3 object), but returns a
/// `Result` to keep the call site uniform with the other provenance helpers.
pub fn quad_reifier(quad: &FoundationQuad) -> Result<String, String> {
    Ok(crate::provenance::reifier_from_strings(
        &quad.subject,
        &quad.predicate,
        &quad.object,
    ))
}

/// Reifier IRI for an explicit `(s, p, o)` IRI triple — used by the cross-world passes.
fn triple_reifier(s: &str, p: &str, o: &str) -> Result<String, String> {
    let sn = Term::NamedNode(NamedNode::new(s).map_err(|e| format!("invalid subject IRI: {e}"))?);
    let pn = NamedNode::new(p).map_err(|e| format!("invalid predicate IRI: {e}"))?;
    let on = Term::NamedNode(NamedNode::new(o).map_err(|e| format!("invalid object IRI: {e}"))?);
    mint_reifier(&sn, &pn, &on)
}

// ── Per-world fact store (insertion-ordered, first-wins) ────────────────────────

/// Insertion-ordered fact store with O(1) dedup, mirroring the Python `fact_index`.
///
/// The Python `fact_index` is a `dict` keyed by the IRI triple, iterated in insertion
/// order during the join (so binding enumeration order is deterministic and
/// first-wins).  This wrapper keeps the same two invariants: a `Vec<Fact>` carries
/// the iteration order, and a `HashSet` of keys provides the membership test.
struct FactStore {
    facts: Vec<Fact>,
    keys: HashSet<(String, String, String)>,
    /// Predicate → row indices into `facts`, in insertion order.  Maintained in
    /// lockstep with `facts` so each bucket's order equals insertion order; this
    /// lets the join scan only the rows for a constant-predicate atom while
    /// returning exactly the subsequence (same relative order) a full scan would.
    predicate_index: HashMap<String, Vec<usize>>,
}

impl FactStore {
    fn new() -> Self {
        Self {
            facts: Vec::new(),
            keys: HashSet::new(),
            predicate_index: HashMap::new(),
        }
    }

    /// Insert `fact` if its key is new; return `true` if it was inserted.
    fn insert(&mut self, fact: Fact) -> bool {
        let key = fact.key();
        if self.keys.contains(&key) {
            return false;
        }
        self.keys.insert(key);
        let idx = self.facts.len();
        self.facts.push(fact);
        // Push the new row index in lockstep with `facts`, preserving insertion
        // order within the predicate bucket. Clone the predicate only on first
        // occurrence to avoid a heap allocation for repeat predicates.
        let pred = &self.facts[idx].predicate;
        if let Some(bucket) = self.predicate_index.get_mut(pred.as_str()) {
            bucket.push(idx);
        } else {
            self.predicate_index.insert(pred.clone(), vec![idx]);
        }
        true
    }

    /// Whether a fact with this key exists.
    fn contains_key(&self, key: &(String, String, String)) -> bool {
        self.keys.contains(key)
    }

    /// Row indices (into `facts`, insertion-ordered) of facts with predicate
    /// `pred`; empty slice if none.
    fn facts_for_predicate(&self, pred: &str) -> &[usize] {
        self.predicate_index
            .get(pred)
            .map_or(&[][..], Vec::as_slice)
    }
}

// ── Body join (semi-naive, NAF, inequality guards) ──────────────────────────────

/// A candidate solution: the variable→IRI bindings plus the N3 keys of the matched
/// positive body facts (the provenance sources).
#[derive(Clone)]
struct Solution {
    bindings: Vec<(&'static str, String)>,
    source_keys: Vec<(String, String, String)>,
}

impl Solution {
    fn get(&self, var_name: &str) -> Option<&str> {
        self.bindings
            .iter()
            .find(|(k, _)| *k == var_name)
            .map(|(_, v)| v.as_str())
    }
}

/// A candidate derivation within a single chase round.
///
/// `sorted_sources` is a sorted copy of `sources` used ONLY for the deterministic
/// tiebreak comparison.  The emitted [`FoundationQuad`] always uses `sources` in its
/// original body-order for `source_quad_ids`; the sorted copy never appears in output.
///
/// Winner selection uses a **quality-ordered total-order** over same-head candidates:
/// `(max_src_depth, sum_src_depth, sorted_sources)` — smaller wins.  This prefers the
/// most-direct (shallowest) derivation, tiebreaks toward asserted-rooted proofs (lower
/// depth sum), and uses lex-min sorted reifiers as the final content-addressed
/// tiebreaker, making the winner fully independent of firing-enumeration order.
#[derive(Clone)]
struct RoundCandidate {
    head: Fact,
    /// Reifiers of matched body facts, in body (scan) order — goes into `source_quad_ids`.
    sources: Vec<String>,
    /// Sorted copy of `sources`, used only for deterministic winner comparison.
    sorted_sources: Vec<String>,
    /// Content-addressed derivation IRI: `mint_derivation_id(ANON_RULE_IRI, &src_refs)`.
    deriv: String,
    /// Maximum derivation depth across the matched source facts.  Depth 0 = asserted.
    /// `depth = 1 + max(depth[source])` for this candidate; minimised first by the tiebreak.
    max_src_depth: u32,
    /// Sum of derivation depths across the matched source facts.  Smaller = closer to
    /// asserted axioms (tiebreak level 2, after `max_src_depth`).
    sum_src_depth: u64,
}

/// Ground a term pattern under bindings to its IRI string, or `None` if an unbound var.
fn ground(term: &TermPat, sol: &Solution) -> Option<String> {
    match term {
        TermPat::Const(iri) => Some((*iri).to_owned()),
        TermPat::Var(name) => sol.get(name).map(str::to_owned),
    }
}

/// Try to match `atom` against fact `f`, extending `base` bindings; return the merged
/// solution or `None`.  Mirrors `_match_atom` + `_merge_bindings`: a repeated variable
/// must agree, a constant must equal the fact term exactly.
fn match_atom(atom: &Atom, f: &Fact, base: &Solution) -> Option<Solution> {
    // Defer cloning `base` until the atom actually matches.  This runs once per
    // candidate fact in the join loop, so cloning up front allocates a fresh
    // Solution for every *non-matching* fact (the common case).  Validate against
    // the existing bindings first, accumulating any new ones, and materialize the
    // merged Solution only on a confirmed match.
    let mut new_bindings: Vec<(&'static str, String)> = Vec::new();
    for (pat, fact_term) in [
        (&atom.subject, &f.subject),
        (&atom.predicate, &f.predicate),
        (&atom.object, &f.object),
    ] {
        match pat {
            TermPat::Const(iri) => {
                if iri != fact_term {
                    return None;
                }
            }
            TermPat::Var(name) => {
                // A repeated variable must agree with a value already bound in
                // `base` or earlier in this same atom.
                let existing = base.get(name).or_else(|| {
                    new_bindings
                        .iter()
                        .find(|(k, _)| *k == *name)
                        .map(|(_, v)| v.as_str())
                });
                match existing {
                    Some(existing) => {
                        if existing != fact_term {
                            return None;
                        }
                    }
                    None => new_bindings.push((name, fact_term.clone())),
                }
            }
        }
    }
    let mut sol = base.clone();
    sol.bindings.extend(new_bindings);
    Some(sol)
}

/// Whether a negated atom has at least one match in the fact store under `sol`.
///
/// Mirrors `_atom_is_satisfied`: NAF is satisfied (the rule fires) iff this returns
/// `false`.  Checked after the negated atom's stratum is settled (stratified NAF).
fn negated_atom_satisfied(atom: &Atom, sol: &Solution, store: &FactStore) -> bool {
    // Fully-ground fast path: when every term grounds to a bound var or constant
    // IRI, an O(1) key-membership test suffices (the foundation lowering's negated
    // atoms are always DL-safe and all-IRI, so this path always applies here).
    let s = ground(&atom.subject, sol);
    let p = ground(&atom.predicate, sol);
    let o = ground(&atom.object, sol);
    if let (Some(s), Some(p), Some(o)) = (&s, &p, &o) {
        return store.contains_key(&(s.clone(), p.clone(), o.clone()));
    }
    // Partially-bound fallback: scan (defensive; not exercised by the foundation rules).
    for f in &store.facts {
        if match_partial(atom, f, sol) {
            return true;
        }
    }
    false
}

/// Partial match used only by the NAF scan fallback.
fn match_partial(atom: &Atom, f: &Fact, sol: &Solution) -> bool {
    for (pat, fact_term) in [
        (&atom.subject, &f.subject),
        (&atom.predicate, &f.predicate),
        (&atom.object, &f.object),
    ] {
        match pat {
            TermPat::Const(iri) => {
                if iri != fact_term {
                    return false;
                }
            }
            TermPat::Var(name) => {
                if let Some(existing) = sol.get(name) {
                    if existing != fact_term {
                        return false;
                    }
                }
            }
        }
    }
    true
}

/// Whether every inequality guard holds for `sol` (N3-form inequality).
///
/// Mirrors `_bindings_satisfy_distinct`: both guard variables MUST be bound by the
/// positive body — an unbound guard variable is a malformed rule (hard error).
fn distinct_pairs_satisfied(
    distinct_pairs: &[(&str, &str)],
    sol: &Solution,
) -> Result<bool, String> {
    for (a, b) in distinct_pairs {
        let va = sol.get(a).ok_or_else(|| {
            format!("Inequality guard variable {a:?} is unbound after body matching")
        })?;
        let vb = sol.get(b).ok_or_else(|| {
            format!("Inequality guard variable {b:?} is unbound after body matching")
        })?;
        if n3(va) == n3(vb) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether a positive atom's binding to fact `f` is restricted to a delta scan.
///
/// In a delta-position scan we walk only the rows in the predicate bucket whose key
/// is in `delta`; in a full-store scan we walk the whole bucket.  Both walk the
/// insertion-ordered bucket so the matched subsequence (and thus `source_keys`
/// order) is identical to a full scan filtered after the fact.
enum Scan {
    /// Bind `a_p` to facts whose key is **in** `delta` (the "new at p" position).
    Delta,
    /// Bind to **any** fact in the full store (no delta constraint).
    Full,
    /// Bind only to facts whose key is **not** in `delta` (the "old after p"
    /// positions, j > p, that keep each delta-touching solution produced once).
    OldOnly,
}

/// Extend each partial solution by matching `atom` against the store under `scan`.
///
/// Uses the predicate index for the constant-predicate case (every foundation rule)
/// and intersects the bucket with `delta` membership for the [`Scan::Delta`] /
/// [`Scan::OldOnly`] positions.  Walks the bucket in insertion order so the produced
/// solutions (and their `source_keys`) match a full insertion-ordered scan.
fn extend_solutions(
    atom: &Atom,
    store: &FactStore,
    delta: &HashSet<(String, String, String)>,
    scan: &Scan,
    solutions: &[Solution],
) -> Vec<Solution> {
    let in_delta = |f: &Fact| delta.contains(&f.key());
    let keep = |f: &Fact| match scan {
        Scan::Delta => in_delta(f),
        Scan::Full => true,
        Scan::OldOnly => !in_delta(f),
    };
    let mut next: Vec<Solution> = Vec::new();
    for sol in solutions {
        match &atom.predicate {
            TermPat::Const(p) => {
                // Constant predicate: scan only the predicate's (insertion-ordered)
                // bucket, gated by the delta membership the scan mode requires.
                for &i in store.facts_for_predicate(p) {
                    let f = &store.facts[i];
                    if !keep(f) {
                        continue;
                    }
                    if let Some(mut merged) = match_atom(atom, f, sol) {
                        merged.source_keys.push(f.key());
                        next.push(merged);
                    }
                }
            }
            TermPat::Var(_) => {
                // Variable predicate: full scan (no foundation rule hits this, but it
                // must remain correct), gated by the same delta membership.
                for f in &store.facts {
                    if !keep(f) {
                        continue;
                    }
                    if let Some(mut merged) = match_atom(atom, f, sol) {
                        merged.source_keys.push(f.key());
                        next.push(merged);
                    }
                }
            }
        }
    }
    next
}

/// Join all body atoms against the fact store with **true semi-naive** delta×full
/// evaluation.
///
/// Instead of computing the full join and discarding non-delta solutions at the end,
/// this enumerates, for each positive body atom position `p`, the term
///
/// ```text
///   a_p  ∈ delta          (new at p)
///   a_j  ∈ full store      for j < p   (any binding before p)
///   a_j  ∈ store \ delta   for j > p   (old after p)
/// ```
///
/// The disjoint "new at p, old after p" decomposition produces every delta-touching
/// solution **exactly once** — at its *first* (lowest-index) delta position — so the
/// union over `p` needs no further deduplication.  A round whose `delta` equals the
/// whole store (the per-stratum first round) degenerates to the full join, as
/// required for correctness.
///
/// NAF literals are filtered after the positive join, and the union over positions is
/// concatenated in increasing `p` order (so within a fixed `p` the source-key order
/// is the insertion-ordered scan order).  Mirrors `_join_body_atoms` semantically —
/// the *answer set* (head facts derived over the whole chase) is identical.
///
/// When multiple firings within a single round derive the same head `(s, p, o)` with
/// different source sets, the winner is chosen by a **quality-ordered total-order**
/// tiebreak — smaller tuple wins, independent of firing-enumeration order:
///
/// 1. **Fewest derivation steps** (`max_src_depth`) — prefer the candidate whose
///    deepest source fact has the lowest depth (asserted facts have depth 0, so a
///    derivation grounded directly in assertions scores 0 here).
/// 2. **Asserted-rooted preference** (`sum_src_depth`) — tiebreak on the sum of
///    source-fact depths; a derivation rooted closer to asserted axioms scores lower.
/// 3. **Lexicographically-minimal sorted source reifiers** (`sorted_sources`) — the
///    final content-addressed tiebreaker that guarantees a unique winner.
///
/// Because all three comparison fields are deterministically computable from the
/// content-addressed source reifiers and depth counts, the winning provenance is
/// entirely **independent of firing-enumeration order** — multiple valid derivations
/// of the same head always collapse to the same winner.
fn join_body(
    rule: &Rule,
    store: &FactStore,
    delta: &HashSet<(String, String, String)>,
) -> Vec<Solution> {
    let positive: Vec<&Atom> = rule.body.iter().filter(|a| !a.negated).collect();
    let negated: Vec<&Atom> = rule.body.iter().filter(|a| a.negated).collect();

    let empty = Solution {
        bindings: Vec::new(),
        source_keys: Vec::new(),
    };

    let mut solutions: Vec<Solution> = if positive.is_empty() {
        // Zero positive atoms: the empty solution is the only candidate.  It touches
        // no facts, so it never satisfies the semi-naive delta condition — emit it
        // only in a "full" round (delta == whole store covers the legitimate first
        // round; otherwise an empty-body rule has nothing new to fire on).  This
        // matches the old end-filter, where an empty source_keys list never passed
        // `any(|k| delta.contains(k))`.
        Vec::new()
    } else {
        // True semi-naive: union over the delta position p of
        //   { a_p ∈ delta, a_{<p} ∈ full, a_{>p} ∈ store \ delta }.
        let k = positive.len();
        let mut all: Vec<Solution> = Vec::new();
        for p in 0..k {
            let mut partial: Vec<Solution> = vec![empty.clone()];
            for (j, atom) in positive.iter().enumerate() {
                let scan = if j < p {
                    Scan::Full
                } else if j == p {
                    Scan::Delta
                } else {
                    Scan::OldOnly
                };
                partial = extend_solutions(atom, store, delta, &scan, &partial);
                if partial.is_empty() {
                    break;
                }
            }
            all.extend(partial);
        }
        all
    };

    // NAF filter: drop any binding whose grounded negated atoms still match a fact.
    if !negated.is_empty() {
        solutions.retain(|sol| {
            !negated
                .iter()
                .any(|neg| negated_atom_satisfied(neg, sol, store))
        });
    }

    solutions
}

// ── Per-world chase ──────────────────────────────────────────────────────────────

/// Run the stratified semi-naive chase in one world, producing asserted + derived
/// quads with full provenance.  Mirrors `_chase_world` with `enable_naf=True`.
///
/// # Winner selection (quality-ordered total-order tiebreak)
///
/// When multiple rule firings in the same round derive the same head `(s, p, o)`, the
/// winner is chosen by comparing `(max_src_depth, sum_src_depth, sorted_sources)` —
/// smaller wins.  This prefers the most-direct derivation (fewest steps from asserted
/// facts), tiebreaks toward asserted-rooted proofs, and uses lex-min sorted reifiers
/// as a final content-addressed guarantee.  The comparison is **independent of
/// firing-enumeration order** by construction.
fn chase_world(world_iri: &str, initial: &[Fact]) -> Result<Vec<FoundationQuad>, String> {
    let mut store = FactStore::new();
    for f in initial {
        store.insert(f.clone());
    }

    // Per-fact derivation-depth map: depth 0 for every asserted (initial) fact;
    // derived facts get depth = 1 + max(source depths) when committed.
    let mut depth: HashMap<(String, String, String), u32> = HashMap::new();

    // Asserted quads: source = [self reifier], rule = logic:assert.
    let mut out: Vec<FoundationQuad> = Vec::with_capacity(initial.len());
    for f in initial {
        depth.insert(f.key(), 0); // asserted facts have depth 0
        let reifier = fact_reifier(f)?;
        let deriv = mint_derivation_id(ASSERT_RULE_IRI, &[reifier.as_str()]);
        out.push(FoundationQuad {
            graph: world_iri.to_owned(),
            subject: f.subject.clone(),
            predicate: f.predicate.clone(),
            object: n3(&f.object),
            rule_iri: ASSERT_RULE_IRI.to_owned(),
            source_quad_ids: vec![reifier],
            derivation_id: deriv,
        });
    }

    let mut derived: Vec<FoundationQuad> = Vec::new();

    for stratum in STRATA {
        // Reset delta to ALL current facts so this stratum re-derives against
        // everything settled below it (mirrors `_chase_world`'s per-stratum reset).
        let mut delta: HashSet<(String, String, String)> = store.keys.clone();

        loop {
            // Per-round canonical-winner map: keyed by head key, holds the candidate
            // chosen by a quality-ordered total-order tiebreak (see struct doc).
            // This makes provenance selection independent of firing-enumeration order.
            let mut round: HashMap<(String, String, String), RoundCandidate> = HashMap::new();

            for rule in stratum.iter() {
                for sol in join_body(rule, &store, &delta) {
                    if !distinct_pairs_satisfied(rule.distinct_pairs, &sol)? {
                        continue;
                    }
                    let hs = ground(&rule.head.subject, &sol)
                        .ok_or("head subject unbound after body matching")?;
                    let hp = ground(&rule.head.predicate, &sol)
                        .ok_or("head predicate unbound after body matching")?;
                    let ho = ground(&rule.head.object, &sol)
                        .ok_or("head object unbound after body matching")?;
                    let head = Fact {
                        subject: hs,
                        predicate: hp,
                        object: ho,
                    };
                    let key = head.key();
                    if store.contains_key(&key) {
                        continue; // a prior round already derived it; earlier round wins
                    }

                    // Provenance: reifiers of the matched positive body facts.  All
                    // foundation terms are IRIs, so every source is included (the
                    // Python filter to URIRef subj/pred never drops any here).
                    let mut sources: Vec<String> = Vec::with_capacity(sol.source_keys.len());
                    // Compute depth fields from source fact keys.  Every source fact was
                    // already committed (asserted or a prior-round winner), so its depth
                    // entry is always present.  The `unwrap_or(&0)` is a defensive guard
                    // only — a missing entry would indicate a chase-ordering bug, NOT a
                    // legitimate absent depth (we never silently mask that by choosing 0).
                    let mut max_sd: u32 = 0;
                    let mut sum_sd: u64 = 0;
                    for sk in &sol.source_keys {
                        // sk holds the bare (s, p, o) IRIs of a matched body fact.
                        sources.push(triple_reifier(&sk.0, &sk.1, &sk.2)?);
                        let d = *depth.get(sk).unwrap_or(&0);
                        max_sd = max_sd.max(d);
                        sum_sd = sum_sd.saturating_add(u64::from(d));
                    }
                    let src_refs: Vec<&str> = sources.iter().map(String::as_str).collect();
                    let deriv = mint_derivation_id(ANON_RULE_IRI, &src_refs);
                    let mut sorted_sources = sources.clone();
                    sorted_sources.sort();

                    // Quality-ordered total-order tiebreak:
                    //   (max_src_depth, sum_src_depth, sorted_sources) — smaller wins.
                    // Level 1: fewest derivation steps (most direct).
                    // Level 2: asserted-rooted preference (lower depth sum).
                    // Level 3: lex-min sorted reifiers (content-addressed final key).
                    let candidate = RoundCandidate {
                        head,
                        sources,
                        sorted_sources,
                        deriv,
                        max_src_depth: max_sd,
                        sum_src_depth: sum_sd,
                    };
                    round
                        .entry(key)
                        .and_modify(|existing| {
                            let cand_key = (
                                candidate.max_src_depth,
                                candidate.sum_src_depth,
                                &candidate.sorted_sources,
                            );
                            let exist_key = (
                                existing.max_src_depth,
                                existing.sum_src_depth,
                                &existing.sorted_sources,
                            );
                            if cand_key < exist_key {
                                *existing = candidate.clone();
                            }
                        })
                        .or_insert(candidate);
                }
            }

            if round.is_empty() {
                break; // fixpoint
            }

            // Commit all winners from this round (commit order doesn't matter; final
            // output is canonically sorted at the call site).
            let mut new_delta: HashSet<(String, String, String)> =
                HashSet::with_capacity(round.len());
            for (key, c) in round {
                // Record the winner's depth: 1 + max(source depths).
                // Empty source sets (zero positive body atoms) get depth 1 by convention.
                let winner_depth = c.max_src_depth.saturating_add(1);
                depth.insert(key.clone(), winner_depth);
                store.insert(c.head.clone());
                derived.push(FoundationQuad {
                    graph: world_iri.to_owned(),
                    subject: c.head.subject,
                    predicate: c.head.predicate,
                    object: n3(&c.head.object),
                    rule_iri: ANON_RULE_IRI.to_owned(),
                    source_quad_ids: c.sources,
                    derivation_id: c.deriv,
                });
                new_delta.insert(key);
            }
            delta = new_delta;
        }
    }

    out.extend(derived);
    Ok(out)
}

/// Strip a leading `<` and trailing `>` from an N3 IRI form, returning the inner
/// IRI only when both delimiters are present (so callers can gate on N3-ness).
fn strip_angle_opt(n3: &str) -> Option<&str> {
    n3.strip_prefix('<').and_then(|s| s.strip_suffix('>'))
}

/// Strip a leading `<` and trailing `>` from an N3 IRI form; identity if absent.
fn strip_angle(n3: &str) -> &str {
    strip_angle_opt(n3).unwrap_or(n3)
}

// ── Cross-world rigidity (post-pass) ─────────────────────────────────────────────

/// The union-of-worlds rigid-type IRI set.  Mirrors `_rigid_type_iris`: a type is
/// rigid iff it is stereotyped `logic:Kind`/`logic:SubKind` in any world (primary)
/// or carries an explicit `logic:rigidlyAppliesTo` marker.
fn rigid_type_iris(quads: &[FoundationQuad]) -> HashSet<String> {
    let rigid_objs: HashSet<String> = RIGID_SORTALS
        .iter()
        .map(|m| n3(&format!("{LOGIC_NS}{m}")))
        .collect();
    let mut rigid = HashSet::new();
    for q in quads {
        // Stereotype-derived rigidity (primary) OR an explicit rigidlyAppliesTo marker.
        let stereotype_rigid = q.predicate == RDF_TYPE && rigid_objs.contains(&q.object);
        if stereotype_rigid || q.predicate == RIGIDLY_APPLIES_TO {
            rigid.insert(q.subject.clone());
        }
    }
    rigid
}

/// Emit `logic:rigidityViolation` quads for cross-world rigidity failures.
///
/// Mirrors `cross_world_rigidity_violations`: for each `(x, T)` rigidly typed in a
/// source world `w1`, every other world `w2` where `x` still exists but is not typed
/// `T` is a violation, recorded in `w2`.  De-duplicated to one quad per `(x, T, w2)`.
/// `source_quad_ids` is empty (cross-world leaf); `derivation_id` hashes the rigidity
/// rule IRI over the reifier of the witnessing typing fact.
fn cross_world_rigidity_violations(
    quads: &[FoundationQuad],
) -> Result<Vec<FoundationQuad>, String> {
    let rigid_types = rigid_type_iris(quads);
    if rigid_types.is_empty() {
        return Ok(Vec::new());
    }

    // Index: subjects-per-world and rigid typings-per-world.
    let mut subjects_by_world: std::collections::BTreeMap<String, HashSet<String>> =
        std::collections::BTreeMap::new();
    let mut typings_by_world: std::collections::BTreeMap<String, HashSet<(String, String)>> =
        std::collections::BTreeMap::new();
    for q in quads {
        subjects_by_world
            .entry(q.graph.clone())
            .or_default()
            .insert(q.subject.clone());
        if q.predicate == RDF_TYPE {
            if let Some(type_iri) = strip_angle_opt(&q.object) {
                if rigid_types.contains(type_iri) {
                    typings_by_world
                        .entry(q.graph.clone())
                        .or_default()
                        .insert((q.subject.clone(), type_iri.to_owned()));
                }
            }
        }
    }

    let worlds: Vec<String> = subjects_by_world.keys().cloned().collect();
    if worlds.len() < 2 {
        return Ok(Vec::new());
    }

    // Closure over ordered world pairs; first witnessing (lexicographically smallest)
    // source world wins — dedup on (x, T, w2).
    let mut seen: HashSet<(String, String, String)> = HashSet::new();
    let mut out: Vec<FoundationQuad> = Vec::new();
    for w1 in &worlds {
        let mut typings: Vec<(String, String)> = typings_by_world
            .get(w1)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        typings.sort();
        for (inst, type_iri) in typings {
            for w2 in &worlds {
                if w2 == w1 {
                    continue;
                }
                let exists_w2 = subjects_by_world.get(w2).is_some_and(|s| s.contains(&inst));
                if !exists_w2 {
                    continue;
                }
                let typed_w2 = typings_by_world
                    .get(w2)
                    .is_some_and(|s| s.contains(&(inst.clone(), type_iri.clone())));
                if typed_w2 {
                    continue;
                }
                let key = (inst.clone(), type_iri.clone(), w2.clone());
                if !seen.insert(key) {
                    continue;
                }
                let witness = triple_reifier(&inst, RDF_TYPE, &type_iri)?;
                let deriv = mint_derivation_id(RIGIDITY_RULE_IRI, &[witness.as_str()]);
                out.push(FoundationQuad {
                    graph: w2.clone(),
                    subject: inst.clone(),
                    predicate: format!("{LOGIC_NS}rigidityViolation"),
                    object: n3(&type_iri),
                    rule_iri: RIGIDITY_RULE_IRI.to_owned(),
                    source_quad_ids: Vec::new(),
                    derivation_id: deriv,
                });
            }
        }
    }
    Ok(out)
}

// ── Anti-rigidity witness policy (post-pass) ─────────────────────────────────────

/// The union-of-worlds anti-rigid-type IRI set.  Mirrors `_anti_rigid_type_iris`.
fn anti_rigid_type_iris(quads: &[FoundationQuad]) -> HashSet<String> {
    let anti_objs: HashSet<String> = ANTI_RIGID_SORTALS
        .iter()
        .map(|m| n3(&format!("{LOGIC_NS}{m}")))
        .collect();
    let mut anti = HashSet::new();
    for q in quads {
        if q.predicate == RDF_TYPE && anti_objs.contains(&q.object) {
            anti.insert(q.subject.clone());
        }
    }
    anti
}

/// Emit the policy-appropriate anti-rigidity instance-level obligation quads.
///
/// Mirrors `anti_rigidity_obligations`.  `schema-only` emits nothing.  For the other
/// two policies, every anti-rigid instantiation `(x, rdf:type, T)` emits one quad in
/// its world: `dischargeObligation` (witness-obligation) or `witnessRequiredViolation`
/// (witness-required, unless a counter-world discharges it).  `source_quad_ids` is
/// empty; `derivation_id` hashes the anti-rigidity rule IRI over the typing reifier.
fn anti_rigidity_obligations(
    quads: &[FoundationQuad],
    policy: AntiRigidityPolicy,
) -> Result<Vec<FoundationQuad>, String> {
    if policy == AntiRigidityPolicy::SchemaOnly {
        return Ok(Vec::new());
    }

    let anti_rigid_types = anti_rigid_type_iris(quads);
    if anti_rigid_types.is_empty() {
        return Ok(Vec::new());
    }

    let mut subjects_by_world: std::collections::BTreeMap<String, HashSet<String>> =
        std::collections::BTreeMap::new();
    let mut typings_by_world: std::collections::BTreeMap<String, HashSet<(String, String)>> =
        std::collections::BTreeMap::new();
    for q in quads {
        subjects_by_world
            .entry(q.graph.clone())
            .or_default()
            .insert(q.subject.clone());
        if q.predicate == RDF_TYPE {
            if let Some(type_iri) = strip_angle_opt(&q.object) {
                if anti_rigid_types.contains(type_iri) {
                    typings_by_world
                        .entry(q.graph.clone())
                        .or_default()
                        .insert((q.subject.clone(), type_iri.to_owned()));
                }
            }
        }
    }

    let predicate = match policy {
        AntiRigidityPolicy::WitnessObligation => format!("{LOGIC_NS}dischargeObligation"),
        AntiRigidityPolicy::WitnessRequired => format!("{LOGIC_NS}witnessRequiredViolation"),
        AntiRigidityPolicy::SchemaOnly => unreachable!("schema-only returned early"),
    };

    let mut out: Vec<FoundationQuad> = Vec::new();
    for (typing_world, typings) in &typings_by_world {
        let mut sorted: Vec<(String, String)> = typings.iter().cloned().collect();
        sorted.sort();
        for (inst, type_iri) in sorted {
            if policy == AntiRigidityPolicy::WitnessRequired {
                // A counter-world w2 != w where inst exists but is NOT typed T discharges it.
                let discharged = subjects_by_world.iter().any(|(w2, subjects)| {
                    w2 != typing_world
                        && subjects.contains(&inst)
                        && !typings_by_world
                            .get(w2)
                            .is_some_and(|t| t.contains(&(inst.clone(), type_iri.clone())))
                });
                if discharged {
                    continue;
                }
            }
            let witness = triple_reifier(&inst, RDF_TYPE, &type_iri)?;
            let deriv = mint_derivation_id(ANTI_RIGIDITY_RULE_IRI, &[witness.as_str()]);
            out.push(FoundationQuad {
                graph: typing_world.clone(),
                subject: inst,
                predicate: predicate.clone(),
                object: n3(&type_iri),
                rule_iri: ANTI_RIGIDITY_RULE_IRI.to_owned(),
                source_quad_ids: Vec::new(),
                derivation_id: deriv,
            });
        }
    }
    Ok(out)
}

// ── Public entry point ───────────────────────────────────────────────────────────

/// Evaluate the foundation disciplines over `store` under the given policy.
///
/// Returns the asserted (echoed input) quads plus all derived helper, violation,
/// rigidity, and obligation quads, sorted by `(graph, subject, predicate, object)`.
///
/// # Errors
///
/// Returns `Err` for an invalid IRI in the input, a non-IRI (literal or blank-node)
/// object — foundation requires all-IRI triples — an unbound inequality-guard
/// variable, or any provenance recipe failure (e.g. an RDF-star term).
pub fn evaluate(
    store: &WorldStore,
    policy: AntiRigidityPolicy,
) -> Result<Vec<FoundationQuad>, String> {
    // Collect per-world initial facts, worlds in sorted order (mirrors the oracle's
    // `for world_iri, facts in sorted(world_facts.items())`).  Within a world, facts
    // are inserted in the store's iteration order; the chase's first-wins provenance
    // is robust to that order for the assertions (each asserted quad is self-sourced).
    let mut worlds = store.worlds();
    worlds.sort();

    // Build the per-world initial fact sets (IRI validation + deterministic sort).
    // This is done sequentially because `store.quads_in_world` takes a shared ref
    // and the error path must abort early.
    let mut world_inputs: Vec<(String, Vec<Fact>)> = Vec::with_capacity(worlds.len());
    for world in &worlds {
        let raw = store.quads_in_world(world);
        // Foundation facts are all-IRI triples; object is an IRI string (oxigraph
        // renders IRIs as `<iri>` in to_string()).  Strip the angle brackets so the
        // Fact carries bare IRIs (object n3 is recomputed on output).
        let mut initial: Vec<Fact> = Vec::with_capacity(raw.len());
        for r in &raw {
            let subject = strip_angle(&r[0]).to_owned();
            let predicate = strip_angle(&r[1]).to_owned();
            // Foundation requires all-IRI triples (no-optionality: no literal/blank
            // support).  oxigraph renders an IRI object as `<iri>`; anything else is a
            // literal or blank node, which can never match an IRI-constant atom and
            // would later abort provenance minting (fact_reifier -> NamedNode::new)
            // with an opaque error.  Reject it up front with a clear message instead.
            let object = match strip_angle_opt(&r[2]) {
                Some(iri) => iri.to_owned(),
                None => {
                    return Err(format!(
                        "foundation requires IRI triples: non-IRI object {:?} \
                         (subject {:?}, predicate {:?}) in world {world}",
                        r[2], r[0], r[1]
                    ));
                }
            };
            initial.push(Fact {
                subject,
                predicate,
                object,
            });
        }
        // Sort the initial facts by N3 key so insertion order is deterministic and
        // independent of oxigraph's internal iteration order.
        initial.sort_by_key(Fact::key);
        world_inputs.push((world.clone(), initial));
    }

    // Chase each world.  When there are multiple worlds each with enough facts to
    // amortize rayon thread-pool overhead, run in parallel — each `chase_world`
    // call is a pure function (read-only rules, no shared mutable state) that
    // produces its own Vec<FoundationQuad>.  Single-world inputs (and the common
    // case of tiny conformance cases) fall through to the sequential path.
    //
    // Threshold: parallel when worlds > 1 AND total facts > 500.  Below that the
    // thread-pool spin-up cost exceeds the chase cost on these small inputs.
    let total_facts: usize = world_inputs.iter().map(|(_, f)| f.len()).sum();
    let use_parallel = world_inputs.len() > 1 && total_facts > 500;

    let mut all: Vec<FoundationQuad> = Vec::new();
    if use_parallel {
        // Parallel: collect results indexed by world position so output order is
        // identical to the sequential case (worlds were sorted above).
        let results: Vec<Result<Vec<FoundationQuad>, String>> = world_inputs
            .par_iter()
            .map(|(world_iri, initial)| chase_world(world_iri, initial))
            .collect();
        for r in results {
            all.extend(r?);
        }
    } else {
        for (world_iri, initial) in &world_inputs {
            let world_quads = chase_world(world_iri, initial)?;
            all.extend(world_quads);
        }
    }

    // Cross-world post-passes operate over the union of all materialized quads.
    let rigidity = cross_world_rigidity_violations(&all)?;
    let obligations = anti_rigidity_obligations(&all, policy)?;
    all.extend(rigidity);
    all.extend(obligations);

    // Final canonical sort (matches the runner's fold + sort).
    all.sort_by(|a, b| {
        (&a.graph, &a.subject, &a.predicate, &a.object).cmp(&(
            &b.graph,
            &b.subject,
            &b.predicate,
            &b.object,
        ))
    });
    Ok(all)
}

/// Evaluate the foundation disciplines and fold the result into a truth-maintenance
/// [`crate::derivation_graph::DerivationGraph`] (issue #820, S6b).
///
/// This is the chase→derivation-graph wiring: it runs [`evaluate`] (which preserves
/// #824's per-world parallel chase and deterministic world/index-ordered fold) and
/// then records each materialized quad as one justification —
/// [`crate::derivation_graph::from_foundation_quads`]. Because `evaluate` returns the
/// quads in canonical content order, and `from_foundation_quads` keys everything by
/// content-addressed reifier IRIs (never numeric runtime ids), the resulting graph
/// is byte-stable across runs and interner-id assignments.
///
/// The self-attestation guard is inherited: a derived quad that referenced its own
/// reifier as a source would be rejected here (it never should — that is a malformed
/// chase).
///
/// # Errors
///
/// Returns `Err` for any error from [`evaluate`] or for a self-referential derived
/// quad (self-attestation).
pub fn derivation_graph(
    store: &WorldStore,
    policy: AntiRigidityPolicy,
) -> Result<crate::derivation_graph::DerivationGraph, String> {
    let quads = evaluate(store, policy)?;
    crate::derivation_graph::from_foundation_quads(&quads)
}

/// The semantic-profile IRI stamped on every emitted quad (exposed for the PyO3 seam).
pub const fn profile_iri() -> &'static str {
    PROFILE_IRI
}

/// The canonical budget-status string for an unbounded foundation run.
pub const fn budget_status() -> &'static str {
    BUDGET_OK
}

#[cfg(test)]
mod tests;
