// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The **five correspondence conformance gates** (F4): the decidable checks that police a
//! `logic:Correspondence` and its derived `put` leg.  The compiler-side gate functions do
//! not own an executor; they consume the total, per-correspondence behavioural verdict map
//! produced by `gmeow_logic::correspondence_exec`.  Recovery is therefore executed once at
//! the runtime seam and checked here without a second structural approximation.
//!
//! | Gate | What it refuses |
//! |---|---|
//! | **Law** | an `ObligationDischarged` law whose witness does not pass (must degrade to `ObligationUnknown`), or a shipped `ObligationViolated` law |
//! | **Overclaim** | a bridge view claiming equivalence; an equivalence claimed on a non-injective rung (Principle 5) |
//! | **Round-trip** | an `iso` / `section` claim whose executed complete source case is not recovered (`put ∘ get ≠ id`) |
//! | **Mnemomorphism** | a declared-recoverable cell whose executed witness loses source atoms, lacks an executable complete case, or sits on a non-injective rung |
//! | **Composition** | a composite whose rung is STRONGER than the lattice-join of its parts (composition may only weaken) |
//!
//! [`evaluate_gates`] produces a structured, serializable [`CorrespondenceGateReport`]
//! (the conformance golden). [`assert_gates`] turns the first RED into an
//! [`OverclaimError`] — the production / pipeline hard-fail. The two are split on purpose:
//! `evaluate_gates` is **total** (it RECORDS a RED), so a deliberately-RED conformance case
//! can be blessed; only the pipeline stage throws.

use std::collections::BTreeMap;

use crate::ir::{Correspondence, DischargeVerdict, MorphismClass, PreservationKind};

use super::OverclaimError;
use super::correspondence::CorrespondenceProgram;
use super::correspondence_gate::assert_relation_no_overclaim;

/// The per-correspondence **executed lens-law verdict** map the gates read, keyed by
/// correspondence IRI. Each verdict is the behavioural section-law outcome (`put ∘ get =
/// id_S`) an engine-adjacent producer computed by RUNNING both legs (never a syntactic path
/// compare); the gates below consume this map instead of re-deriving an inversion. See
/// `gmeow_logic::correspondence_exec` for the single executor.
pub type CorrespondenceVerdicts = BTreeMap<String, DischargeVerdict>;

/// Read the supplied executed verdict for a correspondence.
///
/// This is an **internal invariant**, not a public entry point: it panics when a correspondence
/// the gates evaluate carries no supplied verdict. The invariant holds by construction — every
/// caller reaches [`evaluate_gates`] through [`crate::projections::compile_program`], which
/// derives its correspondence program with the SAME assembly the executed-verdict producer
/// uses (`gmeow_logic::correspondence_exec::logic_program_verdicts`), and that producer emits
/// one entry per correspondence keyed by the identical IRI. So the map is total over the
/// correspondences [`evaluate_gates`] iterates, and this lookup can never miss on any valid
/// correspondence-bearing source. The panic remains as a HARD FAIL guarding an internal wiring
/// regression (a future caller that hand-builds a partial map) — never reachable from public
/// input, and never a silent default/pass.
fn verdict_for(verdicts: &CorrespondenceVerdicts, iri: &str) -> DischargeVerdict {
    *verdicts.get(iri).unwrap_or_else(|| {
        panic!(
            "correspondence gate invariant: <{iri}> has no supplied executed lens-law verdict; \
             every correspondence in the program must be discharged before the gates run \
             (no default, no silent pass)"
        )
    })
}

/// The fixed-template clause naming WHY a non-`ObligationDischarged` executed verdict fails a
/// full-round-trip / recovery gate (golden-stable; no free text). `ObligationDischarged`
/// never reaches here (the caller passes only a failing verdict).
fn refutation_clause(verdict: DischargeVerdict) -> &'static str {
    match verdict {
        DischargeVerdict::ObligationViolated => {
            "the executed put ∘ get section-law discharge refutes it (put ∘ get ≠ id; \
             ObligationViolated)"
        }
        DischargeVerdict::ObligationUnknown => {
            "the executed put ∘ get section-law discharge could not verify it (no executable \
             complete recovery case or atomic leg pair; ObligationUnknown)"
        }
        DischargeVerdict::ObligationDischarged => {
            "the executed put ∘ get section-law discharge \
             holds"
        }
    }
}

const SKOS_EXACT_MATCH: &str = "http://www.w3.org/2004/02/skos/core#exactMatch";

/// The verdict of one gate on one correspondence (or composition). Serialized as a tagged
/// object (`{"status":"pass"}` / `{"status":"red","reason":…}` /
/// `{"status":"not_applicable","reason":…}`) so the golden is stable canonical JSON; RED
/// reasons use fixed templates (no free text) for golden determinism.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GateVerdict {
    /// The gate is satisfied.
    Pass,
    /// The gate is violated — the build-failing case.
    Red {
        /// The fixed-template reason (golden content).
        reason: String,
    },
    /// The gate does not apply to this cell (per-construct scoping, like the CL gate).
    NotApplicable {
        /// Why the gate does not apply.
        reason: String,
    },
}

impl GateVerdict {
    /// Whether this verdict is a RED (a build failure under [`assert_gates`]).
    pub fn is_red(&self) -> bool {
        matches!(self, GateVerdict::Red { .. })
    }
}

/// The five-gate verdict for one correspondence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GateReport {
    /// The correspondence IRI.
    pub correspondence: String,
    /// The Law gate verdict.
    pub law: GateVerdict,
    /// The Overclaim gate verdict.
    pub overclaim: GateVerdict,
    /// The Round-trip gate verdict.
    pub round_trip: GateVerdict,
    /// The Mnemomorphism gate verdict.
    pub mnemomorphism: GateVerdict,
    /// The Preservation-consistency gate verdict.
    pub preservation: GateVerdict,
}

/// The Composition gate verdict for one declared composition `left ∘ right` (with an
/// optional declared composite cell whose claimed rung is checked against the join).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CompositionGateReport {
    /// The left correspondence IRI.
    pub left: String,
    /// The right correspondence IRI.
    pub right: String,
    /// The computed lattice-join rung (the weakest rung the composite may claim).
    pub composed_class: String,
    /// The composed law status by weakest-dominates: the weaker of the two parts' aggregate
    /// discharge verdicts (the strongest law the composite may claim).
    pub composed_law_status: String,
    /// The Composition gate verdict.
    pub composition: GateVerdict,
}

/// The full correspondence gate report: the conformance golden artifact.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CorrespondenceGateReport {
    /// Per-correspondence five-gate verdicts, sorted by correspondence IRI.
    pub per_correspondence: Vec<GateReport>,
    /// Per-composition verdicts, sorted by (left, right).
    pub per_composition: Vec<CompositionGateReport>,
}

/// Whether the rung is injective enough to carry a full round-trip / section claim
/// (the top three rungs). Delegates to [`MorphismClass::is_injective_rung`], the single
/// source of truth — an explicit `matches!`, never the fragile derived-`Ord` comparison.
fn is_injective_rung(class: MorphismClass) -> bool {
    class.is_injective_rung()
}

/// The **Law gate**: a discharged law must have a passing witness; a violated law may not
/// ship. An authored-but-unverified (`ObligationUnknown`) law is honest and passes. The
/// witness is the EXECUTED section-law verdict threaded in `verdicts` — a claim of
/// `ObligationDischarged` whose executed verdict does not hold is refused.
fn law_gate(verdicts: &CorrespondenceVerdicts, c: &Correspondence) -> GateVerdict {
    for claim in &c.law_claims {
        match claim.verdict {
            DischargeVerdict::ObligationDischarged => {
                let executed = verdict_for(verdicts, &c.iri);
                if executed != DischargeVerdict::ObligationDischarged {
                    return GateVerdict::Red {
                        reason: format!(
                            "claims logic:{} as ObligationDischarged but {}; the claim must \
                             degrade to ObligationUnknown",
                            claim.law.as_str(),
                            refutation_clause(executed)
                        ),
                    };
                }
            }
            DischargeVerdict::ObligationViolated => {
                return GateVerdict::Red {
                    reason: format!(
                        "ships logic:{} with an ObligationViolated verdict; a correspondence may \
                         not claim a law it fails",
                        claim.law.as_str()
                    ),
                };
            }
            DischargeVerdict::ObligationUnknown => {}
        }
    }
    GateVerdict::Pass
}

/// The **Overclaim gate**: equivalence may be claimed only by a satisfaction-preserving
/// true equivalence on an injective rung. A bridge view claiming equivalence, or an
/// equivalence claimed on a non-injective / partial / co-projection rung, is a build
/// failure (Principle 5 — bridge by reference, never a sameAs collapse).
fn overclaim_gate(c: &Correspondence) -> GateVerdict {
    if c.relation != crate::ir::CorrespondenceRelation::Equiv {
        // A weaker relation surfaces only skos:relatedMatch — sound by construction.
        return GateVerdict::Pass;
    }
    // Delegate the bridge / relation-strength check to the shared overclaim gate.
    if let Err(e) = assert_relation_no_overclaim(
        "correspondence",
        c.relation,
        c.morphism_class,
        c.morphism_kind,
        SKOS_EXACT_MATCH,
    ) {
        return GateVerdict::Red { reason: e.0 };
    }
    // Rung-strength: equivalence (full round-trip) is satisfiable only on an injective
    // rung; on a lossy / partial / co-projection rung the sound relation is Overlaps.
    if matches!(
        c.morphism_class,
        MorphismClass::LossyLens | MorphismClass::Prism | MorphismClass::AffineCorrespondence
    ) {
        return GateVerdict::Red {
            reason: format!(
                "declares logic:Equiv but its rung is logic:{} (non-injective / partial / \
                 co-projection); equivalence overclaims the lowered legs — the sound relation \
                 is logic:Overlaps (skos:relatedMatch)",
                c.morphism_class.as_str()
            ),
        };
    }
    GateVerdict::Pass
}

/// The **Round-trip gate** (iso / section only): the cell must recover the source under the
/// EXECUTED section-law discharge (`put ∘ get = id`), so its threaded verdict is
/// `ObligationDischarged`. Other rungs make no full round-trip claim (per-construct scoping).
fn round_trip_gate(verdicts: &CorrespondenceVerdicts, c: &Correspondence) -> GateVerdict {
    match c.morphism_class {
        MorphismClass::Isomorphism | MorphismClass::SectionRetraction => {
            let executed = verdict_for(verdicts, &c.iri);
            if executed == DischargeVerdict::ObligationDischarged {
                GateVerdict::Pass
            } else {
                GateVerdict::Red {
                    reason: format!(
                        "logic:{} claims a full round-trip but {}",
                        c.morphism_class.as_str(),
                        refutation_clause(executed)
                    ),
                }
            }
        }
        other => GateVerdict::NotApplicable {
            reason: format!("logic:{} makes no full round-trip claim", other.as_str()),
        },
    }
}

/// The **Mnemomorphism gate**: a declared-recoverable cell's retained witness must
/// actually recover the source — evidenced by the EXECUTED recovery `put` leg. A witness
/// declared on a non-injective rung cannot recover `S ∖ im(get)` and is a build failure.
fn mnemomorphism_gate(verdicts: &CorrespondenceVerdicts, c: &Correspondence) -> GateVerdict {
    if !c.mnemomorphic {
        return GateVerdict::NotApplicable {
            reason: "the cell is not declared mnemomorphic".to_owned(),
        };
    }
    if !is_injective_rung(c.morphism_class) {
        return GateVerdict::Red {
            reason: format!(
                "declared mnemomorphic but its rung is logic:{} (non-injective); a retained \
                 witness cannot recover S ∖ im(get)",
                c.morphism_class.as_str()
            ),
        };
    }
    // The witness recovers the source iff the executed put ∘ get discharge holds. A
    // `WellBehavedLens` makes no full round-trip claim but, when declared mnemomorphic, must
    // still carry a recovering inverse leg — so the recovery evidence is the same verdict.
    let executed = verdict_for(verdicts, &c.iri);
    if executed == DischargeVerdict::ObligationDischarged {
        GateVerdict::Pass
    } else {
        GateVerdict::Red {
            reason: format!(
                "declared mnemomorphic but the witness does not recover the source: {}",
                refutation_clause(executed)
            ),
        }
    }
}

/// The **Preservation-consistency gate**: a correspondence on a non-injective rung
/// (`logic:LossyLens` / `logic:Prism` / `logic:AffineCorrespondence` — the SAME set the
/// overclaim gate keys on) MUST NOT declare `logic:ExactPreservation`. A many-to-one,
/// non-invertible `get` cannot be exactly preserving (the round-trip is undecidable in the
/// lossy direction), so an `Exact` claim on such a rung overclaims its own preservation
/// judgment — a build failure. An injective / bridge rung is unconstrained here
/// (`NotApplicable`); a non-injective rung authoring no rung, or any non-`Exact` kind
/// (e.g. `corrSzsToVerdict`'s `SoundUnderApproximation`), passes.
fn preservation_gate(c: &Correspondence) -> GateVerdict {
    let non_injective = matches!(
        c.morphism_class,
        MorphismClass::LossyLens | MorphismClass::Prism | MorphismClass::AffineCorrespondence
    );
    if !non_injective {
        return GateVerdict::NotApplicable {
            reason: format!(
                "logic:{} is not a non-injective rung; ExactPreservation is not constrained \
                 by the non-invertibility rule",
                c.morphism_class.as_str()
            ),
        };
    }
    match c.preservation {
        Some(PreservationKind::Exact) => GateVerdict::Red {
            reason: format!(
                "declares logic:ExactPreservation on the non-injective rung logic:{}; a \
                 many-to-one non-invertible get cannot be exactly preserving — the sound \
                 preservation is logic:SoundUnderApproximation",
                c.morphism_class.as_str()
            ),
        },
        _ => GateVerdict::Pass,
    }
}

/// The **Composition gate** (take1 §8.1): a sequential composite may only *weaken* the
/// rung. The lattice-join is the weaker of the two parts' rungs (the `Ord` is
/// strongest-first, so the join is the MAX). A declared composite stronger than the join
/// is a build failure.
fn composition_gate(
    by_iri: &BTreeMap<&str, &Correspondence>,
    left: &str,
    right: &str,
    composite: Option<&str>,
) -> CompositionGateReport {
    let lookup = |iri: &str| by_iri.get(iri).copied();
    let (Some(l), Some(r)) = (lookup(left), lookup(right)) else {
        return CompositionGateReport {
            left: left.to_owned(),
            right: right.to_owned(),
            composed_class: String::new(),
            composed_law_status: String::new(),
            composition: GateVerdict::Red {
                reason: "composition references a correspondence not present in the program"
                    .to_owned(),
            },
        };
    };
    // Join = the WEAKER rung (max under the strongest-first Ord): composition weakens.
    // NOTE: this is a LEGITIMATE lattice-join use of the spine `Ord` — the rung order IS
    // the weakening lattice. It is NOT a rung-membership test; those use the explicit
    // `is_injective_rung` predicate. Do not "fix" this `max`/`<` into a `matches!`.
    let join = l.morphism_class.max(r.morphism_class);
    // Law-status by weakest-dominates: the composite may claim no stronger discharge verdict
    // than the WEAKER of its parts'. The rung-class join already carries the loss ordering
    // (a weaker rung is the more-lossy one — the spine IS the unsupported-construct lattice),
    // so the loss dimension is enforced by the class check above; this adds the orthogonal
    // law-status dimension (LOGIC-CONFORMANCE.md § Composition gate).
    let join_status = weaker_law_status(aggregate_law_status(l), aggregate_law_status(r));
    let verdict = match composite {
        Some(comp_iri) => match lookup(comp_iri) {
            Some(comp) => {
                if comp.morphism_class < join {
                    GateVerdict::Red {
                        reason: format!(
                            "composite rung logic:{} is STRONGER than the lattice-join logic:{} \
                             of its parts (logic:{} ∘ logic:{}); composition may only weaken the \
                             rung, never strengthen it",
                            comp.morphism_class.as_str(),
                            join.as_str(),
                            l.morphism_class.as_str(),
                            r.morphism_class.as_str(),
                        ),
                    }
                } else if law_status_strength(aggregate_law_status(comp))
                    > law_status_strength(join_status)
                {
                    GateVerdict::Red {
                        reason: format!(
                            "composite law status logic:{} is STRONGER than the weakest-dominates \
                             join logic:{} of its parts; a composite may not discharge a law its \
                             parts leave unverified or violated",
                            aggregate_law_status(comp).as_str(),
                            join_status.as_str(),
                        ),
                    }
                } else {
                    GateVerdict::Pass
                }
            }
            None => GateVerdict::Red {
                reason: "declared composite is not present in the program".to_owned(),
            },
        },
        // No composite declared: the gate reports the computed join (informational pass).
        None => GateVerdict::Pass,
    };
    CompositionGateReport {
        left: left.to_owned(),
        right: right.to_owned(),
        composed_class: join.as_str().to_owned(),
        composed_law_status: join_status.as_str().to_owned(),
        composition: verdict,
    }
}

/// The composition strength of a discharge verdict (EXPLICIT, never the derived `Ord` on
/// [`DischargeVerdict`] — whose declaration order is Discharged/Unknown/Violated, the
/// REVERSE of strength, so a derived comparison would invert the law). Violated is weakest,
/// Discharged strongest.
fn law_status_strength(v: DischargeVerdict) -> u8 {
    match v {
        DischargeVerdict::ObligationViolated => 0,
        DischargeVerdict::ObligationUnknown => 1,
        DischargeVerdict::ObligationDischarged => 2,
    }
}

/// The weaker of two law statuses (weakest-dominates).
fn weaker_law_status(a: DischargeVerdict, b: DischargeVerdict) -> DischargeVerdict {
    if law_status_strength(a) <= law_status_strength(b) {
        a
    } else {
        b
    }
}

/// A correspondence's aggregate law status: the WEAKEST discharge verdict among its law
/// claims (the weakest claim caps the cell). A cell with no law claims asserts no discharged
/// law, so its status is `ObligationUnknown` — it cannot license a composite's discharged law.
fn aggregate_law_status(c: &Correspondence) -> DischargeVerdict {
    c.law_claims
        .iter()
        .map(|cl| cl.verdict)
        .min_by_key(|v| law_status_strength(*v))
        .unwrap_or(DischargeVerdict::ObligationUnknown)
}

/// Run the five gates over a (derived) correspondence program plus any declared
/// compositions (`(left, right, optional composite)`), producing the structured report.
/// Total — never throws; a RED is recorded, not raised (so a deliberately-RED conformance
/// case can be blessed). The program's correspondences are expected to already carry their
/// derived put legs (see [`CorrespondenceProgram::with_derived_puts`]).
pub fn evaluate_gates(
    program: &CorrespondenceProgram,
    compositions: &[(String, String, Option<String>)],
    verdicts: &CorrespondenceVerdicts,
) -> CorrespondenceGateReport {
    let mut per_correspondence: Vec<GateReport> = program
        .correspondences
        .iter()
        .map(|c| GateReport {
            correspondence: c.iri.clone(),
            law: law_gate(verdicts, c),
            overclaim: overclaim_gate(c),
            round_trip: round_trip_gate(verdicts, c),
            mnemomorphism: mnemomorphism_gate(verdicts, c),
            preservation: preservation_gate(c),
        })
        .collect();
    per_correspondence.sort_by(|a, b| a.correspondence.cmp(&b.correspondence));

    let by_iri: BTreeMap<&str, &Correspondence> = program
        .correspondences
        .iter()
        .map(|c| (c.iri.as_str(), c))
        .collect();
    let mut per_composition: Vec<CompositionGateReport> = compositions
        .iter()
        .map(|(left, right, composite)| {
            composition_gate(&by_iri, left, right, composite.as_deref())
        })
        .collect();
    per_composition.sort_by(|a, b| (a.left.as_str(), a.right.as_str()).cmp(&(&b.left, &b.right)));

    CorrespondenceGateReport {
        per_correspondence,
        per_composition,
    }
}

/// Turn the first RED verdict in a gate report into an [`OverclaimError`] — the
/// production / pipeline hard-fail. `Ok(())` when every gate passes or is not applicable.
pub fn assert_gates(report: &CorrespondenceGateReport) -> Result<(), OverclaimError> {
    for r in &report.per_correspondence {
        for (gate, verdict) in [
            ("Law", &r.law),
            ("Overclaim", &r.overclaim),
            ("Round-trip", &r.round_trip),
            ("Mnemomorphism", &r.mnemomorphism),
            ("Preservation", &r.preservation),
        ] {
            if let GateVerdict::Red { reason } = verdict {
                return Err(OverclaimError(format!(
                    "Correspondence {gate} gate RED on <{}>: {reason}",
                    r.correspondence
                )));
            }
        }
    }
    for c in &report.per_composition {
        if let GateVerdict::Red { reason } = &c.composition {
            return Err(OverclaimError(format!(
                "Correspondence Composition gate RED on <{}> ∘ <{}>: {reason}",
                c.left, c.right
            )));
        }
    }
    Ok(())
}

/// The derived liftability statistic (criterion #4): `lawful / total` over the gate
/// verdicts — the honest replacement for the old SSSOM-heuristic "81% liftable" headline.
/// A cell is *lawful* when its Round-trip OR Mnemomorphism gate PASSES (a recoverable
/// up-lift). Computed over the gate report, never from a heuristic re-read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct LiftabilityLedger {
    /// The count of correspondences with a lawful (recoverable) up-lift.
    pub lawful: usize,
    /// The total count of correspondences.
    pub total: usize,
}

/// Compute the [`LiftabilityLedger`] over a gate report.
pub fn liftability(report: &CorrespondenceGateReport) -> LiftabilityLedger {
    let lawful = report
        .per_correspondence
        .iter()
        .filter(|r| {
            matches!(r.round_trip, GateVerdict::Pass)
                || matches!(r.mnemomorphism, GateVerdict::Pass)
        })
        .count();
    LiftabilityLedger {
        lawful,
        total: report.per_correspondence.len(),
    }
}

#[cfg(test)]
mod tests;
