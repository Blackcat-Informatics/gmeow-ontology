// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The engine adapter onto the relational-core lowering waist (`logic:RelationalCore`).
//!
//! The full-FOL clausifier (NNF → Skolemize → Horn-clause extraction) is the engine-agnostic
//! lane [`gmeow_logic_compile::relational_core`] — *the* single place a
//! [`Formula`](gmeow_logic_compile::ir::Formula) becomes Horn. This module is the thin
//! physical-engine adapter: it asks the lane to lower a program's formulas to relational-core
//! [`RcRule`]s + flagged residue, then maps each `RcRule` onward to the evaluable
//! [`EvalRule`] the chase runs (the native [`TermValue`] bridge that cannot live in the
//! wasm-clean lane).
//!
//! The honest [`PreservationClaim`] is `{exact}` only when the whole formula set lowered, else
//! `{sound-under}` naming the residue — sourced directly from the lane's residue so the engine
//! and the carrier/projections agree (one decomposition, no parallel clausifier).
//!
//! Floor of the supported fragment (the lane's, verbatim): a formula whose negation-normal
//! form is a conjunction of Horn clauses whose atoms are binary, or **fixed-arity n-ary**
//! atoms reified into binary atoms over a reifier node (`∀x̄. A ← B₁ ∧ … ∧ Bₙ`, optionally
//! with a leading existential prefix Skolemized to constants; an n-ary head derives a tuple
//! over an existential reifier). Beyond it — a disjunctive head, a quantifier alternation
//! (`∃` under `∀`), a genuinely unbounded sequence-marker atom, or an n-ary head argument the
//! body does not bind — is carried as flagged residue, never mis-lowered.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::TermValue;

use gmeow_logic_compile::ir::{LOGIC_NAMESPACE, LogicProgram};
use gmeow_logic_compile::relational_core::{RcAtom, RcRule, RcTerm, RcViolationResidue};

use crate::facts::sha1_hex;
use crate::query_ir::{QBuiltin, QTerm};
use crate::result::PreservationClaim;
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm};

/// Wrap a relational-core lowering condition message as a typed diagnostic on the
/// shared substrate, preserving the authored text verbatim.
fn rc_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::RelationalCore { detail })
}

/// The outcome of lowering a program's full-FOL formulas to the evaluable engine IR.
#[derive(Debug, Clone)]
pub(crate) struct RelationalCoreLowering {
    /// The [`EvalRule`]s the Horn-expressible (ordinary, single-head) formula fragment
    /// produced, mapped from the lane's [`RcRule`]s for the native forward chase.
    pub(crate) rules: Vec<EvalRule>,
    /// The typed conjunctive-head existential rules for the program's n-ary
    /// HEAD-derivations. They remain separate from [`Self::rules`] because they
    /// invent reifier nulls and are evaluated by the native restricted chase. Empty when the
    /// program derives no n-ary tuples.
    pub(crate) nary_head_rules: Vec<crate::physical::ExistentialRule>,
    /// An honest preservation claim: `{exact}` when every formula lowered, else
    /// `{sound-under}` carrying a description of each unsupported formula.
    pub(crate) preservation: PreservationClaim,
}

/// Lower every full-FOL formula in `program` to the evaluable engine IR, delegating the
/// clausification to the canonical lane and mapping each resulting [`RcRule`] to an
/// [`EvalRule`]. The non-Horn remainder is carried as the lane's flagged residue. An
/// `RcRule` that cannot be mapped onward is itself flagged as residue rather than
/// mis-lowered (legalization, total).
pub(crate) fn lower_formulas(program: &LogicProgram) -> RelationalCoreLowering {
    let (rc_rules, lane_residue) =
        gmeow_logic_compile::relational_core::lower_formulas_to_rc(program);

    let mut rules: Vec<EvalRule> = Vec::new();
    let mut residue: BTreeSet<String> = lane_residue.into_iter().collect();
    // Partition the lane's rules: an n-ary HEAD-derivation rule (non-empty `head_conjuncts`)
    // maps to a conjunctive-head existential rule; every ordinary (single-head)
    // rule maps onward to an evaluable [`EvalRule`] for the native chase.
    let mut nary: Vec<&RcRule> = Vec::new();
    for rc in &rc_rules {
        if !rc.head_conjuncts.is_empty() {
            nary.push(rc);
            continue;
        }
        match rc_rule_to_eval(rc) {
            Ok(rule) => rules.push(rule),
            Err(reason) => {
                residue.insert(reason.message().to_owned());
            }
        }
    }
    let mut nary_head_rules = Vec::new();
    for rule in nary {
        match rc_rule_to_existential(rule) {
            Ok(rule) => nary_head_rules.push(rule),
            Err(reason) => {
                residue.insert(reason.message().to_owned());
            }
        }
    }

    RelationalCoreLowering {
        preservation: PreservationClaim::for_unsupported(residue),
        rules,
        nary_head_rules,
    }
}

// --------------------------------------------------------------------------- //
// RcRule// --------------------------------------------------------------------------- //
// RcRule → EvalRule (the native TermValue engine bridge)
// --------------------------------------------------------------------------- //

/// Map a lane [`RcRule`] to an evaluable [`EvalRule`]. Mirrors [`crate::lower::lower_rule`]'s
/// `LogicRule → EvalRule` term handling exactly (a `?var` stays a variable, an object literal
/// becomes a plain `xsd:string` `ConstLit`, every other term an IRI constant), so a formula
/// lowered through the lane and the equivalent authored Horn rule produce identical
/// head/body atoms. The `rule_iri` is a deterministic content hash of the rule (a
/// provenance/naming artifact; the chase derivations are unaffected by its exact value).
fn rc_rule_to_eval(rc: &RcRule) -> gmeow_errors::Result<EvalRule> {
    let head = rc_atom_to_eval(&rc.head)?;
    let body: gmeow_errors::Result<Vec<EvalAtom>> = rc.body.iter().map(rc_atom_to_eval).collect();
    let rule_iri = format!("{LOGIC_NAMESPACE}formula-rule/{}", sha1_hex(&rc.key()));
    Ok(EvalRule {
        head,
        body: body?,
        rule_iri,
        distinct_pairs: rc.distinct_pairs.clone(),
        // The relational-core lowering carries no arithmetic builtins.
        builtins: Vec::new(),
        constraint_tag: None,
    })
}

fn rc_rule_to_existential(rc: &RcRule) -> gmeow_errors::Result<crate::physical::ExistentialRule> {
    if rc.body.iter().any(|atom| atom.negated) {
        return Err(rc_err(
            "n-ary existential rule carries a negated body atom the restricted chase cannot honor"
                .to_owned(),
        ));
    }
    let mut head = vec![rc_atom_to_eval(&rc.head)?];
    head.extend(
        rc.head_conjuncts
            .iter()
            .map(rc_atom_to_eval)
            .collect::<gmeow_errors::Result<Vec<_>>>()?,
    );
    let body = rc
        .body
        .iter()
        .map(rc_atom_to_eval)
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    Ok(crate::physical::ExistentialRule {
        rule_iri: format!("{LOGIC_NAMESPACE}formula-nary-head/{}", sha1_hex(&rc.key())),
        body,
        head,
        distinct: rc.distinct_pairs.clone(),
        witness_frontier: None,
        witness_policy: crate::physical::WitnessPolicy::FrontierSkolem,
    })
}

/// Map a lane [`RcAtom`] to an [`EvalAtom`] (native predicate IRI string + [`EvalTerm`]s).
fn rc_atom_to_eval(atom: &RcAtom) -> gmeow_errors::Result<EvalAtom> {
    let subject = rc_term_to_eval(&atom.subject, false)?;
    let object = rc_term_to_eval(&atom.object, true)?;
    Ok(EvalAtom {
        subject,
        predicate: atom.predicate.clone(),
        object,
        negated: atom.negated,
    })
}

/// Map a lane [`RcTerm`] to an [`EvalTerm`]. `is_object` gates a literal to the object slot.
/// A blank node has no engine-term form and never arises from formula clausification (the
/// lane mints Skolem **constants**, not blanks); it is a hard error (no-optionality).
fn rc_term_to_eval(term: &RcTerm, is_object: bool) -> gmeow_errors::Result<EvalTerm> {
    match term {
        // RcTerm::Var already carries the `?` sigil (matching lower::lower_term).
        RcTerm::Var(name) => Ok(EvalTerm::Var(name.clone())),
        RcTerm::Iri(iri) => Ok(EvalTerm::ConstNamed(iri.clone())),
        RcTerm::Literal(lex) => {
            if !is_object {
                return Err(rc_err(format!(
                    "relational-core literal {lex:?} in subject position (only an object may be a \
                     literal)"
                )));
            }
            Ok(EvalTerm::ConstLit(TermValue::simple_literal(lex)))
        }
        RcTerm::Blank(label) => Err(rc_err(format!(
            "relational-core blank node {label:?} in a formula-derived rule — the clausifier \
             mints Skolem constants, never blanks (no-optionality)"
        ))),
    }
}

// --------------------------------------------------------------------------- //
// Constraint violation-rule lowering: logic:Constraint → EvalRule (R5/R6 crux)
// --------------------------------------------------------------------------- //
//
// A `logic:Constraint` whose `logic:integrity` formula's consequent names a relation
// registered here as BUILTIN-BOUND compiles to a VIOLATION-EMITTING forward [`EvalRule`]:
// the antecedent becomes the ordinary positive body (bridged from its HiLog reflection
// relations to the real object-level properties asserted data carries, via `rdfs:seeAlso`),
// and the consequent is bound to the registered native builtin
// ([`crate::query_ir::QBuiltin::DimEqual`] / [`crate::query_ir::QBuiltin::DimProduct`]) that
// decides the law by exact-rational arithmetic. The rule is stamped with
// [`EvalRule::constraint_tag`], the ONLY provenance a builtin needs to run inside the
// forward semi-naive chase and to carry VIOLATION-EMITTING (rather than pruning) `Filter`
// semantics (`crate::physical::seminaive::apply_builtins`).

/// Which native builtin a registered consequent relation binds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DimBuiltinKind {
    /// `math:dimensionEqualityRel(d1, d2)` — the Dim `=:=` commensurability builtin.
    Equal,
    /// `math:dimensionProductRel(dF, dM, dR)` — the Dim `⊕` composition builtin.
    Product,
}

/// Namespace root for the `math:` measure-and-dimension vocabulary — the owner of the
/// two currently-registered builtin-bound consequent relations.
const MATH: &str = "https://blackcatinformatics.ca/math/";

/// The registered BUILTIN-BOUND consequent relations this lowering recognizes — a small,
/// explicit table (never a hardcoded single-relation path): a future non-`math:` law
/// registers here the same way, keyed on its own reflection relation.
fn builtin_consequent_registry() -> BTreeMap<String, DimBuiltinKind> {
    [
        (format!("{MATH}dimensionEqualityRel"), DimBuiltinKind::Equal),
        (
            format!("{MATH}dimensionProductRel"),
            DimBuiltinKind::Product,
        ),
    ]
    .into_iter()
    .collect()
}

/// `rdf:type` — the marker triple's predicate.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Lower every builtin-bound-consequent `logic:Constraint` in `program` into a
/// VIOLATION-EMITTING [`EvalRule`], `constraint_tag`-stamped with the source constraint's
/// IRI.
///
/// `see_also` bridges each antecedent atom's HiLog reflection relation to the real
/// object-level property the asserted data carries (`rdfs:seeAlso`), read from the same
/// source graph the program was compiled from — the lowered body atoms then match real
/// triples (`math:homogeneousOperand`, …) rather than a second, never-asserted
/// reflection-relation-keyed data source. A relation absent from `see_also` (the
/// consequent relations themselves, which are never a body predicate) is left unchanged.
///
/// # Errors
///
/// Returns `Err` if a matched constraint's consequent argument count does not match its
/// registered builtin's arity, or if a consequent/antecedent term is not a variable or
/// IRI (a literal/blank dimension operand — never authored by the two dimension-gate
/// laws) — an internal-invariant hard-fail, never a silent skip.
pub(crate) fn lower_constraint_violation_rules(
    program: &LogicProgram,
    see_also: &BTreeMap<String, String>,
) -> gmeow_errors::Result<Vec<EvalRule>> {
    let registry = builtin_consequent_registry();
    let relations: BTreeSet<String> = registry.keys().cloned().collect();
    let rc_rules =
        gmeow_logic_compile::relational_core::lower_constraints_to_rc(program, &relations);

    let mut rules = Vec::with_capacity(rc_rules.len());
    for rc in &rc_rules {
        let kind = *registry.get(&rc.consequent_relation).ok_or_else(|| {
            rc_err(format!(
                "constraint {} lowered a consequent relation {} outside the builtin \
                 registry — lower_constraints_to_rc must only return registered relations",
                rc.constraint_iri, rc.consequent_relation
            ))
        })?;
        let body = rc
            .body
            .iter()
            .map(|atom| rc_atom_to_eval_bridged(atom, see_also))
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        let head = EvalAtom {
            subject: rc_term_to_eval(&rc.subject, false)?,
            predicate: RDF_TYPE.to_owned(),
            object: EvalTerm::ConstNamed(rc.failure_class.clone()),
            negated: false,
        };
        let builtin = dim_builtin(kind, &rc.consequent_relation, &rc.consequent_args)?;
        let rule_iri = format!(
            "{LOGIC_NAMESPACE}constraint-violation-rule/{}",
            sha1_hex(&rc.constraint_iri)
        );
        rules.push(EvalRule {
            head,
            body,
            rule_iri,
            distinct_pairs: Vec::new(),
            builtins: vec![builtin],
            constraint_tag: Some(rc.constraint_iri.clone()),
        });
    }
    Ok(rules)
}

/// The compiled form of a program's ORDINARY (non-builtin-bound) `logic:Constraint`
/// corpus: the violation rules that lowered, plus the flagged residue naming every
/// constraint that did not and why.
///
/// The residue travels WITH the rules deliberately. A gate that reports "N laws compiled"
/// while discarding the residue is reporting a number nobody can audit; carrying both lets
/// the caller state the compiled fraction and name the shortfall.
#[derive(Debug, Clone)]
pub(crate) struct ViolationLowering {
    /// The violation-emitting rules — one per consequent conjunct of every constraint
    /// that lowered — in canonical (constraint-IRI, conjunct-index) order.
    pub(crate) rules: Vec<EvalRule>,
    /// Every `logic:Constraint` this lowering declined, with the closed reason token.
    pub(crate) residue: Vec<RcViolationResidue>,
}

/// Lower `program`'s ordinary (non-builtin-bound) `logic:Constraint`s into
/// VIOLATION-EMITTING [`EvalRule`]s: an antecedent-plus-negated-consequent body, and a
/// head typing the focus variable with the constraint's `gmeow:enforcesFailureClass`.
///
/// The lane ([`gmeow_logic_compile::relational_core::lower_violation_constraints_to_rc`])
/// owns the formula analysis; this function is the thin native bridge that maps the lane's
/// binary [`RcAtom`]s to the engine's [`EvalAtom`]s and mints the rule identity.
///
/// No `rdfs:seeAlso` reflection-substitution map is threaded through, unlike
/// [`lower_constraint_violation_rules`]. That map exists because the `math:` dimension
/// laws predicate over HiLog REFLECTION relations (`math:hasDimensionRel`, …) that no data
/// ever asserts, so the lowered body had to be bridged to the object-level property. The
/// ordinary constraint corpus does not: its `logic:Formula` ASTs name the object-level
/// properties directly (`logic:fencingIdentity`, `logic:receiptOfAttempt`, `rdf:type`, …),
/// which is exactly what the asserted data carries. Threading a substitution map here
/// would be worse than useless — it would silently REWRITE any authored predicate that
/// happened to carry an `rdfs:seeAlso` for an unrelated documentary reason, pointing the
/// body at a relation the law never mentioned.
///
/// Every emitted rule is `constraint_tag`-stamped with its source constraint's IRI: the
/// law's identity is the only provenance that survives into the chase, and it is what
/// makes a materialized marker traceable back to the authored `logic:Constraint`.
///
/// # Errors
///
/// Returns `Err` if a lowered body atom carries a term with no engine form (a blank node,
/// or a literal in subject position) — an internal-invariant failure of the lane, never a
/// recoverable runtime condition.
pub(crate) fn lower_violation_rules(
    program: &LogicProgram,
) -> gmeow_errors::Result<ViolationLowering> {
    let builtin_relations: BTreeSet<String> =
        builtin_consequent_registry().keys().cloned().collect();
    let (rc_rules, residue) =
        gmeow_logic_compile::relational_core::lower_violation_constraints_to_rc(
            program,
            &builtin_relations,
        );

    let mut rules = Vec::with_capacity(rc_rules.len());
    for rc in &rc_rules {
        let body = rc
            .body
            .iter()
            .map(rc_atom_to_eval)
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        let head = EvalAtom {
            subject: rc_term_to_eval(&rc.subject, false)?,
            predicate: RDF_TYPE.to_owned(),
            object: EvalTerm::ConstNamed(rc.failure_class.clone()),
            negated: false,
        };
        // Conjunct-qualified identity: two conjuncts of one constraint are two rules with
        // two different bodies, so they must not collapse onto one rule IRI.
        let rule_iri = format!(
            "{LOGIC_NAMESPACE}constraint-violation-rule/{}",
            sha1_hex(&format!("{}#{}", rc.constraint_iri, rc.conjunct_index))
        );
        rules.push(EvalRule {
            head,
            body,
            rule_iri,
            distinct_pairs: Vec::new(),
            builtins: Vec::new(),
            constraint_tag: Some(rc.constraint_iri.clone()),
        });
    }
    Ok(ViolationLowering { rules, residue })
}

/// Map one antecedent [`RcAtom`] to an [`EvalAtom`], bridging its predicate through
/// `see_also` when it names a HiLog reflection relation.
fn rc_atom_to_eval_bridged(
    atom: &RcAtom,
    see_also: &BTreeMap<String, String>,
) -> gmeow_errors::Result<EvalAtom> {
    let subject = rc_term_to_eval(&atom.subject, false)?;
    let object = rc_term_to_eval(&atom.object, true)?;
    let predicate = see_also
        .get(&atom.predicate)
        .cloned()
        .unwrap_or_else(|| atom.predicate.clone());
    Ok(EvalAtom {
        subject,
        predicate,
        object,
        negated: atom.negated,
    })
}

/// Bind a registered consequent relation's structural arguments to its native
/// [`QBuiltin`], hard-failing on an arity mismatch or a non-variable/IRI operand.
fn dim_builtin(
    kind: DimBuiltinKind,
    relation: &str,
    args: &[RcTerm],
) -> gmeow_errors::Result<QBuiltin> {
    match kind {
        DimBuiltinKind::Equal => {
            let [d1, d2] = args else {
                return Err(rc_err(format!(
                    "{relation} consequent must have exactly 2 arguments (dimEqual(d1, d2)), \
                     got {}",
                    args.len()
                )));
            };
            Ok(QBuiltin::DimEqual {
                d1: rc_term_to_qterm(d1)?,
                d2: rc_term_to_qterm(d2)?,
            })
        }
        DimBuiltinKind::Product => {
            let [d_f, d_m, d_r] = args else {
                return Err(rc_err(format!(
                    "{relation} consequent must have exactly 3 arguments \
                     (dimProduct(dF, dM, dR)), got {}",
                    args.len()
                )));
            };
            Ok(QBuiltin::DimProduct {
                d_f: rc_term_to_qterm(d_f)?,
                d_m: rc_term_to_qterm(d_m)?,
                d_r: rc_term_to_qterm(d_r)?,
            })
        }
    }
}

/// Map a lane [`RcTerm`] to a [`QTerm`] builtin operand: a variable stays a variable
/// (already `?`-prefixed, matching the body atoms' [`EvalTerm::Var`] keys), an IRI
/// constant becomes a canonical `<iri>` surface (matching `resolve_iri_operand`'s
/// expected `Const` shape). A blank node or literal is a hard error — no dimension-gate
/// consequent argument is ever authored as either.
fn rc_term_to_qterm(term: &RcTerm) -> gmeow_errors::Result<QTerm> {
    match term {
        RcTerm::Var(v) => Ok(QTerm::Var(v.clone())),
        RcTerm::Iri(i) => Ok(QTerm::Const(format!("<{i}>"))),
        RcTerm::Blank(label) => Err(rc_err(format!(
            "dimension-gate consequent argument is a blank node {label:?} — only a \
             variable or IRI is a legal dimEqual/dimProduct operand"
        ))),
        RcTerm::Literal(lex) => Err(rc_err(format!(
            "dimension-gate consequent argument is a literal {lex:?} — only a variable or \
             IRI is a legal dimEqual/dimProduct operand"
        ))),
    }
}

#[cfg(test)]
mod tests;
