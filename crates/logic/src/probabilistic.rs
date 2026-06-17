// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Probabilistic / weighted inference under `logic:ProbabilisticProfile` (#506, v6).
//!
//! This is the final rung of the Logic EPIC (#497): exact marginal inference over
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

use crate::profile_gate::is_probabilistic_profile;
use crate::query_ir::{QAtom, QBodyLit, QGoal, QProbModel, QProgram, QRule, QTerm};
use crate::store::WorldStore;

/// Tolerance for the joint-table sum-to-one check and for treating a marginal as zero.
const EPS: f64 = 1e-9;
/// Decimals of precision the returned marginals are rounded to, so the float matches
/// the captured golden JSON byte-for-byte under `json.dumps` shortest-repr comparison.
const ROUND_DECIMALS: i32 = 10;

/// A ground RDF fact: `(bare predicate IRI, subject const, object const)`.
///
/// Subject/object are in canonical `<iri>`/n3-literal form — the same surface
/// [`QTerm::Const`] and [`crate::provenance::term_n3`] use — so matching is string
/// equality.
type Fact = (String, String, String);

/// Status of a probabilistic resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbStatus {
    /// Marginals computed.
    Ok,
    /// Refused: probabilistic facts present with no declared probability model.
    Unknown,
}

impl ProbStatus {
    /// Canonical lowercase wire string.
    pub fn as_str(self) -> &'static str {
        match self {
            ProbStatus::Ok => "ok",
            ProbStatus::Unknown => "unknown",
        }
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
    /// The resolution status.
    pub status: ProbStatus,
}

/// Evaluate a probabilistic query program against `world` in `store`.
///
/// `profile` must denote `logic:ProbabilisticProfile` (the caller routes on this).
///
/// # Errors
///
/// Returns `Err(String)` on a malformed model (bad probability token, joint
/// probabilities not summing to one, an atom that is both correlated and independent),
/// a `cut` under the probabilistic profile, or an unbound probability variable.
pub fn evaluate(
    store: &WorldStore,
    world: &str,
    program: &QProgram,
    profile: &str,
) -> Result<ProbAnswer, String> {
    if !is_probabilistic_profile(profile) {
        return Err(format!(
            "probabilistic::evaluate called with non-probabilistic profile {profile:?}"
        ));
    }

    // Cut is operational and belongs only to ProceduralPrologProfile; it has no
    // meaning under the declarative probabilistic semantics. Hard-fail (no silent strip).
    if program
        .rules
        .iter()
        .any(|r| r.body.iter().any(|l| matches!(l, QBodyLit::Cut)))
    {
        return Err(
            "program contains cut (`!`) under ProbabilisticProfile; cut is permitted only \
             under ProceduralPrologProfile"
                .to_owned(),
        );
    }

    // ── Refusal guard: probabilistic facts but no declared model → unknown ────
    // Never silently assume independence over the logic:probability facts.
    if !program.prob_facts.is_empty() && program.prob_model.is_none() {
        return Ok(ProbAnswer {
            bindings: vec![],
            status: ProbStatus::Unknown,
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
        if rule.body.is_empty() {
            if let Some(f) = ground_atom_to_fact(&rule.head) {
                deterministic.insert(f);
            }
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

    // ── Enumerate total choices ───────────────────────────────────────────────
    let choices = enumerate_choices(program)?;

    // ── Weighted model counting ───────────────────────────────────────────────
    // Accumulate marginal mass per goal binding (keyed by sorted (var,val) pairs).
    let mut marginals: BTreeMap<Vec<(String, String)>, f64> = BTreeMap::new();
    for (true_facts, p) in &choices {
        let mut facts = deterministic.clone();
        for f in true_facts {
            facts.insert(f.clone());
        }
        let model = closure(facts, &rules);
        for binding in goal_bindings(&program.goal, &model) {
            let key: Vec<(String, String)> = binding.into_iter().collect();
            *marginals.entry(key).or_insert(0.0) += p;
        }
    }

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

    Ok(ProbAnswer {
        bindings,
        status: ProbStatus::Ok,
    })
}

/// Enumerate every total choice as `(facts true in the choice, P(choice))`.
fn enumerate_choices(program: &QProgram) -> Result<Vec<(Vec<Fact>, f64)>, String> {
    // Independent probabilistic facts (always present; under a dependency model
    // they are the facts OUTSIDE the correlated set).
    let mut indep: Vec<(Fact, f64)> = Vec::new();
    for pf in &program.prob_facts {
        indep.push((atom_to_fact(&pf.atom)?, parse_prob(&pf.prob)?));
    }

    // The independent power set, each subset with its product weight.
    let indep_choices = power_set_weights(&indep);

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
                    return Err(format!(
                        "fact {f:?} is declared both as an independent probability(...) and in a \
                         joint(...) outcome; it must be one or the other"
                    ));
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
                return Err(format!(
                    "joint(...) outcome probabilities must sum to 1.0; got {sum}"
                ));
            }
            // Cross product: each joint outcome × each independent subset.
            let mut out = Vec::with_capacity(joint_choices.len() * indep_choices.len());
            for (jfacts, jp) in &joint_choices {
                for (ifacts, ip) in &indep_choices {
                    let mut facts = jfacts.clone();
                    facts.extend(ifacts.iter().cloned());
                    out.push((facts, jp * ip));
                }
            }
            Ok(out)
        }
        Some(QProbModel::FullIndependence) | None => {
            // Pure independence (or a degenerate deterministic query with no prob
            // facts → the single empty choice with weight 1.0).
            Ok(indep_choices)
        }
    }
}

/// The power set of `items` with each subset's product weight:
/// `∏_{i in subset} p_i · ∏_{i not in subset} (1 − p_i)`.
///
/// Returns the single empty subset with weight `1.0` when `items` is empty.
fn power_set_weights(items: &[(Fact, f64)]) -> Vec<(Vec<Fact>, f64)> {
    let n = items.len();
    let mut out = Vec::with_capacity(1usize << n);
    for mask in 0u64..(1u64 << n) {
        let mut facts = Vec::new();
        let mut weight = 1.0_f64;
        for (i, (f, p)) in items.iter().enumerate() {
            if mask & (1 << i) != 0 {
                facts.push(f.clone());
                weight *= *p;
            } else {
                weight *= 1.0 - *p;
            }
        }
        out.push((facts, weight));
    }
    out
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
                if let Some(f) = instantiate_head(&rule.head, &binding) {
                    if facts.insert(f) {
                        added = true;
                    }
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
fn atom_to_fact(atom: &QAtom) -> Result<Fact, String> {
    ground_atom_to_fact(atom).ok_or_else(|| {
        format!(
            "atom {:?} must be ground (no variables) in a probabilistic declaration",
            atom.pred
        )
    })
}

/// Convert an atom to a ground [`Fact`], returning `None` if any arg is a variable.
fn ground_atom_to_fact(atom: &QAtom) -> Option<Fact> {
    let mut comps: Vec<String> = Vec::with_capacity(2);
    for term in &atom.args {
        match term {
            QTerm::Const(c) => comps.push(c.clone()),
            QTerm::Var(_) => return None,
        }
    }
    if comps.len() != 2 {
        return None;
    }
    Some((atom.pred.clone(), comps[0].clone(), comps[1].clone()))
}

/// Parse a probability token to `f64` (already range-validated by the parser).
fn parse_prob(tok: &str) -> Result<f64, String> {
    tok.trim()
        .parse::<f64>()
        .map_err(|e| format!("probability token {tok:?} is not a decimal: {e}"))
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
        let ans = evaluate(&store, WORLD, &prog, PROFILE).unwrap();
        assert_eq!(ans.status, ProbStatus::Ok);
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
        let ans = evaluate(&store, WORLD, &prog, PROFILE).unwrap();
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
        let ans = evaluate(&store, WORLD, &prog, PROFILE).unwrap();
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
        let ans = evaluate(&store, WORLD, &prog, PROFILE).unwrap();
        assert_eq!(ans.status, ProbStatus::Ok);
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
        let ans = evaluate(&store, WORLD, &prog, PROFILE).unwrap();
        assert_eq!(ans.status, ProbStatus::Unknown);
        assert!(
            ans.bindings.is_empty(),
            "refusal yields no marginals: {ans:?}"
        );
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
        let ans = evaluate(&store, WORLD, &prog, PROFILE).unwrap();
        assert_eq!(marginal_for(&ans, "X", "yes"), Some(1.0));
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
        let err = evaluate(&store, WORLD, &prog, PROFILE).unwrap_err();
        assert!(err.contains("cut"), "unexpected error: {err}");
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
        let err = evaluate(&store, WORLD, &prog, PROFILE).unwrap_err();
        assert!(err.contains("sum to 1"), "unexpected error: {err}");
    }
}
