// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! FOL-negation reduction + EL/DL-fragment lowering of a parsed TPTP problem into
//! a world-scoped OWL-RDF EDB the native DL consistency path decides.
//!
//! ## Why the DL projection (not the Horn clausifier)
//!
//! Deciding a first-order problem by **refutation** means showing
//! `premises ∧ ¬conjecture` is unsatisfiable. The native binary-Horn clausifier
//! cannot express that unsatisfiability at all: a disjointness/negation constraint
//! normalizes to an all-negative (headless) clause it drops as residue, so it
//! would report the clash-bearing problem *consistent-but-incomplete* — a missed
//! refutation. The only native path that soundly detects unsatisfiability is the
//! DL consistency calculus, whose clash rule fires when an individual is forced
//! into two `owl:disjointWith` classes (→ `owl:Nothing`). So this lowerer projects
//! the EL/DL-expressible fragment onto that calculus.
//!
//! ## The reduction
//!
//! * **Premises / negated conjectures** are asserted directly:
//!   * a universal implication `∀X.(C(X) → D(X))` → `C rdfs:subClassOf D`;
//!   * a universal disjointness `∀X.¬(C(X) ∧ D(X))` (or `∀X.(C(X) → ¬D(X))`) →
//!     `C owl:disjointWith D`;
//!   * a ground unary atom `C(a)` → `a rdf:type C`;
//!   * a ground binary atom `r(a,b)` → the role triple `a r b`.
//! * A **conjecture** is negated by refutation through the SHARED conclusion-shape
//!   negation calculus ([`gmeow_logic::entail`]) — the same waist the RDF-conclusion
//!   entailment path uses, so the reduction and its soundness convention live in ONE
//!   place:
//!   * a ground unary `C(a)` → [`ConclusionShape::GroundType`] → assert a counter-model
//!     `a ∈ C̄` with `C owl:disjointWith C̄`; the ontology is inconsistent iff the
//!     premises entail `C(a)`;
//!   * a subclass `∀X.(C(X) → D(X))` → [`ConclusionShape::SubClassOf`] → its negation
//!     `∃X.(C(X) ∧ ¬D(X))`, witnessed by one fresh individual `w`.
//!
//! Fresh symbols are minted by the shared [`gmeow_logic::entail::Minter`] in a
//! reserved namespace with a content-addressed suffix, and the minter HARD-FAILS if
//! the problem vocabulary already contains a reserved IRI — sound for arbitrary IRIs
//! (a plain string suffix is not).
//!
//! ## The fragment boundary is a gap, never a wrong answer
//!
//! Any shape outside this fragment — a disjunctive/existential premise, a
//! propositional atom, a binary-predicate conjecture (role negation is not
//! EL-expressible), an alternating quantifier — is a [`LoweringGap`]: the caller
//! records a DlGap ledger row. A gap is an honest "our engine cannot express
//! this", categorically distinct from the oracle's `incomplete`.

use std::collections::BTreeSet;

use gmeow_logic::entail::{self, ConclusionShape, Minter};
use gmeow_logic_compile::ir::{Formula, Term};

use crate::external::lower::premise_ds_to_world_nquads;
use crate::external::status::ExternalOutcome;
use crate::external::tptp::parser::{AnnotatedFormula, TptpRole};

/// The RDF `type` predicate.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The RDFS `subClassOf` predicate.
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
/// The OWL `disjointWith` predicate (drives the native DL clash rule).
const OWL_DISJOINTWITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";

/// A well-formed but out-of-fragment problem: the native engine cannot express
/// this construct, so the caller records a DlGap ledger row (never `incomplete`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweringGap {
    /// Why the problem is outside the EL/DL-expressible fragment.
    pub reason: String,
}

impl std::fmt::Display for LoweringGap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "TPTP problem outside the EL/DL fragment: {}",
            self.reason
        )
    }
}

impl std::error::Error for LoweringGap {}

/// A TPTP problem lowered to a world-scoped OWL-RDF EDB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredProblem {
    /// The single world IRI every EDB quad is scoped under.
    pub world_iri: String,
    /// The world-scoped N-Quads EDB text (sorted, deduped, trailing newline).
    pub input_nq: String,
    /// The EDB quad count.
    pub quad_count: usize,
}

/// Lower a parsed TPTP problem into a world-scoped OWL-RDF EDB.
///
/// # Errors
/// [`LoweringGap`] when the problem uses a construct outside the EL/DL-expressible
/// fragment — the caller turns this into a capability-gap ledger row.
pub fn lower_problem(
    formulas: &[AnnotatedFormula],
    world_iri: &str,
) -> Result<LoweredProblem, LoweringGap> {
    // Build the sound fresh-symbol minter over the whole problem vocabulary, so a
    // minted complement/witness can never collide with a problem symbol (the minter
    // hard-fails on a reserved-namespace collision).
    let mut vocab: BTreeSet<String> = BTreeSet::new();
    for af in formulas {
        collect_formula_iris(&af.formula, &mut vocab);
    }
    let minter = Minter::new(&vocab).map_err(|e| LoweringGap {
        reason: format!("entailment minter rejected the problem vocabulary: {e}"),
    })?;

    let mut triples: Vec<(String, String, String)> = Vec::new();
    for af in formulas {
        match af.role {
            TptpRole::Premise | TptpRole::NegatedConjecture => {
                lower_assertion(&af.formula, &mut triples)?;
            }
            TptpRole::Conjecture => {
                lower_negated_conjecture(&af.formula, &minter, &mut triples)?;
            }
        }
    }
    if triples.is_empty() {
        return Err(LoweringGap {
            reason: "problem lowered to zero EDB triples (a vacuous consistency check \
                     is not permitted)"
                .into(),
        });
    }

    // Reuse the shared lowering waist: build a default-graph dataset via the native
    // N-Triples codec, then world-scope it. All terms are IRIs, so the emitted
    // N-Triples needs no escaping.
    let nt: String = triples
        .iter()
        .map(|(s, p, o)| format!("<{s}> <{p}> <{o}> .\n"))
        .collect();
    let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", Some(world_iri))
        .map_err(|e| LoweringGap {
            reason: format!("lowered EDB failed to parse as N-Triples: {e}"),
        })?;
    let (input_nq, quad_count) =
        premise_ds_to_world_nquads(ds.as_ref(), world_iri).map_err(|e| LoweringGap {
            reason: format!("world-scoping the lowered EDB failed: {e}"),
        })?;

    Ok(LoweredProblem {
        world_iri: world_iri.to_string(),
        input_nq,
        quad_count,
    })
}

/// Lower a problem and decide it natively, returning the normalized outcome.
///
/// Runs the lowered world-scoped EDB through [`gmeow_logic::reason::dl_consistency`].
/// A non-empty native coverage `gaps` set means the DL engine cannot honestly
/// decide the EDB — that is a capability gap, surfaced as [`LoweringGap`], never a
/// silent `incomplete`.
///
/// # Errors
/// [`LoweringGap`] for an out-of-fragment problem (from lowering) or a native DL
/// coverage gap.
pub fn lower_and_decide(
    formulas: &[AnnotatedFormula],
    world_iri: &str,
) -> Result<(ExternalOutcome, LoweredProblem), LoweringGap> {
    let lowered = lower_problem(formulas, world_iri)?;
    let dataset = purrdf::parse_dataset(lowered.input_nq.as_bytes(), "application/n-quads", None)
        .map_err(|e| LoweringGap {
        reason: format!("lowered EDB N-Quads failed to parse: {e}"),
    })?;
    let verdict =
        gmeow_logic::reason::dl_consistency(dataset.as_ref()).map_err(|e| LoweringGap {
            reason: format!("native DL consistency failed: {e}"),
        })?;
    if !verdict.gaps.is_empty() {
        let codes: Vec<&str> = verdict.gaps.iter().map(|g| g.code.as_str()).collect();
        return Err(LoweringGap {
            reason: format!(
                "native DL coverage gap(s) {codes:?} — the engine cannot honestly decide \
                 this EDB (a capability gap, not `incomplete`)"
            ),
        });
    }
    let outcome = if verdict.consistent {
        ExternalOutcome::Consistent
    } else {
        ExternalOutcome::Inconsistent
    };
    Ok((outcome, lowered))
}

// ---------------------------------------------------------------------------
// Fragment recognizers
// ---------------------------------------------------------------------------

/// Assert a premise / negated-conjecture formula as EDB triples.
fn lower_assertion(
    f: &Formula,
    out: &mut Vec<(String, String, String)>,
) -> Result<(), LoweringGap> {
    match f {
        // Ground atom: `C(a)` (type) or `r(a,b)` (role).
        Formula::Atom { relation, args } => {
            let rel = iri_of(relation)?;
            match args.as_slice() {
                [Term::Iri(a)] => {
                    out.push((a.clone(), RDF_TYPE.to_string(), rel));
                    Ok(())
                }
                [Term::Iri(a), Term::Iri(b)] => {
                    out.push((a.clone(), rel, b.clone()));
                    Ok(())
                }
                _ => Err(gap(format!(
                    "non-ground or non-{{unary,binary}} atom on `{rel}` \
                     (only ground unary/binary atoms are EL-expressible)"
                ))),
            }
        }
        // Universal rule shapes.
        Formula::Forall { vars, body } => {
            let [var] = vars.as_slice() else {
                return Err(gap(
                    "multi-variable quantifier block (only single-variable EL axioms are \
                     expressible)"
                        .into(),
                ));
            };
            lower_universal_body(var, body, out)
        }
        _ => Err(gap(format!(
            "premise shape {} is not an EL axiom (expected a ground atom or a \
             single-variable universal rule)",
            shape_name(f)
        ))),
    }
}

/// Lower the body of a single-variable `∀X. body` axiom.
fn lower_universal_body(
    var: &str,
    body: &Formula,
    out: &mut Vec<(String, String, String)>,
) -> Result<(), LoweringGap> {
    match body {
        // `C(X) → D(X)` = subclass; `C(X) → ¬D(X)` = disjointness.
        Formula::Implies(ante, cons) => {
            let c = unary_class_over(ante, var)?;
            match &**cons {
                Formula::Not(inner) => {
                    let d = unary_class_over(inner, var)?;
                    out.push((c, OWL_DISJOINTWITH.to_string(), d));
                    Ok(())
                }
                other => {
                    let d = unary_class_over(other, var)?;
                    out.push((c, RDFS_SUBCLASSOF.to_string(), d));
                    Ok(())
                }
            }
        }
        // `¬(C(X) ∧ D(X))` = disjointness.
        Formula::Not(inner) => match &**inner {
            Formula::And(conjs) if conjs.len() == 2 => {
                let c = unary_class_over(&conjs[0], var)?;
                let d = unary_class_over(&conjs[1], var)?;
                out.push((c, OWL_DISJOINTWITH.to_string(), d));
                Ok(())
            }
            _ => Err(gap(
                "negated body is not a binary conjunction (only `¬(C(X) ∧ D(X))` \
                 disjointness is expressible)"
                    .into(),
            )),
        },
        // A binary CNF clause: `¬C(X) ∨ D(X)` = subclass; `¬C(X) ∨ ¬D(X)` =
        // disjointness. A two-positive clause `C(X) ∨ D(X)` (`⊤ ⊑ C ⊔ D`) is a genuine
        // disjunction — outside the EL fragment.
        Formula::Or(lits) if lits.len() == 2 => {
            let (p0, c0) = classify_literal(&lits[0], var)?;
            let (p1, c1) = classify_literal(&lits[1], var)?;
            match (p0, p1) {
                // ¬c0 ∨ c1 = c0 ⊑ c1.
                (false, true) => out.push((c0, RDFS_SUBCLASSOF.to_string(), c1)),
                // c0 ∨ ¬c1 = c1 ⊑ c0.
                (true, false) => out.push((c1, RDFS_SUBCLASSOF.to_string(), c0)),
                // ¬c0 ∨ ¬c1 = c0 ⊥ c1.
                (false, false) => out.push((c0, OWL_DISJOINTWITH.to_string(), c1)),
                // c0 ∨ c1 = ⊤ ⊑ c0 ⊔ c1 — a disjunctive head, not EL-expressible.
                (true, true) => {
                    return Err(gap(
                        "two-positive clause (`⊤ ⊑ C ⊔ D`) is a disjunction outside the \
                         EL fragment"
                            .into(),
                    ));
                }
            }
            Ok(())
        }
        _ => Err(gap(format!(
            "universal body shape {} is not an EL axiom (expected `C(X) → D(X)`, \
             `C(X) → ¬D(X)`, `¬(C(X) ∧ D(X))`, or a binary CNF clause)",
            shape_name(body)
        ))),
    }
}

/// Classify a CNF clause literal over `var` into `(is_positive, class_iri)`. A
/// `¬C(X)` literal is negative, a bare `C(X)` positive; anything else is a gap.
fn classify_literal(lit: &Formula, var: &str) -> Result<(bool, String), LoweringGap> {
    match lit {
        Formula::Not(inner) => Ok((false, unary_class_over(inner, var)?)),
        atom @ Formula::Atom { .. } => Ok((true, unary_class_over(atom, var)?)),
        _ => Err(gap(format!(
            "clause literal shape {} is not a (negated) unary atom",
            shape_name(lit)
        ))),
    }
}

/// Negate a conjecture by refutation via the SHARED conclusion-shape calculus.
///
/// Recognizes the conjecture into a [`ConclusionShape`], then delegates the actual
/// counter-model minting to [`gmeow_logic::entail::negate`] — the one waist the
/// RDF-conclusion entailment path also uses, so the sound reserved-namespace minting
/// lives in a single place.
fn lower_negated_conjecture(
    f: &Formula,
    minter: &Minter,
    out: &mut Vec<(String, String, String)>,
) -> Result<(), LoweringGap> {
    let shape = match f {
        // Ground unary `C(a)` → ground membership conclusion.
        Formula::Atom { relation, args } => {
            let c = iri_of(relation)?;
            match args.as_slice() {
                [Term::Iri(a)] => ConclusionShape::GroundType {
                    subject: a.clone(),
                    class: c,
                },
                [Term::Iri(_), Term::Iri(_)] => {
                    return Err(gap(
                        "binary-predicate conjecture — negating a role atom is not \
                         EL-expressible (no role negation)"
                            .into(),
                    ));
                }
                _ => {
                    return Err(gap(format!(
                        "non-ground conjecture atom on `{c}` is not refutable in the EL fragment"
                    )));
                }
            }
        }
        // Subclass `∀X.(C(X) → D(X))` → subsumption conclusion.
        Formula::Forall { vars, body } => {
            let [var] = vars.as_slice() else {
                return Err(gap(
                    "multi-variable conjecture quantifier (only a single-variable subclass \
                     conjecture is refutable)"
                        .into(),
                ));
            };
            let Formula::Implies(ante, cons) = &**body else {
                return Err(gap(format!(
                    "universal conjecture body {} is not a subclass `C(X) → D(X)`",
                    shape_name(body)
                )));
            };
            let c = unary_class_over(ante, var)?;
            let d = unary_class_over(cons, var)?;
            ConclusionShape::SubClassOf { sub: c, sup: d }
        }
        _ => {
            return Err(gap(format!(
                "conjecture shape {} is not refutable in the EL fragment (expected a ground \
                 unary atom or a single-variable subclass)",
                shape_name(f)
            )));
        }
    };
    // `negate` refuses a subproperty shape (decided by reachability, not refutation), but
    // the conjecture lowering only ever builds `GroundType`/`SubClassOf` here, so this
    // never fires — surface any invariant violation as a lowering gap rather than panic.
    let negation = entail::negate(&shape, minter).map_err(|d| gap(d.to_string()))?;
    out.extend(negation);
    Ok(())
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn gap(reason: String) -> LoweringGap {
    LoweringGap { reason }
}

/// The IRI of a relation term (always a `Term::Iri` by `Formula::atom`'s invariant).
fn iri_of(relation: &Term) -> Result<String, LoweringGap> {
    match relation {
        Term::Iri(i) => Ok(i.clone()),
        _ => Err(gap(
            "relation term is not an IRI (unexpected — first-orderness)".into(),
        )),
    }
}

/// Extract the class IRI of a unary atom `C(var)` applied to exactly the bound
/// variable `var`. Any other shape (different variable, ground arg, wrong arity)
/// is a gap.
fn unary_class_over(f: &Formula, var: &str) -> Result<String, LoweringGap> {
    match f {
        Formula::Atom { relation, args } => match args.as_slice() {
            [Term::Var(v)] if v == var => iri_of(relation),
            _ => Err(gap(format!(
                "expected a unary atom over the bound variable `{var}`, found a \
                 different argument shape"
            ))),
        },
        _ => Err(gap(format!(
            "expected a unary predication over `{var}`, found {}",
            shape_name(f)
        ))),
    }
}

/// Collect every IRI mentioned in a formula (relations and ground arguments) into
/// `out` — the problem vocabulary the shared minter checks for reserved-namespace
/// collisions.
fn collect_formula_iris(f: &Formula, out: &mut BTreeSet<String>) {
    match f {
        Formula::Atom { relation, args } => {
            if let Term::Iri(i) = relation {
                out.insert(i.clone());
            }
            for a in args {
                if let Term::Iri(i) = a {
                    out.insert(i.clone());
                }
            }
        }
        Formula::Not(inner) => collect_formula_iris(inner, out),
        Formula::And(xs) | Formula::Or(xs) => {
            for x in xs {
                collect_formula_iris(x, out);
            }
        }
        Formula::Implies(a, b) | Formula::Iff(a, b) => {
            collect_formula_iris(a, out);
            collect_formula_iris(b, out);
        }
        Formula::Forall { body, .. } | Formula::Exists { body, .. } => {
            collect_formula_iris(body, out);
        }
    }
}

/// A short human name for a formula's top shape (for gap messages).
fn shape_name(f: &Formula) -> &'static str {
    match f {
        Formula::Atom { .. } => "an atom",
        Formula::Not(_) => "a negation",
        Formula::And(_) => "a conjunction",
        Formula::Or(_) => "a disjunction",
        Formula::Implies(_, _) => "an implication",
        Formula::Iff(_, _) => "a biconditional",
        Formula::Forall { .. } => "a universal",
        Formula::Exists { .. } => "an existential",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::external::tptp::parser::parse_tptp;

    fn decide(src: &str) -> Result<ExternalOutcome, LoweringGap> {
        let fs = parse_tptp(src).expect("parse ok");
        lower_and_decide(&fs, "https://gmeow.example/tptp-test/w").map(|(o, _)| o)
    }

    #[test]
    fn disjointness_clash_is_inconsistent() {
        // unsat-clash: a⊑b, a⊑c, b⊥c, a(x).
        let src = "\
            fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).\n\
            fof(a_sub_c, axiom, ![X] : (a(X) => c(X))).\n\
            fof(b_disj_c, axiom, ![X] : ~(b(X) & c(X))).\n\
            fof(x_is_a, axiom, a(x)).\n";
        assert_eq!(decide(src).unwrap(), ExternalOutcome::Inconsistent);
    }

    #[test]
    fn open_model_is_consistent() {
        // satisfiable-open: a⊑b, a(x) — no clash.
        let src = "\
            fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).\n\
            fof(x_is_a, axiom, a(x)).\n";
        assert_eq!(decide(src).unwrap(), ExternalOutcome::Consistent);
    }

    #[test]
    fn implication_to_negation_is_disjointness() {
        // a⊑b, b⊑¬c (as C→¬D), a(x), c(x) → x∈b and x∈¬c but also x∈c → clash.
        let src = "\
            fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).\n\
            fof(b_disj_c, axiom, ![X] : (b(X) => ~c(X))).\n\
            fof(x_is_a, axiom, a(x)).\n\
            fof(x_is_c, axiom, c(x)).\n";
        assert_eq!(decide(src).unwrap(), ExternalOutcome::Inconsistent);
    }

    #[test]
    fn ground_unary_theorem_refutes_to_inconsistent() {
        // Premises a⊑b, a(x) ⊢ conjecture b(x): refutation is UNSAT.
        let src = "\
            fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).\n\
            fof(x_is_a, axiom, a(x)).\n\
            fof(goal, conjecture, b(x)).\n";
        assert_eq!(decide(src).unwrap(), ExternalOutcome::Inconsistent);
    }

    #[test]
    fn negated_ground_conjecture_is_an_honest_gap() {
        // A conjecture whose shape is `~C(a)` (a `Not(Atom)`) is not expressed by the
        // EL refutation lowerer, so it must surface as an honest capability gap
        // (LoweringGap) — never a silently-decided verdict. Extending the fragment to
        // cover it is a separate, soundness-reviewed change, not a silent approximation.
        let src = "\
            fof(prem, axiom, c(a)).\n\
            fof(goal, conjecture, ~c(a)).\n";
        let err = decide(src).unwrap_err();
        assert!(
            err.reason.contains("refutable") || err.reason.contains("conjecture shape"),
            "{}",
            err.reason
        );
    }

    #[test]
    fn ground_unary_non_theorem_refutes_to_consistent() {
        // Premises a(x) do NOT entail b(x): refutation stays satisfiable.
        let src = "\
            fof(x_is_a, axiom, a(x)).\n\
            fof(goal, conjecture, b(x)).\n";
        assert_eq!(decide(src).unwrap(), ExternalOutcome::Consistent);
    }

    #[test]
    fn subclass_theorem_refutes_via_fresh_witness() {
        // a⊑b, b⊑c ⊢ a⊑c: negate → ∃X.(a(X) ∧ ¬c(X)); witness w∈a ⇒ w∈b ⇒ w∈c, clash w∈c̄.
        let src = "\
            fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).\n\
            fof(b_sub_c, axiom, ![X] : (b(X) => c(X))).\n\
            fof(goal, conjecture, ![X] : (a(X) => c(X))).\n";
        assert_eq!(decide(src).unwrap(), ExternalOutcome::Inconsistent);
    }

    #[test]
    fn subclass_non_theorem_is_consistent() {
        // a⊑b does NOT entail a⊑c.
        let src = "\
            fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).\n\
            fof(goal, conjecture, ![X] : (a(X) => c(X))).\n";
        assert_eq!(decide(src).unwrap(), ExternalOutcome::Consistent);
    }

    #[test]
    fn cnf_disjointness_clash_is_inconsistent() {
        // Same unsat-clash, authored in CNF: ¬a∨b, ¬a∨c, ¬b∨¬c, a(x).
        let src = "\
            cnf(a_sub_b, axiom, ( ~a(X) | b(X) )).\n\
            cnf(a_sub_c, axiom, ( ~a(X) | c(X) )).\n\
            cnf(b_disj_c, axiom, ( ~b(X) | ~c(X) )).\n\
            cnf(x_is_a, axiom, a(x)).\n";
        assert_eq!(decide(src).unwrap(), ExternalOutcome::Inconsistent);
    }

    #[test]
    fn cnf_two_positive_clause_is_a_capability_gap() {
        // `a(X) | b(X)` = ⊤ ⊑ a ⊔ b, a genuine disjunction outside EL.
        let src = "cnf(c, axiom, ( a(X) | b(X) )).\n";
        let err = decide(src).unwrap_err();
        assert!(err.reason.contains("disjunction"), "{err}");
    }

    #[test]
    fn disjunctive_premise_is_a_capability_gap() {
        // A genuine disjunction in a premise body is outside the EL fragment.
        let src = "fof(d, axiom, ![X] : (a(X) => (b(X) | c(X)))).\n";
        let err = decide(src).unwrap_err();
        assert!(err.reason.contains("disjunction"), "{err}");
    }

    #[test]
    fn binary_predicate_conjecture_is_a_capability_gap() {
        let src = "\
            fof(edge, axiom, r(a, b)).\n\
            fof(goal, conjecture, r(a, b)).\n";
        let err = decide(src).unwrap_err();
        assert!(err.reason.contains("role"), "{err}");
    }

    #[test]
    fn world_scoped_edb_shape_matches_seed() {
        let src = "\
            fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).\n\
            fof(x_is_a, axiom, a(x)).\n";
        let fs = parse_tptp(src).unwrap();
        let lowered = lower_problem(&fs, "https://gmeow.example/t/w").unwrap();
        assert_eq!(lowered.quad_count, 2);
        // Every quad is scoped under the single world IRI.
        for line in lowered.input_nq.lines() {
            assert!(
                line.ends_with("<https://gmeow.example/t/w> ."),
                "quad not world-scoped: {line}"
            );
        }
    }
}
