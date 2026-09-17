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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// A semantic fragment boundary or a failure to execute the selected operation.
/// Only `Gap` may be recorded as an honest capability withhold.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DecisionError {
    /// The well-formed problem lies outside the selected native fragment.
    Gap(LoweringGap),
    /// Invalid lowered data or failed native execution; never a semantic verdict.
    Failure {
        /// Preserved diagnostic from invalid input or failed execution.
        detail: String,
    },
}

impl std::fmt::Display for DecisionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gap(gap) => gap.fmt(formatter),
            Self::Failure { detail } => write!(formatter, "TPTP execution failed: {detail}"),
        }
    }
}

impl std::error::Error for DecisionError {}

impl From<LoweringGap> for DecisionError {
    fn from(gap: LoweringGap) -> Self {
        Self::Gap(gap)
    }
}

fn execution_failure(detail: impl std::fmt::Display) -> DecisionError {
    DecisionError::Failure {
        detail: detail.to_string(),
    }
}

/// A TPTP problem lowered directly to a validated, world-scoped native dataset.
#[derive(Debug, Clone)]
pub struct LoweredProblem {
    /// The single world IRI every EDB quad is scoped under.
    pub world_iri: String,
    dataset: std::sync::Arc<purrdf::RdfDataset>,
}

impl LoweredProblem {
    /// Borrow the native EDB used by the consistency evaluator.
    pub fn dataset(&self) -> &purrdf::RdfDataset {
        &self.dataset
    }

    /// Number of distinct asserted EDB quads.
    pub fn quad_count(&self) -> usize {
        self.dataset.quad_count()
    }

    /// Serialize only at the external corpus-writing boundary. Native execution
    /// neither calls this method nor reparses its output.
    pub fn to_nquads(&self) -> Result<String, DecisionError> {
        let bytes = purrdf::serialize_dataset(
            &self.dataset,
            "application/n-quads",
            purrdf::SerializeGraph::Dataset,
        )
        .map_err(execution_failure)?;
        let text = String::from_utf8(bytes).map_err(execution_failure)?;
        let lines: BTreeSet<_> = text.lines().filter(|line| !line.is_empty()).collect();
        Ok(lines.into_iter().map(|line| format!("{line}\n")).collect())
    }
}

/// Lower parsed formulas without an intermediate RDF text representation.
///
/// # Errors
/// `Gap` names an unsupported formula shape. Invalid native terms and reserved
/// symbol collisions are failures, not capability evidence.
pub fn lower_problem(
    formulas: &[AnnotatedFormula],
    world_iri: &str,
) -> Result<LoweredProblem, DecisionError> {
    let mut vocab = BTreeSet::new();
    for formula in formulas {
        collect_formula_iris(&formula.formula, &mut vocab);
    }
    let minter = Minter::new(&vocab).map_err(execution_failure)?;
    let mut edb = NativeEdb::new(world_iri);
    for formula in formulas {
        match formula.role {
            TptpRole::Premise | TptpRole::NegatedConjecture => {
                lower_assertion(&formula.formula, &mut edb)?;
            }
            TptpRole::Conjecture => lower_negated_conjecture(&formula.formula, &minter, &mut edb)?,
            TptpRole::Derived => return Err(gap(format!(
                "formula {:?} is a derived TSTP step (role `plain`); a derivation is not a problem and its steps must not be asserted as axioms",
                formula.name)).into()),
        }
    }
    if !edb.emitted {
        return Err(gap(
            "problem lowered to zero EDB triples (a vacuous consistency check is not permitted)"
                .to_owned(),
        )
        .into());
    }
    Ok(LoweredProblem {
        world_iri: world_iri.to_owned(),
        dataset: edb.builder.freeze().map_err(execution_failure)?,
    })
}

/// Stream each lowered assertion straight into one world-scoped native builder.
struct NativeEdb {
    builder: purrdf::RdfDatasetBuilder,
    world: purrdf::TermId,
    emitted: bool,
}

impl NativeEdb {
    fn new(world_iri: &str) -> Self {
        let mut builder = purrdf::RdfDatasetBuilder::new();
        let world = builder.intern_iri(world_iri);
        Self {
            builder,
            world,
            emitted: false,
        }
    }

    fn push(&mut self, (subject, predicate, object): (String, String, String)) {
        let subject = self.builder.intern_iri(&subject);
        let predicate = self.builder.intern_iri(&predicate);
        let object = self.builder.intern_iri(&object);
        self.builder
            .push_quad(subject, predicate, object, Some(self.world));
        self.emitted = true;
    }

    fn extend(&mut self, triples: impl IntoIterator<Item = (String, String, String)>) {
        for triple in triples {
            self.push(triple);
        }
    }
}

/// Lower and decide using the same native dataset. Only actual unsupported
/// constructs become `Gap`; native execution failures retain their own type.
pub fn lower_and_decide(
    formulas: &[AnnotatedFormula],
    world_iri: &str,
) -> Result<(ExternalOutcome, LoweredProblem), DecisionError> {
    let lowered = lower_problem(formulas, world_iri)?;
    let outcome = decide_lowered_with(&lowered, &gmeow_logic::reason::dl_consistency)?;
    Ok((outcome, lowered))
}

fn decide_lowered_with(
    lowered: &LoweredProblem,
    evaluate: &impl Fn(
        gmeow_logic::reason::PreparedReasoningInput,
        &gmeow_logic::reason::SelectedDomains,
    ) -> gmeow_errors::Result<gmeow_logic::reason::DlVerdict>,
) -> Result<ExternalOutcome, DecisionError> {
    use gmeow_logic::reason::{
        DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld, prepare_reasoning_input,
    };
    let input = prepare_reasoning_input(lowered.dataset()).map_err(execution_failure)?;
    // The lowering operation explicitly owns this named problem theory; unrelated
    // RDF graph names never grant nonempty-domain authority.
    let world = SelectedLogicalWorld::new(
        LogicalGraph::Named(purrdf::TermValue::iri(&lowered.world_iri)),
        DomainProfile::NonemptyObjectDomainV1,
        "gmeow-conformance.tptp-problem.v1".to_owned(),
        *input.ingress_contract(),
    )
    .map_err(execution_failure)?;
    let domains = SelectedDomains::new([world]).map_err(execution_failure)?;
    let verdict = evaluate(input, &domains).map_err(execution_failure)?;
    if !verdict.gaps.is_empty() {
        let codes: Vec<_> = verdict.gaps.iter().map(|gap| gap.code.as_str()).collect();
        return Err(gap(format!("native DL coverage gap(s) {codes:?}")).into());
    }
    Ok(if verdict.consistent {
        ExternalOutcome::Consistent
    } else {
        ExternalOutcome::Inconsistent
    })
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
fn lower_assertion(f: &Formula, out: &mut NativeEdb) -> Result<(), LoweringGap> {
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
fn lower_universal_body(var: &str, body: &Formula, out: &mut NativeEdb) -> Result<(), LoweringGap> {
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
    formula: &Formula,
    minter: &Minter,
    out: &mut NativeEdb,
) -> Result<(), DecisionError> {
    let shape = conjecture_shape(formula)?;
    // An admitted GroundType/SubClassOf must be negatable. Failure is an
    // implementation/input error, never evidence of an unsupported shape.
    let negation = entail::negate(&shape, minter).map_err(execution_failure)?;
    out.extend(negation);
    Ok(())
}

fn conjecture_shape(f: &Formula) -> Result<ConclusionShape, LoweringGap> {
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
    Ok(shape)
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

#[path = "lower_fol.tests.rs"]
#[cfg(test)]
mod tests;
