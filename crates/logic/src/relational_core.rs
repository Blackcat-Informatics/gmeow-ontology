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

use purrdf::TermValue;

use gmeow_logic_compile::ir::{LOGIC_NAMESPACE, LogicProgram};
use gmeow_logic_compile::relational_core::{RcAtom, RcRule, RcTerm};

use crate::facts::sha1_hex;
use crate::result::PreservationClaim;
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm};

/// The outcome of lowering a program's full-FOL formulas to the evaluable engine IR.
#[derive(Debug, Clone)]
pub(crate) struct RelationalCoreLowering {
    /// The [`EvalRule`]s the Horn-expressible (ordinary, single-head) formula fragment
    /// produced, mapped from the lane's [`RcRule`]s. These render to the Nemo `.rls` the
    /// provenance-carrying forward chase runs.
    pub(crate) rules: Vec<EvalRule>,
    /// The conjunctive-head existential `.rls` for the program's n-ary HEAD-derivation rules
    /// (each carrying `head_conjuncts`). This is rendered SEPARATELY from [`Self::rules`]
    /// because it invents a reifier null the Nemo provenance trace cannot follow — it is
    /// evaluated by the native restricted chase, not the provenance oracle. Empty when the
    /// program derives no n-ary tuples.
    pub(crate) nary_head_rls: String,
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
    // renders to a conjunctive-head EXISTENTIAL `.rls` the native chase runs; every ordinary
    // (single-head) rule maps onward to an evaluable [`EvalRule`] for the Nemo chase.
    let mut nary: Vec<&RcRule> = Vec::new();
    for rc in &rc_rules {
        if !rc.head_conjuncts.is_empty() {
            nary.push(rc);
            continue;
        }
        match rc_rule_to_eval(rc) {
            Ok(rule) => rules.push(rule),
            Err(reason) => {
                residue.insert(reason);
            }
        }
    }
    let nary_head_rls = match render_nary_head_rules(&nary) {
        Ok(rls) => rls,
        Err(reason) => {
            // A render failure (e.g. an unexpected blank node) is carried as flagged residue,
            // never a silently-dropped derivation.
            residue.insert(reason);
            String::new()
        }
    };

    RelationalCoreLowering {
        preservation: PreservationClaim::for_unsupported(residue),
        rules,
        nary_head_rls,
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

/// The conjunctive-head EXISTENTIAL `.rls` for the program's n-ary HEAD-derivation formula
/// rules — the leg [`crate::reason::reason_program`] evaluates through the NATIVE restricted
/// chase (never the Nemo provenance chase, whose trace hard-fails on the invented reifier
/// null). Each rule invents one shared reifier `!R` per firing (Nemo existential syntax) and
/// reifies the derived tuple as `logic:instanceOf(R, Rel) ∧ logic:naryArg{i}(R, aᵢ)`, world-
/// threaded like the ordinary Nemo projection. A program that derives no n-ary tuples yields
/// the empty string (no chase leg to run).
pub fn formula_nary_head_rls(program: &LogicProgram) -> String {
    lower_formulas(program).nary_head_rls
}

// --------------------------------------------------------------------------- //
// Conjunctive-head existential RLS renderer (the n-ary HEAD leg)
// --------------------------------------------------------------------------- //

/// Render each n-ary HEAD-derivation [`RcRule`] as ONE conjunctive-head existential Nemo
/// rule in the ternary `<pred>(subject, object, world)` encoding: the shared reifier subject
/// (`?naryH…`) is emitted with Nemo existential syntax `!naryH…` so the chase
/// mints it once per firing and shares it across the whole `instanceOf` + `naryArg{i}`
/// conjunction; every body atom is the ordinary binary reification, rendered in the same
/// fresh world variable. The rule carries a deterministic `#[name(...)]` content-addressed on
/// the rule key.
///
/// # Errors
///
/// Returns `Err` if a rule's reifier subject is not a variable or an atom carries a blank
/// node (the clausifier mints constants, never blanks) — carried as flagged residue rather
/// than emitted as unsound `.rls`.
fn render_nary_head_rules(rules: &[&RcRule]) -> Result<String, String> {
    let mut out = String::new();
    for rc in rules {
        let RcTerm::Var(reifier) = &rc.head.subject else {
            return Err(format!(
                "n-ary head rule reifier subject is not a variable: {:?}",
                rc.head.subject
            ));
        };
        let name = format!("{LOGIC_NAMESPACE}formula-nary-head/{}", sha1_hex(&rc.key()));
        let name_esc = name.replace('\\', "\\\\").replace('"', "\\\"");
        out.push_str(&format!("#[name(\"{name_esc}\")]\n"));

        let world = fresh_world_var(rc);
        // Head = the instanceOf typing atom + every naryArg conjunct, sharing the reifier.
        let mut head_parts = vec![render_rc_atom(&rc.head, &world, reifier)?];
        for hc in &rc.head_conjuncts {
            head_parts.push(render_rc_atom(hc, &world, reifier)?);
        }
        let mut body_parts: Vec<String> = rc
            .body
            .iter()
            .map(|b| render_rc_atom(b, &world, reifier))
            .collect::<Result<_, _>>()?;
        for (a, b) in &rc.distinct_pairs {
            body_parts.push(format!("{a} != {b}"));
        }
        out.push_str(&format!(
            "{} :-\n    {} .\n",
            head_parts.join(", "),
            body_parts.join(",\n    ")
        ));
    }
    Ok(out)
}

/// Render one [`RcAtom`] as `[~]<pred>(subject, object, world)`, emitting the shared reifier
/// variable `existential` with Nemo existential syntax (`!name`).
fn render_rc_atom(atom: &RcAtom, world: &str, existential: &str) -> Result<String, String> {
    let subject = render_rc_term(&atom.subject, existential)?;
    let object = render_rc_term(&atom.object, existential)?;
    let prefix = if atom.negated { "~" } else { "" };
    Ok(format!(
        "{prefix}<{}>({subject}, {object}, {world})",
        atom.predicate
    ))
}

/// Render one [`RcTerm`] in Nemo surface syntax. The `existential` reifier var becomes
/// `!name` (invention); every other variable stays `?name`; an IRI is `<iri>`; a literal is
/// `"lex"`. A blank node has no Nemo surface form and is a hard error (the clausifier mints
/// Skolem constants, never blanks).
fn render_rc_term(term: &RcTerm, existential: &str) -> Result<String, String> {
    match term {
        RcTerm::Var(name) if name == existential => {
            Ok(format!("!{}", name.trim_start_matches('?')))
        }
        RcTerm::Var(name) => Ok(name.clone()),
        RcTerm::Iri(iri) => Ok(format!("<{iri}>")),
        RcTerm::Literal(lex) => {
            let escaped = lex.replace('\\', "\\\\").replace('"', "\\\"");
            Ok(format!("\"{escaped}\""))
        }
        RcTerm::Blank(label) => Err(format!(
            "n-ary head rule carries a blank node {label:?} — the clausifier mints Skolem \
             constants, never blanks (no-optionality)"
        )),
    }
}

/// A `?W`-style world variable not already used by `rc`, so the head's world slot is bound by
/// the body (Nemo safety). Only freshness matters (the world slot is not part of tuple
/// identity), so the exact name is immaterial.
fn fresh_world_var(rc: &RcRule) -> String {
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut note = |t: &RcTerm| {
        if let RcTerm::Var(name) = t {
            used.insert(name.clone());
        }
    };
    note(&rc.head.subject);
    note(&rc.head.object);
    for a in &rc.head_conjuncts {
        note(&a.subject);
        note(&a.object);
    }
    for a in &rc.body {
        note(&a.subject);
        note(&a.object);
    }
    let mut candidate = "?W".to_owned();
    let mut i = 0u32;
    while used.contains(&candidate) {
        i += 1;
        candidate = format!("?W{i}");
    }
    candidate
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

#[cfg(test)]
mod tests;
