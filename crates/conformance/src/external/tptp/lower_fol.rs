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
use gmeow_logic_compile::ir::{EvaluationMode, Formula, ReasoningProgramIr, Term};

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
            // A `plain` TSTP derivation step is a PROOF step, not a problem axiom.
            // Asserting it would re-assert every inference as an independent axiom
            // (a strictly stronger, possibly inconsistent theory), so a derivation
            // is refused here rather than silently lowered as a problem.
            TptpRole::Derived => {
                return Err(gap(format!(
                    "formula {:?} is a derived TSTP step (role `plain`); a derivation is not a \
                     problem and its steps must not be asserted as axioms",
                    af.name
                )));
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
// The Horn / backward-resolution lowering (the proof-minting path)
// ---------------------------------------------------------------------------

/// The namespace a lowered TPTP backward program's identity is minted under.
const TPTP_PROGRAM_NS: &str = "https://blackcatinformatics.ca/gmeow/tptp/program/";

/// Lower a parsed TPTP problem into a compiled `logic:ReasoningProgram` the native
/// BACKWARD engine resolves — the only native path that mints a checkable proof.
///
/// # Why this exists next to [`lower_problem`]
///
/// [`lower_problem`] projects onto the DL consistency calculus, which decides
/// satisfiability and mints **no proof**: its answer is a clash, not a derivation. To lift
/// a TPTP theorem into a proof-as-process artifact the problem must reach
/// `gmeow_logic`'s proof-carrying backward resolver instead, which is what a
/// [`ReasoningProgramIr`] feeds
/// ([`gmeow_logic::proof_tree::prove_reasoning_program`]).
///
/// # The reduction
///
/// The refutation `premises ∧ ¬conjecture` is carried out in the HORN fragment, where a
/// refutation is exactly a derivation of the conjecture's positive content:
///
/// * a premise `∀X̄.(C(X̄) → D(X̄))`, a Horn CNF clause `¬C(X̄) ∨ D(X̄)`, or a fact `C(ā)`
///   becomes one program clause (implicit universals are stripped — a
///   `logic:ReasoningProgram` clause's free variables ARE its universals);
/// * a ground conjecture `C(ā)` becomes the goal: deriving it contradicts the negated
///   conjecture `¬C(ā)`;
/// * a subsumption conjecture `∀X.(C(X) → D(X))` is negated to `∃X.(C(X) ∧ ¬D(X))` through
///   the SHARED [`entail::negate`] waist (so the fresh witness comes from the one sound
///   reserved-namespace [`Minter`], never a forked recipe): its witness `w` is asserted as
///   the fact `C(w)` and the goal becomes `D(w)`, whose derivation contradicts `¬D(w)`.
///
/// Everything outside the Horn fragment — a disjointness axiom `∀X.¬(C(X) ∧ D(X))`, an
/// all-negative clause, a two-positive clause, an existential, a non-ground conjecture atom
/// (proving one instance is not proving a universal) — is a [`LoweringGap`]. Those problems
/// are refuted by the DL clash rule ([`lower_and_decide`]), which is a different, non-proof-
/// carrying decision procedure; approximating them here would fabricate a proof.
///
/// # Errors
///
/// [`LoweringGap`] when the problem carries no conjecture, more than one conjecture, or any
/// construct outside the Horn fragment described above.
pub fn lower_to_fol_program(
    formulas: &[AnnotatedFormula],
) -> Result<ReasoningProgramIr, LoweringGap> {
    let mut vocab: BTreeSet<String> = BTreeSet::new();
    for af in formulas {
        collect_formula_iris(&af.formula, &mut vocab);
    }
    let minter = Minter::new(&vocab).map_err(|e| LoweringGap {
        reason: format!("entailment minter rejected the problem vocabulary: {e}"),
    })?;

    let mut clauses: Vec<Formula> = Vec::new();
    let mut query: Option<Formula> = None;
    for af in formulas {
        match af.role {
            TptpRole::Premise | TptpRole::NegatedConjecture => {
                clauses.push(horn_clause(strip_universals(&af.formula))?);
            }
            TptpRole::Derived => {
                return Err(gap(format!(
                    "formula {:?} is a derived TSTP step (role `plain`); a derivation is not a \
                     problem and its steps must not be asserted as program clauses",
                    af.name
                )));
            }
            TptpRole::Conjecture => {
                if query.is_some() {
                    return Err(gap(
                        "the problem carries more than one conjecture; a backward program \
                         resolves exactly one goal"
                            .into(),
                    ));
                }
                let (extra_fact, goal) = horn_goal(&af.formula, &minter)?;
                if let Some(fact) = extra_fact {
                    clauses.push(fact);
                }
                query = Some(goal);
            }
        }
    }
    let query = query.ok_or_else(|| {
        gap(
            "the problem carries no conjecture, so there is no goal to derive (a backward \
             program is a clause set PLUS a goal)"
                .into(),
        )
    })?;
    if clauses.is_empty() {
        return Err(gap(
            "the problem lowered to zero program clauses; a goal with nothing to resolve \
             against derives nothing"
                .into(),
        ));
    }

    // Content-addressed program identity: a pure function of the lowered clause set and
    // goal (never a positional/source-path token), so the same problem always mints the same
    // program IRI. Components are length-framed so the concatenation is injective.
    let mut payload = String::new();
    for clause in &clauses {
        let key = clause.content_key().into_string();
        payload.push_str(&format!("c{}:{key};", key.len()));
    }
    let goal_key = query.content_key().into_string();
    payload.push_str(&format!("q{}:{goal_key};", goal_key.len()));
    let iri = format!(
        "{TPTP_PROGRAM_NS}{}",
        gmeow_logic::provenance::sha1_hex(&payload)
    );

    ReasoningProgramIr::new(
        iri,
        EvaluationMode::Backward,
        clauses,
        query,
        // A TPTP problem authors no three-valued verdict probe, no per-variable order sort,
        // and no constant `rdf:type` — the lowered program is unsorted and probe-free.
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .map_err(|e| gap(format!("lowered backward program is not well-formed: {e}")))
}

/// Peel every leading universal quantifier: a `logic:ReasoningProgram` clause's FREE
/// variables are its implicit universals, so an explicit `∀` prefix is the same clause.
fn strip_universals(f: &Formula) -> &Formula {
    match f {
        Formula::Forall { body, .. } => strip_universals(body),
        other => other,
    }
}

/// Recognize a quantifier-free matrix as one definite Horn clause: a fact atom, or an
/// `Implies(body, head)` whose head is a single atom and whose body is a conjunction of
/// atoms. A CNF disjunction with exactly one positive literal is converted to that
/// implication form.
fn horn_clause(matrix: &Formula) -> Result<Formula, LoweringGap> {
    match matrix {
        Formula::Atom { .. } => Ok(matrix.clone()),
        Formula::Implies(ante, cons) => {
            if !matches!(&**cons, Formula::Atom { .. }) {
                return Err(gap(format!(
                    "a Horn clause head must be a single atom, found {}",
                    shape_name(cons)
                )));
            }
            let mut body = Vec::new();
            horn_body_atoms(ante, &mut body)?;
            Ok(Formula::Implies(
                Box::new(conjoin(body)),
                Box::new((**cons).clone()),
            ))
        }
        Formula::Or(lits) => {
            let mut positives: Vec<&Formula> = Vec::new();
            let mut negatives: Vec<Formula> = Vec::new();
            for lit in lits {
                match lit {
                    Formula::Not(inner) if matches!(&**inner, Formula::Atom { .. }) => {
                        negatives.push((**inner).clone());
                    }
                    atom @ Formula::Atom { .. } => positives.push(atom),
                    other => {
                        return Err(gap(format!(
                            "clause literal shape {} is not a (negated) atom",
                            shape_name(other)
                        )));
                    }
                }
            }
            match positives.as_slice() {
                [head] if negatives.is_empty() => Ok((*head).clone()),
                [head] => Ok(Formula::Implies(
                    Box::new(conjoin(negatives)),
                    Box::new((*head).clone()),
                )),
                [] => Err(gap(
                    "an all-negative (goal) clause has no Horn head; it is a refutation \
                     constraint the DL clash rule decides, not a derivable clause"
                        .into(),
                )),
                _ => Err(gap(
                    "a clause with two or more positive literals is a genuine disjunction, \
                     outside the definite Horn fragment"
                        .into(),
                )),
            }
        }
        other => Err(gap(format!(
            "premise shape {} is not a definite Horn clause (expected a fact atom, an \
             implication with an atomic head, or a Horn CNF clause)",
            shape_name(other)
        ))),
    }
}

/// Flatten a rule antecedent into its positive body atoms; anything but a conjunction of
/// atoms is outside the definite fragment.
fn horn_body_atoms(f: &Formula, out: &mut Vec<Formula>) -> Result<(), LoweringGap> {
    match f {
        Formula::And(parts) => {
            for p in parts {
                horn_body_atoms(p, out)?;
            }
            Ok(())
        }
        Formula::Atom { .. } => {
            out.push(f.clone());
            Ok(())
        }
        other => Err(gap(format!(
            "a definite Horn body must be a conjunction of atoms, found {}",
            shape_name(other)
        ))),
    }
}

/// Re-conjoin body atoms: a single atom stays bare (the `logic:ReasoningProgram` clause
/// surface `lower_body` expects), several become one `And`.
fn conjoin(mut atoms: Vec<Formula>) -> Formula {
    if atoms.len() == 1 {
        return atoms.remove(0);
    }
    Formula::And(atoms)
}

/// Reduce a conjecture to `(optional witness fact, goal atom)`.
///
/// A ground atom is the goal directly. A subsumption `∀X.(C(X) → D(X))` is negated through
/// the SHARED [`entail::negate`] waist: the minted fresh witness `w` becomes the asserted
/// fact `C(w)` and the goal becomes `D(w)`.
fn horn_goal(
    conjecture: &Formula,
    minter: &Minter,
) -> Result<(Option<Formula>, Formula), LoweringGap> {
    match conjecture {
        Formula::Atom { .. } => {
            if !conjecture.is_ground() {
                return Err(gap(
                    "a non-ground conjecture atom is outside this reduction: deriving ONE \
                     instance would not establish the universally-quantified claim"
                        .into(),
                ));
            }
            Ok((None, conjecture.clone()))
        }
        Formula::Forall { vars, body } => {
            let [var] = vars.as_slice() else {
                return Err(gap(
                    "multi-variable conjecture quantifier (only a single-variable subclass \
                     conjecture is reduced to a witness goal)"
                        .into(),
                ));
            };
            let Formula::Implies(ante, cons) = &**body else {
                return Err(gap(format!(
                    "universal conjecture body {} is not a subclass `C(X) → D(X)`",
                    shape_name(body)
                )));
            };
            let sub = unary_class_over(ante, var)?;
            let sup = unary_class_over(cons, var)?;
            let shape = ConclusionShape::SubClassOf {
                sub: sub.clone(),
                sup: sup.clone(),
            };
            let negation = entail::negate(&shape, minter).map_err(|d| gap(d.to_string()))?;
            // The witness is the individual the shared negation asserts INTO the antecedent
            // class: `(w, rdf:type, sub)`. Reading it back (rather than re-deriving it) keeps
            // the one sound minting recipe unforked.
            let witness = negation
                .iter()
                .find(|(_, p, o)| p == RDF_TYPE && *o == sub)
                .map(|(s, _, _)| s.clone())
                .ok_or_else(|| {
                    gap(
                        "the shared subsumption negation did not assert a witness membership; \
                         refusing to mint one independently"
                            .into(),
                    )
                })?;
            let witness_term = Term::iri(witness).map_err(|e| gap(e.message().to_owned()))?;
            let fact = Formula::atom(
                Term::iri(sub).map_err(|e| gap(e.message().to_owned()))?,
                vec![witness_term.clone()],
            )
            .map_err(|e| gap(e.message().to_owned()))?;
            let goal = Formula::atom(
                Term::iri(sup).map_err(|e| gap(e.message().to_owned()))?,
                vec![witness_term],
            )
            .map_err(|e| gap(e.message().to_owned()))?;
            Ok((Some(fact), goal))
        }
        other => Err(gap(format!(
            "conjecture shape {} is not reducible to a Horn goal (expected a ground atom or a \
             single-variable subclass)",
            shape_name(other)
        ))),
    }
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
    use crate::external::tptp::parser::{TptpSource, TstpTerm, parse_tptp};

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

    // -----------------------------------------------------------------------
    // The Horn / backward-resolution (proof-minting) lowering
    // -----------------------------------------------------------------------

    /// The committed `tptp-mini` problems, by case name.
    const THEOREM_SUBCLASS: &str = include_str!(
        "../../../../../conformance/logic/cases/external/tptp-mini/theorem-subclass/source/problem.p"
    );
    const THEOREM_GROUND: &str = include_str!(
        "../../../../../conformance/logic/cases/external/tptp-mini/theorem-ground/source/problem.p"
    );
    const COUNTERSATISFIABLE: &str = include_str!(
        "../../../../../conformance/logic/cases/external/tptp-mini/countersatisfiable/source/problem.p"
    );
    const SATISFIABLE_OPEN: &str = include_str!(
        "../../../../../conformance/logic/cases/external/tptp-mini/satisfiable-open/source/problem.p"
    );
    const CONTRADICTORY_AXIOMS: &str = include_str!(
        "../../../../../conformance/logic/cases/external/tptp-mini/contradictory-axioms/source/problem.p"
    );
    const CNF_DISJOINT_CLASH: &str = include_str!(
        "../../../../../conformance/logic/cases/external/tptp-mini/cnf-disjoint-clash/source/problem.p"
    );

    fn prove(src: &str) -> gmeow_logic::proof_tree::ProvedProgram {
        let formulas = parse_tptp(src).expect("parse ok");
        let program = lower_to_fol_program(&formulas).expect("Horn lowering");
        gmeow_logic::proof_tree::prove_reasoning_program(&program, &[]).expect("resolution")
    }

    #[test]
    fn theorem_subclass_lowers_to_a_proof_carrying_derivation() {
        // a ⊑ b, b ⊑ c ⊢ a ⊑ c. Negating the conjecture mints a witness w with a(w);
        // the Horn derivation c(w) ← b(w) ← a(w) IS the refutation of ¬c(w).
        let proved = prove(THEOREM_SUBCLASS);
        assert_eq!(proved.status, "ok");
        assert_eq!(proved.answers.len(), 1, "one derived goal instance");
        let tree = &proved.answers[0].tree;
        assert_eq!(tree.len(), 3, "c(w) ← b(w) ← a(w)");
        assert!(!tree.root().asserted, "the root is a rule application");
        assert_eq!(tree.root().premises, vec![1]);
        assert!(
            tree.steps()[2].asserted,
            "the witness membership a(w) is the asserted leaf"
        );
        // Every step's identity is a genuine content-addressed derivation IRI.
        for step in tree.steps() {
            assert!(
                step.derivation_iri
                    .starts_with("https://blackcatinformatics.ca/gmeow/derivation/"),
                "{}",
                step.derivation_iri
            );
        }
    }

    #[test]
    fn theorem_ground_lowers_to_a_two_step_derivation() {
        // a ⊑ b, a(x) ⊢ b(x): one rule application over one asserted fact.
        let proved = prove(THEOREM_GROUND);
        assert_eq!(proved.answers.len(), 1);
        let tree = &proved.answers[0].tree;
        assert_eq!(tree.len(), 2);
        assert!(tree.steps()[1].asserted);
    }

    #[test]
    fn a_non_theorem_lowers_and_derives_nothing() {
        // a(x) does NOT entail b(x): the goal is decided with an EMPTY answer set — no
        // proof exists, and none is fabricated.
        let proved = prove(COUNTERSATISFIABLE);
        assert_eq!(proved.status, "ok");
        assert!(proved.answers.is_empty());
    }

    #[test]
    fn non_horn_and_goal_free_problems_are_honest_gaps() {
        // No conjecture ⇒ no goal to derive.
        let no_goal = lower_to_fol_program(&parse_tptp(SATISFIABLE_OPEN).unwrap()).unwrap_err();
        assert!(no_goal.reason.contains("no conjecture"), "{no_goal}");

        // `∀X.¬(b(X) ∧ c(X))` is a disjointness constraint, not a Horn clause.
        let disjointness =
            lower_to_fol_program(&parse_tptp(CONTRADICTORY_AXIOMS).unwrap()).unwrap_err();
        assert!(disjointness.reason.contains("Horn"), "{disjointness}");

        // `¬b(X) ∨ ¬c(X)` is an all-negative (goal) clause with no Horn head.
        let all_negative =
            lower_to_fol_program(&parse_tptp(CNF_DISJOINT_CLASH).unwrap()).unwrap_err();
        assert!(
            all_negative.reason.contains("all-negative"),
            "{all_negative}"
        );
    }

    #[test]
    fn the_lowered_program_identity_is_content_addressed_and_stable() {
        let a = lower_to_fol_program(&parse_tptp(THEOREM_SUBCLASS).unwrap()).unwrap();
        let b = lower_to_fol_program(&parse_tptp(THEOREM_SUBCLASS).unwrap()).unwrap();
        assert_eq!(a.iri, b.iri, "the same problem mints the same program IRI");
        let other = lower_to_fol_program(&parse_tptp(THEOREM_GROUND).unwrap()).unwrap();
        assert_ne!(a.iri, other.iri, "distinct problems mint distinct IRIs");
    }

    #[test]
    fn the_tstp_derivation_round_trips_through_the_parser() {
        use gmeow_logic::proof_tree::{tstp_step_derivation_iri, tstp_step_name};

        let proved = prove(THEOREM_SUBCLASS);
        let tree = &proved.answers[0].tree;
        let tstp = tree.to_tstp().expect("TSTP projection");

        let parsed = parse_tptp(&tstp).expect("our own TSTP derivation must re-parse");
        assert_eq!(parsed.len(), tree.len(), "one annotated formula per step");

        // Names round-trip to the step identities, and the emitted (reverse) order lines up
        // with the tree's step table read backwards.
        for (i, af) in parsed.iter().enumerate() {
            let step = &tree.steps()[tree.len() - 1 - i];
            assert_eq!(
                af.name,
                tstp_step_name(&step.derivation_iri).expect("name"),
                "step name"
            );
            assert_eq!(
                tstp_step_derivation_iri(&af.name).expect("inverse"),
                step.derivation_iri,
                "name → derivation IRI is the exact inverse"
            );
            match (&step.rule_iri, &af.source, af.role) {
                (None, None, TptpRole::Premise) => {
                    assert!(step.asserted, "an axiom line is an asserted leaf");
                }
                (
                    Some(rule),
                    Some(TptpSource::Inference {
                        rule: parsed_rule,
                        status,
                        parents,
                    }),
                    TptpRole::Derived,
                ) => {
                    assert_eq!(parsed_rule, rule, "the cited firing rule survives");
                    assert_eq!(
                        status,
                        &vec![TstpTerm::Func(
                            "status".into(),
                            vec![TstpTerm::Name("thm".into())]
                        )]
                    );
                    let expected: Vec<String> = step
                        .premises
                        .iter()
                        .map(|&p| tstp_step_name(&tree.steps()[p].derivation_iri).expect("name"))
                        .collect();
                    assert_eq!(parents, &expected, "the parent SET survives");
                }
                other => panic!("step {i} did not round-trip: {other:?}"),
            }
        }
    }

    #[test]
    fn the_committed_tstp_fixture_is_exactly_what_our_reasoner_produces() {
        // The shipped derivation fixture is a PRODUCT of the pipeline above, not a
        // hand-written artifact: regenerate it from the committed problem and require a
        // byte match of its derivation lines (the `%` header is prose). This also parses
        // the fixture as it ships, header and all.
        const FIXTURE: &str =
            include_str!("../../../../logic/tests/fixtures/tstp/theorem-subclass.tstp");

        let regenerated = prove(THEOREM_SUBCLASS).answers[0]
            .tree
            .to_tstp()
            .expect("TSTP projection");
        let committed: String = FIXTURE
            .lines()
            .filter(|l| !l.starts_with('%'))
            .map(|l| format!("{l}\n"))
            .collect();
        assert_eq!(
            committed, regenerated,
            "the committed TSTP fixture drifted from what the reasoner now produces"
        );
        assert_eq!(
            parse_tptp(FIXTURE)
                .expect("the shipped fixture must parse")
                .len(),
            3
        );
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
