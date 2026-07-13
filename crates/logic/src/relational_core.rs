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

use std::collections::BTreeSet;

use purrdf::TermValue;

use gmeow_logic_compile::ir::{LOGIC_NAMESPACE, LogicProgram};
use gmeow_logic_compile::relational_core::{RcAtom, RcRule, RcTerm};

use crate::facts::sha1_hex;
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

#[cfg(test)]
mod tests;
