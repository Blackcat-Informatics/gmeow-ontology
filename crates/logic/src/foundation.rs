// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native Rust evaluator for the OntoUML *foundation* disciplines (issue #636).
//!
//! This module is the byte-faithful Rust port of the Python *foundation oracle*
//! (`gmeow_tools.logic_foundation` + the `enable_naf` path of
//! `gmeow_tools.logic_materialize._chase_world`).  It lowers five OntoUML
//! structural disciplines into a small stratified Datalog program with
//! negation-as-failure and inequality guards, runs that program *per world* as a
//! semi-naive chase, and then applies two cross-world post-passes (positive
//! cross-world rigidity and the anti-rigidity witness policy).
//!
//! # Parity is the whole point
//!
//! The materialized quad *set* alone is not enough: the explanation goldens are
//! content-addressed by **derivation IRIs**, and a derivation IRI is
//! `mint_derivation_id(rule_iri, sorted(source_reifiers))`.  For a quad derivable
//! by more than one rule firing, the Python oracle records the **first** firing
//! under its evaluation order (first-wins dedup).  To reproduce the same
//! derivation IRIs by construction this evaluator mirrors that order exactly:
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

use std::collections::HashSet;

use oxigraph::model::{NamedNode, Term};

use crate::provenance::{mint_derivation_id, mint_reifier};
use crate::store::WorldStore;

// ── Namespace + vocabulary constants ───────────────────────────────────────────

/// The `logic:` vocabulary namespace — term IRIs are `LOGIC_NS + local`.
/// Matches `gmeow_tools.config.LOGIC_NAMESPACE`.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// The `rdf:type` predicate IRI (string form), matching `logic_foundation._RDF_TYPE`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Sentinel rule IRI stamped on asserted (input) quads (`logic:assert`).
const ASSERT_RULE_IRI: &str = "https://blackcatinformatics.ca/logic/assert";

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
}

impl FactStore {
    fn new() -> Self {
        Self {
            facts: Vec::new(),
            keys: HashSet::new(),
        }
    }

    /// Insert `fact` if its key is new; return `true` if it was inserted.
    fn insert(&mut self, fact: Fact) -> bool {
        let key = fact.key();
        if self.keys.contains(&key) {
            return false;
        }
        self.keys.insert(key);
        self.facts.push(fact);
        true
    }

    /// Whether a fact with this key exists.
    fn contains_key(&self, key: &(String, String, String)) -> bool {
        self.keys.contains(key)
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

/// Join all body atoms against the fact store (semi-naive).
///
/// Returns the candidate solutions whose positive body fully matches, whose NAF
/// literals all fail (no matching fact), and at least one of whose positive sources
/// is in `delta` (the semi-naive condition).  Mirrors `_join_body_atoms`.
fn join_body(
    rule: &Rule,
    store: &FactStore,
    delta: &HashSet<(String, String, String)>,
) -> Vec<Solution> {
    let positive: Vec<&Atom> = rule.body.iter().filter(|a| !a.negated).collect();
    let negated: Vec<&Atom> = rule.body.iter().filter(|a| a.negated).collect();

    let mut solutions: Vec<Solution> = vec![Solution {
        bindings: Vec::new(),
        source_keys: Vec::new(),
    }];

    for atom in positive {
        let mut next: Vec<Solution> = Vec::new();
        for sol in &solutions {
            for f in &store.facts {
                if let Some(mut merged) = match_atom(atom, f, sol) {
                    merged.source_keys.push(f.key());
                    next.push(merged);
                }
            }
        }
        solutions = next;
        if solutions.is_empty() {
            break;
        }
    }

    // NAF filter: drop any binding whose grounded negated atoms still match a fact.
    if !negated.is_empty() {
        solutions.retain(|sol| {
            !negated
                .iter()
                .any(|neg| negated_atom_satisfied(neg, sol, store))
        });
    }

    // Semi-naive filter: at least one source key must be in the delta.
    solutions
        .into_iter()
        .filter(|sol| sol.source_keys.iter().any(|k| delta.contains(k)))
        .collect()
}

// ── Per-world chase ──────────────────────────────────────────────────────────────

/// Run the stratified semi-naive chase in one world, producing asserted + derived
/// quads with full provenance.  Mirrors `_chase_world` with `enable_naf=True`.
fn chase_world(world_iri: &str, initial: &[Fact]) -> Result<Vec<FoundationQuad>, String> {
    let mut store = FactStore::new();
    for f in initial {
        store.insert(f.clone());
    }

    // Asserted quads: source = [self reifier], rule = logic:assert.
    let mut out: Vec<FoundationQuad> = Vec::with_capacity(initial.len());
    for f in initial {
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
            let mut new_delta: HashSet<(String, String, String)> = HashSet::new();

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
                        continue; // first-wins dedup
                    }

                    // Provenance: reifiers of the matched positive body facts.  All
                    // foundation terms are IRIs, so every source is included (the
                    // Python filter to URIRef subj/pred never drops any here).
                    let mut sources: Vec<String> = Vec::with_capacity(sol.source_keys.len());
                    for sk in &sol.source_keys {
                        // sk holds the bare (s, p, o) IRIs of a matched body fact.
                        sources.push(triple_reifier(&sk.0, &sk.1, &sk.2)?);
                    }
                    let src_refs: Vec<&str> = sources.iter().map(String::as_str).collect();
                    let deriv = mint_derivation_id(ANON_RULE_IRI, &src_refs);

                    store.insert(head.clone());
                    new_delta.insert(key);
                    derived.push(FoundationQuad {
                        graph: world_iri.to_owned(),
                        subject: head.subject,
                        predicate: head.predicate,
                        object: n3(&head.object),
                        rule_iri: ANON_RULE_IRI.to_owned(),
                        source_quad_ids: sources,
                        derivation_id: deriv,
                    });
                }
            }

            if new_delta.is_empty() {
                break; // fixpoint
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

    let mut all: Vec<FoundationQuad> = Vec::new();
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
        let world_quads = chase_world(world, &initial)?;
        all.extend(world_quads);
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
