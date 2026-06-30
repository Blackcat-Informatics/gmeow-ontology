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
//! wasm-clean lane), and renders them to Nemo `.rls`.
//!
//! The honest [`PreservationClaim`] is `{exact}` only when the whole formula set lowered, else
//! `{sound-under}` naming the residue — sourced directly from the lane's residue so the engine
//! and the carrier/projections agree (one decomposition, no parallel clausifier).
//!
//! Floor of the supported fragment (the lane's, verbatim): a formula whose negation-normal
//! form is a conjunction of Horn clauses of **binary** atoms (`∀x̄. A ← B₁ ∧ … ∧ Bₙ`,
//! optionally with a leading existential prefix Skolemized to constants). Beyond it — a
//! disjunctive head, a quantifier alternation (`∃` under `∀`), a non-binary or sequence-marker
//! atom — is carried as flagged residue, never mis-lowered.

use std::collections::BTreeSet;

use gmeow_rdf::TermValue;

use gmeow_logic_compile::ir::{LogicProgram, LOGIC_NAMESPACE};
use gmeow_logic_compile::relational_core::{RcAtom, RcRule, RcTerm};

use crate::encode::sha1_hex;
use crate::result::PreservationClaim;
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm};

/// The outcome of lowering a program's full-FOL formulas to the evaluable engine IR.
#[derive(Debug, Clone)]
pub(crate) struct RelationalCoreLowering {
    /// The [`EvalRule`]s the Horn-expressible formula fragment produced, mapped from the
    /// lane's [`RcRule`]s.
    pub(crate) rules: Vec<EvalRule>,
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
    for rc in &rc_rules {
        match rc_rule_to_eval(rc) {
            Ok(rule) => rules.push(rule),
            Err(reason) => {
                residue.insert(reason);
            }
        }
    }

    RelationalCoreLowering {
        preservation: PreservationClaim::for_unsupported(residue),
        rules,
    }
}

/// Lower a program's full-FOL formulas to evaluable Nemo `.rls` rule text, paired with the
/// honest [`PreservationClaim`] disclosing any non-evaluable residue.
///
/// The public seam the chase consumers share — [`crate::reason::reason_program`] and the
/// conformance harness both append this RLS to the program's Horn rules so the
/// Horn-expressible formula fragment evaluates in the same chase, while the residue is
/// disclosed in the preservation claim rather than silently dropped. A formula-free program
/// yields an empty string and an `{exact}` claim, so it changes no existing chase.
pub fn formula_eval_rls(program: &LogicProgram) -> (String, PreservationClaim) {
    let lowering = lower_formulas(program);
    let rls = crate::rule_ir::eval_rules_to_rls(&lowering.rules);
    (rls, lowering.preservation)
}

// --------------------------------------------------------------------------- //
// RcRule → EvalRule (the native TermValue engine bridge)
// --------------------------------------------------------------------------- //

/// Map a lane [`RcRule`] to an evaluable [`EvalRule`]. Mirrors [`crate::lower::lower_rule`]'s
/// `LogicRule → EvalRule` term handling exactly (a `?var` stays a variable, an object literal
/// becomes a plain `xsd:string` `ConstLit`, every other term an IRI constant), so a formula
/// lowered through the lane and the equivalent authored Horn rule produce identical
/// head/body atoms. The `rule_iri` is a deterministic content hash of the rule (a
/// provenance/naming artifact; the chase derivations are unaffected by its exact value).
fn rc_rule_to_eval(rc: &RcRule) -> Result<EvalRule, String> {
    let head = rc_atom_to_eval(&rc.head)?;
    let body: Result<Vec<EvalAtom>, String> = rc.body.iter().map(rc_atom_to_eval).collect();
    let rule_iri = format!(
        "{LOGIC_NAMESPACE}formula-rule/{}",
        sha1_hex(&rc_rule_key(rc))
    );
    Ok(EvalRule {
        head,
        body: body?,
        rule_iri,
        distinct_pairs: rc.distinct_pairs.clone(),
    })
}

/// Map a lane [`RcAtom`] to an [`EvalAtom`] (native predicate IRI string + [`EvalTerm`]s).
fn rc_atom_to_eval(atom: &RcAtom) -> Result<EvalAtom, String> {
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
fn rc_term_to_eval(term: &RcTerm, is_object: bool) -> Result<EvalTerm, String> {
    match term {
        // RcTerm::Var already carries the `?` sigil (matching lower::lower_term).
        RcTerm::Var(name) => Ok(EvalTerm::Var(name.clone())),
        RcTerm::Iri(iri) => Ok(EvalTerm::ConstNamed(iri.clone())),
        RcTerm::Literal(lex) => {
            if !is_object {
                return Err(format!(
                    "relational-core literal {lex:?} in subject position (only an object may be a \
                     literal)"
                ));
            }
            Ok(EvalTerm::ConstLit(TermValue::simple_literal(lex)))
        }
        RcTerm::Blank(label) => Err(format!(
            "relational-core blank node {label:?} in a formula-derived rule — the clausifier \
             mints Skolem constants, never blanks (no-optionality)"
        )),
    }
}

/// A deterministic content key for an [`RcRule`], used to mint its stable `rule_iri`.
fn rc_rule_key(rc: &RcRule) -> String {
    let body = rc
        .body
        .iter()
        .map(rc_atom_surface)
        .collect::<Vec<_>>()
        .join("\u{1d}");
    let distinct = rc
        .distinct_pairs
        .iter()
        .map(|(a, b)| format!("{a}\u{1f}{b}"))
        .collect::<Vec<_>>()
        .join("\u{1d}");
    format!("{}\u{1c}{body}\u{1c}{distinct}", rc_atom_surface(&rc.head))
}

/// A stable surface for one [`RcAtom`] (subject ▸ predicate ▸ object ▸ negated).
fn rc_atom_surface(a: &RcAtom) -> String {
    format!(
        "{}\u{1e}{}\u{1e}{}\u{1e}{}",
        rc_term_surface(&a.subject),
        a.predicate,
        rc_term_surface(&a.object),
        a.negated,
    )
}

/// A stable, type-tagged surface for one [`RcTerm`].
fn rc_term_surface(t: &RcTerm) -> String {
    match t {
        RcTerm::Var(v) => format!("V\u{1f}{v}"),
        RcTerm::Iri(i) => format!("I\u{1f}{i}"),
        RcTerm::Blank(b) => format!("B\u{1f}{b}"),
        RcTerm::Literal(l) => format!("L\u{1f}{l}"),
    }
}

#[cfg(test)]
mod tests;
