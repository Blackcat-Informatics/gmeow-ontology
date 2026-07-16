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
//    working tree, `ceilingCount(working) <= ceilingCount(base)` — a RAISE is a
//    hard violation; a deletion (base-only) is allowed because dropping a ceiling
//    only ever tightens the effective ceiling to the vocab default.
// 3. **Grandfather (new ceilings only, driven by the `gmeow-dev` CLI — no pure
//    comparator lives in this crate because it needs the base FILESET, not just
//    the base ceiling map): for every (slice, vocab) whose committed ceiling is
//    NEW in working (absent at base), `ceilingCount(working) <= measured(base)`,
//    where `measured(base)` is reconstructed by feeding the SAME
//    [`crate::counting::residue`] counter the merge-base bytes over the SAME
//    ratchet surface set ([`crate::ratchet_surface_paths`]) — a surface absent at
//    base contributes 0, a surface present-but-unreadable at base is a HARD-FAIL
//    (never silently 0). This closes the "author N ungrounded constructs and
//    commit an N-ceiling in the same PR" loophole invariants 1-2 alone cannot see.
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

/// The outcome of a projection-ceiling monotonicity diff: hard `violations` that
/// red the gate.
#[derive(Debug, Default)]
pub struct CeilingMonotonicity {
    /// Hard violations — a committed ceiling RAISED for a (slice, vocab) key
    /// shared by base and working.
    pub violations: Vec<String>,
}

/// Projection-ceiling monotonicity (ratchet invariant **2**, base∩working) — the
/// exact inverse of [`axis_floor_monotonicity`] (flip `<` → `>`, "LOWERED" →
/// "RAISED"). Ceilings are LOWER-ONLY: for every (slice, vocab) key present in
/// BOTH `base` and `working`, `working <= base`; a RAISE is a hard violation.
///
/// **Deletions** (base-only) are ALLOWED with no liveness check — unlike a floor
/// deletion, dropping a ceiling can only ever TIGHTEN the effective ceiling (it
/// falls back to the vocab's `default_ceiling`, `0` today), so it is never a
/// loosening the way removing a floor would be.
///
/// **Additions** (working-only) are NOT validated here — that is ratchet
/// invariant **3**, the grandfather gate, which needs the base TTL fileset (not
/// just the base ceiling map) to reconstruct `measured(base)` and is therefore
/// driven by the `gmeow-dev` CLI, not this pure comparator.
///
/// There is deliberately NO in-repo permit to raise a ceiling — exactly as
/// [`axis_floor_monotonicity`]'s floor doctrine, a raise is a maintainer-only
/// decision authorized out-of-band by merging past the resulting red, never by
/// any tool, flag, or record.
#[must_use]
pub fn projection_ceiling_monotonicity(
    file_label: &str,
    base: &BTreeMap<(String, String), u64>,
    working: &BTreeMap<(String, String), u64>,
) -> CeilingMonotonicity {
    let mut out = CeilingMonotonicity::default();
    for ((slice, vocab), before) in base {
        if let Some(now) = working.get(&(slice.clone(), vocab.clone()))
            && now > before
        {
            out.violations.push(format!(
                "{file_label}: slice {slice} vocab {vocab} projection ceiling RAISED {before} → {now} — ceilings are lower-only; a raise grants net-new headroom and is a maintainer-only decision authorized out-of-band (merging past this red), never by a tool"
            ));
        }
    }
    out
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

/// The `gmeow:sliceQualityTier` a slice's `manifest.ttl` declares, resolved against
/// the rubric's ladder — `None` when the slice has not opted in.
///
/// # Errors
/// Returns a message if the manifest cannot be read or names a tier the rubric
/// does not define (a hard error — an unknown tier is not silently ignored).
pub fn declared_tier(slice_dir: &Path, rubric: &Rubric) -> gmeow_errors::Result<Option<Tier>> {
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
        Some(tier_iri) => rubric
            .standard
            .tier(&tier_iri)
            .cloned()
            .map(Some)
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Gate {
                    detail: format!("{slice_iri} declares unknown quality tier {tier_iri}"),
                })
            }),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GovernanceFloors, MeasurementStandard};

    #[test]
    fn undeclared_slice_always_passes() {
        // (c) undeclared → advisory only, never fails — even measured at the floor.
        assert_eq!(evaluate_ratchet(None, 0, None), RatchetVerdict::Pass);
        assert_eq!(evaluate_ratchet(None, 0, Some(4)), RatchetVerdict::Pass);
    }

    #[test]
    fn measured_below_declared_fails() {
        // (b) declared Linked(2) but measured Grounded(1) → fail.
        assert_eq!(
            evaluate_ratchet(Some(2), 1, None),
            RatchetVerdict::MeasuredBelowDeclared
        );
        // Holding exactly at the declared tier passes.
        assert_eq!(evaluate_ratchet(Some(2), 2, None), RatchetVerdict::Pass);
        // Exceeding the declared tier passes.
        assert_eq!(evaluate_ratchet(Some(1), 3, None), RatchetVerdict::Pass);
    }

    /// An axis binding the given producer with an otherwise-minimal shape.
    fn mk_axis(producer: &str) -> crate::model::Axis {
        use crate::model::{Axis, ContextScope};
        Axis {
            iri: format!("ex:{producer}"),
            label: String::new(),
            producer: producer.to_owned(),
            dimension_iri: "ex:d".to_owned(),
            thresholds: vec![],
            weight: 1.0,
            scope: ContextScope::SliceLocal,
            advice: String::new(),
        }
    }

    #[test]
    fn binding_gate_reds_when_producer_resolves_to_no_item() {
        // A rubric in perfect bijection with the kernel's closed IMPLEMENTED set
        // still reds if a producer resolves to no real Rust item — so the gate
        // proves real resolution, not mere list membership. This is the H4 fix:
        // a producer left in IMPLEMENTED but whose backing fn is gone must red.
        let axes: Vec<crate::model::Axis> = axes::IMPLEMENTED.iter().map(|p| mk_axis(p)).collect();
        let rubric = Rubric {
            standard: MeasurementStandard {
                tiers: vec![],
                axes,
            },
            floors: GovernanceFloors::default(),
        };
        // Every producer resolves → green (bijection holds and all resolve).
        assert!(
            binding_gate(&rubric, |_| true).is_empty(),
            "a full, resolving bijection is green"
        );
        // One producer's Rust item is missing → exactly that producer reds, even
        // though it is still present in IMPLEMENTED and the rubric.
        let errs = binding_gate(&rubric, |s| s != "grounding_axis");
        assert_eq!(
            errs.len(),
            1,
            "exactly the unresolved producer reds: {errs:#?}"
        );
        assert!(
            errs[0].contains("resolves to no Rust primitive item")
                && errs[0].contains("grounding_axis"),
            "the red names the unresolved producer: {errs:#?}"
        );
    }

    #[test]
    fn binding_gate_reds_on_prefix_producer() {
        // (a) A producer that is a strict PREFIX of a real item name must red:
        // the resolver here recognises only the full name `grounding_axis`, so the
        // prefix `grounding_ax` does not resolve — proving the substring/prefix
        // false-positive is gone (a naive `contains("fn grounding_ax")` would have
        // matched `fn grounding_axis`).
        let real: BTreeSet<&str> = axes::IMPLEMENTED.iter().copied().collect();
        let rubric = Rubric {
            standard: MeasurementStandard {
                tiers: vec![],
                axes: vec![mk_axis("grounding_ax")],
            },
            floors: GovernanceFloors::default(),
        };
        let errs = binding_gate(&rubric, |s| real.contains(s));
        assert!(
            errs.iter()
                .any(|e| e.contains("grounding_ax")
                    && e.contains("resolves to no Rust primitive item")),
            "a strict-prefix producer must red on real resolution: {errs:#?}"
        );
    }

    #[test]
    fn staleness_reds_when_producer_resolves() {
        use crate::model::Exemption;
        let rubric = Rubric {
            standard: MeasurementStandard {
                tiers: vec![],
                axes: vec![],
            },
            floors: GovernanceFloors {
                exemptions: vec![Exemption {
                    iri: "ex:e".to_owned(),
                    axis_iri: "ex:a".to_owned(),
                    reason: "unlanded".to_owned(),
                    date: "2026-07-07".to_owned(),
                    producer: "DocMaturity".to_owned(),
                }],
                commitments: vec![],
                tier_floors: vec![],
                ..Default::default()
            },
        };
        // Producer not in-repo → not stale.
        assert!(stale_exemptions(&rubric, |_| false).is_empty());
        // Producer resolves in-repo → stale (the exemption must be retired).
        let stale = stale_exemptions(&rubric, |s| s == "DocMaturity");
        assert_eq!(
            stale.len(),
            1,
            "a resolved producer makes its exemption stale"
        );
    }

    #[test]
    fn completeness_gate_reds_on_empty_exemption_reason() {
        // (d) An exemption whose reason is empty/whitespace must red — a dated
        // exemption cannot pass without a doctrine-anchored justification.
        use crate::model::{Axis, ContextScope, Exemption, Threshold};
        let axis = Axis {
            iri: "ex:a".to_owned(),
            label: String::new(),
            producer: "p".to_owned(),
            dimension_iri: "ex:d".to_owned(),
            thresholds: vec![Threshold {
                tier_iri: "ex:t".to_owned(),
                floor: 0.0,
            }],
            weight: 1.0,
            scope: ContextScope::SliceLocal,
            advice: String::new(),
        };
        let rubric = Rubric {
            standard: MeasurementStandard {
                tiers: vec![],
                axes: vec![axis],
            },
            floors: GovernanceFloors {
                exemptions: vec![Exemption {
                    iri: "ex:e".to_owned(),
                    axis_iri: "ex:a".to_owned(),
                    reason: "   ".to_owned(),
                    date: "2026-07-08".to_owned(),
                    producer: "DocMaturity".to_owned(),
                }],
                commitments: vec![],
                tier_floors: vec![],
                ..Default::default()
            },
        };
        let errs = completeness_gate(&rubric);
        assert!(
            errs.iter().any(|e| e.contains("empty/whitespace reason")),
            "empty exemption reason must red: {errs:#?}"
        );
    }

    #[test]
    fn axis_floor_pass_and_fail() {
        // Exactly at the floor passes.
        assert_eq!(evaluate_axis_floor(1.0, 1.0), AxisRatchetVerdict::Pass);
        // Above the floor passes.
        assert_eq!(evaluate_axis_floor(0.99, 0.5), AxisRatchetVerdict::Pass);
        // Below the floor fails — a real regression.
        assert_eq!(
            evaluate_axis_floor(0.90, 1.0),
            AxisRatchetVerdict::MeasuredBelowFloor
        );
        assert!(evaluate_axis_floor(0.90, 1.0).is_failure());
        assert!(!evaluate_axis_floor(1.0, 1.0).is_failure());
    }

    fn tf(rank: i64, local: &str) -> TierFloor {
        TierFloor {
            rank,
            local: local.to_owned(),
        }
    }

    #[test]
    fn tier_floor_lowering_is_a_hard_violation() {
        let mut base = BTreeMap::new();
        base.insert("ex:logic".to_owned(), tf(2, "tierLinked"));
        base.insert("ex:math".to_owned(), tf(1, "tierGrounded"));
        base.insert("ex:gone".to_owned(), tf(1, "tierGrounded"));

        // A lowered floor (logic 2→1) is a HARD VIOLATION — floors are raise-only and
        // only the maintainer re-baselines a floor down, out-of-band. A raised floor
        // (math 1→3), an added slice (tags), and a deletion of a no-longer-live slice
        // (`gone`) are all clean.
        let mut working = BTreeMap::new();
        working.insert("ex:logic".to_owned(), tf(1, "tierGrounded"));
        working.insert("ex:math".to_owned(), tf(3, "tierExemplified"));
        working.insert("ex:tags".to_owned(), tf(0, "tierRegistered"));

        let live = |s: &str| s != "ex:gone"; // every base slice but `gone` still exists
        let out = tier_floor_monotonicity("floors.tsv", &base, &working, live);
        assert_eq!(out.violations.len(), 1, "only the lowering reds: {out:#?}");
        assert!(
            out.violations[0].contains("ex:logic")
                && out.violations[0].contains("LOWERED")
                && out.violations[0].contains("tierLinked")
                && out.violations[0].contains("tierGrounded"),
            "the violation names the slice and old → new: {out:#?}"
        );
    }

    #[test]
    fn tier_floor_monotonicity_reds_on_still_live_deletion() {
        // A floor removed from the working file for a slice that STILL EXISTS is a
        // hard fail — greenfield removal is allowed only when the slice is gone.
        let mut base = BTreeMap::new();
        base.insert("ex:logic".to_owned(), tf(2, "tierLinked"));
        let working = BTreeMap::new();
        // Slice still live → deletion reds.
        let out = tier_floor_monotonicity("floors.tsv", &base, &working, |_| true);
        assert_eq!(
            out.violations.len(),
            1,
            "still-live deletion reds: {out:#?}"
        );
        assert!(out.violations[0].contains("DELETED") && out.violations[0].contains("ex:logic"));
        // Slice no longer exists → deletion allowed (greenfield removal).
        assert!(
            tier_floor_monotonicity("floors.tsv", &base, &working, |_| false)
                .violations
                .is_empty()
        );
    }

    #[test]
    fn tier_floor_monotonicity_passes_on_raise_and_addition() {
        let mut base = BTreeMap::new();
        base.insert("ex:logic".to_owned(), tf(1, "tierGrounded"));
        let mut working = BTreeMap::new();
        working.insert("ex:logic".to_owned(), tf(2, "tierLinked")); // raise — allowed
        working.insert("ex:new".to_owned(), tf(0, "tierRegistered")); // addition — allowed
        let out = tier_floor_monotonicity("floors.tsv", &base, &working, |_| true);
        assert!(
            out.violations.is_empty(),
            "a raise plus an addition is clean: {out:#?}"
        );
        // Holding exactly at the same rank is also clean.
        let mut same = BTreeMap::new();
        same.insert("ex:logic".to_owned(), tf(1, "tierGrounded"));
        let held = tier_floor_monotonicity("floors.tsv", &base, &same, |_| true);
        assert!(held.violations.is_empty());
    }

    #[test]
    fn axis_floor_lowering_is_a_hard_violation() {
        let key = |s: &str| ("ex:logic".to_owned(), s.to_owned());
        let mut base = BTreeMap::new();
        base.insert(key("axisGmn1Coverage"), 0.98_f64);
        let mut working = BTreeMap::new();
        // Lowered below tolerance → HARD VIOLATION (raise-only ratchet).
        working.insert(key("axisGmn1Coverage"), 0.90_f64);
        let out = axis_floor_monotonicity("axis.tsv", &base, &working, |_, _| true);
        assert_eq!(out.violations.len(), 1, "the lowering reds: {out:#?}");
        assert!(
            out.violations[0].contains("ex:logic")
                && out.violations[0].contains("axisGmn1Coverage")
                && out.violations[0].contains("LOWERED"),
            "names the slice, axis, and lowering: {out:#?}"
        );
        // A raise passes silently.
        let mut raised = BTreeMap::new();
        raised.insert(key("axisGmn1Coverage"), 1.0_f64);
        let up = axis_floor_monotonicity("axis.tsv", &base, &raised, |_, _| true);
        assert!(up.violations.is_empty());
        // Holding exactly at the floor passes silently (within EPSILON).
        let mut same = BTreeMap::new();
        same.insert(key("axisGmn1Coverage"), 0.98_f64);
        let held = axis_floor_monotonicity("axis.tsv", &base, &same, |_, _| true);
        assert!(held.violations.is_empty());
    }

    #[test]
    fn axis_floor_monotonicity_deletion_liveness() {
        let key = ("ex:logic".to_owned(), "axisGmn1Coverage".to_owned());
        let mut base = BTreeMap::new();
        base.insert(key, 1.0_f64);
        let working = BTreeMap::new();
        // Slice + axis still live → deletion reds.
        let out = axis_floor_monotonicity("axis.tsv", &base, &working, |_, _| true);
        assert_eq!(
            out.violations.len(),
            1,
            "still-live axis deletion reds: {out:#?}"
        );
        assert!(out.violations[0].contains("DELETED"));
        // Axis (or slice) no longer live → deletion allowed.
        assert!(
            axis_floor_monotonicity("axis.tsv", &base, &working, |_, _| false)
                .violations
                .is_empty()
        );
    }

    #[test]
    fn declared_below_floor_fails() {
        // (a) committed floor Linked(2) but manifest lowered to Grounded(1) → fail,
        // regardless of what is measured (the ratchet forbids the lowering itself).
        assert_eq!(
            evaluate_ratchet(Some(1), 4, Some(2)),
            RatchetVerdict::DeclaredBelowFloor
        );
        // Declaring at or above the floor is allowed (measured then decides).
        assert_eq!(evaluate_ratchet(Some(2), 2, Some(2)), RatchetVerdict::Pass);
        assert_eq!(evaluate_ratchet(Some(3), 3, Some(2)), RatchetVerdict::Pass);
    }

    // --- Floor-coherence fixtures ---------------------------------------------
    // Small synthetic rubrics: a Registered(0)/Grounded(1)/Linked(2) ladder and
    // axes whose thresholds put the Grounded floor at 0.60 and the Linked floor at
    // 0.75, so a floor of 0.10 grades to Registered, 0.65 to Grounded, 0.80 to
    // Linked. Coherence reads BOTH floor levels straight off the rubric.

    fn co_tier(local: &str, rank: i64) -> Tier {
        Tier {
            iri: format!("ex:{local}"),
            label: local.to_owned(),
            rank,
        }
    }

    fn co_ladder() -> Vec<Tier> {
        vec![
            co_tier("tierRegistered", 0),
            co_tier("tierGrounded", 1),
            co_tier("tierLinked", 2),
        ]
    }

    fn co_axis(iri: &str) -> Axis {
        use crate::model::{ContextScope, Threshold};
        Axis {
            iri: iri.to_owned(),
            label: iri.to_owned(),
            producer: "test".to_owned(),
            dimension_iri: "ex:d".to_owned(),
            thresholds: vec![
                Threshold {
                    tier_iri: "ex:tierGrounded".to_owned(),
                    floor: 0.60,
                },
                Threshold {
                    tier_iri: "ex:tierLinked".to_owned(),
                    floor: 0.75,
                },
            ],
            weight: 1.0,
            scope: ContextScope::SliceLocal,
            advice: String::new(),
        }
    }

    fn afc(slice: &str, axis: &str, floor: f64) -> AxisFloorCommitment {
        AxisFloorCommitment {
            slice: slice.to_owned(),
            axis: axis.to_owned(),
            floor,
        }
    }

    fn stf(slice: &str, tier: &str) -> crate::model::SliceTierFloorCommitment {
        crate::model::SliceTierFloorCommitment {
            slice: slice.to_owned(),
            tier: tier.to_owned(),
        }
    }

    fn co_rubric(
        axes: Vec<Axis>,
        commitments: Vec<AxisFloorCommitment>,
        tier_floors: Vec<crate::model::SliceTierFloorCommitment>,
    ) -> Rubric {
        Rubric {
            standard: MeasurementStandard {
                tiers: co_ladder(),
                axes,
            },
            floors: GovernanceFloors {
                exemptions: vec![],
                commitments,
                tier_floors,
                ..Default::default()
            },
        }
    }

    #[test]
    fn coherence_backing_and_tightness_hold_on_a_coherent_fixture() {
        // (a) A slice with a tier floor Grounded(1) and an axis floor on EVERY axis,
        // each grading to Grounded(1) — the backing invariant holds (1 >= 1) and the
        // tightness check holds (meet == 1 == floor). No violation.
        let rubric = co_rubric(
            vec![co_axis("ex:axisA"), co_axis("ex:axisB")],
            vec![
                afc("ex:s", "ex:axisA", 0.65), // → Grounded(1)
                afc("ex:s", "ex:axisB", 0.70), // → Grounded(1)
            ],
            vec![stf("ex:s", "ex:tierGrounded")],
        );
        assert!(
            evaluate_coherence(&rubric).is_empty(),
            "a coherent floored slice passes: {:#?}",
            evaluate_coherence(&rubric)
        );
    }

    #[test]
    fn coherence_reds_when_an_axis_floor_implies_below_the_tier_floor() {
        // (b) Tier floor Linked(2); axisA floor 0.80 → Linked(2) (backs it) but
        // axisB floor 0.10 → Registered(0), below the tier floor. Only 2 of 3 axes
        // are floored, so tightness is skipped and exactly the backing invariant reds.
        let rubric = co_rubric(
            vec![
                co_axis("ex:axisA"),
                co_axis("ex:axisB"),
                co_axis("ex:axisC"),
            ],
            vec![
                afc("ex:s", "ex:axisA", 0.80), // → Linked(2)
                afc("ex:s", "ex:axisB", 0.10), // → Registered(0) — below Linked(2)
            ],
            vec![stf("ex:s", "ex:tierLinked")],
        );
        let v = evaluate_coherence(&rubric);
        assert_eq!(v.len(), 1, "exactly the backing invariant reds: {v:#?}");
        assert_eq!(v[0].kind, CoherenceKind::BackingInvariant);
        assert!(
            v[0].message.contains("ex:s")
                && v[0].message.contains("axisB")
                && v[0].message.contains("tierRegistered")
                && v[0].message.contains("tierLinked"),
            "names slice, axis, implied tier, and tier floor: {}",
            v[0].message
        );
    }

    #[test]
    fn coherence_reds_on_a_loose_tier_floor_when_floored_on_every_axis() {
        // (c) Floored on EVERY axis (both grade to Grounded(1), so meet == 1) but the
        // committed tier floor is Registered(0) — below the achievable meet. The
        // backing invariant holds (1 >= 0); exactly the tightness check reds.
        let rubric = co_rubric(
            vec![co_axis("ex:axisA"), co_axis("ex:axisB")],
            vec![
                afc("ex:s", "ex:axisA", 0.65), // → Grounded(1)
                afc("ex:s", "ex:axisB", 0.70), // → Grounded(1)
            ],
            vec![stf("ex:s", "ex:tierRegistered")],
        );
        let v = evaluate_coherence(&rubric);
        assert_eq!(v.len(), 1, "exactly the tightness check reds: {v:#?}");
        assert_eq!(v[0].kind, CoherenceKind::Tightness);
        assert!(
            v[0].message.contains("ex:s")
                && v[0].message.contains("tierGrounded") // the meet
                && v[0].message.contains("tierRegistered"), // the loose floor
            "names slice, meet tier, and tier floor: {}",
            v[0].message
        );
    }

    #[test]
    fn coherence_skips_slices_missing_either_floor_level() {
        // (d) sliceA has a tier floor but NO axis floor; sliceB has axis floors but
        // NO tier floor. Neither pairing exists, so both are skipped — no violation.
        let rubric = co_rubric(
            vec![co_axis("ex:axisA")],
            vec![afc("ex:sB", "ex:axisA", 0.10)], // sliceB axis floor, no tier floor
            vec![stf("ex:sA", "ex:tierLinked")],  // sliceA tier floor, no axis floor
        );
        assert!(
            evaluate_coherence(&rubric).is_empty(),
            "a tier-floor-only slice and an axis-floor-only slice are both skipped: {:#?}",
            evaluate_coherence(&rubric)
        );
    }

    // --- Projection-ceiling ratchet fixtures -----------------------------------

    #[test]
    fn ceiling_pass_at_or_below_ceiling() {
        assert_eq!(evaluate_projection_ceiling(0, 0), CeilingVerdict::Pass);
        assert_eq!(evaluate_projection_ceiling(3, 3), CeilingVerdict::Pass);
        assert_eq!(evaluate_projection_ceiling(2, 5), CeilingVerdict::Pass);
        assert!(!evaluate_projection_ceiling(3, 3).is_failure());
    }

    #[test]
    fn ceiling_fails_above_ceiling() {
        assert_eq!(
            evaluate_projection_ceiling(4, 3),
            CeilingVerdict::MeasuredAboveCeiling
        );
        assert!(evaluate_projection_ceiling(4, 3).is_failure());
        // Default ceiling 0: any nonzero residue on an absent commitment reds.
        assert_eq!(
            evaluate_projection_ceiling(1, 0),
            CeilingVerdict::MeasuredAboveCeiling
        );
    }

    fn ck(slice: &str, vocab: &str) -> (String, String) {
        (slice.to_owned(), vocab.to_owned())
    }

    #[test]
    fn ceiling_monotonicity_reds_on_a_raised_shared_key() {
        let mut base = BTreeMap::new();
        base.insert(ck("ex:logic", "sh"), 5_u64);
        let mut working = BTreeMap::new();
        working.insert(ck("ex:logic", "sh"), 7_u64); // RAISED — hard violation
        let out = projection_ceiling_monotonicity("module.ttl", &base, &working);
        assert_eq!(out.violations.len(), 1, "the raise reds: {out:#?}");
        assert!(
            out.violations[0].contains("ex:logic")
                && out.violations[0].contains("sh")
                && out.violations[0].contains("RAISED")
                && out.violations[0].contains("5")
                && out.violations[0].contains("7"),
            "names the slice, vocab, and old → new: {out:#?}"
        );
    }

    #[test]
    fn ceiling_monotonicity_silent_on_hold_lower_delete_add() {
        let mut base = BTreeMap::new();
        base.insert(ck("ex:logic", "sh"), 5_u64); // held
        base.insert(ck("ex:math", "gufo"), 4_u64); // lowered
        base.insert(ck("ex:gone", "bfo"), 3_u64); // deleted (base-only, always allowed)

        let mut working = BTreeMap::new();
        working.insert(ck("ex:logic", "sh"), 5_u64); // hold — clean
        working.insert(ck("ex:math", "gufo"), 2_u64); // lower — clean
        working.insert(ck("ex:new", "sssom"), 1_u64); // addition — not this check's concern

        let out = projection_ceiling_monotonicity("module.ttl", &base, &working);
        assert!(
            out.violations.is_empty(),
            "hold, lower, delete, and add are all clean here: {out:#?}"
        );
    }

    fn vocab(prefix: &str, ns: &[&str], dc: u64) -> crate::model::ProjectionVocabulary {
        crate::model::ProjectionVocabulary {
            prefix: prefix.to_owned(),
            namespaces: ns.iter().map(|s| (*s).to_owned()).collect(),
            subsumed_by: "s".to_owned(),
            owner: "s".to_owned(),
            count_kind: crate::model::CountKind::TypedAxiom,
            default_ceiling: dc,
            preservation: "p".to_owned(),
            alignment_predicates: Vec::new(),
            counted_predicates: Vec::new(),
        }
    }

    #[test]
    fn registry_ratchet_reds_on_weakening_and_silent_on_strengthening() {
        let base = vec![vocab("bfo", &["obo/BFO_"], 0), vocab("gufo", &["g#"], 0)];
        // gufo deleted; bfo namespace narrowed AND default-ceiling raised.
        let weaker = vec![vocab("bfo", &[], 1)];
        let v = registry_ratchet_monotonicity("module.ttl", &base, &weaker);
        assert!(
            v.iter()
                .any(|m| m.contains("gufo") && m.contains("DELETED")),
            "{v:#?}"
        );
        assert!(
            v.iter()
                .any(|m| m.contains("bfo") && m.contains("NARROWED")),
            "{v:#?}"
        );
        assert!(
            v.iter()
                .any(|m| m.contains("bfo") && m.contains("default-ceiling RAISED")),
            "{v:#?}"
        );

        // A new vocab + a WIDER namespace on bfo is pure strengthening — clean.
        let stronger = vec![
            vocab("bfo", &["obo/BFO_", "obo/BFO2_"], 0),
            vocab("gufo", &["g#"], 0),
            vocab("sumo", &["sumo#"], 0),
        ];
        assert!(
            registry_ratchet_monotonicity("module.ttl", &base, &stronger).is_empty(),
            "strengthening must not red"
        );
    }
}
