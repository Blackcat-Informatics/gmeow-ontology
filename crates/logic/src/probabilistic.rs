// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Probabilistic / weighted inference under `logic:ProbabilisticProfile` (v6).
//!
//! This is the final rung of the Logic EPIC: exact marginal inference over
//! probabilistic facts, ProbLog-style, by **weighted model counting**.
//!
//! # Semantics
//!
//! A probabilistic fact (`:- probability(pred(S,O), p).`) is a Bernoulli variable.
//! A **total choice** θ fixes a truth value for every probabilistic variable. Under
//! the declared model:
//!
//! - [`QProbModel::FullIndependence`] (`logic:FullIndependence`): every probabilistic
//!   fact is independent, so `P(θ) = ∏_{f true in θ} p_f · ∏_{f false in θ} (1 − p_f)`.
//! - [`QProbModel::Dependency`] (`logic:DependencyModel`): an explicit joint table over a
//!   correlated fact set replaces the product for those facts; any `probability(...)`
//!   fact outside the correlated set stays independent and factorizes as usual.
//!
//! For each θ we compute the **least Herbrand model** of `(Horn rules ∪ deterministic
//! facts ∪ the facts θ makes true)`. The **marginal** of a query binding is the sum of
//! `P(θ)` over the choices whose model derives it. Inference is exact by enumeration —
//! `#P-hard` in general (matches `logic_certify.py`'s `"probabilistic/#P-hard"`), which
//! is the right tool for the tiny, fully-enumerable conformance corpora.
//!
//! # Epistemic hygiene (the named failure mode this prevents)
//!
//! Probabilities enter ONLY through `:- probability(...)` / `:- joint(...)`. A
//! `:- confidence(...)` annotation is treated as metadata on an asserted (deterministic)
//! fact and is **never** read as a probability — the structural confidence≠probability
//! guard (LOGIC-SEMANTICS §Confidence, probability, weight, and evidence). And with
//! probabilistic facts but **no declared model**, inference REFUSES (returns
//! [`ProbStatus::Unknown`]) rather than silently assuming independence.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::simd::{Select, Simd, cmp::SimdPartialEq};

use crate::profile_gate::is_probabilistic_profile;
use crate::query_ir::{QAtom, QBodyLit, QGoal, QProbModel, QProgram, QRule, QTerm};
use crate::store::WorldStore;

/// Wrap a probabilistic-inference condition message as a typed diagnostic on the
/// shared substrate, preserving the authored text verbatim.
fn probabilistic_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Probabilistic { detail })
}

/// Tolerance for the joint-table sum-to-one check and for treating a marginal as zero.
const EPS: f64 = 1e-9;
/// Decimals of precision the returned marginals are rounded to, so the float matches
/// the captured golden JSON byte-for-byte under `json.dumps` shortest-repr comparison.
const ROUND_DECIMALS: i32 = 10;
/// Hard cap on the number of independent probabilistic facts. Exact weighted model
/// counting enumerates `2^N` total choices, so `N` is bounded both to keep the
/// `1u64 << N` shift well-defined (it would overflow at `N ≥ 64`) and to refuse
/// runs that would exhaust memory/CPU well before that. Over the cap is a hard
/// failure with a clear message — never a silent truncation (no-optionality doctrine).
const MAX_INDEPENDENT_FACTS: usize = 20;

/// A ground RDF fact: `(bare predicate IRI, subject const, object const)`.
///
/// Subject/object are in canonical `<iri>`/n3-literal form — the same surface
/// [`QTerm::Const`] and [`crate::provenance::term_n3`] use — so matching is string
/// equality.
type Fact = (String, String, String);

/// Status of a probabilistic resolution.
///
/// Engine-internal: the *public* answer status is the typed
/// [`crate::result::ReasoningResult`] on [`ProbAnswer`], folded via [`prob_result`];
/// the byte-pinned conformance string projects back via [`prob_status_string`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbStatus {
    /// Marginals computed.
    Ok,
    /// Refused: probabilistic facts present with no declared probability model.
    Unknown,
}

impl ProbStatus {
    /// Canonical lowercase wire string (retained only for the [`prob_status_string`]
    /// round-trip cross-check; the public status projects from the typed result).
    #[cfg(test)]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ProbStatus::Ok => "ok",
            ProbStatus::Unknown => "unknown",
        }
    }
}

/// Fold the engine-internal [`ProbStatus`] into the typed shared
/// [`crate::result::ReasoningResult`] — the canonical answer status. A
/// no-declared-model refusal is `unsupported` + `not-evaluated` (NOT the Belnap
/// `neither`); a computed run is `completed` + `complete-for-fragment` with
/// `information=undetermined` (the graded marginal carries no discretization
/// policy, SEMANTICS:303-305).
///
/// `world` is propagated into [`ResultProvenance`] (lossless, fixes the prior
/// empty-string drop). `payload` carries the marginal bindings for the `Ok` path
/// or an empty [`ResultPayload::Marginals`] for the refusal path — never
/// [`ResultPayload::Empty`], so the typed model is fully lossless.
/// `projection_class` mirrors `preservation` (same idiom as result.rs:821/:883).
fn prob_result(
    status: ProbStatus,
    world: &str,
    payload: crate::result::ResultPayload,
) -> crate::result::ReasoningResult {
    use crate::result::{
        CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
        ReasoningResult, ResultProvenance,
    };
    let mut provenance = ResultProvenance::native("probabilistic", world);
    match status {
        ProbStatus::Ok => {
            let preservation = PreservationClaim::exact();
            provenance.projection_class = preservation.clone();
            ReasoningResult::new(
                InputStatus::Valid,
                EvaluationStatus::Completed,
                CompletenessStatus::CompleteForFragment,
                preservation,
                InformationState::Undetermined,
                provenance,
                payload,
            )
        }
        ProbStatus::Unknown => {
            let preservation = PreservationClaim::default();
            provenance.projection_class = preservation.clone();
            ReasoningResult::new(
                InputStatus::Valid,
                EvaluationStatus::Unsupported,
                CompletenessStatus::Unknown,
                preservation,
                InformationState::NotEvaluated,
                provenance,
                payload,
            )
        }
    }
}

/// Project a probabilistic [`crate::result::ReasoningResult`] back to the
/// byte-pinned conformance answer string (`ok`/`unknown`). The lossless inverse
/// of [`prob_result`].
pub fn prob_status_string(result: &crate::result::ReasoningResult) -> &'static str {
    use crate::result::EvaluationStatus;
    if result.evaluation == EvaluationStatus::Unsupported {
        "unknown"
    } else {
        "ok"
    }
}

/// One answer binding with its marginal probability.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbBinding {
    /// Goal-variable bindings (variable name → canonical constant). Empty for a
    /// fully-ground goal.
    pub vars: BTreeMap<String, String>,
    /// The marginal probability of this binding, rounded to [`ROUND_DECIMALS`].
    pub probability: f64,
}

/// The result of a probabilistic query.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbAnswer {
    /// Bindings with non-zero marginal, canonically sorted.
    pub bindings: Vec<ProbBinding>,
    /// The typed shared result status — the canonical answer status. The
    /// historical string projects from it via [`prob_status_string`].
    pub result: crate::result::ReasoningResult,
}

impl ProbAnswer {
    /// The byte-pinned conformance status string for this answer.
    pub fn status_str(&self) -> &'static str {
        prob_status_string(&self.result)
    }
}

/// Evaluate a probabilistic query program against `world` in `store`.
///
/// `profile` must denote `logic:ProbabilisticProfile` (the caller routes on this).
///
/// `declared_row_schema`: when `Some`, the result's bindings are validated against
/// the caller's declared schema and the schema is attached via
/// [`crate::result::ReasoningResult::with_declared_row_schema`] as a post-step.
/// When `None`, behaviour is unchanged.
///
/// # Errors
///
/// Returns `Err(String)` on a malformed model (bad probability token, joint
/// probabilities not summing to one, an atom that is both correlated and independent),
/// a `cut` under the probabilistic profile, an unbound probability variable, or a
/// [`crate::result_shape::ContractViolation`] when the declared schema does not
/// match the result bindings.
pub fn evaluate(
    store: &WorldStore,
    world: &str,
    program: &QProgram,
    profile: &str,
    declared_row_schema: Option<crate::result_shape::ResultShape>,
) -> gmeow_errors::Result<ProbAnswer> {
    if !is_probabilistic_profile(profile) {
        return Err(probabilistic_err(format!(
            "probabilistic::evaluate called with non-probabilistic profile {profile:?}"
        )));
    }

    // Cut is operational and belongs only to ProceduralPrologProfile; it has no
    // meaning under the declarative probabilistic semantics. Hard-fail (no silent strip).
    if program
        .rules
        .iter()
        .any(|r| r.body.iter().any(|l| matches!(l, QBodyLit::Cut)))
    {
        return Err(probabilistic_err(
            "program contains cut (`!`) under ProbabilisticProfile; cut is permitted only \
             under ProceduralPrologProfile"
                .to_owned(),
        ));
    }

    // ── Refusal guard: probabilistic facts but no declared model → unknown ────
    // Never silently assume independence over the logic:probability facts.
    if !program.prob_facts.is_empty() && program.prob_model.is_none() {
        let result = prob_result(
            ProbStatus::Unknown,
            world,
            crate::result::ResultPayload::Marginals(vec![]),
        );
        let result = if let Some(schema) = declared_row_schema {
            result
                .with_declared_row_schema(schema)
                .map_err(|v| probabilistic_err(v.to_string()))?
        } else {
            result
        };
        return Ok(ProbAnswer {
            bindings: vec![],
            result,
        });
    }

    // ── Controlled facts: those whose truth a probabilistic choice fixes ──────
    // Probabilistic facts and all atoms named by any joint outcome. These are
    // EXCLUDED from the deterministic set so the enumeration fully controls them
    // (a base triple accidentally also asserted in the EDB must not pin them true).
    let mut controlled: HashSet<Fact> = HashSet::new();
    for pf in &program.prob_facts {
        controlled.insert(atom_to_fact(&pf.atom)?);
    }
    if let Some(QProbModel::Dependency { joints }) = &program.prob_model {
        for j in joints {
            for a in &j.true_atoms {
                controlled.insert(atom_to_fact(a)?);
            }
        }
    }

    // ── Deterministic facts D ─────────────────────────────────────────────────
    // EDB (materialized world) + body-less program facts + confidence-annotated
    // atoms (asserted facts; the confidence value is metadata, never a probability),
    // minus the controlled set.
    let mut deterministic: HashSet<Fact> = HashSet::new();
    for q in store.quads_in_world(world) {
        // q = [subject, predicate, object, world]; predicate is `<iri>` → bare.
        let pred = strip_angle(&q[1]);
        deterministic.insert((pred, q[0].clone(), q[2].clone()));
    }
    for rule in &program.rules {
        if rule.body.is_empty()
            && let Some(f) = ground_atom_to_fact(&rule.head)
        {
            deterministic.insert(f);
        }
    }
    for c in &program.confidences {
        deterministic.insert(atom_to_fact(&c.atom)?);
    }
    for f in &controlled {
        deterministic.remove(f);
    }

    // ── Horn rules (non-fact rules) ───────────────────────────────────────────
    let rules: Vec<&QRule> = program
        .rules
        .iter()
        .filter(|r| !r.body.is_empty())
        .collect();

    // ── Weighted model counting ───────────────────────────────────────────────
    // Accumulate marginal mass per goal binding (keyed by sorted (var,val) pairs).
    // Total choices are STREAMED one at a time (see `for_each_choice`): each
    // subset's fact list is materialized on demand, weighted into the model
    // count, then dropped — so we never hold all `2^N` fact lists at once.
    let mut marginals: BTreeMap<Vec<(String, String)>, f64> = BTreeMap::new();
    for_each_choice(program, &mut |true_facts: &[Fact], p: f64| {
        let mut facts = deterministic.clone();
        for f in true_facts {
            facts.insert(f.clone());
        }
        let model = closure(facts, &rules);
        for binding in goal_bindings(&program.goal, &model) {
            let key: Vec<(String, String)> = binding.into_iter().collect();
            *marginals.entry(key).or_insert(0.0) += p;
        }
    })?;

    // ── Build sorted, rounded, non-zero answer bindings ───────────────────────
    let mut bindings: Vec<ProbBinding> = marginals
        .into_iter()
        .filter_map(|(key, mass)| {
            let prob = round(mass);
            if prob <= 0.0 {
                None
            } else {
                Some(ProbBinding {
                    vars: key.into_iter().collect(),
                    probability: prob,
                })
            }
        })
        .collect();
    // Deterministic order: by the binding pairs, then probability.
    bindings.sort_by(|a, b| {
        let ap: Vec<(&String, &String)> = a.vars.iter().collect();
        let bp: Vec<(&String, &String)> = b.vars.iter().collect();
        ap.cmp(&bp).then(
            a.probability
                .partial_cmp(&b.probability)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });

    let result = prob_result(
        ProbStatus::Ok,
        world,
        crate::result::ResultPayload::Marginals(bindings.clone()),
    );
    let result = if let Some(schema) = declared_row_schema {
        result
            .with_declared_row_schema(schema)
            .map_err(|v| probabilistic_err(v.to_string()))?
    } else {
        result
    };
    Ok(ProbAnswer { bindings, result })
}

/// Stream every total choice as `(facts true in the choice, P(choice))`, invoking
/// `sink` once per choice. Choices are produced one at a time and the fact list is
/// materialized on demand, so the caller never holds all `2^N` subsets at once.
///
/// The choice order, the per-choice fact order, and the per-choice weight (and its
/// scalar product order) are **byte-identical** to the previous materialize-all
/// implementation — the only change is when, not how, each subset is built.
///
/// ## SIMD (`std::simd`) — vertical-lane vectorization
/// The per-subset weight is `∏ p_i (in subset) · ∏ (1 − p_i) (out of subset)`,
/// accumulated as a sequential scalar `f64` in index order `i = 0..n`.
/// [`power_set_weights`] vectorizes this **vertically**: each SIMD lane carries
/// one mask's running product, advancing four masks in lock-step over the same
/// sequential `i = 0..n` loop. Because there is no horizontal cross-lane
/// reduction (which would reassociate a single product and drift the last ULP),
/// every lane reproduces the exact scalar multiply order — so the marginals stay
/// the same as the exact-tested goldens (`assert_eq!(.., Some(0.75))` and the
/// `conformance/logic/cases/profiles/probabilistic-*` cases). On-demand mask
/// materialization (`push_facts_for_mask`, streamed through `sink`) still avoids
/// the upfront `2^N Vec<Fact>` allocation.
fn for_each_choice(
    program: &QProgram,
    sink: &mut dyn FnMut(&[Fact], f64),
) -> gmeow_errors::Result<()> {
    // Independent probabilistic facts (always present; under a dependency model
    // they are the facts OUTSIDE the correlated set). Each fact must be declared
    // at most once: a duplicate `:- probability(...)` would otherwise be counted
    // as two independent variables, silently corrupting the marginal — so a repeat
    // is a hard failure, not a merge.
    let mut indep: Vec<(Fact, f64)> = Vec::new();
    let mut seen: HashSet<Fact> = HashSet::new();
    for pf in &program.prob_facts {
        let fact = atom_to_fact(&pf.atom)?;
        if !seen.insert(fact.clone()) {
            return Err(probabilistic_err(format!(
                "duplicate probability(...) declaration for fact {fact:?}; each probabilistic \
                 fact must be declared at most once"
            )));
        }
        indep.push((fact, parse_prob(&pf.prob)?));
    }

    // Exact enumeration is 2^N over the independent facts; refuse an N that would
    // overflow the `1u64 << N` shift or exhaust memory/CPU (see MAX_INDEPENDENT_FACTS).
    if indep.len() > MAX_INDEPENDENT_FACTS {
        return Err(probabilistic_err(format!(
            "too many independent probabilistic facts ({}); exact weighted model counting is \
             limited to {MAX_INDEPENDENT_FACTS} to avoid overflow and out-of-memory",
            indep.len()
        )));
    }

    // The independent power set as `(u64 mask, product weight)` — masks only, so we
    // never materialize all `2^N` fact lists. Each subset's facts are built on
    // demand via `push_facts_for_mask` at the single point a consumer needs them.
    let indep_masks = power_set_weights(&indep);

    match &program.prob_model {
        Some(QProbModel::Dependency { joints }) => {
            // Correlated atoms come from the joint table; they must be disjoint
            // from the independent prob_facts (a fact is correlated XOR independent).
            let correlated: HashSet<Fact> = joints
                .iter()
                .flat_map(|j| j.true_atoms.iter())
                .map(atom_to_fact)
                .collect::<Result<HashSet<_>, _>>()?;
            for (f, _) in &indep {
                if correlated.contains(f) {
                    return Err(probabilistic_err(format!(
                        "fact {f:?} is declared both as an independent probability(...) and in a \
                         joint(...) outcome; it must be one or the other"
                    )));
                }
            }
            // Joint outcome probabilities must sum to one.
            let mut sum = 0.0;
            let mut joint_choices: Vec<(Vec<Fact>, f64)> = Vec::with_capacity(joints.len());
            for j in joints {
                let p = parse_prob(&j.prob)?;
                sum += p;
                let facts: Vec<Fact> = j
                    .true_atoms
                    .iter()
                    .map(atom_to_fact)
                    .collect::<Result<Vec<_>, _>>()?;
                joint_choices.push((facts, p));
            }
            if (sum - 1.0).abs() > EPS {
                return Err(probabilistic_err(format!(
                    "joint(...) outcome probabilities must sum to 1.0; got {sum}"
                )));
            }
            // Cross product: each joint outcome × each independent subset, streamed.
            // The combined fact list is built in a reusable scratch buffer and
            // dropped after `sink` returns, preserving the exact historical fact
            // order (`jfacts` then the mask facts) and the exact `jp * ip` order.
            let mut scratch: Vec<Fact> = Vec::new();
            for (jfacts, jp) in &joint_choices {
                for (mask, ip) in &indep_masks {
                    scratch.clear();
                    scratch.extend(jfacts.iter().cloned());
                    push_facts_for_mask(&indep, *mask, &mut scratch);
                    sink(&scratch, jp * ip);
                }
            }
            Ok(())
        }
        Some(QProbModel::FullIndependence) | None => {
            // Pure independence (or a degenerate deterministic query with no prob
            // facts → the single empty choice with weight 1.0). Materialize each
            // subset's fact list on demand from its mask in the same i=0..n order,
            // reusing one scratch buffer and dropping it after each `sink` call.
            let mut scratch: Vec<Fact> = Vec::new();
            for (mask, w) in &indep_masks {
                scratch.clear();
                push_facts_for_mask(&indep, *mask, &mut scratch);
                sink(&scratch, *w);
            }
            Ok(())
        }
    }
}

/// SIMD lane width for the power-set weight computation. `f64x4` is the widest
/// width that maps to a single AVX/AVX2 register on the portable `x86-64-v3`
/// floor this crate targets (4×f64 = 256 bits = one YMM register).
const LANES: usize = 4;

/// The power set of `items` as `(u64 mask, product weight)`, one entry per subset:
/// `weight = ∏_{i in subset} p_i · ∏_{i not in subset} (1 − p_i)`.
///
/// Returns the single empty subset (mask `0`) with weight `1.0` when `items` is
/// empty. Only the `2^N` masks + weights are allocated here — the `Vec<Fact>` for
/// each subset is built on demand by [`push_facts_for_mask`], not up front.
///
/// # Vectorization (`std::simd`)
///
/// This is a *vertical* SIMD over masks: lane `j ∈ 0..LANES` holds the running
/// product for mask `base + j`. The masks of `LANES` consecutive subsets are
/// processed together. For each item `i` (outer, sequential `i = 0..n`) every
/// lane multiplies its product by `p_i` if bit `i` is set in that lane's mask,
/// else by `q_i = 1 − p_i`. The choice is a SIMD `select` over a per-lane mask
/// vector built from `(base + j) & (1<<i)`.
///
/// Because the per-item loop `i = 0..n` is still sequential and identical across
/// all lanes, **each lane's product is the same sequence of multiplies, in the
/// same order, as the historical scalar loop** — the only thing SIMD changes is
/// that four masks' products advance in step. There is **no horizontal reduction
/// across lanes** (that would reassociate a single product and drift the ULP);
/// every lane is an independent, in-order product. The blend chooses between two
/// already-computed scalars per lane, it does not sum across lanes. So the result
/// is expected to be bit-identical to the scalar product — see the analysis in
/// the PR/report.
fn power_set_weights(items: &[(Fact, f64)]) -> Vec<(u64, f64)> {
    let n = items.len();
    // Precompute the complement `1 - p_i` once per item; same value, same order.
    let p: Vec<f64> = items.iter().map(|(_, p)| *p).collect();
    let q: Vec<f64> = items.iter().map(|(_, p)| 1.0 - *p).collect();

    let total: u64 = 1u64 << n; // number of subsets (n ≤ MAX_INDEPENDENT_FACTS = 20)
    let mut out: Vec<(u64, f64)> = Vec::with_capacity(total as usize);

    // ── Vectorized body: process LANES consecutive masks at a time ────────────
    let chunks = total / LANES as u64; // number of full SIMD chunks
    for c in 0..chunks {
        let base = c * LANES as u64;
        // Per-lane mask values: [base, base+1, base+2, base+3].
        let mut lane_masks = [0u64; LANES];
        for (j, m) in lane_masks.iter_mut().enumerate() {
            *m = base + j as u64;
        }
        let masks_v: Simd<u64, LANES> = Simd::from_array(lane_masks);

        // Running product per lane, in-order over i = 0..n (one multiply per item).
        let mut weight_v: Simd<f64, LANES> = Simd::splat(1.0_f64);
        for i in 0..n {
            // bit_v[lane] = (mask_lane >> i) & 1  ; compare == 1 → SIMD bool mask.
            let bit_v = (masks_v >> Simd::splat(i as u64)) & Simd::splat(1u64);
            let set_mask = bit_v.simd_eq(Simd::splat(1u64));
            // Choose p_i where bit set, q_i where clear — both lanes multiply by a
            // single scalar broadcast, so each lane's product order is unchanged.
            let factor = set_mask.select(Simd::splat(p[i]), Simd::splat(q[i]));
            weight_v *= factor;
        }

        let weights = weight_v.to_array();
        for (j, w) in weights.iter().enumerate() {
            out.push((base + j as u64, *w));
        }
    }

    // ── Scalar tail: the remaining masks (total not a multiple of LANES) ──────
    // Same in-order i = 0..n product as the vector lanes. With n ≥ 2 the count
    // 2^n is a multiple of 4 so this never runs; for n = 0 (1 mask) and n = 1
    // (2 masks) the whole computation falls through to here.
    for mask in (chunks * LANES as u64)..total {
        let mut weight = 1.0_f64;
        for i in 0..n {
            if mask & (1 << i) != 0 {
                weight *= p[i];
            } else {
                weight *= q[i];
            }
        }
        out.push((mask, weight));
    }

    out
}

/// Push the in-subset facts selected by `mask` onto `out`, in index order
/// `i = 0..n` — the same order the previous `power_set_weights` built each
/// subset's `Vec<Fact>`, so any downstream fact ordering is preserved.
fn push_facts_for_mask(items: &[(Fact, f64)], mask: u64, out: &mut Vec<Fact>) {
    for (i, (f, _)) in items.iter().enumerate() {
        if mask & (1 << i) != 0 {
            out.push(f.clone());
        }
    }
}

/// Least-Herbrand-model closure of `facts` under the Horn `rules` (naive fixpoint).
///
/// `rules` carry only [`QBodyLit::Atom`] bodies (cut is rejected before this is
/// called). Bodies and heads are binary atoms; head variables must be bound by the
/// body (an unbound head variable derives nothing).
fn closure(mut facts: HashSet<Fact>, rules: &[&QRule]) -> HashSet<Fact> {
    loop {
        let mut added = false;
        for rule in rules {
            let body: Vec<QAtom> = rule
                .body
                .iter()
                .filter_map(|l| l.clone().into_atom())
                .collect();
            for binding in solve_conjunction(&body, &facts) {
                if let Some(f) = instantiate_head(&rule.head, &binding)
                    && facts.insert(f)
                {
                    added = true;
                }
            }
        }
        if !added {
            break;
        }
    }
    facts
}

/// All variable bindings satisfying the conjunction `atoms` over `facts`.
fn solve_conjunction(atoms: &[QAtom], facts: &HashSet<Fact>) -> Vec<BTreeMap<String, String>> {
    let mut partials = vec![BTreeMap::new()];
    for atom in atoms {
        let mut next: Vec<BTreeMap<String, String>> = Vec::new();
        for binding in &partials {
            for fact in facts {
                if fact.0 != atom.pred {
                    continue;
                }
                if let Some(b2) = try_match(atom, fact, binding) {
                    next.push(b2);
                }
            }
        }
        partials = next;
        if partials.is_empty() {
            break;
        }
    }
    partials
}

/// Try to match `atom`'s two args against `fact`'s (subject, object) under `binding`.
///
/// Returns the extended binding on success, `None` on a constant clash or variable
/// inconsistency. Predicate equality is the caller's responsibility.
fn try_match(
    atom: &QAtom,
    fact: &Fact,
    binding: &BTreeMap<String, String>,
) -> Option<BTreeMap<String, String>> {
    let comps = [&fact.1, &fact.2];
    let mut b = binding.clone();
    for (i, term) in atom.args.iter().enumerate() {
        let comp = comps[i];
        match term {
            QTerm::Const(c) => {
                if c != comp {
                    return None;
                }
            }
            QTerm::Var(v) => match b.get(v) {
                Some(bound) if bound != comp => return None,
                Some(_) => {}
                None => {
                    b.insert(v.clone(), comp.clone());
                }
            },
            // A bare numeric operand only matches its own decimal text. Probabilistic
            // programs never carry builtins (gated), so this is for exhaustiveness.
            QTerm::Num(n) => {
                if n.to_string() != *comp {
                    return None;
                }
            }
            // A structured term never appears in a probabilistic (flat) program.
            QTerm::Struct(_) => return None,
        }
    }
    Some(b)
}

/// Instantiate a binary head atom to a ground fact under `binding`.
///
/// Returns `None` if a head variable is unbound (no derivation).
fn instantiate_head(head: &QAtom, binding: &BTreeMap<String, String>) -> Option<Fact> {
    let mut comps: Vec<String> = Vec::with_capacity(2);
    for term in &head.args {
        match term {
            QTerm::Const(c) => comps.push(c.clone()),
            QTerm::Var(v) => comps.push(binding.get(v)?.clone()),
            QTerm::Num(n) => comps.push(n.to_string()),
            QTerm::Struct(_) => return None,
        }
    }
    Some((head.pred.clone(), comps[0].clone(), comps[1].clone()))
}

/// All distinct goal-variable bindings satisfied by `model`.
fn goal_bindings(goal: &QGoal, model: &HashSet<Fact>) -> Vec<BTreeMap<String, String>> {
    let solved = solve_conjunction(&goal.atoms, model);
    // Deduplicate (the same binding may be produced by multiple fact orders).
    let set: BTreeSet<Vec<(String, String)>> = solved
        .into_iter()
        .map(|b| b.into_iter().collect::<Vec<_>>())
        .collect();
    set.into_iter().map(|v| v.into_iter().collect()).collect()
}

// ── small helpers ─────────────────────────────────────────────────────────────

/// Convert a (possibly variable-bearing) atom to a ground [`Fact`], erroring on a
/// variable — used for `probability`/`joint`/`confidence` atoms, which must be ground.
fn atom_to_fact(atom: &QAtom) -> gmeow_errors::Result<Fact> {
    ground_atom_to_fact(atom).ok_or_else(|| {
        probabilistic_err(format!(
            "atom {:?} must be ground (no variables) in a probabilistic declaration",
            atom.pred
        ))
    })
}

/// Convert an atom to a ground [`Fact`], returning `None` if any arg is a variable.
fn ground_atom_to_fact(atom: &QAtom) -> Option<Fact> {
    let mut comps: Vec<String> = Vec::with_capacity(2);
    for term in &atom.args {
        match term {
            QTerm::Const(c) => comps.push(c.clone()),
            QTerm::Var(_) => return None,
            // A ground probabilistic atom never carries a bare number; treat it as a
            // non-ground/invalid term for fact conversion.
            QTerm::Num(_) => return None,
            QTerm::Struct(_) => return None,
        }
    }
    if comps.len() != 2 {
        return None;
    }
    Some((atom.pred.clone(), comps[0].clone(), comps[1].clone()))
}

/// Parse a probability token to `f64` (already range-validated by the parser).
fn parse_prob(tok: &str) -> gmeow_errors::Result<f64> {
    tok.trim()
        .parse::<f64>()
        .map_err(|e| probabilistic_err(format!("probability token {tok:?} is not a decimal: {e}")))
}

/// Strip surrounding angle brackets from an `<iri>` string; pass other forms through.
fn strip_angle(s: &str) -> String {
    if s.starts_with('<') && s.ends_with('>') && s.len() >= 2 {
        s[1..s.len() - 1].to_owned()
    } else {
        s.to_owned()
    }
}

/// Round a marginal to [`ROUND_DECIMALS`] decimals and clamp to `[0, 1]`.
fn round(p: f64) -> f64 {
    let factor = 10f64.powi(ROUND_DECIMALS);
    let r = (p * factor).round() / factor;
    r.clamp(0.0, 1.0)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_ir::parse_query_program;

    const PROFILE: &str = "https://blackcatinformatics.ca/logic/ProbabilisticProfile";
    const WORLD: &str = "https://example.org/prob/world";
    const BASE: &str = "https://example.org/prob/";

    fn const_iri(local: &str) -> String {
        format!("<{BASE}{local}>")
    }

    /// Find the marginal for a single-variable binding `var = <BASE+local>`.
    fn marginal_for(ans: &ProbAnswer, var: &str, local: &str) -> Option<f64> {
        ans.bindings
            .iter()
            .find(|b| b.vars.get(var) == Some(&const_iri(local)))
            .map(|b| b.probability)
    }

    // ── AC1: independent marginals ────────────────────────────────────────────

    #[test]
    fn independent_or_marginal_is_noisy_or() {
        // wet :- rain.  wet :- sprinkler.   rain=0.5 (indep), sprinkler=0.5 (indep).
        // P(wet) = 1 - (1-0.5)(1-0.5) = 0.75.
        let store = WorldStore::new();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n\
             :- probability(ex:rain(ex:today, ex:true), 0.5).\n\
             :- probability(ex:sprinkler(ex:today, ex:true), 0.5).\n\
             ex:wet(D, ex:true) :- ex:rain(D, ex:true).\n\
             ex:wet(D, ex:true) :- ex:sprinkler(D, ex:true).\n\
             ?- ex:wet(ex:today, X).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let ans = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap();
        assert_eq!(ans.status_str(), "ok");
        assert_eq!(ans.bindings.len(), 1, "exactly one binding: {ans:?}");
        assert_eq!(marginal_for(&ans, "X", "true"), Some(0.75));
    }

    #[test]
    fn independent_and_marginal_is_product() {
        // both :- a, b.   a=0.5, b=0.4 independent → P(both) = 0.2.
        let store = WorldStore::new();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n\
             :- probability(ex:a(ex:s, ex:on), 0.5).\n\
             :- probability(ex:b(ex:s, ex:on), 0.4).\n\
             ex:both(S, ex:on) :- ex:a(S, ex:on), ex:b(S, ex:on).\n\
             ?- ex:both(ex:s, X).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let ans = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap();
        assert_eq!(marginal_for(&ans, "X", "on"), Some(0.2));
    }

    // ── Dependency joint: correlated facts differ from the independence reading ─

    #[test]
    fn dependency_joint_marginal_uses_the_joint() {
        // a and b are perfectly correlated: joint(0.5, a, b), joint(0.5).
        // both :- a, b.  P(both) = 0.5  (vs 0.5*0.5=0.25 under independence).
        let store = WorldStore::new();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- probability_model(dependency).\n\
             :- joint(0.5, ex:a(ex:s, ex:on), ex:b(ex:s, ex:on)).\n\
             :- joint(0.5).\n\
             ex:both(S, ex:on) :- ex:a(S, ex:on), ex:b(S, ex:on).\n\
             ?- ex:both(ex:s, X).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let ans = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap();
        assert_eq!(
            marginal_for(&ans, "X", "on"),
            Some(0.5),
            "perfectly-correlated joint gives 0.5, not the independent 0.25: {ans:?}"
        );
    }

    // ── AC2: confidence is NOT promoted to probability ────────────────────────

    #[test]
    fn confidence_is_not_a_probability_guard() {
        // diagnosis is asserted with confidence 0.9 — under ProbabilisticProfile with
        // a declared model, its marginal MUST be 1.0 (an asserted fact), NEVER 0.9.
        let store = WorldStore::new();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n\
             :- confidence(ex:diagnosis(ex:patient, ex:flu), 0.9).\n\
             ?- ex:diagnosis(ex:patient, X).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let ans = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap();
        assert_eq!(ans.status_str(), "ok");
        let m = marginal_for(&ans, "X", "flu");
        assert_eq!(
            m,
            Some(1.0),
            "confidence must NOT become probability: {ans:?}"
        );
        assert_ne!(m, Some(0.9), "0.9 confidence leaked as a probability");
    }

    // ── No-model refusal guard ────────────────────────────────────────────────

    #[test]
    fn no_declared_model_refuses_with_unknown() {
        // A probability fact with NO probability_model → unknown (never assume independence).
        // The parser allows this; the evaluator refuses.
        let store = WorldStore::new();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- probability(ex:rain(ex:today, ex:true), 0.5).\n\
             ex:wet(D, ex:true) :- ex:rain(D, ex:true).\n\
             ?- ex:wet(ex:today, X).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let ans = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap();
        assert_eq!(ans.status_str(), "unknown");
        assert!(
            ans.bindings.is_empty(),
            "refusal yields no marginals: {ans:?}"
        );
    }

    #[test]
    fn prob_status_string_round_trips() {
        // The typed ReasoningResult losslessly carries the prob status.
        assert_eq!(
            prob_status_string(&prob_result(
                ProbStatus::Ok,
                WORLD,
                crate::result::ResultPayload::Marginals(vec![])
            )),
            ProbStatus::Ok.as_str()
        );
        assert_eq!(
            prob_status_string(&prob_result(
                ProbStatus::Unknown,
                WORLD,
                crate::result::ResultPayload::Marginals(vec![])
            )),
            ProbStatus::Unknown.as_str()
        );
    }

    #[test]
    fn prob_unknown_is_unsupported_not_evaluated() {
        use crate::result::{EvaluationStatus, InformationState};
        // A no-declared-model refusal is unsupported + not-evaluated — explicitly
        // NOT the Belnap `neither`, and distinct from cf's revision-tie unknown.
        let r = prob_result(
            ProbStatus::Unknown,
            WORLD,
            crate::result::ResultPayload::Marginals(vec![]),
        );
        assert_eq!(r.evaluation, EvaluationStatus::Unsupported);
        assert_eq!(r.information, InformationState::NotEvaluated);
        assert!(r.validate().is_ok());
    }

    // ── Deterministic EDB facts participate with probability 1.0 ──────────────

    #[test]
    fn edb_fact_is_deterministic_probability_one() {
        let store = WorldStore::new();
        store.insert_quad(
            WORLD,
            &format!("{BASE}s"),
            &format!("{BASE}known"),
            &format!("{BASE}yes"),
        );
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n\
             ?- ex:known(ex:s, X).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let ans = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap();
        assert_eq!(marginal_for(&ans, "X", "yes"), Some(1.0));
    }

    // ── Duplicate probabilistic fact is rejected (no double-counting) ─────────

    #[test]
    fn duplicate_probability_fact_is_rejected() {
        // The same fact declared twice would be counted as two independent
        // variables, corrupting the marginal — must hard-fail.
        let store = WorldStore::new();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n\
             :- probability(ex:rain(ex:today, ex:yes), 0.5).\n\
             :- probability(ex:rain(ex:today, ex:yes), 0.3).\n\
             ?- ex:rain(ex:today, X).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let err = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap_err();
        assert!(
            err.message().contains("duplicate"),
            "unexpected error: {err}"
        );
    }

    // ── Too many independent facts is refused, not panicked ───────────────────

    #[test]
    fn too_many_independent_facts_is_rejected() {
        // 2^N enumeration is capped: over MAX_INDEPENDENT_FACTS hard-fails with a
        // clear message rather than overflowing the shift or exhausting memory.
        let store = WorldStore::new();
        let mut src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n"
        );
        for i in 0..(MAX_INDEPENDENT_FACTS + 1) {
            src.push_str(&format!(":- probability(ex:f{i}(ex:s, ex:yes), 0.5).\n"));
        }
        src.push_str("?- ex:f0(ex:s, X).\n");
        let prog = parse_query_program(&src).unwrap();
        let err = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap_err();
        assert!(
            err.message().contains("too many"),
            "unexpected error: {err}"
        );
    }

    // ── Cut is rejected under the probabilistic profile ───────────────────────

    #[test]
    fn cut_is_rejected() {
        let store = WorldStore::new();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n\
             ex:p(X, Y) :- ex:q(X, Y), !.\n\
             ?- ex:p(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let err = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap_err();
        assert!(err.message().contains("cut"), "unexpected error: {err}");
    }

    // ── Malformed joint table: probabilities must sum to one ──────────────────

    #[test]
    fn joint_probabilities_must_sum_to_one() {
        let store = WorldStore::new();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- probability_model(dependency).\n\
             :- joint(0.5, ex:a(ex:s, ex:on)).\n\
             :- joint(0.2).\n\
             ?- ex:a(ex:s, X).\n"
        );
        let prog = parse_query_program(&src).unwrap();
        let err = evaluate(&store, WORLD, &prog, PROFILE, None).unwrap_err();
        assert!(
            err.message().contains("sum to 1"),
            "unexpected error: {err}"
        );
    }

    // ── SIMD bit-identity probe: power_set_weights matches independent scalar ──
    //
    // Acceptance criterion: for every n in 0..=20 and every mask in 0..(1<<n),
    // the SIMD-produced weight is bit-identical (f64::to_bits() equal, max_ulp == 0)
    // to an independent scalar reference that re-derives the same product in the
    // same i = 0..n multiply order.

    #[test]
    fn power_set_weights_simd_bit_identical_to_scalar() {
        // ── Independent scalar reference ──────────────────────────────────────
        // Computes, for each mask in 0..(1<<n), the product
        //   ∏_{i: bit i set} p_i · ∏_{i: bit i clear} (1 - p_i)
        // iterating i in 0..n order — the SAME multiply order the SIMD lanes use.
        // This is written from scratch (no call to power_set_weights) so that the
        // test is a genuine cross-check, not a tautology.
        let scalar_ref = |items: &[(Fact, f64)]| -> Vec<(u64, f64)> {
            let n = items.len();
            let p: Vec<f64> = items.iter().map(|(_, prob)| *prob).collect();
            let q: Vec<f64> = p.iter().map(|&pi| 1.0 - pi).collect();
            let total: u64 = 1u64 << n;
            let mut out = Vec::with_capacity(total as usize);
            for mask in 0..total {
                let mut weight = 1.0_f64;
                for i in 0..n {
                    if mask & (1u64 << i) != 0 {
                        weight *= p[i];
                    } else {
                        weight *= q[i];
                    }
                }
                out.push((mask, weight));
            }
            out
        };

        // ── Deterministic, non-trivial, varied probabilities ──────────────────
        // p_i = 0.05 + 0.9 * ((i * 7 + 3) % 19) / 19  — all strictly in (0, 1).
        // Using 20 as the upper bound covers n = 0..=20 (2^20 ≈ 1.05 M masks).
        // n = 0 → 1 mask, n = 1 → 2 masks: both fall through to the scalar tail
        //   (total < LANES = 4 so chunks = 0).
        // n ≥ 2 → 2^n is a multiple of 4, exercising the full SIMD chunked path.
        // All n up to 20 are swept to satisfy the issue's acceptance criterion.
        for n in 0usize..=20 {
            let items: Vec<(Fact, f64)> = (0..n)
                .map(|i| {
                    let raw = 0.05_f64 + 0.9_f64 * ((i * 7 + 3) % 19) as f64 / 19.0_f64;
                    // Clamp defensively to strict (0, 1) — the formula already satisfies
                    // this for all i, but explicit clamping documents the intent.
                    let prob = raw.clamp(f64::MIN_POSITIVE, 1.0_f64 - f64::EPSILON);
                    let fact: Fact = (
                        "https://example.org/simd/pred".to_string(),
                        format!("<https://example.org/simd/s{i}>"),
                        format!("<https://example.org/simd/o{i}>"),
                    );
                    (fact, prob)
                })
                .collect();

            let simd_result = power_set_weights(&items);
            let scalar_result = scalar_ref(&items);

            assert_eq!(
                simd_result.len(),
                scalar_result.len(),
                "n={n}: length mismatch: simd={} scalar={}",
                simd_result.len(),
                scalar_result.len()
            );

            for ((simd_mask, simd_w), (scalar_mask, scalar_w)) in
                simd_result.iter().zip(scalar_result.iter())
            {
                assert_eq!(
                    simd_mask, scalar_mask,
                    "n={n} mask={simd_mask}: mask ordering diverged"
                );
                assert_eq!(
                    simd_w.to_bits(),
                    scalar_w.to_bits(),
                    "n={n} mask={simd_mask}: SIMD weight {simd_w:?} (bits={:#018x}) \
                     != scalar weight {scalar_w:?} (bits={:#018x}); max_ulp > 0",
                    simd_w.to_bits(),
                    scalar_w.to_bits()
                );
            }
        }
    }

    // ── row_schema facet: declared schema is validated and attached ────────────

    /// A matching schema: the result binds IRI-valued `X`; the schema declares
    /// `Required Iri` for `X`. Schema is attached and `row_schema.is_some()`.
    #[test]
    fn declared_schema_matching_attaches_row_schema() {
        use gmeow_logic_compile::result_shape::{
            ColumnBinding, ColumnKind, ResultColumn, ResultShape, RowCardinality,
        };

        let store = WorldStore::new();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n\
             :- probability(ex:rain(ex:today, ex:true), 0.5).\n\
             ex:wet(D, ex:true) :- ex:rain(D, ex:true).\n\
             ?- ex:wet(ex:today, X).\n"
        );
        let prog = parse_query_program(&src).unwrap();

        // Declare: one Required IRI column `X`, any number of rows.
        let schema = ResultShape::new(
            vec![ResultColumn {
                var: "X".to_owned(),
                kind: ColumnKind::Iri,
                binding: ColumnBinding::Required,
            }],
            RowCardinality::Contains,
        );

        let ans = evaluate(&store, WORLD, &prog, PROFILE, Some(schema)).unwrap();
        assert_eq!(ans.status_str(), "ok");
        assert!(
            ans.result.row_schema.is_some(),
            "row_schema must be attached when a declared schema matches"
        );
    }

    /// A mismatching schema: the result binds IRI-valued `X`; the schema declares
    /// `Required BlankNode` for `X`. Must return Err (ContractViolation propagated).
    #[test]
    fn declared_schema_mismatch_returns_err() {
        use gmeow_logic_compile::result_shape::{
            ColumnBinding, ColumnKind, ResultColumn, ResultShape, RowCardinality,
        };

        let store = WorldStore::new();
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             :- probability_model(full_independence).\n\
             :- probability(ex:rain(ex:today, ex:true), 0.5).\n\
             ex:wet(D, ex:true) :- ex:rain(D, ex:true).\n\
             ?- ex:wet(ex:today, X).\n"
        );
        let prog = parse_query_program(&src).unwrap();

        // Declare: `X` must be a blank-node — but the binding is an IRI → mismatch.
        let schema = ResultShape::new(
            vec![ResultColumn {
                var: "X".to_owned(),
                kind: ColumnKind::BlankNode,
                binding: ColumnBinding::Required,
            }],
            RowCardinality::Contains,
        );

        let err = evaluate(&store, WORLD, &prog, PROFILE, Some(schema)).unwrap_err();
        assert!(
            err.message().contains("result-shape violation"),
            "ContractViolation must be propagated as Err: {err}"
        );
    }
}
