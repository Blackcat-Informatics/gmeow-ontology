// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The public, STRUCTURED proof view — a proof TREE, not a flattened hash — plus its TSTP
//! derivation projection.
//!
//! # What this adds over [`crate::goal_directed`]
//!
//! The proof-carrying backward engine ([`crate::physical::resolve_fol`]) builds a genuine
//! Curry–Howard proof TERM per answer (`crate::physical::proof`), but the only public façade
//! over it, [`crate::goal_directed::evaluate_reasoning_programs`], flattens each proof to a
//! single content-addressed `derivation_iri` string. That is enough to *cite* a proof and
//! nothing else: the rule applications, the premise edges, and the asserted leaves — the
//! proof-as-process — never leave the crate.
//!
//! This module is the missing structured projection. [`ProofTree::of_answer`] decodes the
//! arena proof term through `crate::physical::proof`'s OWN `ProofShape` decoder (never a
//! second parse of the `App` framing) into a flat, arena-independent step table of
//! [`ProofStepView`]s, in deterministic DFS pre-order with the root first.
//!
//! # Identity parity is the point (§19 single-path identity)
//!
//! Every step's [`ProofStepView::derivation_iri`] is minted by
//! `crate::physical::proof::structured_derivation_iri` — the SAME recipe
//! [`crate::goal_directed`] already projects and, through
//! [`crate::provenance::mint_derivation_id`], the same one the forward reasoner's
//! [`crate::derivation_graph::RuleApplication`] folds. A tree step and the flattened
//! `derivation_iri` of the same proof node therefore agree byte-for-byte; there is no forked
//! hash recipe here and `proof_tree_derivation_iris_match_the_proof_projection` pins it.
//!
//! # No fabricated substitutions
//!
//! `crate::physical::proof::check` computes the most general unifier of a `by_rule` step's
//! body atoms against its checked premises and DISCARDS it. A [`ProofStepView`] deliberately
//! carries **no substitution field**: the engine registers a content-addressed GROUND-instance
//! clause per firing (see `resolve_fol::build_proofs`), so the `RuleCtx` clause a step cites is
//! already ground and its MGU against the (identical) ground premises is the empty
//! substitution — recomputing it would yield nothing, while reporting the ORIGINAL program
//! clause's binding would require a general-clause ↔ firing correspondence the proof term does
//! not carry. Publishing an invented substitution would be worse than publishing none, so the
//! field is omitted rather than guessed.
//!
//! # The TSTP projection
//!
//! [`ProofTree::to_tstp`] renders the tree as a TSTP (TPTP-solution) derivation: one
//! `cnf(<name>, axiom, <conclusion>).` line per asserted leaf and one
//! `cnf(<name>, plain, <conclusion>, inference(<rule>,[status(thm)],[<parents>])).` line per
//! rule application. Step names are the exact image of the step's `derivation_iri` under
//! [`tstp_step_name`] (whose inverse is [`tstp_step_derivation_iri`]), so a parsed derivation
//! re-identifies each step with its content-addressed proof identity.
//!
//! Predicates, constants, and rule identities are IRIs, so they ride as TPTP **single-quoted
//! atoms** (`'https://…#c'('https://…#w')`) rather than being lossily shortened to a local
//! name: quoting is total and injective, a local-name projection is neither.
//!
//! [`ProofTree::of_answer`]: crate::proof_tree::ProofTree::of_answer
//! [`ProofTree::to_tstp`]: crate::proof_tree::ProofTree::to_tstp
//! [`ProofStepView`]: crate::proof_tree::ProofStepView
//! [`ProofStepView::derivation_iri`]: crate::proof_tree::ProofStepView::derivation_iri
//! [`tstp_step_name`]: crate::proof_tree::tstp_step_name
//! [`tstp_step_derivation_iri`]: crate::proof_tree::tstp_step_derivation_iri

use std::collections::{BTreeMap, HashMap};

use purrdf::TermValue;

use gmeow_logic_compile::ir::ReasoningProgramIr;
use gmeow_term_arena::engine::{NodeData, TermDag};

use crate::physical::id::{NodeId, TermId};
use crate::physical::proof::{ProofShape, check, classify, structured_derivation_iri};
use crate::physical::resolve_fol::{FolBinding, FolControl, render, resolve_fol};
use crate::provenance;
use crate::query_ir::Budget;

/// The prefix every TSTP step name carries, so a derivation name is a valid TPTP
/// `<lower_word>` (`[a-z][A-Za-z0-9_]*`) even though the content address it wraps may start
/// with a digit.
pub const TSTP_STEP_NAME_PREFIX: &str = "d_";

/// A proof-tree projection failure, routed through the same physical-engine diagnostic kind
/// the backward resolver raises.
fn tree_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Physical { detail })
}

// ── The step view ───────────────────────────────────────────────────────────────────────

/// One step of a checked proof: a rule application or an asserted leaf.
///
/// Every field is a plain owned string / index, so a [`ProofTree`] outlives the arena the
/// proof term lives in and can cross a crate boundary (which the arena `NodeId`s cannot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofStepView {
    /// The step's content-addressed derivation IRI, minted by
    /// `crate::physical::proof::structured_derivation_iri` — byte-identical to the IRI the
    /// rest of the system already mints for this proof node (see the module docs).
    pub derivation_iri: String,
    /// The cited ground-instance firing rule IRI, or `None` for an asserted leaf (whose
    /// justification is the assert sentinel, not a rule).
    pub rule_iri: Option<String>,
    /// The atom this step concludes, rendered to the engine's own functional surface
    /// (`crate::physical::resolve_fol::render`) — the SAME text
    /// [`crate::goal_directed::GoalDirectedAnswer::atom`] carries.
    pub conclusion: String,
    /// The premise steps, as indices into [`ProofTree::steps`], in the proof term's own
    /// argument order.
    pub premises: Vec<usize>,
    /// Whether this step is an asserted (EDB) leaf rather than a rule application.
    pub asserted: bool,
}

// ── The tree ────────────────────────────────────────────────────────────────────────────

/// A checked proof as a structured, arena-independent step table.
///
/// Steps are in DFS **pre-order with the root first** (`steps()[0]` is always the root), and
/// a proof node shared by two premises (the arena hash-conses, so maximal sharing is real)
/// appears EXACTLY ONCE, with both parents citing its single index. The tree is therefore the
/// proof DAG's faithful projection, not an exponential unfolding of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofTree {
    /// The step table, root first.
    steps: Vec<ProofStepView>,
    /// Per-step TSTP-serializable conclusion term (parallel to [`Self::steps`]). Kept beside
    /// the human-facing [`ProofStepView::conclusion`] because the TSTP surface needs the
    /// atom's STRUCTURE (functor + arguments, each a quoted IRI), which the comma-joined
    /// render surface cannot be re-parsed back into.
    tstp_conclusions: Vec<String>,
}

impl ProofTree {
    /// The step table, root first.
    #[must_use]
    pub fn steps(&self) -> &[ProofStepView] {
        &self.steps
    }

    /// The root step — the one that concludes the answer atom.
    ///
    /// Infallible: [`Self::of_answer`] always pushes the root before any premise, so the
    /// table is never empty.
    #[must_use]
    pub fn root(&self) -> &ProofStepView {
        &self.steps[0]
    }

    /// The number of distinct proof steps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Whether the tree has no steps. Always `false` for a tree built by [`Self::of_answer`];
    /// present so the `len`/`is_empty` pair is complete.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Decode the proof term of one resolved answer into a structured tree.
    ///
    /// # Errors
    ///
    /// Hard-fails if the node is not a well-formed proof term, if a projected leaf is not an
    /// IRI, if a conclusion atom is not a ground application (so it has no TSTP surface), or
    /// if two distinct steps mint the same derivation IRI (which would collapse two TSTP step
    /// names onto one and silently merge the derivation).
    fn of_answer(dag: &TermDag, proof: NodeId) -> gmeow_errors::Result<Self> {
        let mut steps: Vec<ProofStepView> = Vec::new();
        let mut tstp_conclusions: Vec<String> = Vec::new();
        let mut index: HashMap<NodeId, usize> = HashMap::new();
        visit(dag, proof, &mut steps, &mut tstp_conclusions, &mut index)?;

        // Two distinct steps sharing a derivation IRI would mint one TSTP name for two
        // different inferences. Within one resolved outcome this cannot happen (the engine
        // proves each ground atom exactly once, and both minting recipes are pure functions of
        // the proved content), but the tree is a published surface — assert it rather than
        // let a future engine change silently merge two steps.
        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
        for (i, step) in steps.iter().enumerate() {
            if let Some(prior) = seen.insert(step.derivation_iri.as_str(), i) {
                return Err(tree_err(format!(
                    "proof steps {prior} and {i} share the derivation IRI {:?}; a proof tree's \
                     step identities must be distinct or the TSTP projection would merge them",
                    step.derivation_iri
                )));
            }
        }

        Ok(Self {
            steps,
            tstp_conclusions,
        })
    }

    /// Render the tree as a TSTP derivation.
    ///
    /// One line per step, emitted in REVERSE step order (leaves first, root last) so every
    /// parent name is defined before the inference that cites it — the conventional TSTP
    /// reading order. Names are content-addressed ([`tstp_step_name`]), so the emitted text is
    /// independent of that presentation order.
    ///
    /// # Errors
    ///
    /// Hard-fails if a step's derivation IRI is not under
    /// [`crate::provenance::DERIVATION_PREFIX`] with a name-safe content address.
    pub fn to_tstp(&self) -> gmeow_errors::Result<String> {
        let mut out = String::new();
        for (i, step) in self.steps.iter().enumerate().rev() {
            let name = tstp_step_name(&step.derivation_iri)?;
            let conclusion = &self.tstp_conclusions[i];
            match &step.rule_iri {
                None => {
                    out.push_str(&format!("cnf({name}, axiom, {conclusion}).\n"));
                }
                Some(rule) => {
                    let mut parents = Vec::with_capacity(step.premises.len());
                    for &p in &step.premises {
                        parents.push(tstp_step_name(&self.steps[p].derivation_iri)?);
                    }
                    out.push_str(&format!(
                        "cnf({name}, plain, {conclusion}, inference({}, [status(thm)], [{}])).\n",
                        quoted_atom(rule),
                        parents.join(", ")
                    ));
                }
            }
        }
        Ok(out)
    }
}

/// Walk one proof node, appending its step (pre-order, root first) and returning its index.
/// A node already visited returns its existing index, so a shared subproof is emitted once.
fn visit(
    dag: &TermDag,
    node: NodeId,
    steps: &mut Vec<ProofStepView>,
    tstp: &mut Vec<String>,
    index: &mut HashMap<NodeId, usize>,
) -> gmeow_errors::Result<usize> {
    if let Some(&existing) = index.get(&node) {
        return Ok(existing);
    }
    // Decode through the ONE proof-framing decoder (`crate::physical::proof::classify`).
    let shape = classify(dag, node).map_err(|e| {
        tree_err(format!(
            "cannot decode a proof node into a tree step: {e:?}"
        ))
    })?;
    // The step's identity is the SAME content-addressed IRI the rest of the system mints.
    let derivation_iri = structured_derivation_iri(dag, node)?;

    // Reserve this node's slot BEFORE recursing so the pre-order index is the visit order and
    // a shared subproof resolves to one index.
    let idx = steps.len();
    steps.push(ProofStepView {
        derivation_iri: derivation_iri.clone(),
        rule_iri: None,
        conclusion: String::new(),
        premises: Vec::new(),
        asserted: false,
    });
    tstp.push(String::new());
    index.insert(node, idx);

    match shape {
        ProofShape::Assert { goal, reifier: _ } => {
            steps[idx].conclusion = render(dag, goal);
            steps[idx].asserted = true;
            tstp[idx] = tstp_term(dag, goal)?;
        }
        ProofShape::ByRule {
            goal,
            rule,
            subproofs,
        } => {
            let mut premises = Vec::with_capacity(subproofs.len());
            for sub in &subproofs {
                premises.push(visit(dag, *sub, steps, tstp, index)?);
            }
            let conclusion = render(dag, goal);
            let term = tstp_term(dag, goal)?;
            let step = &mut steps[idx];
            step.rule_iri = Some(atom_iri(dag, rule)?);
            step.conclusion = conclusion;
            step.premises = premises;
            tstp[idx] = term;
        }
    }
    Ok(idx)
}

// ── TSTP name ↔ derivation IRI ──────────────────────────────────────────────────────────

/// The TSTP step name for a content-addressed derivation IRI: the
/// [`crate::provenance::DERIVATION_PREFIX`] content address under the
/// [`TSTP_STEP_NAME_PREFIX`] sigil, so the name is a valid TPTP `<lower_word>`.
///
/// [`tstp_step_derivation_iri`] is its exact inverse.
///
/// # Errors
///
/// Hard-fails if `derivation_iri` is not under [`crate::provenance::DERIVATION_PREFIX`], or if
/// its content address is empty or carries a character outside `[0-9a-z_]` (which would make
/// the TSTP name unlexable). No lossy sanitization: an unrepresentable identity is refused,
/// never silently mangled into a colliding name.
pub fn tstp_step_name(derivation_iri: &str) -> gmeow_errors::Result<String> {
    let digest = derivation_iri
        .strip_prefix(provenance::DERIVATION_PREFIX)
        .ok_or_else(|| {
            tree_err(format!(
                "proof-step derivation IRI {derivation_iri:?} is not under the canonical \
                 derivation prefix {:?}; a TSTP step name must be the exact image of the \
                 content-addressed identity",
                provenance::DERIVATION_PREFIX
            ))
        })?;
    if digest.is_empty()
        || !digest
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(tree_err(format!(
            "derivation content address {digest:?} is not a TPTP-name-safe token \
             ([0-9a-z_]+); refusing to mangle it into a possibly-colliding step name"
        )));
    }
    Ok(format!("{TSTP_STEP_NAME_PREFIX}{digest}"))
}

/// The derivation IRI a TSTP step name denotes — the exact inverse of [`tstp_step_name`].
///
/// # Errors
///
/// Hard-fails if `name` does not carry the [`TSTP_STEP_NAME_PREFIX`] sigil or its content
/// address is empty.
pub fn tstp_step_derivation_iri(name: &str) -> gmeow_errors::Result<String> {
    let digest = name.strip_prefix(TSTP_STEP_NAME_PREFIX).ok_or_else(|| {
        tree_err(format!(
            "TSTP step name {name:?} does not carry the {TSTP_STEP_NAME_PREFIX:?} sigil, so it \
             names no proof-step derivation identity"
        ))
    })?;
    if digest.is_empty() {
        return Err(tree_err(format!(
            "TSTP step name {name:?} carries an empty content address"
        )));
    }
    Ok(format!("{}{digest}", provenance::DERIVATION_PREFIX))
}

// ── TSTP term surfaces ──────────────────────────────────────────────────────────────────

/// Render a ground arena term to a TPTP term: an application becomes `'functor'(arg, …)`, a
/// leaf becomes a single-quoted atom carrying its full IRI / literal surface.
///
/// # Errors
///
/// Hard-fails on a non-ground term (a metavariable or a bound/de-Bruijn occurrence) or a
/// binder — a proof's conclusion is always a ground atom, so any of those is a malformed
/// proof rather than a shape to approximate.
fn tstp_term(dag: &TermDag, node: NodeId) -> gmeow_errors::Result<String> {
    match dag.data(node) {
        NodeData::Leaf(tid) | NodeData::Free(tid) => {
            Ok(quoted_atom(&atom_surface(dag.atom_value(*tid))))
        }
        NodeData::App { op, args } => {
            let (op, args) = (*op, args.clone());
            let functor = match dag.data(op) {
                NodeData::Leaf(tid) | NodeData::Free(tid) => {
                    quoted_atom(&atom_surface(dag.atom_value(*tid)))
                }
                other => {
                    return Err(tree_err(format!(
                        "a proof conclusion's operator must be an atomic leaf, found {other:?}"
                    )));
                }
            };
            if args.is_empty() {
                return Ok(functor);
            }
            let mut rendered = Vec::with_capacity(args.len());
            for a in args.iter() {
                rendered.push(tstp_term(dag, *a)?);
            }
            Ok(format!("{functor}({})", rendered.join(", ")))
        }
        other => Err(tree_err(format!(
            "a proof conclusion must be a ground application or leaf term, found {other:?}"
        ))),
    }
}

/// The bare surface of an atomic term value: an IRI's own text, or a literal's canonical
/// display form.
fn atom_surface(value: &TermValue) -> String {
    match value {
        TermValue::Iri(iri) => iri.clone(),
        other => provenance::term_display(other),
    }
}

/// Wrap `s` as a TPTP single-quoted atom, escaping `\` and `'` exactly as the parser's
/// single-quote lexer unescapes them.
fn quoted_atom(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\\' || c == '\'' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// The IRI string of an atom handle, hard-failing if it is not an IRI leaf.
fn atom_iri(dag: &TermDag, atom: TermId) -> gmeow_errors::Result<String> {
    match dag.atom_value(atom) {
        TermValue::Iri(iri) => Ok(iri.clone()),
        other => Err(tree_err(format!(
            "a proof step's cited rule handle must be an IRI, found {other:?}"
        ))),
    }
}

// ── The public proving entry ────────────────────────────────────────────────────────────

/// One proof-carrying answer with its full structured proof tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvedAnswer {
    /// The ground answer atom's functional surface (the SAME rendering
    /// [`crate::goal_directed::GoalDirectedAnswer::atom`] carries).
    pub atom: String,
    /// The goal variable → resolved sub-term surface map (deterministic, sorted keys).
    pub bindings: BTreeMap<String, String>,
    /// The answer's checked proof, as a structured tree.
    pub tree: ProofTree,
}

/// The full result of resolving one reasoning program for structured proofs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvedProgram {
    /// The authored program IRI.
    pub iri: String,
    /// The rendered goal template (free metavariables shown as `?n`).
    pub goal: String,
    /// The resolution's budget status (`ok` / `partial` / `exhausted`) — disclosed rather
    /// than swallowed, so a caller can see a truncated grounding for what it is.
    pub status: String,
    /// The proof-checked answers, in a total order over `(atom, bindings, root derivation
    /// IRI)` for determinism.
    pub answers: Vec<ProvedAnswer>,
}

/// Resolve a compiled `logic:ReasoningProgram` through the backward engine and project every
/// answer's CHECKED proof term into a structured [`ProofTree`].
///
/// This is [`crate::goal_directed::evaluate_reasoning_programs`]'s sibling, not a fork: it
/// lowers through the SAME single `ReasoningProgramIr` → `FolProgram` compiler
/// (`crate::goal_directed::lower_reasoning_program`), resolves through the SAME
/// `crate::physical::resolve_fol::resolve_fol`, and validates through the SAME
/// `crate::physical::proof::check`. The only difference is what it publishes: the proof TREE
/// instead of the flattened derivation IRI.
///
/// `subsort_edges` is the caller's reasoned `rdfs:subClassOf` closure, narrowed to the sorts
/// the program references (empty for an unsorted program).
///
/// # Errors
///
/// Hard-fails if the program is outside the backward engine's fragment
/// (`FolControl::Unsupported`), if any answer's proof fails to [`check`] or re-derives a
/// different atom, or if a proof term cannot be decoded into a tree.
pub fn prove_reasoning_program(
    program: &ReasoningProgramIr,
    subsort_edges: &[(String, String)],
) -> gmeow_errors::Result<ProvedProgram> {
    let built = crate::goal_directed::lower_reasoning_program(program, subsort_edges)?;
    let crate::goal_directed::BuiltDemonstrator {
        mut dag,
        program: fol,
        ctx,
        verdict_probes: _,
    } = built;
    let goal = render(&dag, fol.goal);
    let outcome = match resolve_fol(&mut dag, &fol, &ctx, &Budget::default())? {
        FolControl::Decided(outcome) => outcome,
        FolControl::Unsupported(kind) => {
            return Err(tree_err(format!(
                "reasoning program {:?} is unsupported by the backward engine: {kind:?}",
                program.iri
            )));
        }
    };
    let status = outcome.status.as_str().to_owned();

    let mut answers = Vec::with_capacity(outcome.answers.len());
    for ans in &outcome.answers {
        let FolBinding {
            bindings,
            atom,
            proof,
        } = ans;
        // Curry–Howard check FIRST: a tree is only worth publishing for a proof that
        // independently re-derives exactly its answer atom.
        let checked = check(&mut dag, *proof, &outcome.rule_ctx).map_err(|e| {
            tree_err(format!(
                "reasoning program {:?} answer proof failed to check: {e:?}",
                program.iri
            ))
        })?;
        if checked != *atom {
            return Err(tree_err(format!(
                "reasoning program {:?} proof re-derives a different atom than its answer",
                program.iri
            )));
        }
        let tree = ProofTree::of_answer(&dag, *proof)?;
        answers.push(ProvedAnswer {
            atom: render(&dag, *atom),
            bindings: bindings.clone(),
            tree,
        });
    }
    // A total order over the answer's own content (mirrors `goal_directed`'s G12 rationale):
    // two answers can share a ground atom via distinct derivations, so the root derivation IRI
    // breaks the tie deterministically.
    answers.sort_by(|a, b| {
        a.atom
            .cmp(&b.atom)
            .then_with(|| a.bindings.cmp(&b.bindings))
            .then_with(|| {
                a.tree
                    .root()
                    .derivation_iri
                    .cmp(&b.tree.root().derivation_iri)
            })
    });

    Ok(ProvedProgram {
        iri: program.iri.clone(),
        goal,
        status,
        answers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use gmeow_logic_compile::ir::{EvaluationMode, Formula, Term};

    const NS: &str = "https://example.org/pt#";

    fn atom(pred: &str, args: &[Term]) -> Formula {
        Formula::atom(Term::iri(format!("{NS}{pred}")).unwrap(), args.to_vec()).unwrap()
    }

    fn var(name: &str) -> Term {
        Term::var(name).unwrap()
    }

    fn konst(name: &str) -> Term {
        Term::iri(format!("{NS}{name}")).unwrap()
    }

    /// `b(X) :- a(X).  c(X) :- b(X).  a(w).`  ?- `c(w)` — the Horn shape a subclass
    /// refutation (`a ⊑ b, b ⊑ c ⊢ a ⊑ c` with a fresh witness `w`) reduces to.
    fn subclass_chain() -> ReasoningProgramIr {
        ReasoningProgramIr::new(
            "https://example.org/pt/subclass-chain",
            EvaluationMode::Backward,
            vec![
                Formula::Implies(
                    Box::new(atom("a", &[var("X")])),
                    Box::new(atom("b", &[var("X")])),
                ),
                Formula::Implies(
                    Box::new(atom("b", &[var("X")])),
                    Box::new(atom("c", &[var("X")])),
                ),
                atom("a", &[konst("w")]),
            ],
            atom("c", &[konst("w")]),
            vec![],
            vec![],
            vec![],
        )
        .expect("well-formed reasoning program")
    }

    #[test]
    fn proof_tree_decodes_parent_edges_and_asserted_leaves() {
        let proved = prove_reasoning_program(&subclass_chain(), &[]).expect("resolves");
        assert_eq!(proved.status, "ok", "the tiny program resolves in budget");
        assert_eq!(proved.answers.len(), 1, "exactly one answer: c(w)");
        let tree = &proved.answers[0].tree;
        assert_eq!(tree.len(), 3, "c(w) ← b(w) ← a(w) is three steps");

        // Root first, and it concludes the answer atom.
        let root = tree.root();
        assert_eq!(root.conclusion, format!("{NS}c({NS}w)"));
        assert!(!root.asserted, "the root is a rule application");
        assert!(
            root.rule_iri.is_some(),
            "a derived step cites a firing rule"
        );
        assert_eq!(root.premises, vec![1], "the root has one premise");

        let mid = &tree.steps()[1];
        assert_eq!(mid.conclusion, format!("{NS}b({NS}w)"));
        assert!(!mid.asserted);
        assert_eq!(mid.premises, vec![2]);

        let leaf = &tree.steps()[2];
        assert_eq!(leaf.conclusion, format!("{NS}a({NS}w)"));
        assert!(leaf.asserted, "a(w) is the asserted EDB leaf");
        assert!(
            leaf.rule_iri.is_none(),
            "an asserted leaf cites no firing rule"
        );
        assert!(leaf.premises.is_empty());
    }

    #[test]
    fn proof_tree_derivation_iris_match_the_proof_projection() {
        // Identity parity: rebuild the SAME proof term through the engine and assert every
        // tree step's IRI is byte-identical to `structured_derivation_iri`'s own projection of
        // the corresponding proof node — the tree must never fork the minting recipe.
        let program = subclass_chain();
        let built = crate::goal_directed::lower_reasoning_program(&program, &[]).expect("lower");
        let crate::goal_directed::BuiltDemonstrator {
            mut dag,
            program: fol,
            ctx,
            ..
        } = built;
        let outcome = match resolve_fol(&mut dag, &fol, &ctx, &Budget::default()).expect("resolve")
        {
            FolControl::Decided(o) => o,
            FolControl::Unsupported(kind) => panic!("unsupported: {kind:?}"),
        };
        assert_eq!(outcome.answers.len(), 1);
        let proof = outcome.answers[0].proof;
        let tree = ProofTree::of_answer(&dag, proof).expect("tree");

        // Walk the proof term independently (root, then its single premise chain) and compare.
        let mut node = proof;
        for step in tree.steps() {
            let expected = structured_derivation_iri(&dag, node).expect("mint");
            assert_eq!(
                step.derivation_iri, expected,
                "tree step identity must equal proof.rs's own minting"
            );
            match classify(&dag, node).expect("classify") {
                ProofShape::ByRule { subproofs, .. } => {
                    assert_eq!(subproofs.len(), 1);
                    node = subproofs[0];
                }
                ProofShape::Assert { .. } => break,
            }
        }
        // And every step's IRI is a genuine derivation-namespace address.
        for step in tree.steps() {
            assert!(
                step.derivation_iri
                    .starts_with(provenance::DERIVATION_PREFIX),
                "{}",
                step.derivation_iri
            );
        }
    }

    #[test]
    fn tstp_step_name_round_trips_the_derivation_iri() {
        let iri = provenance::mint_derivation_id(
            "https://example.org/rule",
            &["https://example.org/reifier/1"],
        );
        let name = tstp_step_name(&iri).expect("name");
        assert!(name.starts_with(TSTP_STEP_NAME_PREFIX));
        assert_eq!(tstp_step_derivation_iri(&name).expect("inverse"), iri);

        assert!(
            tstp_step_name("https://example.org/not-a-derivation").is_err(),
            "an IRI outside the derivation namespace has no step name"
        );
        assert!(
            tstp_step_derivation_iri("no_sigil").is_err(),
            "a name without the sigil denotes no derivation"
        );
    }

    #[test]
    fn tstp_derivation_has_one_line_per_step_leaves_first() {
        let proved = prove_reasoning_program(&subclass_chain(), &[]).expect("resolves");
        let tstp = proved.answers[0].tree.to_tstp().expect("tstp");
        let lines: Vec<&str> = tstp.lines().collect();
        assert_eq!(lines.len(), 3, "one line per step:\n{tstp}");
        assert!(
            lines[0].contains(", axiom, "),
            "leaves come first: {}",
            lines[0]
        );
        assert!(
            lines[2].contains(", plain, ") && lines[2].contains("inference("),
            "the root is the last, derived line: {}",
            lines[2]
        );
        // Every parent name is defined by an earlier line.
        for (i, line) in lines.iter().enumerate() {
            let root_name = tstp_step_name(&proved.answers[0].tree.steps()[2 - i].derivation_iri)
                .expect("name");
            assert!(
                line.starts_with(&format!("cnf({root_name}, ")),
                "line {i} names the reverse-order step: {line}"
            );
        }
        // The IRIs ride as single-quoted atoms, never lossily shortened.
        assert!(tstp.contains(&format!("'{NS}c'('{NS}w')")), "{tstp}");
    }

    #[test]
    fn quoted_atom_escapes_backslash_and_quote() {
        assert_eq!(quoted_atom("a'b\\c"), "'a\\'b\\\\c'");
    }
}
