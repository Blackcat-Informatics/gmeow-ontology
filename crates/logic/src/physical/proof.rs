// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! First-class, CHECKABLE proof objects as hash-consed arena terms.
//!
//! # A proof IS a term
//!
//! A proof is not a side structure that mirrors a derivation — it is a
//! [`TermDag`](crate::physical::term_dag::TermDag) node in the SAME persistent arena as the
//! goals it proves, built from two constructors:
//!
//! - [`proof_by_rule`] → `App{op: Leaf(logic:byRule), args: [goal, Leaf(rule_iri), subproof₀, …]}`
//! - [`proof_assert`]  → `App{op: Leaf(logic:assert), args: [goal, Leaf(reifier_iri)]}`
//!
//! Because the arena hash-conses, two structurally-identical proofs intern to ONE
//! [`NodeId`] (maximal sharing), and a proof's sub-structure is shared with the very goal
//! terms it establishes.
//!
//! # The de-Bruijn / Curry–Howard criterion
//!
//! [`check`] is the whole point: a proof term is only trustworthy if an independent checker
//! can *re-derive* it. `check` walks the proof bottom-up:
//!
//! - an `assert` proof holds iff its goal is a member of the asserted EDB the caller supplies
//!   in [`RuleCtx`];
//! - a `by_rule` proof holds iff, after recursively checking every subproof to the atom it
//!   proves, the cited rule's body atoms UNIFY (Task-4 [`unify`]) with those proven atoms and
//!   the resulting substitution, [`apply`]'d to the rule's head, is NodeId-EQUAL to the
//!   proof's stated goal.
//!
//! So a proof that does not actually prove its goal — a wrong rule, a wrong or missing
//! premise, a tampered goal — is REJECTED (`Err`), never rubber-stamped. The checker never
//! trusts the proof's stated goal; it recomputes it.
//!
//! # Identity parity with the reasoner (§19 single-path identity)
//!
//! A proof node projects to the SAME content-addressed provenance IRIs the forward reasoner
//! mints, reusing [`crate::provenance`] byte-for-byte (never a forked hash recipe):
//!
//! - [`derivation_iri`] folds `(rule_iri, sorted source reifier IRIs)` through
//!   [`crate::provenance::mint_derivation_id`], so a `by_rule` proof and the
//!   [`crate::derivation_graph::RuleApplication`] for the same firing agree to the byte.
//! - [`reify`] folds a ground term's resolved N3 argument surfaces through
//!   [`crate::provenance::mint_nary_reifier`], inheriting its `TermValue::Triple` hard-fail,
//!   so a structured term's DAG identity and its content-addressed reifier IRI coincide.
//!
//! Runtime handles ([`NodeId`]/[`TermId`]) are NEVER hashed — only resolved IRI/N3 surfaces
//! enter a provenance digest, exactly as the arena doctrine demands.

use std::collections::{HashMap, HashSet};

use purrdf::TermValue;

use crate::physical::id::{NodeId, TermId};
use crate::physical::lower::canon;
use crate::physical::term_dag::{NodeData, TermDag};
use crate::physical::unify::{Subst, Unified, apply, unify};
use crate::provenance;

/// Wrap a proof-projection hard failure as a typed provenance diagnostic on the shared
/// substrate, preserving the authored text verbatim (mirrors
/// [`crate::derivation_graph`]'s `provenance_err`).
fn proof_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Provenance { detail })
}

// ── Proof-checking failure ─────────────────────────────────────────────────────

/// Why a proof term is not a valid proof of its stated goal.
///
/// Every variant is a NORMAL rejection of an invalid proof, never an engine fault: a checker
/// that could not tell a bad proof from a good one would defeat the entire point of a
/// checkable-proof discipline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProofError {
    /// The node is not a well-formed proof term (wrong root kind, unknown operator, or an
    /// argument of the wrong kind for the constructor).
    Malformed {
        /// A human-readable account of the structural defect.
        detail: String,
    },
    /// An `assert` proof's goal is not a member of the asserted EDB.
    NotAsserted {
        /// The goal that was not found in [`RuleCtx::asserted`].
        goal: NodeId,
    },
    /// A `by_rule` proof cites a rule IRI absent from the [`RuleCtx`].
    UnknownRule {
        /// The cited rule-IRI leaf handle.
        rule: TermId,
    },
    /// A `by_rule` proof supplies a premise count differing from the cited rule's body arity.
    ArityMismatch {
        /// The cited rule.
        rule: TermId,
        /// The rule body's atom count.
        body: usize,
        /// The number of subproofs supplied.
        premises: usize,
    },
    /// A rule body atom failed to unify with the checked subproof's proven goal.
    PremiseMismatch {
        /// The cited rule.
        rule: TermId,
        /// The rule body atom that did not unify.
        body_atom: NodeId,
        /// The premise the subproof actually proved.
        proven: NodeId,
    },
    /// The rule head instantiated from the checked premises differs from the stated goal.
    HeadMismatch {
        /// The cited rule.
        rule: TermId,
        /// The head instantiated by re-derivation.
        derived: NodeId,
        /// The goal the proof claimed to establish.
        stated: NodeId,
    },
}

// ── The rule/EDB context the checker re-derives against ─────────────────────────

/// The small caller-supplied context [`check`] re-derives against.
///
/// The caller lowers each rule clause into the SAME arena as the proof: a rule is a
/// head atom [`NodeId`] plus its body atom [`NodeId`]s, whose variables are unification
/// metavariables ([`TermDag::fresh_meta`]). `asserted` is the set of proven EDB goal nodes
/// an `assert` leaf may cite.
#[derive(Debug, Default)]
pub(crate) struct RuleCtx {
    /// Rule clause per rule-IRI leaf handle: `(head atom, body atoms)`.
    pub(crate) rules: HashMap<TermId, (NodeId, Vec<NodeId>)>,
    /// The proven EDB goals (asserted facts) an `assert` proof may appeal to.
    pub(crate) asserted: HashSet<NodeId>,
}

// ── Constructors ────────────────────────────────────────────────────────────────

/// Intern a rule-application proof `by_rule(goal, rule_iri, subproofs…)` as an arena term.
///
/// The proof node is `App{op: Leaf(logic:byRule), args: [goal, Leaf(rule_iri), subproof₀, …]}`
/// — first-class and hash-consed, so an identical proof interns once.
pub(crate) fn proof_by_rule(
    dag: &mut TermDag,
    goal: NodeId,
    rule_iri: TermId,
    subproofs: &[NodeId],
) -> NodeId {
    let op = dag.intern_leaf(TermValue::iri(canon::BY_RULE));
    let rule_leaf = dag.intern_leaf_atom(rule_iri);
    let mut args = Vec::with_capacity(2 + subproofs.len());
    args.push(goal);
    args.push(rule_leaf);
    args.extend_from_slice(subproofs);
    dag.intern_app(op, args)
}

/// Intern an assertion proof `assert(goal, reifier_iri)` as an arena term.
///
/// The proof node is `App{op: Leaf(logic:assert), args: [goal, Leaf(reifier_iri)]}`. The
/// reifier handle is the content-addressed IRI of the asserted fact (the caller mints it via
/// [`crate::provenance::mint_reifier`]/[`crate::provenance::mint_nary_reifier`]), carried so
/// [`derivation_iri`] can project the proof to the exact source reifier a firing consumed.
pub(crate) fn proof_assert(dag: &mut TermDag, goal: NodeId, reifier_iri: TermId) -> NodeId {
    let op = dag.intern_leaf(TermValue::iri(canon::ASSERT));
    let reifier_leaf = dag.intern_leaf_atom(reifier_iri);
    dag.intern_app(op, vec![goal, reifier_leaf])
}

// ── Structural decoding ─────────────────────────────────────────────────────────

/// The decoded shape of a proof node — the one place the `App` framing is parsed.
enum ProofShape {
    /// An `assert(goal, reifier)` leaf.
    Assert {
        /// The asserted goal.
        goal: NodeId,
        /// The asserted fact's content-addressed reifier handle.
        reifier: TermId,
    },
    /// A `by_rule(goal, rule, subproofs…)` node.
    ByRule {
        /// The stated goal.
        goal: NodeId,
        /// The cited rule-IRI leaf handle.
        rule: TermId,
        /// The premise subproofs.
        subproofs: Vec<NodeId>,
    },
}

/// The atom handle of `node` if it is a [`NodeData::Leaf`], else `None`.
fn leaf_atom(dag: &TermDag, node: NodeId) -> Option<TermId> {
    match dag.data(node) {
        NodeData::Leaf(atom) => Some(*atom),
        _ => None,
    }
}

/// The IRI string of `node` if it is a leaf carrying a [`TermValue::Iri`], else `None`.
fn leaf_iri(dag: &TermDag, node: NodeId) -> Option<String> {
    match dag.atom_value(leaf_atom(dag, node)?) {
        TermValue::Iri(iri) => Some(iri.clone()),
        _ => None,
    }
}

/// The IRI string of an atom handle, hard-failing if it is not an IRI leaf.
fn atom_iri(dag: &TermDag, atom: TermId) -> gmeow_errors::Result<String> {
    match dag.atom_value(atom) {
        TermValue::Iri(iri) => Ok(iri.clone()),
        other => Err(proof_err(format!(
            "a proof leaf projected for provenance must be an IRI, found {other:?}"
        ))),
    }
}

/// Decode a proof node into its [`ProofShape`], or a [`ProofError::Malformed`] on any
/// structural defect.
fn classify(dag: &TermDag, node: NodeId) -> Result<ProofShape, ProofError> {
    // Copy the operator and clone the arg ids to release the immutable borrow on `dag`.
    let (op, args) = match dag.data(node) {
        NodeData::App { op, args } => (*op, args.clone()),
        other => {
            return Err(ProofError::Malformed {
                detail: format!("a proof node must be an App term, found {other:?}"),
            });
        }
    };
    let op_iri = leaf_iri(dag, op).ok_or_else(|| ProofError::Malformed {
        detail: "a proof node's operator must be an IRI leaf".to_owned(),
    })?;
    if op_iri == canon::BY_RULE {
        if args.len() < 2 {
            return Err(ProofError::Malformed {
                detail: format!(
                    "a by_rule proof needs at least [goal, rule]; found {} arg(s)",
                    args.len()
                ),
            });
        }
        let rule = leaf_atom(dag, args[1]).ok_or_else(|| ProofError::Malformed {
            detail: "a by_rule proof's rule argument must be a leaf".to_owned(),
        })?;
        Ok(ProofShape::ByRule {
            goal: args[0],
            rule,
            subproofs: args[2..].to_vec(),
        })
    } else if op_iri == canon::ASSERT {
        if args.len() != 2 {
            return Err(ProofError::Malformed {
                detail: format!(
                    "an assert proof needs exactly [goal, reifier]; found {} arg(s)",
                    args.len()
                ),
            });
        }
        let reifier = leaf_atom(dag, args[1]).ok_or_else(|| ProofError::Malformed {
            detail: "an assert proof's reifier argument must be a leaf".to_owned(),
        })?;
        Ok(ProofShape::Assert {
            goal: args[0],
            reifier,
        })
    } else {
        Err(ProofError::Malformed {
            detail: format!("unknown proof operator {op_iri}"),
        })
    }
}

// ── The checkable-proof core ────────────────────────────────────────────────────

/// Check `proof` bottom-up against `ctx`, returning the goal it proves or a [`ProofError`].
///
/// This is the de-Bruijn criterion: the checker RE-DERIVES the proof rather than trusting
/// its stated goal. An `assert` proof must cite an EDB member; a `by_rule` proof must, after
/// its subproofs are recursively checked, have the cited rule's body unify with those proven
/// premises and the instantiated head equal the stated goal (NodeId equality, i.e. alpha- and
/// structural equality via hash-consing). Any deviation is an `Err` — a tampered proof cannot
/// pass.
///
/// # Metavariable isolation
///
/// Each `by_rule` step uses its OWN fresh [`Subst`] and every subproof is checked in an
/// independent recursion that returns a resolved goal node, so a rule's variables can never
/// leak across sibling firings; within one firing the shared `Subst` is exactly the join that
/// forces a repeated rule variable to agree across body atoms.
pub(crate) fn check(dag: &mut TermDag, proof: NodeId, ctx: &RuleCtx) -> Result<NodeId, ProofError> {
    match classify(dag, proof)? {
        ProofShape::Assert { goal, reifier: _ } => {
            if ctx.asserted.contains(&goal) {
                Ok(goal)
            } else {
                Err(ProofError::NotAsserted { goal })
            }
        }
        ProofShape::ByRule {
            goal,
            rule,
            subproofs,
        } => {
            let (head, body) = match ctx.rules.get(&rule) {
                Some((head, body)) => (*head, body.clone()),
                None => return Err(ProofError::UnknownRule { rule }),
            };
            // Recursively check each subproof to the (proven) premise atom it establishes.
            let mut proven = Vec::with_capacity(subproofs.len());
            for sub in &subproofs {
                proven.push(check(dag, *sub, ctx)?);
            }
            if body.len() != proven.len() {
                return Err(ProofError::ArityMismatch {
                    rule,
                    body: body.len(),
                    premises: proven.len(),
                });
            }
            // Re-derive: unify each body atom with its proven premise, accumulating the MGU.
            let mut subst = Subst::new();
            for (&body_atom, &proven_atom) in body.iter().zip(proven.iter()) {
                if unify(dag, body_atom, proven_atom, &mut subst) != Unified::Ok {
                    return Err(ProofError::PremiseMismatch {
                        rule,
                        body_atom,
                        proven: proven_atom,
                    });
                }
            }
            // The instantiated head MUST equal the stated goal, else the proof is a forgery.
            let derived = apply(dag, &subst, head);
            if derived == goal {
                Ok(goal)
            } else {
                Err(ProofError::HeadMismatch {
                    rule,
                    derived,
                    stated: goal,
                })
            }
        }
    }
}

// ── Provenance projection (byte-identical to the forward reasoner) ──────────────

/// Project `proof` to its content-addressed derivation IRI, byte-identical to the
/// [`crate::derivation_graph::RuleApplication`] for the same firing.
///
/// A `by_rule` proof folds `(rule_iri, sorted source reifier IRIs)` through
/// [`crate::provenance::mint_derivation_id`]; an `assert` proof folds the assert-rule sentinel
/// ([`crate::provenance::ASSERT_RULE_IRI`]) with its single reifier, matching golden-6's
/// asserted-fact derivation. The source reifier of a premise is that fact's reifier IRI —
/// carried directly on an `assert` premise, or [`reify`]'d from a `by_rule` premise's goal.
///
/// # Errors
///
/// Hard-fails if the node is not a well-formed proof, if a projected leaf is not an IRI, or if
/// a premise term is not reifiable (inheriting [`reify`]'s `TermValue::Triple` hard-fail).
pub(crate) fn derivation_iri(dag: &TermDag, proof: NodeId) -> gmeow_errors::Result<String> {
    match classify(dag, proof)
        .map_err(|e| proof_err(format!("cannot project proof to a derivation IRI: {e:?}")))?
    {
        ProofShape::Assert { goal: _, reifier } => {
            let reifier_iri = atom_iri(dag, reifier)?;
            Ok(provenance::mint_derivation_id(
                provenance::ASSERT_RULE_IRI,
                &[reifier_iri.as_str()],
            ))
        }
        ProofShape::ByRule {
            goal: _,
            rule,
            subproofs,
        } => {
            let rule_iri = atom_iri(dag, rule)?;
            let mut sources = Vec::with_capacity(subproofs.len());
            for sub in &subproofs {
                sources.push(source_reifier(dag, *sub)?);
            }
            let refs: Vec<&str> = sources.iter().map(String::as_str).collect();
            Ok(provenance::mint_derivation_id(&rule_iri, &refs))
        }
    }
}

/// The content-addressed derivation IRI of a STRUCTURED (function-symbol) proof, reusing
/// the canonical [`crate::provenance::mint_derivation_id`] recipe over the resolver's own
/// content-addressed term keys.
///
/// [`derivation_iri`] projects a FLAT proof by reifying each source fact to a binary
/// [`crate::provenance::mint_nary_reifier`] IRI; that path hard-fails on a structured term
/// ([`reify`] requires atomic-leaf arguments). A goal-directed proof over function-symbol
/// terms (Peano / lists / …) has no binary reifier, so each source fact is content-addressed
/// by the resolver's canonical term key ([`TermDag::key`]) — the SAME injective, bottom-up,
/// alpha-invariant key the well-founded model uses for set membership, so it is ground,
/// canonical, and byte-stable run-to-run. The minting recipe (rule IRI + sorted sources) is
/// unchanged, so a structured derivation IRI shares the derivation namespace and
/// order-independence of a flat one, and no numeric [`NodeId`] handle is ever serialized.
pub(crate) fn structured_derivation_iri(
    dag: &TermDag,
    proof: NodeId,
) -> gmeow_errors::Result<String> {
    match classify(dag, proof).map_err(|e| {
        proof_err(format!(
            "cannot project structured proof to a derivation IRI: {e:?}"
        ))
    })? {
        // An asserted (RDF-grounded) premise still carries a genuine binary reifier IRI.
        ProofShape::Assert { goal: _, reifier } => {
            let reifier_iri = atom_iri(dag, reifier)?;
            Ok(provenance::mint_derivation_id(
                provenance::ASSERT_RULE_IRI,
                &[reifier_iri.as_str()],
            ))
        }
        ProofShape::ByRule {
            goal: _,
            rule,
            subproofs,
        } => {
            let rule_iri = atom_iri(dag, rule)?;
            let mut sources = Vec::with_capacity(subproofs.len());
            for sub in &subproofs {
                sources.push(structured_source_key(dag, *sub)?);
            }
            let refs: Vec<&str> = sources.iter().map(String::as_str).collect();
            Ok(provenance::mint_derivation_id(&rule_iri, &refs))
        }
    }
}

/// The content-addressed source key of a premise subproof for a structured derivation: an
/// asserted premise's genuine binary reifier IRI, or (for a structured premise with no binary
/// reifier) the canonical term key of the fact it concludes.
fn structured_source_key(dag: &TermDag, subproof: NodeId) -> gmeow_errors::Result<String> {
    match classify(dag, subproof).map_err(|e| {
        proof_err(format!(
            "cannot project structured premise to a source key: {e:?}"
        ))
    })? {
        ProofShape::Assert { goal: _, reifier } => atom_iri(dag, reifier),
        ProofShape::ByRule { goal, .. } => Ok(dag.key(goal).to_owned()),
    }
}

/// The content-addressed reifier IRI of the fact a premise subproof proves.
fn source_reifier(dag: &TermDag, subproof: NodeId) -> gmeow_errors::Result<String> {
    match classify(dag, subproof)
        .map_err(|e| proof_err(format!("cannot project premise to a source reifier: {e:?}")))?
    {
        // An asserted premise carries its content-addressed reifier IRI directly.
        ProofShape::Assert { goal: _, reifier } => atom_iri(dag, reifier),
        // A derived premise's source reifier is the reifier of the fact it concludes.
        ProofShape::ByRule { goal, .. } => reify(dag, goal),
    }
}

/// Fold a ground n-ary application term to its content-addressed reifier IRI, byte-identical
/// to [`crate::provenance::mint_nary_reifier`] over the same resolved arguments.
///
/// `node` must be `App{op: Leaf(relation IRI), args: [Leaf, …]}` — a ground atom. Its resolved
/// argument [`TermValue`]s are fed to the shared minting recipe (never a forked hash), so the
/// term's DAG identity and its reifier IRI agree (§19 single-path identity).
///
/// # Errors
///
/// Hard-fails if `node` is not a ground n-ary application, if the operator/arguments are not
/// atomic leaves, or if any argument is a `TermValue::Triple` (inheriting
/// [`crate::provenance::term_n3`]'s RDF-star hard-fail — an unhashable quoted triple).
pub(crate) fn reify(dag: &TermDag, node: NodeId) -> gmeow_errors::Result<String> {
    let (op, args) = match dag.data(node) {
        NodeData::App { op, args } => (*op, args.clone()),
        other => {
            return Err(proof_err(format!(
                "reify expects a ground n-ary application term, found {other:?}"
            )));
        }
    };
    let relation = leaf_iri(dag, op)
        .ok_or_else(|| proof_err("a reifiable term's operator must be an IRI leaf".to_owned()))?;
    let mut arg_values = Vec::with_capacity(args.len());
    for &arg in args.iter() {
        let atom = leaf_atom(dag, arg).ok_or_else(|| {
            proof_err(format!(
                "reify requires a ground term: argument node {arg:?} is not an atomic leaf"
            ))
        })?;
        arg_values.push(dag.atom_value(atom).clone());
    }
    provenance::mint_nary_reifier(&relation, &arg_values)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::derivation_graph::{FactKey, RuleApplication};

    const P: &str = "https://example.org/p";
    const Q: &str = "https://example.org/q";
    const A: &str = "https://example.org/a";
    const B: &str = "https://example.org/b";
    const C: &str = "https://example.org/c";
    const MUL: &str = "https://example.org/mul";
    const RULE: &str = "https://blackcatinformatics.ca/logic/rules/p_from_q";

    fn iri(s: &str) -> TermValue {
        TermValue::iri(s)
    }

    /// Build the rule `p(X) :- q(X)` in `dag`: its rule-IRI handle, head atom, and body
    /// atoms, sharing one metavariable `X` between head and body.
    fn build_rule(dag: &mut TermDag) -> (TermId, NodeId, Vec<NodeId>) {
        let (_x_meta, x) = dag.fresh_meta();
        let p = dag.intern_leaf(iri(P));
        let q = dag.intern_leaf(iri(Q));
        let head = dag.intern_app(p, vec![x]);
        let body = dag.intern_app(q, vec![x]);
        let rule_tid = dag.intern_atom(&iri(RULE));
        (rule_tid, head, vec![body])
    }

    /// Build the ground atom `rel(args…)` in `dag`.
    fn ground_atom(dag: &mut TermDag, rel: &str, args: &[&str]) -> NodeId {
        let op = dag.intern_leaf(iri(rel));
        let arg_nodes: Vec<NodeId> = args.iter().map(|a| dag.intern_leaf(iri(a))).collect();
        dag.intern_app(op, arg_nodes)
    }

    /// A leaf handle for `iri_str`, so it can be carried as a proof reifier argument.
    fn reifier_handle(dag: &mut TermDag, iri_str: &str) -> TermId {
        dag.intern_atom(&iri(iri_str))
    }

    // ── Test 1: check accepts a valid proof ─────────────────────────────────────────

    #[test]
    fn check_accepts_a_valid_rule_application() {
        let mut dag = TermDag::new();
        let (rule_tid, head, body) = build_rule(&mut dag);
        let q_a = ground_atom(&mut dag, Q, &[A]);
        let p_a = ground_atom(&mut dag, P, &[A]);

        let mut ctx = RuleCtx::default();
        ctx.rules.insert(rule_tid, (head, body));
        ctx.asserted.insert(q_a);

        let q_a_reifier = provenance::mint_nary_reifier(Q, &[iri(A)]).unwrap();
        let reifier_tid = reifier_handle(&mut dag, &q_a_reifier);
        let assert_qa = proof_assert(&mut dag, q_a, reifier_tid);

        // by_rule(p(a); rule=p(X):-q(X); [assert(q(a))]) re-derives p(a).
        let proof = proof_by_rule(&mut dag, p_a, rule_tid, &[assert_qa]);
        assert_eq!(
            check(&mut dag, proof, &ctx),
            Ok(p_a),
            "a valid proof checks to its goal"
        );
    }

    // ── Test 2: check rejects tampered proofs ───────────────────────────────────────

    #[test]
    fn check_rejects_tampered_proofs() {
        let mut dag = TermDag::new();
        let (rule_tid, head, body) = build_rule(&mut dag);
        let q_a = ground_atom(&mut dag, Q, &[A]);
        let p_a = ground_atom(&mut dag, P, &[A]);
        let p_b = ground_atom(&mut dag, P, &[B]);
        let q_b = ground_atom(&mut dag, Q, &[B]);

        let mut ctx = RuleCtx::default();
        ctx.rules.insert(rule_tid, (head, body));
        ctx.asserted.insert(q_a); // ONLY q(a) is asserted.

        let q_a_reifier = provenance::mint_nary_reifier(Q, &[iri(A)]).unwrap();
        let q_a_reifier_tid = reifier_handle(&mut dag, &q_a_reifier);
        let assert_qa = proof_assert(&mut dag, q_a, q_a_reifier_tid);

        // (a) Tampered goal: by_rule(p(b); rule; [assert(q(a))]) re-derives p(a) ≠ p(b).
        let tampered_goal = proof_by_rule(&mut dag, p_b, rule_tid, &[assert_qa]);
        assert!(
            matches!(
                check(&mut dag, tampered_goal, &ctx),
                Err(ProofError::HeadMismatch { .. })
            ),
            "a proof whose stated goal is not the re-derived head is rejected"
        );

        // (b1) Missing premise: assert(q(b)) where only q(a) is asserted.
        let q_b_reifier = provenance::mint_nary_reifier(Q, &[iri(B)]).unwrap();
        let q_b_reifier_tid = reifier_handle(&mut dag, &q_b_reifier);
        let assert_qb = proof_assert(&mut dag, q_b, q_b_reifier_tid);
        let wrong_premise = proof_by_rule(&mut dag, p_a, rule_tid, &[assert_qb]);
        assert!(
            matches!(
                check(&mut dag, wrong_premise, &ctx),
                Err(ProofError::NotAsserted { .. })
            ),
            "a premise appealing to a non-asserted EDB fact is rejected"
        );

        // (b2) Empty subproofs where the rule demands one premise → arity mismatch.
        let no_premise = proof_by_rule(&mut dag, p_a, rule_tid, &[]);
        assert!(
            matches!(
                check(&mut dag, no_premise, &ctx),
                Err(ProofError::ArityMismatch { .. })
            ),
            "a premise count differing from the rule body arity is rejected"
        );

        // (c) Unknown rule IRI (not in the RuleCtx).
        let unknown_tid = reifier_handle(&mut dag, "https://example.org/no_such_rule");
        let unknown_rule = proof_by_rule(&mut dag, p_a, unknown_tid, &[assert_qa]);
        assert!(
            matches!(
                check(&mut dag, unknown_rule, &ctx),
                Err(ProofError::UnknownRule { .. })
            ),
            "a proof citing a rule absent from the context is rejected"
        );
    }

    // ── Test 3: derivation_iri byte-parity with mint_derivation_id / RuleApplication ─

    #[test]
    fn derivation_iri_is_byte_identical_to_mint_derivation_id() {
        let mut dag = TermDag::new();
        let (rule_tid, _head, _body) = build_rule(&mut dag);
        let q_a = ground_atom(&mut dag, Q, &[A]);
        let p_a = ground_atom(&mut dag, P, &[A]);

        // The reifier of q(a), content-addressed from its ground surface (no numeric ids).
        let q_a_reifier = provenance::mint_nary_reifier(Q, &[iri(A)]).unwrap();
        let reifier_tid = reifier_handle(&mut dag, &q_a_reifier);
        let assert_qa = proof_assert(&mut dag, q_a, reifier_tid);
        let proof = proof_by_rule(&mut dag, p_a, rule_tid, &[assert_qa]);

        let got = derivation_iri(&dag, proof).unwrap();

        // (i) equals a hand-computed mint_derivation_id over the SAME string inputs — the
        // only inputs are the rule IRI and the source reifier IRI, never a NodeId/TermId.
        let expected = provenance::mint_derivation_id(RULE, &[q_a_reifier.as_str()]);
        assert_eq!(
            got, expected,
            "derivation_iri must reuse mint_derivation_id"
        );

        // (ii) equals the forward reasoner's RuleApplication id for the same firing.
        let app = RuleApplication::new(RULE, [FactKey::from(q_a_reifier.as_str())]);
        assert_eq!(
            got,
            app.derivation_id(),
            "proof and RuleApplication derivation ids must agree byte-for-byte"
        );
    }

    // ── Test 4: reify parity + TermValue::Triple hard-fail ──────────────────────────

    #[test]
    fn reify_matches_mint_nary_reifier_and_hard_fails_on_triple() {
        let mut dag = TermDag::new();
        let mul_abc = ground_atom(&mut dag, MUL, &[A, B, C]);

        let got = reify(&dag, mul_abc).unwrap();
        let expected = provenance::mint_nary_reifier(MUL, &[iri(A), iri(B), iri(C)]).unwrap();
        assert_eq!(
            got, expected,
            "reify must reuse mint_nary_reifier on the resolved args"
        );

        // A term carrying an RDF-star quoted-triple argument inherits term_n3's hard fail.
        let triple = TermValue::Triple {
            s: Box::new(iri(A)),
            p: Box::new(iri(P)),
            o: Box::new(iri(B)),
        };
        let op = dag.intern_leaf(iri(MUL));
        let triple_leaf = dag.intern_leaf(triple);
        let with_triple = dag.intern_app(op, vec![triple_leaf]);
        assert!(
            reify(&dag, with_triple).is_err(),
            "a term with a TermValue::Triple argument must hard-fail to reify"
        );
    }

    // ── Test 5: proofs hash-cons (maximal sharing) ──────────────────────────────────

    #[test]
    fn structurally_identical_proofs_share_one_node() {
        let mut dag = TermDag::new();
        let (rule_tid, _head, _body) = build_rule(&mut dag);
        let q_a = ground_atom(&mut dag, Q, &[A]);
        let p_a = ground_atom(&mut dag, P, &[A]);
        let q_a_reifier = provenance::mint_nary_reifier(Q, &[iri(A)]).unwrap();
        let reifier_tid = reifier_handle(&mut dag, &q_a_reifier);

        let assert1 = proof_assert(&mut dag, q_a, reifier_tid);
        let assert2 = proof_assert(&mut dag, q_a, reifier_tid);
        assert_eq!(
            assert1, assert2,
            "identical assert proofs intern to one NodeId"
        );

        let proof1 = proof_by_rule(&mut dag, p_a, rule_tid, &[assert1]);
        let proof2 = proof_by_rule(&mut dag, p_a, rule_tid, &[assert2]);
        assert_eq!(
            proof1, proof2,
            "identical by_rule proofs intern to one NodeId (maximal sharing)"
        );
    }
}
