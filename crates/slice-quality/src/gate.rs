// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The opt-in slice-quality tier ratchet.
//!
//! A slice opts in by declaring `gmeow:sliceQualityTier` in its manifest (the sole
//! tier truth, Principle 16). The gate then enforces two things, both pure lattice
//! comparisons:
//! - **measured ≥ declared** — the slice must currently hold the tier it promises;
//! - **declared ≥ committed floor** — the declaration is a ratchet: it may only be
//!   raised, checked against a committed floor artifact so lowering is detectable
//!   without git archaeology.
//!
//! An undeclared slice is purely advisory — it never fails the gate.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::axes;
use crate::graph::{self, id, instances_of, one_iri};
use crate::model::{Axis, AxisFloorCommitment, Rubric, Tier};

/// The pipeline projection surfaces the rubric must account for — each must be
/// covered by a landed quality axis OR by a dated `gmeow:AxisExemption`. Adding a
/// new pipeline projection target means adding it here (this is one of the
/// enumerated projection-target-add sites) with either an axis that measures it or
/// a dated exemption, so the quality rubric can never silently fall behind the
/// pipeline it measures. `covered_by_axis == true` means a landed axis measures the
/// surface; `false` means it must carry an exemption keyed by the producer symbol.
pub const PROJECTION_SURFACES: &[(&str, bool)] = &[
    ("shacl", true),
    ("shex", true),
    ("sssom", true),
    ("edoal", true),
    ("fno", true),
    ("docs-pages", true),
    ("loss-ledger", true),
    ("gmn", false),
    ("doc-maturity", true),
    ("docs-panels", false),
];

/// The producer symbol each not-yet-landed projection surface's exemption must name.
fn exemption_producer_for(surface: &str) -> Option<&'static str> {
    match surface {
        "gmn" => Some("GmnProjectionTarget"),
        "doc-maturity" => Some("DocMaturity"),
        "docs-panels" => Some("DocMaturityPanels"),
        _ => None,
    }
}

/// Axis→producer AST-binding gate. Two independent proofs, both of which must hold:
///
/// 1. **Bijection with the kernel's closed set** — the rubric's `gmeow:axisProducer`
///    strings and [`axes::IMPLEMENTED`] must be in bijection. A renamed/removed
///    rubric producer becomes an unbound axis; a renamed/removed entry in the closed
///    set becomes an orphan.
/// 2. **Real symbol resolution** — every rubric producer must additionally `resolves`
///    to an actual Rust *item* definition (the caller passes the constitution-gate
///    AST resolver over the crate source, so this is a real `fn`/item lookup, not a
///    substring or list-membership test). This catches the drift the bijection alone
///    cannot: a producer that survives in the hand-kept `IMPLEMENTED` list but whose
///    backing primitive `fn` in `axes.rs` (or `reasoner.rs`) is gone or renamed reds
///    here instead of passing. A producer that is a strict *prefix* of a real item
///    (e.g. `grounding_ax` vs `grounding_axis`) does NOT resolve — the resolver is
///    identifier-boundary-correct.
///
/// Any of the three conditions reds.
#[must_use]
pub fn binding_gate(rubric: &Rubric, resolves: impl Fn(&str) -> bool) -> Vec<String> {
    let implemented: BTreeSet<&str> = axes::IMPLEMENTED.iter().copied().collect();
    let bound: BTreeSet<&str> = rubric
        .standard
        .axes
        .iter()
        .map(|a| a.producer.as_str())
        .collect();
    let mut errs = Vec::new();
    for axis in &rubric.standard.axes {
        let producer = axis.producer.as_str();
        if !implemented.contains(producer) {
            errs.push(format!(
                "axis {} names producer '{}' with no implemented primitive (stale binding)",
                axis.iri, producer
            ));
        }
        if !resolves(producer) {
            errs.push(format!(
                "axis {} names producer '{producer}' that resolves to no Rust primitive item in the crate source (unbound producer — the backing fn is missing or renamed)",
                axis.iri
            ));
        }
    }
    for imp in &implemented {
        if !bound.contains(imp) {
            errs.push(format!(
                "implemented primitive '{imp}' is bound by no rubric axis (orphan)"
            ));
        }
    }
    errs
}

/// Projection-target completeness gate: every enumerated projection surface maps to
/// a landed axis or a dated exemption, and every exemption is well-formed (names a
/// real axis, a reason, a date, and a producer). Reds on a surface with no covering
/// axis and no exemption, or a malformed exemption.
#[must_use]
pub fn completeness_gate(rubric: &Rubric) -> Vec<String> {
    let mut errs = Vec::new();
    let exemption_producers: BTreeSet<&str> = rubric
        .floors
        .exemptions
        .iter()
        .map(|e| e.producer.as_str())
        .collect();

    for (surface, covered_by_axis) in PROJECTION_SURFACES {
        if *covered_by_axis {
            continue; // a landed axis measures this surface
        }
        let Some(producer) = exemption_producer_for(surface) else {
            errs.push(format!("projection surface '{surface}' has no covering axis and no known exemption producer"));
            continue;
        };
        if !exemption_producers.contains(producer) {
            errs.push(format!(
                "projection surface '{surface}' is unlanded but carries no dated exemption (producer '{producer}')"
            ));
        }
    }
    for ex in &rubric.floors.exemptions {
        if ex.axis_iri.is_empty() {
            errs.push(format!("exemption {} names no axis", ex.iri));
        }
        if ex.reason.trim().is_empty() {
            errs.push(format!(
                "exemption {} has an empty/whitespace reason — a dated exemption must carry a doctrine-anchored justification",
                ex.iri
            ));
        }
        if ex.date.is_empty() {
            errs.push(format!("exemption {} is undated", ex.iri));
        }
        if ex.producer.is_empty() {
            errs.push(format!("exemption {} names no producer symbol", ex.iri));
        }
        if !rubric.standard.axes.iter().any(|a| a.iri == ex.axis_iri) {
            errs.push(format!(
                "exemption {} exempts unknown axis {}",
                ex.iri, ex.axis_iri
            ));
        }
    }
    errs
}

/// Exemption-staleness gate: an exemption whose producer symbol now RESOLVES in the
/// repo is stale — the producer has landed, so the exemption must be retired and the
/// axis built. `resolves` reports whether a symbol is defined in-repo.
#[must_use]
pub fn stale_exemptions(rubric: &Rubric, resolves: impl Fn(&str) -> bool) -> Vec<String> {
    rubric
        .floors
        .exemptions
        .iter()
        .filter(|e| resolves(&e.producer))
        .map(|e| {
            format!(
                "exemption {} is STALE: its producer '{}' now resolves in-repo — remove the exemption and build the axis",
                e.iri, e.producer
            )
        })
        .collect()
}

/// The verdict for one slice's ratchet check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatchetVerdict {
    /// Undeclared (advisory) or declared-and-holding — the gate passes.
    Pass,
    /// The measured roll-up tier is below the declared tier.
    MeasuredBelowDeclared,
    /// The declared tier is below the committed ratchet floor (a lowering).
    DeclaredBelowFloor,
}

impl RatchetVerdict {
    /// Whether this verdict fails the gate.
    #[must_use]
    pub fn is_failure(self) -> bool {
        !matches!(self, Self::Pass)
    }
}

/// Evaluate the ratchet for one slice from three tier ranks.
///
/// `declared_rank` is `None` when the slice has not opted in (advisory → pass).
/// `floor_rank` is `None` when the slice is absent from the committed floor file.
#[must_use]
pub fn evaluate_ratchet(
    declared_rank: Option<i64>,
    measured_rank: i64,
    floor_rank: Option<i64>,
) -> RatchetVerdict {
    let Some(declared) = declared_rank else {
        return RatchetVerdict::Pass; // undeclared → advisory, never gates
    };
    if let Some(floor) = floor_rank
        && declared < floor
    {
        return RatchetVerdict::DeclaredBelowFloor;
    }
    if measured_rank < declared {
        return RatchetVerdict::MeasuredBelowDeclared;
    }
    RatchetVerdict::Pass
}

/// The verdict for one slice's PER-AXIS committed-floor check — distinct from
/// and additional to [`RatchetVerdict`]'s roll-up-tier ratchet. A
/// per-axis floor gates one axis's raw MEASURED score directly (never a tier), so a
/// grounding slice cannot clear the gate on `axisGmn1Coverage < 1.0` regardless of
/// its other axes' scores or its own roll-up tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisRatchetVerdict {
    /// The measured score meets or exceeds the committed floor.
    Pass,
    /// The measured score has fallen below the committed floor (a regression).
    MeasuredBelowFloor,
}

impl AxisRatchetVerdict {
    /// Whether this verdict fails the gate.
    #[must_use]
    pub fn is_failure(self) -> bool {
        !matches!(self, Self::Pass)
    }
}

/// Evaluate one axis's committed-floor check: `Pass` iff `measured >= floor`, a pure
/// comparator with no new scoring path — the caller supplies both the measured
/// `gmeow:AxisGrade.score` and the floor resolved from `governance/
/// slice-quality-axis-floors.tsv` (defaulting to `1.0` for a grounding slice absent
/// from the file — see the caller in `gmeow-dev-cli`'s `slice_quality_gate`).
#[must_use]
pub fn evaluate_axis_floor(measured: f64, floor: f64) -> AxisRatchetVerdict {
    if measured + f64::EPSILON >= floor {
        AxisRatchetVerdict::Pass
    } else {
        AxisRatchetVerdict::MeasuredBelowFloor
    }
}

/// A parsed committed TIER-floor entry. The ladder `rank` drives the monotonic
/// comparison; the `local` tier name is retained verbatim so a violation message
/// can echo exactly what the floor file recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TierFloor {
    /// The tier's ladder rank (resolved through the rubric at parse time).
    pub rank: i64,
    /// The tier local name exactly as the floor file recorded it.
    pub local: String,
}

/// The outcome of a floor-monotonicity diff: hard `violations` that red the gate.
///
/// Floors are RAISE-ONLY. LOWERING a committed floor is a hard violation, and so is
/// deleting a still-live floor. There is deliberately NO in-repo mechanism to permit a
/// lowering: re-baselining a floor downward is a maintainer-only decision, authorized
/// out-of-band by the maintainer merging past the resulting red — never by any code
/// path, flag, record, doc, or signal a tool or agent could set. (Additions and
/// greenfield removals — the slice/axis is gone — are allowed.)
#[derive(Debug, Default)]
pub struct FloorMonotonicity {
    /// Hard violations (a committed floor LOWERED, or a still-live floor DELETED).
    /// Non-empty ⇒ the gate reds.
    pub violations: Vec<String>,
}

/// Floor-monotonicity check for the per-slice TIER floor file: diff the committed
/// floor at the merge base (`base`) against the working tree (`working`). Floors are
/// RAISE-ONLY: a lowering is a hard violation, and so is the *deletion* of a floor for a
/// slice that is still live (`live_slice` returns `true`). Only the maintainer may
/// re-baseline a floor downward, and only out-of-band by authorizing the merge past the
/// resulting red — there is no in-repo permit.
///
/// Rules (all pure, order-deterministic via the `BTreeMap` iteration):
/// - a `(slice)` present in BOTH maps: a raise/hold is silent; a lowering is a
///   hard violation;
/// - an **addition** (`working` only) is always allowed;
/// - a **deletion** (`base` only) is allowed ONLY when the slice is no longer live
///   (greenfield removal); deleting a still-live floor is a violation.
#[must_use]
pub fn tier_floor_monotonicity(
    file_label: &str,
    base: &BTreeMap<String, TierFloor>,
    working: &BTreeMap<String, TierFloor>,
    live_slice: impl Fn(&str) -> bool,
) -> FloorMonotonicity {
    let mut out = FloorMonotonicity::default();
    for (slice, before) in base {
        match working.get(slice) {
            Some(now) if now.rank < before.rank => out.violations.push(format!(
                "{file_label}: slice {slice} tier floor LOWERED {} → {} — floors are raise-only; a downward re-baseline is a maintainer-only decision authorized out-of-band (merging past this red), never by a tool",
                before.local, now.local
            )),
            Some(_) => {}
            None if live_slice(slice) => out.violations.push(format!(
                "{file_label}: slice {slice} tier floor {} DELETED while the slice is still live — a live floor may not be removed",
                before.local
            )),
            None => {}
        }
    }
    out
}

/// Floor-monotonicity check for the PER-AXIS floor file — the axis-level analogue
/// of [`tier_floor_monotonicity`]. A `(slice, axis)` lowering (under the SAME
/// `f64::EPSILON` tolerance [`evaluate_axis_floor`] uses) is a hard violation;
/// additions are allowed; a deletion is a violation only when the `(slice, axis)` is
/// still live (`live` returns `true` iff the slice still exists AND the axis is still a
/// rubric axis). Floors are raise-only; only the maintainer re-baselines a floor
/// downward, and only out-of-band by authorizing the merge past the red — there is no
/// in-repo permit. Pure; the caller feeds both parsed maps.
#[must_use]
pub fn axis_floor_monotonicity(
    file_label: &str,
    base: &BTreeMap<(String, String), f64>,
    working: &BTreeMap<(String, String), f64>,
    live: impl Fn(&str, &str) -> bool,
) -> FloorMonotonicity {
    let mut out = FloorMonotonicity::default();
    for ((slice, axis), before) in base {
        match working.get(&(slice.clone(), axis.clone())) {
            Some(now) if *now + f64::EPSILON < *before => out.violations.push(format!(
                "{file_label}: slice {slice} axis {axis} floor LOWERED {before:.6} → {now:.6} — floors are raise-only; a downward re-baseline is a maintainer-only decision authorized out-of-band (merging past this red), never by a tool"
            )),
            Some(_) => {}
            None if live(slice, axis) => out.violations.push(format!(
                "{file_label}: slice {slice} axis {axis} floor {before:.6} DELETED while still live — a live floor may not be removed"
            )),
            None => {}
        }
    }
    out
}

// -----------------------------------------------------------------------------
// The projection-vocabulary RATCHET — the inverse-polarity twin of the raise-only
// floor gate above. Three HARD-FAIL invariants, all pure comparisons over a
// `(slice IRI, vocab prefix) -> u64` residue/ceiling map:
//
// 1. **Count gate (working tree, [`evaluate_projection_ceiling`]):** for every
//    (slice, vocab) with `measured(working) > 0`,
//    `measured(working) <= effectiveCeiling(working)`, where `effectiveCeiling` is
//    the committed `gmeow:ceilingCount` if present, else that vocab's
//    `gmeow:vocabularyDefaultCeiling` (`0`) — so a slice's first UNGROUNDED use of
//    a previously-absent vocab reds immediately.
// 2. **Monotonicity (base∩working, [`projection_ceiling_monotonicity`]):** for
//    every (slice, vocab) with a committed ceiling in BOTH the merge-base and the
//    working tree, `ceilingCount(working) <= ceilingCount(base) + inflow` — a
//    RAISE beyond the relocation-adjusted base is a hard violation; a deletion
//    (base-only) is allowed because dropping a ceiling only ever tightens the
//    effective ceiling to the vocab default.
// 3. **Grandfather (new ceilings only, evaluated by the SAME
//    [`projection_ceiling_monotonicity`] under the SAME rule, fed the base
//    FILESET measurement by the `gmeow-dev` CLI): for every (slice, vocab) whose
//    committed ceiling is NEW in working (absent at base),
//    `ceilingCount(working) <= measured(base) + inflow`, where `measured(base)` is
//    reconstructed by feeding the SAME [`crate::counting::residue`] counter the
//    merge-base bytes over the SAME ratchet surface set
//    ([`crate::ratchet_surface_paths`]) — a surface absent at base contributes 0, a
//    surface present-but-unreadable at base is a HARD-FAIL (never silently 0).
//    This closes the "author N ungrounded constructs and commit an N-ceiling in the
//    same PR" loophole invariants 1-2 alone cannot see.
//
//    `inflow` is the transported residue a DECLARED-and-CORROBORATED
//    `gmeow:CeilingRelocation` moved INTO the cell. A ceiling budgets NET-NEW
//    UNGROUNDED AUTHORING, which is location-independent, so the base ceiling is
//    RE-PROJECTED through the relocation before the lower-only comparison runs —
//    the invariant itself never changes, and with no declarations `inflow` is
//    identically 0. No tool ever creates headroom.
// 4. **Conservation ([`ceiling_conservation`], base∩working):** per vocabulary,
//    `Σ working <= Σ base` over the cells committed on BOTH sides — the aggregate
//    backstop proving a relocation only ever MOVED budget.
//
// **Back-ref integrity** (binds invariant 1's `measured`): a construct is excluded
// from the residue as "grounded" ONLY if its `logic:formalizes`/`logic:grounds`
// back-ref RESOLVES to an existing `logic:` axiom — a dangling back-ref does not
// ground ([`crate::counting`]'s `CountMode::FullResidue`). A parse/read failure
// anywhere on this gate's path is a HARD-FAIL, never a silent fall-back to residue
// zero.
// -----------------------------------------------------------------------------

/// The verdict for one (slice, vocab) cell's projection-CEILING check — the
/// inverse-polarity twin of [`AxisRatchetVerdict`]: a ceiling is lower-only, so
/// this passes iff the measured residue does NOT exceed it (the opposite
/// direction of [`evaluate_axis_floor`], which passes iff measured meets or
/// exceeds a raise-only floor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CeilingVerdict {
    /// The measured residue is at or below the committed (or default) ceiling.
    Pass,
    /// The measured residue exceeds the ceiling — a hard violation.
    MeasuredAboveCeiling,
}

impl CeilingVerdict {
    /// Whether this verdict fails the gate.
    #[must_use]
    pub fn is_failure(self) -> bool {
        !matches!(self, Self::Pass)
    }
}

/// Evaluate one (slice, vocab) cell's projection-ceiling check (ratchet invariant
/// **1**, the count gate): `Pass` iff `measured <= ceiling` — ceilings are
/// lower-only, the exact inverse of [`evaluate_axis_floor`] (`measured >= floor`).
/// The caller resolves `ceiling` as the committed `gmeow:ceilingCount` if present,
/// else the vocab's `gmeow:vocabularyDefaultCeiling` (`0` for every guarded vocab
/// today), so an absent commitment for a nonzero-residue vocab always reds.
#[must_use]
pub fn evaluate_projection_ceiling(measured: u64, ceiling: u64) -> CeilingVerdict {
    if measured <= ceiling {
        CeilingVerdict::Pass
    } else {
        CeilingVerdict::MeasuredAboveCeiling
    }
}

/// One relocation transfer the gate ACCEPTED — a witnessed, declared, and paid-for
/// unit flow along a single `(from → to, vocab)` edge. Minted onto the diagnostics
/// ledger by the driver so the accepted adjustment is a joinable finding, never a
/// bare printed line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedTransfer {
    /// The guarded vocabulary prefix the transfer moved residue in.
    pub vocab: String,
    /// The source slice IRI whose committed ceiling paid for the transfer.
    pub from: String,
    /// The destination slice IRI whose base ceiling was re-projected upward by it.
    pub to: String,
    /// The number of residue units transferred along this edge.
    pub units: u64,
    /// The witnessed anchor term IRIs backing the transfer — the constructs that
    /// genuinely DEPARTED `from` and ARRIVED at `to`, in sorted order. These are the
    /// ledger antecedents of the accepted finding.
    pub witnesses: Vec<String>,
    /// The `gmeow:CeilingRelocation` declaration IRIs that authorized the edge.
    pub declarations: Vec<String>,
}

/// The outcome of the projection-ceiling REBALANCE diff: hard `violations` that red
/// the gate, plus the `accepted` relocation transfers whose adjustment was witnessed,
/// declared, and paid for.
///
/// This is not "the tool granted a raise". The base ceiling of every affected cell is
/// RE-PROJECTED through the declared-and-corroborated relocation, and the comparison
/// that then runs is the unchanged lower-only invariant
/// `working <= relocation_adjusted_base`. No tool ever creates headroom: every unit of
/// upward adjustment at a destination is matched by a unit of downward adjustment at a
/// source that really lost the construct.
#[derive(Debug, Default)]
pub struct CeilingRebalance {
    /// Hard violations — a committed ceiling above its relocation-adjusted base for a
    /// (slice, vocab) key, or a declaration that contradicts the derived witness.
    /// Non-empty ⇒ the gate reds.
    pub violations: Vec<String>,
    /// The accepted transfers, deterministically ordered by `(vocab, from, to)`.
    pub accepted: Vec<AcceptedTransfer>,
}

/// Per `(from slice, to slice, vocab prefix)` edge, the residue-conservation reasons
/// the relocation accounting derived from the two real datasets, keyed by the
/// relocation-invariant anchor IRI.
///
/// Residue is a function of `(dataset, surface_iri)`, not of the construct alone, so
/// moving a construct across a vocabulary's owner boundary — or away from the
/// `logic:Formula` that grounded it — creates or destroys residue with no authoring.
/// These reasons are COMPUTED from real measurements
/// ([`crate::counting::relocation_reasons`]), never inferred from a count delta.
pub type EdgeRelocationReasons = BTreeMap<
    (String, String, String),
    BTreeMap<String, BTreeSet<crate::counting::RelocationReason>>,
>;

/// The full input set the relocation-aware ceiling comparator reads. Grouped into one
/// borrowed struct because the comparator joins SIX independent maps and a parameter
/// list of that width is unreadable (and trips `clippy::too_many_arguments`).
///
/// Every field is supplied by the `gmeow-dev` ratchet-gate driver, which owns the
/// measurement side (it materializes the merge-base tree and runs the SAME residue
/// counter over both sides). The comparator itself is pure and order-deterministic.
pub struct CeilingComparison<'a> {
    /// The human-facing label prefixing every violation message.
    pub file_label: &'a str,
    /// Committed `gmeow:ceilingCount` at the merge base, keyed `(slice, vocab)`.
    pub base_ceilings: &'a BTreeMap<(String, String), u64>,
    /// Committed `gmeow:ceilingCount` in the working tree, keyed `(slice, vocab)`.
    pub working_ceilings: &'a BTreeMap<(String, String), u64>,
    /// Measured base residue for the cells whose ceiling is NEW in working — the
    /// grandfather gate's allowance. A key absent here reads as `0`.
    pub base_measured: &'a BTreeMap<(String, String), u64>,
    /// Measured working residue, keyed `(slice, vocab)`. A key absent here reads as
    /// `0`. Used by the pin rule: a raised cell's ceiling must EQUAL its measured
    /// residue, so a relocation that also deletes pre-existing residue cannot bank
    /// durable surplus headroom.
    pub working_measured: &'a BTreeMap<(String, String), u64>,
    /// The base residue CONSTRUCTS, keyed `(slice, vocab)` — the departure half of the
    /// derived witness.
    pub base_constructs: &'a BTreeMap<(String, String), Vec<crate::counting::Construct>>,
    /// The working residue CONSTRUCTS, keyed `(slice, vocab)` — the arrival half.
    pub working_constructs: &'a BTreeMap<(String, String), Vec<crate::counting::Construct>>,
    /// Each guarded vocabulary's `gmeow:vocabularyDefaultCeiling` — the effective
    /// ceiling of a cell with NO committed commitment on a given side. A DELETED
    /// commitment resolves here (never "absent → skip"), so dropping a ceiling to the
    /// default is a real lowering that can fund a transfer.
    pub default_ceilings: &'a BTreeMap<String, u64>,
    /// The AUTHORED `gmeow:CeilingRelocation` declarations.
    pub declarations: &'a [crate::model::CeilingRelocation],
    /// The per-edge residue-conservation reasons, reported verbatim on a refusal so a
    /// maintainer sees WHY residue was not conserved across the move.
    pub edge_reasons: &'a EdgeRelocationReasons,
}

/// One `(slice, vocab)` cell's derived witness sets — the anchored residue term IRIs
/// on each side, plus the count of constructs that have NO cross-dataset identity.
#[derive(Debug, Default, Clone)]
struct CellWitness {
    base_keys: BTreeSet<String>,
    working_keys: BTreeSet<String>,
    /// Working-side residue constructs whose subject is a blank node with no named
    /// `sh:property`/`sh:node` ancestor. These can NEVER be a relocation witness.
    working_non_relocatable: u64,
}

/// The deterministic bipartite transport network for ONE vocabulary.
///
/// `pub`: this is also the shared surface [`solve_transport`] exposes to callers
/// OUTSIDE the ratchet gate (e.g. the `gmeow-dev slice-quality-relocation-preview`
/// command) that need to answer "would the gate accept this move?" without
/// standing up a second, hand-rolled flow computation. Building a `Transport` and
/// calling [`solve_transport`] is the ONLY sanctioned way to answer that question —
/// a caller that instead re-derives credit/demand/unpaid arithmetic on its own
/// risks promising an acceptance the gate then refuses, exactly the divergence
/// this type exists to make impossible.
#[derive(Debug, Default)]
pub struct Transport {
    /// `source slice -> available supply` (already `min`-clamped against the declared,
    /// witnessed departure set).
    pub supply: BTreeMap<String, u64>,
    /// `destination slice -> requested demand`.
    pub demand: BTreeMap<String, u64>,
    /// `(source, destination) -> witnessed capacity`.
    pub capacity: BTreeMap<(String, String), u64>,
    /// `(source, destination) -> the witnessed anchor IRIs backing that capacity`.
    pub witnesses: BTreeMap<(String, String), BTreeSet<String>>,
    /// `(source, destination) -> the declaration IRIs authorizing that edge`.
    pub declarations: BTreeMap<(String, String), BTreeSet<String>>,
}

/// The resolved flow: `(source, destination) -> units`, plus each destination's
/// residual (unsaturated) demand.
#[derive(Debug, Default)]
pub struct Flow {
    /// `(source, destination) -> units actually pushed along that edge.`
    pub edges: BTreeMap<(String, String), u64>,
    /// `destination -> demand this flow left unsatisfied (0 == fully paid).`
    pub residual: BTreeMap<String, u64>,
}

/// A node in the [`Transport`] network's residual graph: a super-source feeding
/// every source slice, a super-sink drained by every destination slice, and the
/// source/destination slices themselves in between. `Ord` is derived (variant
/// declaration order, then the wrapped slice name) purely to give `BTreeMap`/
/// `BTreeSet` a total, deterministic order to iterate in — it carries no semantic
/// weight.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum FlowNode {
    SuperSource,
    Source(String),
    Destination(String),
    SuperSink,
}

/// Solve the transport feasibility problem by REAL DETERMINISTIC MAX FLOW
/// (Edmonds-Karp: repeated breadth-first shortest-augmenting-path search over the
/// residual graph, including `destination -> source` back-edges) — not a
/// per-destination greedy sum.
///
/// A single forward pass that visits each destination once and pulls from a
/// shared remaining-supply pool is NOT max flow whenever a source has out-edges
/// to more than one destination: the greedy pass can spend that source's supply
/// on whichever destination it happens to process first, even when a DIFFERENT
/// source could have paid that same destination instead and freed the shared
/// source for a destination with no other option. That reassignment — undo the
/// first source's edge to the first destination, route a different source there
/// instead, and use the freed supply on the second destination — is exactly a
/// residual back-edge augmenting path; a search with no back-edges can never find
/// it and falsely refuses a feasible relocation. (Concretely: supply `s1 = 1,
/// s2 = 1`; capacity `s1→d1 = 1, s1→d2 = 1, s2→d1 = 1`; demand `d1 = 1, d2 = 1`.
/// The unique max flow is `s2→d1, s1→d2`, but a greedy pass that reaches `d1`
/// first and prefers `s1` spends `s1` on `d1` and then has nothing left for `d2`.)
///
/// The network is modelled with an explicit super-source (capacity-`supply` edges
/// into every source) and super-sink (capacity-`demand` edges out of every
/// destination) so ONE shortest-path search per iteration finds a global
/// augmenting path rather than requiring bespoke per-destination bookkeeping.
/// Every adjacency and residual-capacity lookup iterates in `BTreeMap`/`BTreeSet`
/// key order — over [`FlowNode`], whose `Ord` is total and stable — so the
/// discovered augmenting path (and therefore the final flow) is bit-identical
/// across runs on the same input, independent of hashing or memory layout.
///
/// Graphs here are single-digit-sized (a relocation touches a handful of slices),
/// so Edmonds-Karp's `O(V E^2)` bound is irrelevant; determinism is what matters.
pub fn solve_transport(network: &Transport) -> Flow {
    use FlowNode::{Destination, Source, SuperSink, SuperSource};

    // Residual capacity for every directed edge, forward AND back. Kept in `u64`
    // throughout — the network's `supply`/`demand`/`capacity` inputs are already
    // `u64`, so a native `u64` residual graph can represent every input exactly;
    // routing capacities through `i64` (as an earlier version of this solver did)
    // required clamping any input above `i64::MAX` to `i64::MAX`, which silently
    // computed the WRONG flow instead of the true one — a false unpaid-relocation
    // refusal on a governance gate. `u64` has no such gap for these inputs, and the
    // Edmonds-Karp invariant (forward residual + back-edge residual == the original
    // forward capacity, always) guarantees every subtraction below stays
    // non-negative and every accumulation stays within the edge's own capacity, so
    // no value here can legitimately need more than `u64` provides. `add_edge`
    // seeds the back-edge at 0 capacity so the adjacency list already carries it —
    // augmenting a forward edge can then always increase its back-edge in place.
    let mut residual: BTreeMap<(FlowNode, FlowNode), u64> = BTreeMap::new();
    let mut add_edge = |a: FlowNode, b: FlowNode, cap: u64| {
        let key = (a.clone(), b.clone());
        let entry = residual.entry(key).or_insert(0);
        *entry = entry.checked_add(cap).unwrap_or_else(|| {
            panic!(
                "slice-quality transport: residual capacity for edge {a:?} -> {b:?} would overflow u64 \
                 (existing capacity + {cap} exceeds u64::MAX) — refusing to silently wrap a governance-gate capacity"
            )
        });
        residual.entry((b, a)).or_insert(0);
    };
    for (src, cap) in &network.supply {
        add_edge(SuperSource, Source(src.clone()), *cap);
    }
    for ((src, dst), cap) in &network.capacity {
        add_edge(Source(src.clone()), Destination(dst.clone()), *cap);
    }
    for (dst, cap) in &network.demand {
        add_edge(Destination(dst.clone()), SuperSink, *cap);
    }

    let mut adjacency: BTreeMap<FlowNode, BTreeSet<FlowNode>> = BTreeMap::new();
    for (from, to) in residual.keys() {
        adjacency
            .entry(from.clone())
            .or_default()
            .insert(to.clone());
    }

    loop {
        // BFS shortest augmenting path from the super-source, in deterministic
        // adjacency order — the Edmonds-Karp refinement of Ford-Fulkerson, which
        // is what guarantees termination and the polynomial edge bound.
        let mut parent: BTreeMap<FlowNode, FlowNode> = BTreeMap::new();
        let mut queue: std::collections::VecDeque<FlowNode> = std::collections::VecDeque::new();
        queue.push_back(SuperSource);
        parent.insert(SuperSource, SuperSource); // marks SuperSource as visited
        while let Some(node) = queue.pop_front() {
            if node == SuperSink {
                break;
            }
            let Some(neighbors) = adjacency.get(&node) else {
                continue;
            };
            for next in neighbors {
                if parent.contains_key(next) {
                    continue;
                }
                let cap = residual
                    .get(&(node.clone(), next.clone()))
                    .copied()
                    .unwrap_or(0);
                if cap == 0 {
                    continue;
                }
                parent.insert(next.clone(), node.clone());
                queue.push_back(next.clone());
            }
        }
        if !parent.contains_key(&SuperSink) {
            break; // no augmenting path left — current flow is maximum
        }

        // Walk the discovered path back from the sink, find its bottleneck
        // residual capacity, then push that much flow along every edge on the
        // path (and pull it back on every corresponding back-edge).
        let mut path: Vec<(FlowNode, FlowNode)> = Vec::new();
        let mut cur = SuperSink;
        while cur != SuperSource {
            let prev = parent[&cur].clone();
            path.push((prev.clone(), cur.clone()));
            cur = prev;
        }
        let bottleneck = path
            .iter()
            .map(|edge| residual[edge])
            .min()
            .expect("a discovered path has at least one edge");
        for (a, b) in &path {
            let fwd = residual
                .get_mut(&(a.clone(), b.clone()))
                .expect("edge exists");
            *fwd = fwd.checked_sub(bottleneck).expect(
                "bottleneck is the minimum residual capacity along this path, so subtracting it \
                 from any edge on the path cannot underflow",
            );
            let back = residual
                .get_mut(&(b.clone(), a.clone()))
                .expect("back-edge exists");
            *back = back.checked_add(bottleneck).expect(
                "forward residual + back-edge residual is invariant at the edge's original \
                 capacity (already a valid u64), so the back-edge can never need more than that",
            );
        }
    }

    // The flow on a source->destination edge is exactly its original capacity
    // minus what remains in the residual graph.
    let mut flow = Flow::default();
    for ((src, dst), cap) in &network.capacity {
        let left = residual
            .get(&(Source(src.clone()), Destination(dst.clone())))
            .copied()
            .unwrap_or(0);
        let pushed = cap.checked_sub(left).expect(
            "residual capacity conservation: left-over residual can never exceed the edge's \
             original capacity",
        );
        if pushed > 0 {
            flow.edges.insert((src.clone(), dst.clone()), pushed);
        }
    }
    for (dst, wanted) in &network.demand {
        let delivered: u64 = flow
            .edges
            .iter()
            .filter(|((_, d), _)| d == dst)
            .map(|(_, units)| *units)
            .sum();
        flow.residual
            .insert(dst.clone(), wanted.saturating_sub(delivered));
    }
    flow
}

/// Projection-ceiling REBALANCE — ratchet invariants **2** (base∩working
/// monotonicity) and **3** (the grandfather gate for a NEW ceiling), evaluated under
/// ONE rule. Ceilings are LOWER-ONLY relative to their RELOCATION-ADJUSTED base:
///
/// ```text
/// working_ceiling(dst, v) <= base_allowance(dst, v) + inflow(dst, v)
/// ```
///
/// where `base_allowance` is the committed base ceiling when the cell had one and the
/// measured BASE residue when the ceiling is new (the grandfather allowance), and
/// `inflow` is the transported units a declared-and-corroborated relocation moved INTO
/// the cell. With no declarations `inflow` is identically `0` and this reduces
/// EXACTLY to the pre-relocation comparator: `working <= base`, and a new ceiling
/// `<= measured(base)`.
///
/// **A ceiling budgets NET-NEW UNGROUNDED AUTHORING, which is location-independent.**
/// Relocating a term moves residue between two cells without authoring any, so the
/// base ceiling is re-projected through the relocation BEFORE the comparison rather
/// than the comparison being relaxed. The invariant itself is unchanged, and there is
/// still NO in-repo permit to raise a ceiling beyond that adjustment: a raise past the
/// adjusted base reds exactly as before, and is a maintainer-only decision authorized
/// out-of-band by merging past the red. No tool ever creates headroom.
///
/// Every unit of `inflow` must clear FOUR independent tests:
///
/// 1. **Declared** — the moved term is a `gmeow:relocationTerm` of a
///    [`crate::model::CeilingRelocation`] naming exactly that `(from, to)` pair (and
///    that vocabulary, when the declaration is vocabulary-scoped).
/// 2. **Witnessed** — the term DEPARTED the source (present in the source's base
///    residue, absent from its working residue) AND ARRIVED at the destination
///    (present in working, absent at base). The departure requirement is load-bearing:
///    without it a construct COPIED into a second slice — two second-sources-of-truth,
///    strictly worse than one, and exactly what the ratchet exists to prevent — is
///    indistinguishable from a relocated one and would be netted as a transfer. A
///    [`crate::counting::Witness::NonRelocatable`] construct can never witness anything.
/// 3. **Paid** — a max-flow transport solution routes the unit from a source whose
///    committed ceiling FELL by at least that much. The source supply is additionally
///    clamped to its declared, witnessed departures: the corpus carries large STALE
///    headroom, and lowering dead headroom surrenders no authoring, so it must never
///    buy live headroom elsewhere.
/// 4. **Pinned** — every raised destination's committed ceiling EQUALS its measured
///    working residue. Without this a relocation that also deletes pre-existing residue
///    banks durable surplus headroom, spendable forever with no witness.
///
/// A declaration that names a term which did NOT move, or whose relocation is fully
/// ABSORBED at base (its terms already sit at the destination on both sides), is a
/// HARD FAIL — otherwise declarations accumulate into standing permits, which is
/// exactly what the doctrine forbids.
///
/// **Deletions** (base-only cells) are ALLOWED with no liveness check — dropping a
/// ceiling can only ever TIGHTEN the effective ceiling to the vocab default, so it is
/// never a loosening the way removing a floor would be.
#[must_use]
pub fn projection_ceiling_monotonicity(cmp: &CeilingComparison<'_>) -> CeilingRebalance {
    let mut out = CeilingRebalance::default();
    let label = cmp.file_label;

    // --- Derived witness, per (slice, vocab) cell ------------------------------
    let mut cells: BTreeMap<(String, String), CellWitness> = BTreeMap::new();
    for (key, constructs) in cmp.base_constructs {
        let cell = cells.entry(key.clone()).or_default();
        for c in constructs {
            if let Some(anchor) = c.witness.anchor() {
                cell.base_keys.insert(anchor.to_owned());
            }
        }
    }
    for (key, constructs) in cmp.working_constructs {
        let cell = cells.entry(key.clone()).or_default();
        for c in constructs {
            match c.witness.anchor() {
                Some(anchor) => {
                    cell.working_keys.insert(anchor.to_owned());
                }
                None => cell.working_non_relocatable += 1,
            }
        }
    }
    let empty_cell = CellWitness::default();
    let cell_of = |slice: &str, vocab: &str| -> &CellWitness {
        cells
            .get(&(slice.to_owned(), vocab.to_owned()))
            .unwrap_or(&empty_cell)
    };
    let departed = |slice: &str, vocab: &str| -> BTreeSet<String> {
        let c = cell_of(slice, vocab);
        c.base_keys.difference(&c.working_keys).cloned().collect()
    };
    let arrived = |slice: &str, vocab: &str| -> BTreeSet<String> {
        let c = cell_of(slice, vocab);
        c.working_keys.difference(&c.base_keys).cloned().collect()
    };

    // Every vocabulary any input mentions, so a cell present on one side only is
    // still considered.
    let mut vocabs: BTreeSet<String> = BTreeSet::new();
    for (_, v) in cmp
        .base_ceilings
        .keys()
        .chain(cmp.working_ceilings.keys())
        .chain(cells.keys())
    {
        vocabs.insert(v.clone());
    }
    for d in cmp.declarations {
        if let Some(v) = &d.vocabulary {
            vocabs.insert(v.clone());
        }
    }

    // --- Declaration integrity (independent of any raise) ----------------------
    out.violations.extend(declaration_integrity(
        label,
        cmp,
        &vocabs,
        &departed,
        |s, v| cell_of(s, v).base_keys.clone(),
        |s, v| cell_of(s, v).working_keys.clone(),
    ));

    // --- Per-vocabulary transport feasibility ----------------------------------
    for vocab in &vocabs {
        let default_ceiling = cmp.default_ceilings.get(vocab).copied().unwrap_or(0);
        let effective = |map: &BTreeMap<(String, String), u64>, slice: &str| -> u64 {
            map.get(&(slice.to_owned(), vocab.clone()))
                .copied()
                .unwrap_or(default_ceiling)
        };

        // Every slice this vocabulary's ceiling accounting touches.
        let mut slices: BTreeSet<String> = BTreeSet::new();
        for (s, v) in cmp.base_ceilings.keys().chain(cmp.working_ceilings.keys()) {
            if v == vocab {
                slices.insert(s.clone());
            }
        }
        for d in cmp.declarations {
            if d.vocabulary.as_ref().is_none_or(|dv| dv == vocab) {
                slices.insert(d.from_slice.clone());
                slices.insert(d.to_slice.clone());
            }
        }

        // Demands: the raise each destination asks for, against its BASE ALLOWANCE.
        let mut network = Transport::default();
        let mut demand_pin_failures: BTreeMap<String, String> = BTreeMap::new();
        for slice in &slices {
            let key = (slice.clone(), vocab.clone());
            let Some(&work_ceil) = cmp.working_ceilings.get(&key) else {
                continue; // no committed working ceiling → nothing is being asked for
            };
            let (allowance, is_new) = match cmp.base_ceilings.get(&key) {
                Some(&base_ceil) => (base_ceil, false),
                None => (cmp.base_measured.get(&key).copied().unwrap_or(0), true),
            };
            if work_ceil <= allowance {
                continue;
            }
            network.demand.insert(slice.clone(), work_ceil - allowance);
            // The PIN rule: a raised cell's ceiling must equal its measured residue.
            let measured = cmp.working_measured.get(&key).copied().unwrap_or(0);
            if work_ceil != measured {
                demand_pin_failures.insert(
                    slice.clone(),
                    format!(
                        "{label}: slice {slice} vocab {vocab} {} projection ceiling {work_ceil} is \
                         above its relocation-adjusted base allowance {allowance} AND is not \
                         pinned to its measured residue {measured} — a relocation may re-project \
                         the base ceiling only onto exactly the residue that arrived; an unpinned \
                         ceiling banks durable surplus headroom with no witness",
                        if is_new { "NEW" } else { "shared" }
                    ),
                );
            }
        }
        if network.demand.is_empty() {
            continue; // nothing raised for this vocabulary → invariant already holds
        }

        // Supplies: what each source's lowering can pay, clamped to its DECLARED,
        // WITNESSED departures. The `min` is load-bearing — lowering dead headroom
        // surrenders no authoring, so it must never buy live headroom elsewhere.
        for slice in &slices {
            let base_ceil = effective(cmp.base_ceilings, slice);
            let work_ceil = effective(cmp.working_ceilings, slice);
            let lowering = base_ceil.saturating_sub(work_ceil);
            if lowering == 0 {
                continue;
            }
            let declared_out: BTreeSet<String> = cmp
                .declarations
                .iter()
                .filter(|d| {
                    &d.from_slice == slice && d.vocabulary.as_ref().is_none_or(|dv| dv == vocab)
                })
                .flat_map(|d| d.terms.iter().cloned())
                .collect();
            let live = departed(slice, vocab).intersection(&declared_out).count() as u64;
            let supply = lowering.min(live);
            if supply > 0 {
                network.supply.insert(slice.clone(), supply);
            }
        }

        // Edges: the witnessed capacity of each declared (src → dst) pair.
        for d in cmp.declarations {
            if d.vocabulary.as_ref().is_some_and(|dv| dv != vocab) {
                continue;
            }
            if !network.demand.contains_key(&d.to_slice) {
                continue;
            }
            let declared: BTreeSet<String> = d.terms.iter().cloned().collect();
            let witnessed: BTreeSet<String> = departed(&d.from_slice, vocab)
                .intersection(&arrived(&d.to_slice, vocab))
                .filter(|t| declared.contains(*t))
                .cloned()
                .collect();
            if witnessed.is_empty() {
                continue;
            }
            let edge = (d.from_slice.clone(), d.to_slice.clone());
            network
                .declarations
                .entry(edge.clone())
                .or_default()
                .insert(d.iri.clone());
            network
                .witnesses
                .entry(edge.clone())
                .or_default()
                .extend(witnessed);
            let total = network.witnesses[&edge].len() as u64;
            network.capacity.insert(edge, total);
        }

        let flow = solve_transport(&network);

        for ((src, dst), units) in &flow.edges {
            let edge = (src.clone(), dst.clone());
            out.accepted.push(AcceptedTransfer {
                vocab: vocab.clone(),
                from: src.clone(),
                to: dst.clone(),
                units: *units,
                witnesses: network
                    .witnesses
                    .get(&edge)
                    .map(|w| w.iter().cloned().collect())
                    .unwrap_or_default(),
                declarations: network
                    .declarations
                    .get(&edge)
                    .map(|d| d.iter().cloned().collect())
                    .unwrap_or_default(),
            });
        }

        for (dst, residual) in &flow.residual {
            let asked = network.demand.get(dst).copied().unwrap_or(0);
            if *residual == 0 {
                // Fully transported — but a raised cell must ALSO be pinned to measured.
                if let Some(msg) = demand_pin_failures.remove(dst) {
                    out.violations.push(msg);
                }
                continue;
            }
            let shortfall = explain_shortfall(
                cmp,
                &network,
                &flow,
                vocab,
                dst,
                asked,
                *residual,
                cell_of(dst, vocab).working_non_relocatable,
                &arrived(dst, vocab),
            );
            let base_ceil = cmp
                .base_ceilings
                .get(&(dst.clone(), vocab.clone()))
                .copied();
            let work_ceil = cmp
                .working_ceilings
                .get(&(dst.clone(), vocab.clone()))
                .copied()
                .unwrap_or(default_ceiling);
            let head = match base_ceil {
                Some(before) => format!(
                    "{label}: slice {dst} vocab {vocab} projection ceiling RAISED {before} → {work_ceil}"
                ),
                None => format!(
                    "{label}: NEW projection ceiling slice {dst} vocab {vocab} count {work_ceil} exceeds base measured residue {}",
                    cmp.base_measured
                        .get(&(dst.clone(), vocab.clone()))
                        .copied()
                        .unwrap_or(0)
                ),
            };
            out.violations.push(format!(
                "{head} — ceilings are lower-only relative to their relocation-adjusted base; \
                 {residual} of {asked} unit(s) of this raise are unpaid. {} A raise beyond the \
                 declared-and-corroborated relocation grants net-new headroom and is a \
                 maintainer-only decision authorized out-of-band (merging past this red), never \
                 by a tool.",
                shortfall.join(" ")
            ));
            // The unpaid violation already names this cell; a second pin message for
            // the same cell would be noise.
            demand_pin_failures.remove(dst);
        }
    }

    out.accepted
        .sort_by(|a, b| (&a.vocab, &a.from, &a.to).cmp(&(&b.vocab, &b.from, &b.to)));
    out
}

/// Verify every AUTHORED declaration against the DERIVED witness, independently of
/// whether any ceiling was raised. Two mismatches are hard fails:
///
/// * **stale** — the relocation is fully ABSORBED at the merge base: nothing departed
///   the source, and every declared term already sits at the destination on BOTH
///   sides. The declaration is dead and must be deleted; leaving it would let
///   declarations accumulate into standing permits.
/// * **never moved** — a declared term departed NO covered source cell and arrived at
///   NO covered destination cell. The declaration contradicts the witness.
fn declaration_integrity(
    label: &str,
    cmp: &CeilingComparison<'_>,
    vocabs: &BTreeSet<String>,
    departed: &impl Fn(&str, &str) -> BTreeSet<String>,
    base_keys: impl Fn(&str, &str) -> BTreeSet<String>,
    working_keys: impl Fn(&str, &str) -> BTreeSet<String>,
) -> Vec<String> {
    let mut errs = Vec::new();
    for d in cmp.declarations {
        let covered: Vec<&String> = vocabs
            .iter()
            .filter(|v| d.vocabulary.as_ref().is_none_or(|dv| &dv == v))
            .collect();
        let mut any_departed = false;
        let mut absorbed = false;
        let mut moved_terms: BTreeSet<String> = BTreeSet::new();
        for v in &covered {
            let dep = departed(&d.from_slice, v);
            for t in &d.terms {
                if dep.contains(t) {
                    any_departed = true;
                    moved_terms.insert(t.clone());
                }
            }
            let at_dst_base = base_keys(&d.to_slice, v);
            let at_dst_work = working_keys(&d.to_slice, v);
            if d.terms.iter().all(|t| at_dst_base.contains(t))
                && d.terms.iter().all(|t| at_dst_work.contains(t))
            {
                absorbed = true;
            }
        }
        if !any_departed && absorbed {
            errs.push(format!(
                "{label}: stale-declaration: gmeow:CeilingRelocation {} (dated {}) is fully \
                 ABSORBED at the merge base — every declared term already sits at {} on BOTH \
                 sides and nothing departed {}; delete the declaration (a relocation declaration \
                 that outlives its relocation is a standing permit, which the ratchet forbids)",
                d.iri, d.date, d.to_slice, d.from_slice
            ));
            continue;
        }
        if !any_departed {
            errs.push(format!(
                "{label}: gmeow:CeilingRelocation {} (dated {}) declares term(s) {} moved from {} \
                 to {}, but NONE of them departed {} in the derived witness — a declared term \
                 that did not move contradicts the measurement and can authorize nothing",
                d.iri,
                d.date,
                d.terms.join(", "),
                d.from_slice,
                d.to_slice,
                d.from_slice
            ));
            continue;
        }
        let stragglers: Vec<&String> = d
            .terms
            .iter()
            .filter(|t| !moved_terms.contains(*t))
            .collect();
        if !stragglers.is_empty() {
            errs.push(format!(
                "{label}: gmeow:CeilingRelocation {} (dated {}) declares term(s) {} moved from {} \
                 to {}, but they did not depart {} in the derived witness — a declaration must \
                 name exactly the terms that moved",
                d.iri,
                d.date,
                stragglers
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                d.from_slice,
                d.to_slice,
                d.from_slice
            ));
        }
    }
    errs
}

/// Name the SHORTFALL behind an unmet demand: how much of the raise is unwitnessed,
/// how much is witnessed but unpaid, which arrivals no declaration covers, how many
/// destination constructs have no relocation-invariant identity at all, and the
/// residue-conservation reason codes derived by the edge accounting.
#[allow(clippy::too_many_arguments)]
fn explain_shortfall(
    cmp: &CeilingComparison<'_>,
    network: &Transport,
    flow: &Flow,
    vocab: &str,
    dst: &str,
    asked: u64,
    residual: u64,
    non_relocatable: u64,
    arrivals: &BTreeSet<String>,
) -> Vec<String> {
    let mut reasons = Vec::new();

    // Total witnessed capacity into this destination, across every declared edge.
    let witnessed: u64 = network
        .capacity
        .iter()
        .filter(|((_, d), _)| d == dst)
        .map(|(_, c)| *c)
        .sum();
    if witnessed < asked {
        reasons.push(format!("unwitnessed: {witnessed} of {asked}."));
    }
    // The credit actually DELIVERED by the transport solution — never the raw supply
    // of the in-edges, because a source's lowering may already be spent funding
    // another destination. Naming the delivered amount is what makes the refusal
    // consistent with its own audit lines.
    let delivered = asked.saturating_sub(residual);
    if delivered < asked {
        reasons.push(format!("unpaid: credit {delivered} < demand {asked}."));
        // Name the BLOCKING edges: every witnessed in-edge whose source could not
        // deliver its full capacity because its own supply was exhausted.
        for ((src, edge_dst), cap) in &network.capacity {
            if edge_dst != dst {
                continue;
            }
            let pushed = flow
                .edges
                .get(&(src.clone(), dst.to_owned()))
                .copied()
                .unwrap_or(0);
            if pushed < *cap {
                reasons.push(format!(
                    "blocking edge {src} → {dst}: {pushed} of {cap} witnessed unit(s) delivered \
                     (source supply {} is exhausted or absent).",
                    network.supply.get(src).copied().unwrap_or(0)
                ));
            }
        }
    }

    // Arrivals no declaration covers — the copy-vs-move and undeclared-move cases.
    let declared_in: BTreeSet<&String> = cmp
        .declarations
        .iter()
        .filter(|d| d.to_slice == dst && d.vocabulary.as_ref().is_none_or(|dv| dv == vocab))
        .flat_map(|d| d.terms.iter())
        .collect();
    for term in arrivals {
        if !declared_in.contains(term) {
            reasons.push(format!(
                "undeclared: term {term} moved but no relocation declaration covers it."
            ));
        }
    }
    if non_relocatable > 0 {
        reasons.push(format!(
            "non-relocatable: {non_relocatable} blank-subject construct(s) with no named anchor."
        ));
    }
    for ((from, to, edge_vocab), anchors) in cmp.edge_reasons {
        if to != dst || edge_vocab != vocab {
            continue;
        }
        for (anchor, codes) in anchors {
            let codes: Vec<&str> = codes.iter().map(|c| c.code()).collect();
            reasons.push(format!(
                "residue not conserved moving {anchor} from {from}: {}.",
                codes.join(", ")
            ));
        }
    }
    if reasons.is_empty() {
        reasons.push(format!(
            "unwitnessed: 0 of {asked} — no gmeow:CeilingRelocation declares any term arriving \
             here."
        ));
    }
    reasons
}

/// Aggregate CONSERVATION over the cells present in BOTH the merge base and the
/// working tree: for every guarded vocabulary,
/// `Σ_{cells ∈ base∩working} working <= Σ_{cells ∈ base∩working} base`.
///
/// This is the total-budget backstop behind the per-cell rebalance: relocation moves
/// residue between cells, so no relocation can ever raise the total. A per-cell pass
/// that accepted more inflow than it funded would show up here as a risen sum.
///
/// **The scoping to `base ∩ working` is load-bearing.** Ratchet invariant 3 explicitly
/// PERMITS a brand-new ceiling up to `measured(base)` — a new slice carrying
/// pre-existing residue commits a matching ceiling — and every such legitimate
/// addition raises an unscoped Σ while violating nothing. New cells are governed by
/// invariant 3; deletions only ever lower Σ. An unscoped sum would therefore false-red
/// the exact workflow the ratchet documentation advertises.
#[must_use]
pub fn ceiling_conservation(
    file_label: &str,
    base: &BTreeMap<(String, String), u64>,
    working: &BTreeMap<(String, String), u64>,
) -> Vec<String> {
    let mut base_totals: BTreeMap<&str, u64> = BTreeMap::new();
    let mut working_totals: BTreeMap<&str, u64> = BTreeMap::new();
    for ((slice, vocab), before) in base {
        let Some(now) = working.get(&(slice.clone(), vocab.clone())) else {
            continue; // deletion — governed by nothing here; it only lowers Σ
        };
        *base_totals.entry(vocab.as_str()).or_insert(0) += *before;
        *working_totals.entry(vocab.as_str()).or_insert(0) += *now;
    }
    let mut errs = Vec::new();
    for (vocab, before) in &base_totals {
        let now = working_totals.get(vocab).copied().unwrap_or(0);
        if now > *before {
            errs.push(format!(
                "{file_label}: vocab {vocab} TOTAL committed projection ceiling ROSE {before} → \
                 {now} across the cells committed in both the merge base and the working tree — \
                 a relocation moves residue between cells and can never raise the total; the \
                 aggregate budget for net-new ungrounded authoring is lower-only"
            ));
        }
    }
    errs
}

/// Registry meta-ratchet (base∩working) — the guarded-vocabulary REGISTRY itself is
/// lower-only in gate strength: a change that lets more ungrounded authoring through
/// WITHOUT raising a per-cell ceiling is a hard violation. For every vocabulary
/// present in BOTH base and working (keyed by prefix), a violation is recorded when
/// the working registry is WEAKER than base along any axis:
/// - the vocabulary is DELETED (base-only) — a guard silently dropped;
/// - a `gmeow:vocabularyNamespace` was REMOVED (the match surface narrowed);
/// - the `count_kind` was WEAKENED to the non-counting `NonRdfSurface`;
/// - a `StructuralAxiom` `counted_predicate` was REMOVED (fewer axioms counted);
/// - the `default_ceiling` was RAISED (more free headroom for unlisted slices);
/// - the `alignment_predicate` exemption set was EXPANDED (more bridges waved through).
///
/// New vocabularies and any STRENGTHENING (wider namespaces, more counted predicates,
/// lower default ceiling, fewer exemptions) are allowed with no violation.
#[must_use]
pub fn registry_ratchet_monotonicity(
    file_label: &str,
    base: &[crate::model::ProjectionVocabulary],
    working: &[crate::model::ProjectionVocabulary],
) -> Vec<String> {
    use crate::model::CountKind;
    let by_prefix = |vs: &[crate::model::ProjectionVocabulary]| {
        vs.iter()
            .map(|v| (v.prefix.clone(), v.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    let working_map = by_prefix(working);
    let mut violations = Vec::new();
    for b in base {
        let Some(w) = working_map.get(&b.prefix) else {
            violations.push(format!(
                "{file_label}: guarded vocabulary {} DELETED from the registry — dropping a guard weakens the gate; retire it only by an out-of-band maintainer decision, never silently",
                b.prefix
            ));
            continue;
        };
        for ns in &b.namespaces {
            if !w.namespaces.contains(ns) {
                violations.push(format!(
                    "{file_label}: guarded vocabulary {} namespace NARROWED — {ns} removed; the match surface may only widen",
                    b.prefix
                ));
            }
        }
        if b.count_kind != CountKind::NonRdfSurface && w.count_kind == CountKind::NonRdfSurface {
            violations.push(format!(
                "{file_label}: guarded vocabulary {} count-kind WEAKENED to countKindNonRdfSurface — it now counts nothing",
                b.prefix
            ));
        }
        for cp in &b.counted_predicates {
            if !w.counted_predicates.contains(cp) {
                violations.push(format!(
                    "{file_label}: guarded vocabulary {} counted-predicate allowlist NARROWED — {cp} removed; fewer structural axioms are now counted",
                    b.prefix
                ));
            }
        }
        if w.default_ceiling > b.default_ceiling {
            violations.push(format!(
                "{file_label}: guarded vocabulary {} default-ceiling RAISED {} → {} — grants free headroom to every unlisted slice",
                b.prefix, b.default_ceiling, w.default_ceiling
            ));
        }
        for ap in &w.alignment_predicates {
            if !b.alignment_predicates.contains(ap) {
                violations.push(format!(
                    "{file_label}: guarded vocabulary {} alignment-predicate set EXPANDED — {ap} added; a new exemption waves more bridges through",
                    b.prefix
                ));
            }
        }
    }
    violations
}

/// The manifest-reading + ladder-resolution step shared by [`declared_tier`] (the
/// repo-anchored ratchet gate, resolving against a full [`Rubric`]'s ladder) and
/// [`crate::lint::declared_quality_tier`] (the checkout-free consumer lint gate,
/// resolving against a bundle-flattened [`crate::model::MeasurementStandard`]'s
/// ladder) — one manifest-reading authority, never two independently-drifting
/// copies. `tiers` is whichever ladder half the caller holds; the PREDICATE read
/// is always `gmeow:sliceQualityTier` (never `gmeow:sliceTier`, a distinct
/// domain predicate). `None` when the slice declares no claim (undeclared,
/// advisory-only).
///
/// # Errors
/// Returns a message if the manifest cannot be read or names a tier `tiers` does
/// not define (a hard error — an unknown tier is not silently ignored).
pub(crate) fn declared_tier_against(
    slice_dir: &Path,
    tiers: &[Tier],
) -> gmeow_errors::Result<Option<Tier>> {
    let manifest = slice_dir.join("manifest.ttl");
    let ds = crate::dataset_from_paths(&[&manifest])?;
    let Some(slice_iri) = instances_of(&ds, &graph::g("Slice")).into_iter().next() else {
        return Ok(None);
    };
    let (Some(sid), Some(pred)) = (id(&ds, &slice_iri), id(&ds, &graph::g("sliceQualityTier")))
    else {
        return Ok(None);
    };
    match one_iri(&ds, sid, pred) {
        None => Ok(None),
        Some(tier_iri) => tiers
            .iter()
            .find(|t| t.iri == tier_iri)
            .cloned()
            .map(Some)
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Gate {
                    detail: format!("{slice_iri} declares unknown quality tier {tier_iri}"),
                })
            }),
    }
}

/// The `gmeow:sliceQualityTier` a slice's `manifest.ttl` declares, resolved against
/// the rubric's ladder — `None` when the slice has not opted in.
///
/// # Errors
/// Returns a message if the manifest cannot be read or names a tier the rubric
/// does not define (a hard error — an unknown tier is not silently ignored).
pub fn declared_tier(slice_dir: &Path, rubric: &Rubric) -> gmeow_errors::Result<Option<Tier>> {
    declared_tier_against(slice_dir, &rubric.standard.tiers)
}

/// The local name of an IRI (the tail after the last `/` or `#`) — used to name a
/// slice or axis compactly in a coherence-violation message.
fn local_name(iri: &str) -> &str {
    iri.rsplit(['/', '#']).next().unwrap_or(iri)
}

/// The tier a committed axis *floor* value implies: grade the floor decimal through
/// THAT axis's rubric thresholds exactly as a measured score would be graded (the
/// strongest tier whose floor the value meets, else the ladder bottom). This is the
/// lattice morphism that carries a per-axis floor up into the tier ladder, so the
/// per-axis floor level and the roll-up tier-floor level can be compared. Pure: it
/// reuses [`crate::lattice::grade_axis`] and adds no new grading path.
#[must_use]
pub fn axis_floor_implied_tier(axis: &Axis, floor: f64, rubric: &Rubric) -> Tier {
    crate::lattice::grade_axis(axis, floor, &rubric.standard).tier
}

/// Which of the two coherence sub-checks a [`CoherenceViolation`] records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoherenceKind {
    /// A committed axis floor grades to a tier strictly BELOW the slice's committed
    /// tier floor — an internal contradiction between two commitments (the roll-up
    /// is a meet, so a tier floor `T` demands every axis floor to imply `≥ T`).
    BackingInvariant,
    /// A slice floored on EVERY rubric axis whose committed tier floor does not
    /// EQUAL the meet of its axis floors' implied tiers — a tier floor below the
    /// achievable meet is a dead guarantee (it should be raised to the meet).
    Tightness,
}

/// One floor-coherence violation: the slice, which sub-check failed, and a message
/// naming every relevant tier/axis/floor so the failure is self-explanatory.
#[derive(Debug, Clone)]
pub struct CoherenceViolation {
    /// The slice IRI the contradiction is committed against.
    pub slice: String,
    /// Which sub-check the violation belongs to.
    pub kind: CoherenceKind,
    /// The human-facing failure message.
    pub message: String,
}

/// FLOOR COHERENCE — a pure consistency assertion tying the two committed floor
/// levels together (a lattice morphism), reading BOTH levels straight from the
/// rubric. It is NO-CALIBRATION-safe: it compares COMMITTED floors against each
/// other, never a measured score, so it needs no scoring sweep at all.
///
/// For every slice carrying a `gmeow:SliceTierFloor` of tier rank `T` AND at least
/// one `gmeow:AxisFloorCommitment`:
///
/// 1. **Backing invariant** (always applicable): grade EACH committed axis floor
///    through its axis's rubric thresholds ([`axis_floor_implied_tier`]) to an
///    implied tier rank `A`; assert `A >= T`. The roll-up is a meet (min tier over
///    every axis), so a tier floor `T` requires every axis to be `≥ T`; an axis
///    floor implying a tier below `T` is a contradiction between two commitments.
/// 2. **Tightness** (coverage-gated): additionally, when the slice is floored on
///    EVERY rubric axis, assert `T == meet(implied tiers)`. A tier floor strictly
///    below the achievable meet is a dead guarantee; strictly above is impossible
///    once the backing invariant holds.
///
/// Slices with a tier floor but no axis floor, or axis floors but no tier floor, are
/// skipped (no contradiction to check). Deterministic: violations follow the
/// rubric's tier-floor then commitment iteration order.
#[must_use]
pub fn evaluate_coherence(rubric: &Rubric) -> Vec<CoherenceViolation> {
    let mut out = Vec::new();
    for tf in &rubric.floors.tier_floors {
        // The tier floor's ladder tier. An unresolvable floorTier is a HARD FAIL the
        // caller's `tier_floors_from_rubric` raises before this runs; here it is a
        // skip so this pure fn never panics on a rubric the caller already rejected.
        let Some(t_tier) = rubric.standard.tier(&tf.tier) else {
            continue;
        };
        // This slice's committed axis floors, in rubric-commitment order.
        let slice_floors: Vec<&AxisFloorCommitment> = rubric
            .floors
            .commitments
            .iter()
            .filter(|c| c.slice == tf.slice)
            .collect();
        if slice_floors.is_empty() {
            continue; // no axis floor → both sub-checks require ≥ 1
        }
        // Grade each axis floor to its implied tier, checking the backing invariant.
        let mut implied: Vec<Tier> = Vec::with_capacity(slice_floors.len());
        for c in &slice_floors {
            let Some(axis) = rubric.standard.axes.iter().find(|a| a.iri == c.axis) else {
                continue; // an axis floor naming no rubric axis cannot be graded
            };
            let a_tier = axis_floor_implied_tier(axis, c.floor, rubric);
            if a_tier.rank < t_tier.rank {
                out.push(CoherenceViolation {
                    slice: tf.slice.clone(),
                    kind: CoherenceKind::BackingInvariant,
                    message: format!(
                        "slice {} axis {} committed floor {:.6} grades to tier {} (rank {}) — below the slice's committed tier floor {} (rank {}); the roll-up is a meet, so a tier floor requires every axis floor to back it",
                        tf.slice,
                        local_name(&c.axis),
                        c.floor,
                        t_tier_label(&a_tier),
                        a_tier.rank,
                        t_tier_label(t_tier),
                        t_tier.rank
                    ),
                });
            }
            implied.push(a_tier);
        }
        // TIGHTNESS — only when the slice is floored on EVERY rubric axis.
        let floored_all_axes = !rubric.standard.axes.is_empty()
            && rubric
                .standard
                .axes
                .iter()
                .all(|a| slice_floors.iter().any(|c| c.axis == a.iri));
        if floored_all_axes
            && let Some(meet) = implied.iter().min()
            && meet.rank != t_tier.rank
        {
            out.push(CoherenceViolation {
                slice: tf.slice.clone(),
                kind: CoherenceKind::Tightness,
                message: format!(
                    "slice {} is floored on every rubric axis: the meet of its axis floors' implied tiers is {} (rank {}) but its committed tier floor is {} (rank {}) — a tier floor below the achievable meet is a dead guarantee (raise it to the meet)",
                    tf.slice,
                    t_tier_label(meet),
                    meet.rank,
                    t_tier_label(t_tier),
                    t_tier.rank
                ),
            });
        }
    }
    out
}

/// A tier's most readable name for a message: its `rdfs:label` if present, else the
/// IRI local name (a synthetic test tier may carry an empty label).
fn t_tier_label(tier: &Tier) -> &str {
    if tier.label.is_empty() {
        local_name(&tier.iri)
    } else {
        tier.label.as_str()
    }
}

#[path = "gate.tests.rs"]
#[cfg(test)]
mod tests;
