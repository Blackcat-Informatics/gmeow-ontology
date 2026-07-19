// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native Rust evaluator for the OntoUML *foundation* disciplines.
//!
//! This module is the **canonical** provider of the OntoUML *foundation*
//! disciplines.  (The Python foundation oracle that preceded it —
//! `logic_foundation.py` plus the `enable_naf` chase path of
//! `logic_materialize.py` — was retired.)  It lowers five OntoUML structural
//! disciplines into a small stratified Datalog program with negation-as-failure and
//! inequality guards, runs that program on the crate's **single** native chase
//! ([`crate::physical::materialize_native`]), and then applies the OntoUML
//! post-passes (positive cross-world rigidity, the anti-rigidity witness policy, and
//! the per-world characteristic / distinctness passes).  This module is now a *thin
//! consumer* of the shared engine — it owns the rule program and the post-passes, not
//! a chase of its own.
//!
//! # Canonical evaluation contract
//!
//! The materialized quad *set* alone is not enough: the explanation goldens are
//! content-addressed by **derivation IRIs**, and a derivation IRI is
//! `mint_derivation_id(rule_iri, sorted(source_reifiers))`.  For a quad derivable
//! by more than one rule firing, the chase records the **first** firing under its
//! evaluation order (first-wins dedup).  The shared chase reproduces byte-identical
//! provenance because it honours the same ordering constraints:
//!
//! 1. **Stratum order** — the physical stratifier ([`crate::physical`]'s
//!    Bellman-Ford longest-path `stratify`) assigns each predicate a stratum from the
//!    predicate dependency graph, reproducing the partition (helpers/markers before
//!    the NAF-dependent helpers before the violation rules) so a negated atom is only
//!    checked once the predicate it negates is at fixpoint.
//! 2. **Rule order within a stratum** — rules fire in the order the lowering emits
//!    them, which is the [`STRATA`] table order (the canonical `_sort_key` order:
//!    head, then body, then distinct pairs).
//! 3. **Body-binding enumeration order** — the join walks facts in *insertion
//!    order*, and the columnar store is filled in lockstep, so the matched
//!    subsequence (and hence `source_quad_ids` order) is deterministic.
//! 4. **First-wins dedup** — a quad whose `(s, p, o)` key already exists is dropped,
//!    keeping the first derivation's provenance; ties on the quality-ordered
//!    `(max_src_depth, sum_src_depth, sorted_sources, rule_iri)` tiebreak are
//!    order-independent.
//!
//! The provenance recipe itself is reused verbatim from [`crate::provenance`]
//! (`mint_reifier` / `mint_derivation_id`), which is already golden-pinned.
//!
//! # No-optionality
//!
//! An unknown anti-rigidity policy is a hard error ([`AntiRigidityPolicy::from_str`]
//! returns `Err`).  A malformed inequality guard (an unbound guard variable) is a
//! hard error.  There is no silent default and no degraded fallback.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;

use purrdf::TermValue;

use crate::provenance::{mint_derivation_id, mint_reifier};
use crate::store::WorldStore;

/// Wrap a foundation multi-world chase condition message as a typed diagnostic on
/// the shared substrate, preserving the authored text verbatim.
fn foundation_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Foundation { detail })
}

// ── Namespace + vocabulary constants ───────────────────────────────────────────

/// The `logic:` vocabulary namespace — term IRIs are `LOGIC_NS + local`.
/// Matches `gmeow_tools.config.LOGIC_NAMESPACE`.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// The `rdf:type` predicate IRI (string form).
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The `owl:FunctionalProperty` class IRI (string form).  A mediation role typed
/// with it reaches exactly one relatum (one end); a role WITHOUT it reaches two or
/// more (two ends) — the entity-count reading the relator-mediation discipline needs.
const OWL_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";

/// Sentinel rule IRI stamped on asserted (input) quads (`logic:assert`).
pub const ASSERT_RULE_IRI: &str = "https://blackcatinformatics.ca/logic/assert";

/// Rule IRI stamped on every in-world foundation rule firing.  The foundation
/// rules carry no `scope.provenance`, so this evaluator stamps them all with
/// `logic:rule/anonymous`.
const ANON_RULE_IRI: &str = "https://blackcatinformatics.ca/logic/rule/anonymous";

/// Rule IRI for the cross-world rigidity closure pass (`logic:rule/cross-world-rigidity`).
const RIGIDITY_RULE_IRI: &str = "https://blackcatinformatics.ca/logic/rule/cross-world-rigidity";

/// Rule IRI for the anti-rigidity witness pass (`logic:rule/anti-rigidity-witness`).
const ANTI_RIGIDITY_RULE_IRI: &str =
    "https://blackcatinformatics.ca/logic/rule/anti-rigidity-witness";

// ── Property-characteristic vocabulary (H4) ──────────────────────────────────────
//
// The characteristic post-pass reads BOTH the OWL characteristic classes (the
// OWL-facing declaration on a `gmeow:` property) and their `logic:` analogues (the
// canonical carrier, asserted centrally with a `logic:formalizes` back-ref), so
// every declared characteristic — not just the ones re-stated in `logic:` — is
// enforced by the native gate.

/// `owl:TransitiveProperty` class IRI.
const OWL_TRANSITIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
/// `owl:SymmetricProperty` class IRI.
const OWL_SYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
/// `owl:IrreflexiveProperty` class IRI.
const OWL_IRREFLEXIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#IrreflexiveProperty";
/// `owl:AsymmetricProperty` class IRI.
const OWL_ASYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AsymmetricProperty";

/// Predicate linking a central characteristic record to the property it characterises.
const LOGIC_CHARACTERIZES: &str = "https://blackcatinformatics.ca/logic/characterizes";
/// Predicate linking a central characteristic record to its characteristic sort.
const LOGIC_CHARACTERISTIC_SORT: &str = "https://blackcatinformatics.ca/logic/characteristicSort";

/// Predicate linking a `logic:RelatumDistinctnessAssertion` to its target class.
const LOGIC_DISTINCTNESS_TARGET: &str = "https://blackcatinformatics.ca/logic/distinctnessTarget";
/// Predicate linking a `logic:RelatumDistinctnessAssertion` to one of its two roles.
const LOGIC_DISTINCTNESS_ROLE: &str = "https://blackcatinformatics.ca/logic/distinctnessRole";

/// Rule IRI stamped on a transitive-closure edge derived by the characteristic pass.
const CHAR_TRANSITIVE_RULE_IRI: &str =
    "https://blackcatinformatics.ca/logic/rule/property-characteristic-transitive";
/// Rule IRI stamped on a symmetric-mirror edge derived by the characteristic pass.
const CHAR_SYMMETRIC_RULE_IRI: &str =
    "https://blackcatinformatics.ca/logic/rule/property-characteristic-symmetric";
/// Rule IRI stamped on an irreflexivity/asymmetry violation raised by the pass.
const CHAR_CLASH_RULE_IRI: &str =
    "https://blackcatinformatics.ca/logic/rule/property-characteristic-clash";

/// Violation discipline: an irreflexive property holds between an entity and itself.
const IRREFLEXIVITY_VIOLATION: &str = "https://blackcatinformatics.ca/logic/IrreflexivityViolation";
/// Violation discipline: an asymmetric property holds in both directions of a pair.
const ASYMMETRY_VIOLATION: &str = "https://blackcatinformatics.ca/logic/AsymmetryViolation";
/// Violation discipline: an acyclic property's transitive closure returns to its start —
/// a node reaches itself by following the property one or more steps.
const ACYCLICITY_VIOLATION: &str = "https://blackcatinformatics.ca/logic/AcyclicityViolation";
/// Rule IRI stamped on an acyclicity violation raised by the characteristic pass.
const CHAR_ACYCLIC_RULE_IRI: &str =
    "https://blackcatinformatics.ca/logic/rule/property-characteristic-acyclic";
/// Violation discipline: a relator's two distinctness roles bind the same value on one
/// focus node (the mutual-inequality integrity condition is broken).
const RELATUM_DISTINCTNESS_VIOLATION: &str =
    "https://blackcatinformatics.ca/logic/RelatumDistinctnessViolation";
/// Rule IRI stamped on a relatum-distinctness violation.
const RELATUM_DISTINCTNESS_RULE_IRI: &str =
    "https://blackcatinformatics.ca/logic/rule/relatum-distinctness";
/// Violation discipline: a DL-projectable `logic:` characteristic record whose OWL
/// projection (`owl:{Transitive,Symmetric,Functional}Property`) is missing — the two
/// carriers of one characteristic have drifted (Principle 17: the `logic:` record is
/// canonical, the OWL marker is its projection, and the projection must not be dropped).
const CHARACTERISTIC_CARRIER_DISAGREEMENT: &str =
    "https://blackcatinformatics.ca/logic/CharacteristicCarrierDisagreement";
/// Rule IRI stamped on a carrier-disagreement violation.
const CHAR_CARRIER_RULE_IRI: &str =
    "https://blackcatinformatics.ca/logic/rule/property-characteristic-carrier-agreement";

/// The semantic-profile IRI stamped on every emitted quad — the only profile the
/// v1 oracle applies.  Matches `py.rs::ASSERTED_PROFILE`.
const PROFILE_IRI: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";

/// Budget status stamped on every quad — this evaluator runs to full fixpoint with
/// no budget ceiling, so every quad is `"ok"`.
const BUDGET_OK: &str = "ok";

/// Rigid sortals (supply / inherit a principle of identity) — the **primary**
/// rigid-type path.
const RIGID_SORTALS: [&str; 2] = ["Kind", "SubKind"];

/// Anti-rigid sortals (classify instances only contingently).
const ANTI_RIGID_SORTALS: [&str; 2] = ["Phase", "Role"];

/// Marker the schema may carry to declare a type rigid explicitly (honoured in
/// addition to the stereotype-derived path).
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
    // Named `from_str` deliberately (the PyO3 seam + the spec call it by this
    // name); the fallible `String`-error signature does not match `std::str::FromStr`.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> gmeow_errors::Result<Self> {
        match value {
            "witness-obligation" => Ok(Self::WitnessObligation),
            "schema-only" => Ok(Self::SchemaOnly),
            "witness-required" => Ok(Self::WitnessRequired),
            other => Err(foundation_err(format!(
                "Unknown anti_rigidity_policy {other:?}; must be one of \
                 [\"schema-only\", \"witness-obligation\", \"witness-required\"]"
            ))),
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

// ── The foundation program (data) ───────────────────────────────────────────────
//
// The rules are authored as five ordered groups.  The physical stratifier
// re-derives the strata from the predicate dependency graph (it is authoritative —
// no foundation-specific stratum boundary is fed to the engine), and the groups here
// are just a readable ordering that reproduces the certifier's `stratify` partition
// (helpers before NAF-dependent helpers before violations).  The rule order WITHIN
// each group is the canonical `_sort_key` order (head, then body, then distinct
// pairs), and the body of every rule is likewise in canonical sorted order — the
// parity anchor: lowering these rules in this order and chasing them with first-wins
// dedup yields byte-identical derivation IRIs.

// Group 0 is empty (no rule's head predicate lands in the lowest SCC layer, which
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
    // functionalProperty(?P, ?P) :- ?P rdf:type owl:FunctionalProperty
    // A mediation role that is functional reaches exactly one relatum (one "end");
    // a non-functional role reaches two or more.  This marker lifts the property
    // characteristic into the chase so the entity-count reading of the mediation
    // discipline can weight each role — a single non-functional role already
    // mediates two distinct entities, so it satisfies the discipline on its own.
    // Pure-positive over EDB, so it settles in this stratum, below the stratum-2
    // hasTwoMediatedRelata rules whose NAF ranges over it.
    //
    // The functional characteristic has two carriers in the EDB and this marker
    // UNIONS both, exactly as the cross-world `collect_characteristics` post-pass
    // does: the deprecated OWL type marker below, and the canonical central
    // `logic:PropertyCharacteristicAssertion` record above.  Deriving from the
    // carrier is what keeps the relator-mediation entity-count identical after the
    // `owl:FunctionalProperty` SOURCE declarations are removed from the slices —
    // the carrier record survives removal, the OWL marker (still authored on raw
    // external/conformance inputs, and re-emitted by the OWL grounding VIEW) does
    // not vanish for those graphs.
    Rule {
        head: pos(
            var("?P"),
            TermPat::Const(logic_iri!("functionalProperty")),
            var("?P"),
        ),
        body: &[pos(
            var("?P"),
            TermPat::Const(RDF_TYPE),
            TermPat::Const(OWL_FUNCTIONAL_PROPERTY),
        )],
        distinct_pairs: NO_GUARD,
    },
    // functionalProperty(?P, ?P) :-
    //     ?rec logic:characterizes ?P,
    //     ?rec logic:characteristicSort logic:functionalProperty
    // The canonical carrier derivation: a central characteristic record naming ?P
    // functional is the greenfield source of the property characteristic (the OWL
    // marker rule above is its lossy projection).  Join the record's `characterizes`
    // and `characteristicSort` edges on the record IRI, exactly as
    // `collect_characteristics` (foundation.rs) joins them for the cross-world
    // characteristic post-pass.  Pure-positive over EDB, same stratum as the OWL
    // marker rule, so the two settle together below the stratum-2 NAF that reads
    // `functionalProperty`.
    Rule {
        head: pos(
            var("?P"),
            TermPat::Const(logic_iri!("functionalProperty")),
            var("?P"),
        ),
        body: &[
            pos(
                var("?rec"),
                TermPat::Const(LOGIC_CHARACTERIZES),
                var("?P"),
            ),
            pos(
                var("?rec"),
                TermPat::Const(LOGIC_CHARACTERISTIC_SORT),
                TermPat::Const(logic_iri!("functionalProperty")),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // mediates(?C, ?R) :- subClassOfT(?C, ?P), mediates(?P, ?R)
    // A relator subclass inherits the relata its ancestors mediate: a gmeow:Contract
    // IS a gmeow:Agreement and mediates the same parties; a gmeow:Finding IS a
    // gmeow:Observation and mediates the same observed feature and vantage.  Without
    // this, a concrete (leaf) relator that specialises a mediated relator would count
    // zero mediated relata of its own and spuriously trip RelComp.  Pure-positive, so
    // it settles by fixpoint in this stratum, below the stratum-2 hasTwoMediatedRelata
    // rules that count the relata a relator reaches.
    Rule {
        head: pos(var("?C"), TermPat::Const(logic_iri!("mediates")), var("?R")),
        body: &[
            pos(
                var("?C"),
                TermPat::Const(logic_iri!("subClassOfT")),
                var("?P"),
            ),
            pos(var("?P"), TermPat::Const(logic_iri!("mediates")), var("?R")),
        ],
        distinct_pairs: NO_GUARD,
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
    // ── Typed/contextual mereology + holon kernel (C1) ──────────────────
    // Positive prerequisites: overlap, the supplementation-profile marker, and the
    // unary holon projection.  All depend only on the asserted (EDB) relations
    // logic:properPartOf and logic:underMereologyProfile, so they are inert on inputs
    // that carry neither — the earlier foundation goldens are unaffected.
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
    // ── YAMATO occurrent mereology ──────────────────────────────────────────────
    // Inert on inputs without logic:causalPartOf — temporalPartOf has no other
    // consumer in this program, so these only ever ADD causal/temporal-part facts.
    //
    // temporalPartOf(?X, ?Y) :- causalPartOf(?X, ?Y)   (causal part is a temporal part)
    Rule {
        head: pos(
            var("?X"),
            TermPat::Const(logic_iri!("temporalPartOf")),
            var("?Y"),
        ),
        body: &[pos(
            var("?X"),
            TermPat::Const(logic_iri!("causalPartOf")),
            var("?Y"),
        )],
        distinct_pairs: NO_GUARD,
    },
    // causalPartOf(?X, ?Z) :- causalPartOf(?X, ?Y), causalPartOf(?Y, ?Z)
    // Kept in-chase, NOT delegated to the generic property-characteristic post-pass:
    // the causal⊑temporal lift above consumes the transitively-closed causalPartOf
    // within the same fixpoint, so the closure must be visible to that downstream rule.
    // A post-pass runs after the chase and cannot feed it — removing this rule drops
    // both the closed causalPartOf edge and its temporalPartOf lift. The characteristic
    // post-pass is additive: it fires only on a property carrying a characteristic
    // marker/record (the occurrent fixtures carry none) and is idempotent with this rule.
    Rule {
        head: pos(
            var("?X"),
            TermPat::Const(logic_iri!("causalPartOf")),
            var("?Z"),
        ),
        body: &[
            pos(
                var("?X"),
                TermPat::Const(logic_iri!("causalPartOf")),
                var("?Y"),
            ),
            pos(
                var("?Y"),
                TermPat::Const(logic_iri!("causalPartOf")),
                var("?Z"),
            ),
        ],
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
    // ── Holonic emergence: aggregate reduction (C2) ──────────────────────
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
    // ── Holonic governance: override marker (C3) ─────────────────────────
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
    // ── Holonic agency: the two co-equal tendency markers (C4) ────────────
    // Koestler's Janus-faced holon carries a self-assertive (autonomy-as-a-whole) and an
    // integrative (subordination-as-a-part) tendency.  These are CO-EQUAL vantage facets
    // (Principle 9): the two markers are built by IDENTICAL rules — a holon evidences a
    // tendency when its declared logic:HolonicAgencyProfile carries a basis value (the
    // selfAssertiveBasis / integrativeBasis twin) and the holon bears that value — so
    // neither face is privileged in the vocabulary or the firing order.  Both settle in
    // stratum 1 so the pathology NAF (stratum 3) and the unknown NAF (stratum 4) are
    // stratified.  Inert on inputs with no logic:AgencyAssessment.  Agency is a DECLARED
    // profile a holarchy adopts, not a universal rule.  (LOGIC-FOUNDATION.md
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
    // ── Holonic level coherence: position presence marker (C5) ───────────
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
    // multiplyPositioned(?X, ?X) :- positionEntity(?P1, ?X), positionEntity(?P2, ?X)  [?P1 != ?P2]
    //
    // ME9, the positive companion to the stratum-4 logic:HolonicLevelIncoherence
    // (which fires for a profiled holon occupying NO position).  This marker fires when an
    // entity occupies TWO OR MORE distinct logic:HolonicPositions — the structural signature
    // of a DAG node sitting on several paths through a holarchy, so its logic:holonicLevel is
    // genuinely path-relative (two paths may place it at different depths, both correct) and a
    // non-trivial logic:holonicLevelMin..Max band exists.  It grounds only the band's
    // EXISTENCE: the all-IRI stratified chase has no numeric comparison, so it cannot derive
    // or check the band's integer endpoints (those stay operator-asserted EDB).  Keyed on the
    // position reifiers via positionEntity (position-subject, entity-object), distinct by the
    // ?P1≠?P2 guard.  Depends only on the EDB position axis, so it settles in stratum 1.
    // (LOGIC-FOUNDATION.md §mereology+holons.)
    Rule {
        head: pos(
            var("?X"),
            TermPat::Const(logic_iri!("multiplyPositioned")),
            var("?X"),
        ),
        body: &[
            pos(
                var("?P1"),
                TermPat::Const(logic_iri!("positionEntity")),
                var("?X"),
            ),
            pos(
                var("?P2"),
                TermPat::Const(logic_iri!("positionEntity")),
                var("?X"),
            ),
        ],
        distinct_pairs: &[("?P1", "?P2")],
    },
];

const STRATUM_2: &[Rule] = &[
    // ── Relator mediation, entity-count reading ─────────────────────────────────────
    // A Relator must mediate at least two distinct ENTITIES.  Mediation is carried by
    // role properties (logic:mediates names each role); the count of entities a relator
    // reaches is the count of its roles WEIGHTED by cardinality — a functional role
    // reaches one entity (one end), a non-functional role reaches two or more (two
    // ends).  So a relator satisfies the discipline when it either mediates via two
    // distinct roles, or mediates via a single non-functional role.  These two rules
    // are the canonical, engine-native reading of the mediation discipline; the gUFO
    // downcast is a lossy projection of them, never the authority.  Both must sit above
    // stratum 1: the second negates functionalProperty, which settles there.
    //
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
    // hasTwoMediatedRelata(?C, ?C) :- mediates(?C, ?R), NOT functionalProperty(?R, ?R)
    // A single non-functional mediation role reaches two or more entities, so it
    // discharges the discipline on its own.
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("hasTwoMediatedRelata")),
            var("?C"),
        ),
        body: &[
            pos(var("?C"), TermPat::Const(logic_iri!("mediates")), var("?R")),
            neg(
                var("?R"),
                TermPat::Const(logic_iri!("functionalProperty")),
                var("?R"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
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
    // isRelatorClass(?C, ?C) :- subClassOfT(?C, logic:Relator)
    // Production relators are typed `a logic:Kind ; rdfs:subClassOf logic:Relator` — a
    // class-edge to the Relator meta-class, never a direct `a logic:Relator`.  The two
    // hasMetaClass-keyed markers above never bind on that shape (logic:Relator itself
    // carries no hasMetaClass fact), leaving the relator-mediation discipline (RelComp)
    // dormant on every production relator.  This marker confers relator-hood on every
    // subclass of the Relator category so the discipline reaches them.  logic:Relator
    // itself is never flagged: subClassOfT is non-reflexive, and even were it derived,
    // concreteRelator is guarded by NOT hasLogicSubclass, which holds for logic:Relator.
    Rule {
        head: pos(
            var("?C"),
            TermPat::Const(logic_iri!("isRelatorClass")),
            var("?C"),
        ),
        body: &[pos(
            var("?C"),
            TermPat::Const(logic_iri!("subClassOfT")),
            TermPat::Const(logic_iri!("Relator")),
        )],
        distinct_pairs: NO_GUARD,
    },
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
    // ── Holonic emergence: aggregate verdict projection (C2) ─────────────
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
    // ── Holonic governance: overridden verdict projection (C3) ───────────
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
    // ── Holonic agency: integral verdict projection (C4) ─────────────────
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
    // ── Mereology NAF helpers (C1) ──────────────────────────────────────
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
    // ── Holonic emergence: emergent verdict (C2) ─────────────────────────
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
    // ── Holonic governance: binding marker (C3) ──────────────────────────
    // A downward constraint BINDS its named target by negation-as-failure over the
    // override derivation, WHILE the constraint still binds a declared logic:GovernanceRegime
    // (?R) whose activationBasis carries the constrained state (?S) — so the verdict is
    // regime-relative, never a bare "unconstrained" default.  NON-TRANSITIVE by default
    // the constraint is read only for the explicitly named ?P (a proper part of
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
    // ── Holonic agency: the two co-equal pathology verdicts (C4) ──────────
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
    // ── YAMATO occurrent constraints ────────────────────────────────────────────
    // Inert unless the input carries logic:occurrentBoundary facts.
    //
    // violation(?E, OccurrentChangeAsymmetry) :-
    //     occurrentBoundary(?E, logic:Closed), bearsProperty(?E, ?F), ?F a logic:Fluent
    // A completed/closed occurrent must not bear a time-varying Fluent.
    Rule {
        head: pos(
            var("?E"),
            TermPat::Const(logic_iri!("violation")),
            TermPat::Const(logic_iri!("OccurrentChangeAsymmetry")),
        ),
        body: &[
            pos(
                var("?E"),
                TermPat::Const(logic_iri!("occurrentBoundary")),
                TermPat::Const(logic_iri!("Closed")),
            ),
            pos(
                var("?E"),
                TermPat::Const(logic_iri!("bearsProperty")),
                var("?F"),
            ),
            pos(
                var("?F"),
                TermPat::Const(RDF_TYPE),
                TermPat::Const(logic_iri!("Fluent")),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // violation(?E, OccurrentBoundaryMismatch) :-
    //     occurrentBoundary(?E, logic:Open), occurrentBoundary(?E, logic:Closed)
    // An occurrent must not be both Open and Closed.
    Rule {
        head: pos(
            var("?E"),
            TermPat::Const(logic_iri!("violation")),
            TermPat::Const(logic_iri!("OccurrentBoundaryMismatch")),
        ),
        body: &[
            pos(
                var("?E"),
                TermPat::Const(logic_iri!("occurrentBoundary")),
                TermPat::Const(logic_iri!("Open")),
            ),
            pos(
                var("?E"),
                TermPat::Const(logic_iri!("occurrentBoundary")),
                TermPat::Const(logic_iri!("Closed")),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // ── YAMATO quality constraints ──────────────────────────────────────────────
    // Inert unless the input carries logic:qualityRole / logic:unit facts.
    //
    // violation(?Q, QualityRoleWithoutGeneric) :-
    //     qualityRole(?Q, ?R), NOT genericQuality(?Q, ?G)
    // A quality playing a quality-role must instantiate the generic quality the role
    // contextualizes (Principle 11 in role terms).
    Rule {
        head: pos(
            var("?Q"),
            TermPat::Const(logic_iri!("violation")),
            TermPat::Const(logic_iri!("QualityRoleWithoutGeneric")),
        ),
        body: &[
            pos(
                var("?Q"),
                TermPat::Const(logic_iri!("qualityRole")),
                var("?R"),
            ),
            neg(
                var("?Q"),
                TermPat::Const(logic_iri!("genericQuality")),
                var("?G"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
    // violation(?M, MeasurementFrameMissing) :-
    //     unit(?M, ?U), NOT referenceFrame(?M, ?F)
    // A value expressed in a unit must declare the reference frame it is read in
    // (Principle 11).  The foundation chase is all-IRI, so the rule keys on the
    // IRI-valued logic:unit witness, not the literal logic:measuredValue it qualifies.
    Rule {
        head: pos(
            var("?M"),
            TermPat::Const(logic_iri!("violation")),
            TermPat::Const(logic_iri!("MeasurementFrameMissing")),
        ),
        body: &[
            pos(var("?M"), TermPat::Const(logic_iri!("unit")), var("?U")),
            neg(
                var("?M"),
                TermPat::Const(logic_iri!("referenceFrame")),
                var("?F"),
            ),
        ],
        distinct_pairs: NO_GUARD,
    },
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
    // ── Weak supplementation (C1) ───────────────────────────────────────
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
    // ── Holonic level coherence: incoherence violation (C5) ─────────────────
    // PROFILE-SCOPED, exactly like weak supplementation (and per profile-relativity):
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
    // ── Holonic emergence: unknown verdict (C2) ──────────────────────────
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
    // ── Holonic governance: unknown verdict (C3) ─────────────────────────
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
    // ── Holonic agency: unknown verdict (C4) ─────────────────────────────
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

/// The set of constant predicate IRIs that appear as a rule HEAD anywhere in the
/// foundation program.
///
/// This is the syntactic-reachability oracle for a [`crate::obligations`]
/// non-entailment check: a predicate the chase can ever derive must be the head of
/// some rule, so a predicate absent from this set is *unreachable* — no rule head
/// unifies with it, directly or through any chain of rule applications (every
/// derived fact is some rule's head). A `logic:NonEntailmentObligation` whose
/// forbidden predicate is not in this set is discharged by syntactic reachability;
/// one whose forbidden predicate IS in it is violated. The foundation heads are all
/// `logic:`-namespaced, so an assertion-only `gmeow:` predicate (e.g.
/// `gmeow:counterpartOf`, `gmeow:deceptiveIntentClaim`) is discharged today and trips
/// only if a future rule introduces it as a head.
#[must_use]
pub fn head_predicate_iris() -> std::collections::BTreeSet<String> {
    let mut set = std::collections::BTreeSet::new();
    for stratum in STRATA {
        for rule in stratum {
            if let TermPat::Const(iri) = rule.head.predicate {
                set.insert(iri.to_owned());
            }
        }
    }
    set
}

// ── Lowering onto the single native chase ───────────────────────────────────────
//
// The OntoUML foundation discipline is *data*: the `STRATA` rule tables above are
// authored by hand, but the chase itself is the crate's ONE native engine
// ([`crate::physical::materialize_native`]).  These helpers lower the static [`Rule`]
// tables into the shared [`crate::rule_ir::EvalRule`] IR and drive the physical core,
// so foundation contributes only its rule program — and, at the [`evaluate`] call
// site, its cross-world post-passes — never a second chase.
//
// The physical stratifier is authoritative: it re-derives the strata from the
// predicate dependency graph, so the flat table order (not the 5-way grouping) is
// what the engine consumes.  Every foundation derivation is stamped with the single
// [`ANON_RULE_IRI`], so the engine's `(max_src_depth, sum_src_depth, sorted_sources,
// rule_iri)` first-wins tiebreak reduces to foundation's original
// `(max_src_depth, sum_src_depth, sorted_sources)` — the fourth key is constant
// across every foundation firing — and the content-addressed `derivation_id`s are
// byte-identical.

/// Lower a foundation [`TermPat`] into the shared [`crate::rule_ir::EvalTerm`].
/// Variable names carry their leading `?` verbatim (the surface the shared join
/// binds on and the inequality guards compare).
fn lower_term_pat(term: TermPat) -> crate::rule_ir::EvalTerm {
    match term {
        TermPat::Var(name) => crate::rule_ir::EvalTerm::Var(name.to_owned()),
        TermPat::Const(iri) => crate::rule_ir::EvalTerm::ConstNamed(iri.to_owned()),
    }
}

/// Lower a foundation [`Atom`] into the shared [`crate::rule_ir::EvalAtom`].  A
/// foundation predicate is always a constant IRI (the program is Datalog over a
/// fixed vocabulary); a variable predicate is a malformed rule and cannot occur.
fn lower_atom(atom: &Atom) -> crate::rule_ir::EvalAtom {
    let predicate = match atom.predicate {
        TermPat::Const(iri) => iri.to_owned(),
        TermPat::Var(name) => unreachable!(
            "foundation rule atom has a variable predicate {name:?}; every foundation \
             predicate is a constant IRI"
        ),
    };
    crate::rule_ir::EvalAtom {
        subject: lower_term_pat(atom.subject),
        predicate,
        object: lower_term_pat(atom.object),
        negated: atom.negated,
    }
}

/// Lower the whole foundation program ([`STRATA`], in table order) into the shared
/// [`crate::rule_ir::EvalRule`] IR.  Every rule is stamped with [`ANON_RULE_IRI`] —
/// the single rule IRI foundation derivations carry — so the shared engine's
/// provenance recipe matches foundation's byte for byte.
fn lower_foundation_rules() -> Vec<crate::rule_ir::EvalRule> {
    let mut out: Vec<crate::rule_ir::EvalRule> = Vec::new();
    for stratum in STRATA {
        for rule in stratum {
            out.push(crate::rule_ir::EvalRule {
                head: lower_atom(&rule.head),
                body: rule.body.iter().map(lower_atom).collect(),
                rule_iri: ANON_RULE_IRI.to_owned(),
                distinct_pairs: rule
                    .distinct_pairs
                    .iter()
                    .map(|&(a, b)| (a.to_owned(), b.to_owned()))
                    .collect(),
                builtins: Vec::new(),
            });
        }
    }
    out
}

/// Assert that every object in `store` is an IRI — foundation's all-IRI contract.
///
/// The shared engine is happy with literal objects, but the foundation disciplines
/// are all-IRI by construction; a non-IRI object is a HARD FAIL (no-optionality),
/// with the exact message the pre-unification per-world input builder emitted.
///
/// # Errors
///
/// Returns `Err` on the first non-IRI object.
fn require_all_iri_objects(store: &WorldStore) -> gmeow_errors::Result<()> {
    let mut worlds = store.worlds();
    worlds.sort();
    for world in &worlds {
        for r in &store.quads_in_world(world) {
            if strip_angle_opt(&r[2]).is_none() {
                return Err(foundation_err(format!(
                    "foundation requires IRI triples: non-IRI object {:?} \
                     (subject {:?}, predicate {:?}) in world {world}",
                    r[2], r[0], r[1]
                )));
            }
        }
    }
    Ok(())
}

/// Chase every world with the single native engine and adapt the derived rows into
/// [`FoundationQuad`]s (the asserted-EDB echo included).
///
/// This is the OntoUML foundation program running on
/// [`crate::physical::materialize_native`]; [`evaluate`] layers the cross-world
/// post-passes on top of the result.  The foundation program is stratified by
/// construction, so an `Unsupported` outcome is a contract violation (hard fail),
/// never a degraded fallback.
///
/// # Errors
///
/// Returns `Err` for a non-IRI object, an unbound head/guard variable, a provenance
/// recipe failure, or an unexpected native gap.
fn chase_all_worlds_physical(store: &WorldStore) -> gmeow_errors::Result<Vec<FoundationQuad>> {
    require_all_iri_objects(store)?;
    let rules = lower_foundation_rules();
    // The foundation program and its contract are process-stable; cache the immutable
    // executable rather than repeating stratification/SIPS planning on every evaluation.
    // A negative cache entry is a contract violation (hard fail), never a fallback.
    let lookup = crate::physical::compile_cached("gmeow-foundation-v1", rules);
    let Some(executable) = lookup.executable else {
        return Err(foundation_err(
            "foundation program unexpectedly non-stratifiable under the native chase \
             (the foundation program is stratified by construction)"
                .to_owned(),
        ));
    };
    // Unbounded: the foundation oracle runs to full fixpoint (`BUDGET_OK`).
    let outcome = crate::physical::materialize_native(store, executable.as_ref(), None)?;
    let rows = match outcome {
        crate::physical::NativeOutcome::Decided(budgeted) => budgeted.rows,
        crate::physical::NativeOutcome::Unsupported(kind) => {
            return Err(foundation_err(format!(
                "foundation program unexpectedly unsupported by the native chase: {kind:?} \
                 (the foundation program is stratified by construction)"
            )));
        }
    };
    let mut out: Vec<FoundationQuad> = Vec::with_capacity(rows.len());
    for row in rows {
        // `subject`/`predicate` are bare IRIs on a `FoundationQuad`; `object` is N3.
        // A `DerivedRow` carries native terms whose `term_display` is the N3 surface.
        out.push(FoundationQuad {
            graph: row.graph,
            subject: strip_angle(&crate::provenance::term_display(&row.subject)).to_owned(),
            predicate: row.predicate,
            object: crate::provenance::term_display(&row.object),
            rule_iri: row.rule_iri,
            source_quad_ids: row.source_quad_ids,
            derivation_id: row.derivation_id,
        });
    }
    Ok(out)
}

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

/// N3 form of an IRI: `<iri>`.
fn n3(iri: &str) -> String {
    format!("<{iri}>")
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
pub fn quad_reifier(quad: &FoundationQuad) -> gmeow_errors::Result<String> {
    Ok(crate::provenance::reifier_from_strings(
        &quad.subject,
        &quad.predicate,
        &quad.object,
    ))
}

/// Reifier IRI for an explicit `(s, p, o)` IRI triple — used by the cross-world passes.
fn triple_reifier(s: &str, p: &str, o: &str) -> gmeow_errors::Result<String> {
    let sn = TermValue::iri(s);
    let on = TermValue::iri(o);
    mint_reifier(&sn, p, &on)
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
) -> gmeow_errors::Result<Vec<FoundationQuad>> {
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
        if q.predicate == RDF_TYPE
            && let Some(type_iri) = strip_angle_opt(&q.object)
            && rigid_types.contains(type_iri)
        {
            typings_by_world
                .entry(q.graph.clone())
                .or_default()
                .insert((q.subject.clone(), type_iri.to_owned()));
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
) -> gmeow_errors::Result<Vec<FoundationQuad>> {
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
        if q.predicate == RDF_TYPE
            && let Some(type_iri) = strip_angle_opt(&q.object)
            && anti_rigid_types.contains(type_iri)
        {
            typings_by_world
                .entry(q.graph.clone())
                .or_default()
                .insert((q.subject.clone(), type_iri.to_owned()));
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

// ── Property characteristics (post-pass, H4) ─────────────────────────────────────

/// The property-characteristic sorts the pass understands.  `Functional` is
/// recognised so a record/marker declaring it is not misread as unknown, but the
/// pass takes no action on it: functional cardinality is enforced by the property's
/// `owl:FunctionalProperty` declaration through native DL consistency.  (The
/// stratum-1 in-chase `functionalProperty` marker is a distinct signal that only
/// feeds the relator-mediation entity-count and does not apply to ordinary
/// functional properties such as the lineage relations `gmeow:versionOf` /
/// `gmeow:editionOf`.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CharSort {
    Transitive,
    Symmetric,
    Irreflexive,
    Asymmetric,
    Functional,
    Acyclic,
}

/// Map a characteristic-sort IRI — either the OWL characteristic class or its
/// `logic:` analogue — to the [`CharSort`] the pass enforces.  A non-characteristic
/// IRI is `None`.
fn char_sort_of(iri: &str) -> Option<CharSort> {
    match iri {
        OWL_TRANSITIVE_PROPERTY => Some(CharSort::Transitive),
        OWL_SYMMETRIC_PROPERTY => Some(CharSort::Symmetric),
        OWL_IRREFLEXIVE_PROPERTY => Some(CharSort::Irreflexive),
        OWL_ASYMMETRIC_PROPERTY => Some(CharSort::Asymmetric),
        OWL_FUNCTIONAL_PROPERTY => Some(CharSort::Functional),
        _ => match iri.strip_prefix(LOGIC_NS) {
            Some("transitiveProperty") => Some(CharSort::Transitive),
            Some("symmetricProperty") => Some(CharSort::Symmetric),
            Some("irreflexiveProperty") => Some(CharSort::Irreflexive),
            Some("asymmetricProperty") => Some(CharSort::Asymmetric),
            Some("functionalProperty") => Some(CharSort::Functional),
            Some("acyclicProperty") => Some(CharSort::Acyclic),
            _ => None,
        },
    }
}

/// Whether a characteristic sort has an OWL projection under this ontology's
/// convention.  Transitive/symmetric/functional are DL-clean and carried as OWL
/// characteristic classes; irreflexive and asymmetric are deliberately `logic:`-only
/// (they would break the OWL 2 EL profile), so they carry no OWL projection to
/// cross-check.
fn char_sort_has_owl_projection(sort: CharSort) -> bool {
    matches!(
        sort,
        CharSort::Transitive | CharSort::Symmetric | CharSort::Functional
    )
}

/// One DL-projectable characteristic sort asserted by a central `logic:` record, with
/// the provenance needed to raise a carrier-disagreement violation.
struct LogicCharacteristicRecord {
    /// The property the record characterises.
    property: String,
    /// The characteristic sort asserted.
    sort: CharSort,
    /// The sort marker IRI (`logic:transitiveProperty` …) — provenance object.
    sort_iri: String,
    /// The record IRI (`?rec`) — provenance subject.
    record: String,
    /// The named graph the record's `logic:characteristicSort` quad lives in.
    graph: String,
}

/// Split the characteristic declarations by carrier for the agreement check:
/// - `owl_sorts`: property → sorts declared by a direct `owl:{…}Property` type marker.
/// - `logic_records`: one entry per `logic:` record sort, in deterministic order.
///
/// Unlike [`collect_characteristics`] (which unions the two carriers), this keeps them
/// separate so the agreement pass can detect a canonical record whose OWL projection is
/// missing.
fn collect_characteristic_carriers(
    quads: &[FoundationQuad],
) -> (
    BTreeMap<String, BTreeSet<CharSort>>,
    Vec<LogicCharacteristicRecord>,
) {
    let mut owl_sorts: BTreeMap<String, BTreeSet<CharSort>> = BTreeMap::new();
    let mut rec_prop: HashMap<String, String> = HashMap::new();
    let mut rec_sorts: HashMap<String, Vec<(CharSort, String, String)>> = HashMap::new();
    for q in quads {
        let Some(obj) = strip_angle_opt(&q.object) else {
            continue;
        };
        if q.predicate == RDF_TYPE {
            if let Some(sort) = char_sort_of(obj) {
                owl_sorts.entry(q.subject.clone()).or_default().insert(sort);
            }
        } else if q.predicate == LOGIC_CHARACTERIZES {
            rec_prop.insert(q.subject.clone(), obj.to_owned());
        } else if q.predicate == LOGIC_CHARACTERISTIC_SORT
            && let Some(sort) = char_sort_of(obj)
        {
            rec_sorts.entry(q.subject.clone()).or_default().push((
                sort,
                obj.to_owned(),
                q.graph.clone(),
            ));
        }
    }
    let mut logic_records: Vec<LogicCharacteristicRecord> = Vec::new();
    for (rec, prop) in &rec_prop {
        if let Some(sorts) = rec_sorts.get(rec) {
            for (sort, sort_iri, graph) in sorts {
                logic_records.push(LogicCharacteristicRecord {
                    property: prop.clone(),
                    sort: *sort,
                    sort_iri: sort_iri.clone(),
                    record: rec.clone(),
                    graph: graph.clone(),
                });
            }
        }
    }
    // The joins above iterate `HashMap`s, so sort into a canonical order before the
    // agreement pass consumes them — output must be byte-stable across runs.
    logic_records.sort_by(|a, b| {
        (&a.property, a.sort, &a.sort_iri, &a.record, &a.graph).cmp(&(
            &b.property,
            b.sort,
            &b.sort_iri,
            &b.record,
            &b.graph,
        ))
    });
    (owl_sorts, logic_records)
}

/// Cross-check the two characteristic carriers for drift.  A central
/// `logic:PropertyCharacteristicAssertion` of a DL-projectable sort
/// (transitive/symmetric/functional) is the canonical characteristic; its OWL marker is
/// the lossy projection.  When the canonical record is present but its OWL projection is
/// missing, the carriers have drifted — raise `logic:violation
/// logic:CharacteristicCarrierDisagreement` on the property, keyed per (graph, property).
/// Irreflexive/asymmetric sorts are `logic:`-only by design and are never cross-checked.
///
/// # Errors
///
/// Returns `Err` only for a provenance-recipe failure (an un-mintable reifier).
fn characteristic_carrier_agreement_pass(
    quads: &[FoundationQuad],
) -> gmeow_errors::Result<Vec<FoundationQuad>> {
    let (owl_sorts, logic_records) = collect_characteristic_carriers(quads);
    let mut emitted: BTreeSet<(String, String)> = BTreeSet::new();
    let mut out: Vec<FoundationQuad> = Vec::new();
    for rec in &logic_records {
        if !char_sort_has_owl_projection(rec.sort) {
            continue;
        }
        let owl_has = owl_sorts
            .get(&rec.property)
            .is_some_and(|s| s.contains(&rec.sort));
        if owl_has {
            continue;
        }
        if !emitted.insert((rec.graph.clone(), rec.property.clone())) {
            continue;
        }
        let sources = vec![triple_reifier(
            &rec.record,
            LOGIC_CHARACTERISTIC_SORT,
            &rec.sort_iri,
        )?];
        let source_refs: Vec<&str> = sources.iter().map(String::as_str).collect();
        let derivation_id = mint_derivation_id(CHAR_CARRIER_RULE_IRI, &source_refs);
        out.push(FoundationQuad {
            graph: rec.graph.clone(),
            subject: rec.property.clone(),
            predicate: format!("{LOGIC_NS}violation"),
            object: n3(CHARACTERISTIC_CARRIER_DISAGREEMENT),
            rule_iri: CHAR_CARRIER_RULE_IRI.to_owned(),
            source_quad_ids: sources,
            derivation_id,
        });
    }
    Ok(out)
}

/// Collect, per property IRI, the union of characteristic sorts declared for it —
/// from direct `rdf:type` markers (`?P a owl:TransitiveProperty` and the `logic:`
/// analogues) and from central records (`?rec logic:characterizes ?P`,
/// `?rec logic:characteristicSort ?sort`).  Characteristics are global (TBox), so the
/// union is taken across all worlds and applied per-world to that world's edges.
fn collect_characteristics(quads: &[FoundationQuad]) -> BTreeMap<String, BTreeSet<CharSort>> {
    let mut prop_sorts: BTreeMap<String, BTreeSet<CharSort>> = BTreeMap::new();

    // Direct type markers on the property itself.
    for q in quads {
        if q.predicate == RDF_TYPE
            && let Some(obj) = strip_angle_opt(&q.object)
            && let Some(sort) = char_sort_of(obj)
        {
            prop_sorts
                .entry(q.subject.clone())
                .or_default()
                .insert(sort);
        }
    }

    // Central records: join `characterizes` and `characteristicSort` on the record IRI.
    let mut rec_prop: HashMap<String, String> = HashMap::new();
    let mut rec_sorts: HashMap<String, Vec<CharSort>> = HashMap::new();
    for q in quads {
        if q.predicate == LOGIC_CHARACTERIZES {
            if let Some(obj) = strip_angle_opt(&q.object) {
                rec_prop.insert(q.subject.clone(), obj.to_owned());
            }
        } else if q.predicate == LOGIC_CHARACTERISTIC_SORT
            && let Some(obj) = strip_angle_opt(&q.object)
            && let Some(sort) = char_sort_of(obj)
        {
            rec_sorts.entry(q.subject.clone()).or_default().push(sort);
        }
    }
    for (rec, prop) in &rec_prop {
        if let Some(sorts) = rec_sorts.get(rec) {
            for sort in sorts {
                prop_sorts.entry(prop.clone()).or_default().insert(*sort);
            }
        }
    }

    prop_sorts
}

/// Per-world, per-property edge sets: world IRI → property IRI → `{(subject, object)}`.
type WorldPropEdges = BTreeMap<String, BTreeMap<String, BTreeSet<(String, String)>>>;

/// A first-derivation record for a derived pair: `(rule IRI, sorted source reifiers)`.
type Derivation = (&'static str, Vec<String>);

/// Enforce property characteristics over the materialized quads, per world.
///
/// For each property carrying a characteristic, this closes transitive edges and
/// mirrors symmetric edges (emitting only the derived edges not already
/// materialized — so it is idempotent with the in-chase `causalPartOf`/`overlaps`
/// rules), then raises `logic:violation logic:IrreflexivityViolation` /
/// `logic:AsymmetryViolation` for irreflexive/asymmetric properties that hold of a
/// self-pair or a mutual pair in the closed+mirrored relation.  An asymmetric
/// property is treated as irreflexive too (asymmetry entails irreflexivity).
///
/// Determinism: worlds, properties, and pairs are visited in sorted order and the
/// first derivation of each pair wins (matching the chase's first-wins provenance).
///
/// # Errors
///
/// Returns `Err` only for a provenance-recipe failure (an un-mintable reifier).
fn property_characteristic_pass(
    quads: &[FoundationQuad],
) -> gmeow_errors::Result<Vec<FoundationQuad>> {
    let prop_sorts = collect_characteristics(quads);
    if prop_sorts.is_empty() {
        return Ok(Vec::new());
    }

    // Existing materialized edges, for dedup of derived edges: (graph, s, p, o).
    let mut existing: HashSet<(String, String, String, String)> = HashSet::new();
    // Per-world, per-property base edge sets: world → property → {(s, o)}.
    let mut edges: WorldPropEdges = BTreeMap::new();
    for q in quads {
        if let Some(obj) = strip_angle_opt(&q.object) {
            existing.insert((
                q.graph.clone(),
                q.subject.clone(),
                q.predicate.clone(),
                obj.to_owned(),
            ));
            if prop_sorts.contains_key(&q.predicate) {
                edges
                    .entry(q.graph.clone())
                    .or_default()
                    .entry(q.predicate.clone())
                    .or_default()
                    .insert((q.subject.clone(), obj.to_owned()));
            }
        }
    }

    let mut out: Vec<FoundationQuad> = Vec::new();
    for (world, props) in &edges {
        for (prop, base) in props {
            let sorts = prop_sorts.get(prop).ok_or_else(|| {
                foundation_err(format!("missing characteristic sorts for property {prop}"))
            })?;
            let transitive = sorts.contains(&CharSort::Transitive);
            let symmetric = sorts.contains(&CharSort::Symmetric);
            let asymmetric = sorts.contains(&CharSort::Asymmetric);
            // Asymmetry entails irreflexivity, so a self-pair on an asymmetric
            // property is an irreflexivity violation as well.
            let irreflexive = asymmetric || sorts.contains(&CharSort::Irreflexive);

            // Close + mirror to a combined fixpoint, recording the first derivation of
            // each new pair (rule IRI + sorted source reifiers).
            let mut current: BTreeSet<(String, String)> = base.clone();
            let mut derived: BTreeMap<(String, String), Derivation> = BTreeMap::new();
            if transitive || symmetric {
                loop {
                    let snapshot: Vec<(String, String)> = current.iter().cloned().collect();
                    let mut round: Vec<((String, String), Derivation)> = Vec::new();
                    if symmetric {
                        for (s, o) in &snapshot {
                            if s == o {
                                continue;
                            }
                            let pair = (o.clone(), s.clone());
                            if current.contains(&pair) || derived.contains_key(&pair) {
                                continue;
                            }
                            let src = triple_reifier(s, prop, o)?;
                            round.push((pair, (CHAR_SYMMETRIC_RULE_IRI, vec![src])));
                        }
                    }
                    if transitive {
                        // Group edges by subject once per round so the closure is
                        // O(E·d) rather than O(E²): for each edge a→b, extend only over
                        // b's out-neighbours.  `snapshot` is sorted, so each adjacency
                        // list is already in sorted order and the derived pairs keep the
                        // same first-wins visitation order as a full nested scan.
                        let mut by_subject: HashMap<&str, Vec<&str>> = HashMap::new();
                        for (x, y) in &snapshot {
                            by_subject.entry(x.as_str()).or_default().push(y.as_str());
                        }
                        for (a, b) in &snapshot {
                            let Some(neighbours) = by_subject.get(b.as_str()) else {
                                continue;
                            };
                            for &c in neighbours {
                                let pair = (a.clone(), c.to_owned());
                                if current.contains(&pair) || derived.contains_key(&pair) {
                                    continue;
                                }
                                let mut sources =
                                    vec![triple_reifier(a, prop, b)?, triple_reifier(b, prop, c)?];
                                sources.sort();
                                round.push((pair, (CHAR_TRANSITIVE_RULE_IRI, sources)));
                            }
                        }
                    }
                    if round.is_empty() {
                        break;
                    }
                    // First-wins: the earliest justification of each pair is kept (a pair
                    // may be derived more than once within a round via distinct paths).
                    for (pair, just) in round {
                        if current.contains(&pair) {
                            continue;
                        }
                        if let std::collections::btree_map::Entry::Vacant(slot) =
                            derived.entry(pair.clone())
                        {
                            current.insert(pair);
                            slot.insert(just);
                        }
                    }
                }
            }

            // Emit derived edges not already materialized.
            for ((s, o), (rule, sources)) in &derived {
                if existing.contains(&(world.clone(), s.clone(), prop.clone(), o.clone())) {
                    continue;
                }
                let source_refs: Vec<&str> = sources.iter().map(String::as_str).collect();
                let derivation_id = mint_derivation_id(rule, &source_refs);
                out.push(FoundationQuad {
                    graph: world.clone(),
                    subject: s.clone(),
                    predicate: prop.clone(),
                    object: n3(o),
                    rule_iri: (*rule).to_owned(),
                    source_quad_ids: sources.clone(),
                    derivation_id,
                });
            }

            // Acyclicity: a node that reaches itself by following `prop` one or more
            // steps is a violation.  Reachability is computed INTERNALLY over the asserted
            // immediate edges and never emitted, so an acyclic-but-not-transitive property
            // such as gmeow:linkNext keeps its one-step semantics — no `prop+` closure edge
            // is materialised (design/LOGIC-VALIDATION.md; the risk-slice linkNext prose
            // states it is NOT transitive).  Full reachability (a visited set), not a depth
            // cap, so a cycle longer than any bound is still caught.
            if sorts.contains(&CharSort::Acyclic) {
                let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
                for (s, o) in base {
                    adj.entry(s.as_str()).or_default().push(o.as_str());
                }
                for (start, succs) in &adj {
                    // Find the first outgoing edge (successors are in sorted `base` order,
                    // so the choice is deterministic) whose target can reach `start` again.
                    // That `start -> witness` edge genuinely lies ON the cycle, so it is the
                    // correct provenance — unlike `start`'s lexicographically-smallest edge,
                    // which may branch to a dead end that never closes a cycle. A node fires
                    // iff some successor reaches it, so the violating-node set is unchanged.
                    let mut witness: Option<&str> = None;
                    for &succ in succs {
                        let mut seen: HashSet<&str> = HashSet::new();
                        let mut stack: Vec<&str> = vec![succ];
                        while let Some(n) = stack.pop() {
                            if n == *start {
                                witness = Some(succ);
                                break;
                            }
                            if seen.insert(n)
                                && let Some(next) = adj.get(n)
                            {
                                stack.extend(next.iter().copied());
                            }
                        }
                        if witness.is_some() {
                            break;
                        }
                    }
                    if let Some(witness) = witness {
                        let source = triple_reifier(start, prop, witness)?;
                        let derivation_id =
                            mint_derivation_id(CHAR_ACYCLIC_RULE_IRI, &[source.as_str()]);
                        out.push(FoundationQuad {
                            graph: world.clone(),
                            subject: (*start).to_owned(),
                            predicate: format!("{LOGIC_NS}violation"),
                            object: n3(ACYCLICITY_VIOLATION),
                            rule_iri: CHAR_ACYCLIC_RULE_IRI.to_owned(),
                            source_quad_ids: vec![source],
                            derivation_id,
                        });
                    }
                }
            }

            // Clash detection over the closed+mirrored relation.  Each violation is
            // keyed on `(subject, discipline)` (first witnessing pair wins) and carries
            // the reifier(s) of the offending edge(s) as its provenance sources.
            if !irreflexive && !asymmetric {
                continue;
            }
            use std::collections::btree_map::Entry;
            let mut violated: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
            for (a, b) in &current {
                if a == b {
                    if irreflexive {
                        let key = (a.clone(), IRREFLEXIVITY_VIOLATION.to_owned());
                        if let Entry::Vacant(slot) = violated.entry(key) {
                            slot.insert(vec![triple_reifier(a, prop, a)?]);
                        }
                    }
                } else if asymmetric && current.contains(&(b.clone(), a.clone())) {
                    let key = (a.clone(), ASYMMETRY_VIOLATION.to_owned());
                    if let Entry::Vacant(slot) = violated.entry(key) {
                        let mut sources =
                            vec![triple_reifier(a, prop, b)?, triple_reifier(b, prop, a)?];
                        sources.sort();
                        slot.insert(sources);
                    }
                }
            }
            for ((subject, discipline), sources) in violated {
                let source_refs: Vec<&str> = sources.iter().map(String::as_str).collect();
                let derivation_id = mint_derivation_id(CHAR_CLASH_RULE_IRI, &source_refs);
                out.push(FoundationQuad {
                    graph: world.clone(),
                    subject,
                    predicate: format!("{LOGIC_NS}violation"),
                    object: n3(&discipline),
                    rule_iri: CHAR_CLASH_RULE_IRI.to_owned(),
                    source_quad_ids: sources,
                    derivation_id,
                });
            }
        }
    }

    Ok(out)
}

/// Enforce relatum-distinctness assertions over the materialized quads, per world.
///
/// For each `logic:RelatumDistinctnessAssertion` naming a target class
/// (`logic:distinctnessTarget`) and exactly two roles (`logic:distinctnessRole`), this
/// raises `logic:violation logic:RelatumDistinctnessViolation` on any focus node of that
/// class whose two roles bind the **same** value — a coincident-value equality join, the
/// broken half of the mutual-inequality condition (OWL functionality would infer
/// `owl:sameAs` here, never a rejection; only this closed-world join rejects it).
///
/// Determinism: worlds, subjects, constraints, and values are visited in sorted order and
/// the first coincident value witnesses the violation.
///
/// # Errors
///
/// Returns `Err` only for a provenance-recipe failure (an un-mintable reifier).
fn relatum_distinctness_pass(
    quads: &[FoundationQuad],
) -> gmeow_errors::Result<Vec<FoundationQuad>> {
    // Collect the assertion records: record IRI → target class, record IRI → {roles}.
    // A record is enforced HERE only when its `distinctnessTarget` is present in this
    // fact set: partial projections (e.g. the relator-mediation fact set) carry the
    // `rdf:type` stereotype pun WITHOUT the target/role edges, and there is nothing to
    // enforce there. Whether a target-bearing record is well-formed is what this pass
    // validates; completeness of an authored record (target present at all) is the
    // projector's job over the full ontology.
    let mut rec_target: HashMap<String, String> = HashMap::new();
    let mut rec_roles: HashMap<String, BTreeSet<String>> = HashMap::new();
    for q in quads {
        let Some(obj) = strip_angle_opt(&q.object) else {
            continue;
        };
        if q.predicate == LOGIC_DISTINCTNESS_TARGET {
            rec_target.insert(q.subject.clone(), obj.to_owned());
        } else if q.predicate == LOGIC_DISTINCTNESS_ROLE {
            rec_roles
                .entry(q.subject.clone())
                .or_default()
                .insert(obj.to_owned());
        }
    }
    // One `(target, role1, role2)` constraint per record, roles ordered by IRI for a
    // stable emit. A record that names a target but NOT exactly two roles is a malformed
    // axiom and HARD-FAILS — mirroring the SHACL projector (constraint_shapes.rs), so the
    // native and projected halves of one axiom agree on malformed input rather than the
    // native side silently dropping it (no-optionality).
    let mut constraints: BTreeSet<(String, String, String)> = BTreeSet::new();
    for (rec, target) in &rec_target {
        let role_count = rec_roles.get(rec).map_or(0, BTreeSet::len);
        if role_count != 2 {
            return Err(foundation_err(format!(
                "relatum-distinctness assertion {rec} must name exactly two distinctnessRole values, found {role_count}"
            )));
        }
        let roles = &rec_roles[rec];
        let mut it = roles.iter();
        let r1 = it.next().expect("two roles").clone();
        let r2 = it.next().expect("two roles").clone();
        constraints.insert((target.clone(), r1, r2));
    }
    if constraints.is_empty() {
        return Ok(Vec::new());
    }

    // Index, per world: the type edges of each subject, and the values each relevant role
    // binds on each subject.
    let relevant_roles: BTreeSet<&str> = constraints
        .iter()
        .flat_map(|(_, r1, r2)| [r1.as_str(), r2.as_str()])
        .collect();
    let mut types: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
    let mut role_vals: BTreeMap<(String, String, String), BTreeSet<String>> = BTreeMap::new();
    for q in quads {
        let Some(obj) = strip_angle_opt(&q.object) else {
            continue;
        };
        if q.predicate == RDF_TYPE {
            types
                .entry(q.graph.clone())
                .or_default()
                .entry(q.subject.clone())
                .or_default()
                .insert(obj.to_owned());
        } else if relevant_roles.contains(q.predicate.as_str()) {
            role_vals
                .entry((q.graph.clone(), q.subject.clone(), q.predicate.clone()))
                .or_default()
                .insert(obj.to_owned());
        }
    }

    let empty: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<FoundationQuad> = Vec::new();
    for (world, subjects) in &types {
        for (subject, classes) in subjects {
            for (target, r1, r2) in &constraints {
                if !classes.contains(target) {
                    continue;
                }
                let v1 = role_vals
                    .get(&(world.clone(), subject.clone(), r1.clone()))
                    .unwrap_or(&empty);
                let v2 = role_vals
                    .get(&(world.clone(), subject.clone(), r2.clone()))
                    .unwrap_or(&empty);
                let Some(v) = v1.intersection(v2).next() else {
                    continue;
                };
                let mut sources = vec![
                    triple_reifier(subject, r1, v)?,
                    triple_reifier(subject, r2, v)?,
                ];
                sources.sort();
                let source_refs: Vec<&str> = sources.iter().map(String::as_str).collect();
                let derivation_id = mint_derivation_id(RELATUM_DISTINCTNESS_RULE_IRI, &source_refs);
                out.push(FoundationQuad {
                    graph: world.clone(),
                    subject: subject.clone(),
                    predicate: format!("{LOGIC_NS}violation"),
                    object: n3(RELATUM_DISTINCTNESS_VIOLATION),
                    rule_iri: RELATUM_DISTINCTNESS_RULE_IRI.to_owned(),
                    source_quad_ids: sources,
                    derivation_id,
                });
            }
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
) -> gmeow_errors::Result<Vec<FoundationQuad>> {
    // Chase every world with the single native engine
    // ([`crate::physical::materialize_native`]).  The foundation program is lowered
    // into the shared `EvalRule` IR and run against the whole store; the result
    // includes the asserted-EDB echo and every derived quad, with byte-identical
    // provenance.  All-IRI validation and the hard-fail contract live inside
    // `chase_all_worlds_physical`.
    let mut all: Vec<FoundationQuad> = chase_all_worlds_physical(store)?;

    // Cross-world post-passes operate over the union of all materialized quads.  Each
    // reads the chase result, so they are computed before any is folded back in.
    let rigidity = cross_world_rigidity_violations(&all)?;
    let obligations = anti_rigidity_obligations(&all, policy)?;
    let characteristics = property_characteristic_pass(&all)?;
    let carrier_agreement = characteristic_carrier_agreement_pass(&all)?;
    let distinctness = relatum_distinctness_pass(&all)?;
    all.extend(rigidity);
    all.extend(obligations);
    all.extend(characteristics);
    all.extend(carrier_agreement);
    all.extend(distinctness);

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
/// [`crate::derivation_graph::DerivationGraph`] (S6b).
///
/// This is the chase→derivation-graph wiring: it runs [`evaluate`] (which preserves
/// the per-world parallel chase and deterministic world/index-ordered fold) and
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
) -> gmeow_errors::Result<crate::derivation_graph::DerivationGraph> {
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
