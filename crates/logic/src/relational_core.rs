// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The relational-core lowering waist (`logic:RelationalCore`).
//!
//! The first-class Datalog±-with-stratified-negation sub-language between the full-FOL
//! formula core ([`gmeow_logic_compile::ir::Formula`]) and the physical engine. A formula
//! is lowered by negation-normal-form rewriting, existential Skolemization, and clause
//! extraction; the Horn-expressible fragment becomes [`EvalRule`]s the engine runs, and
//! every formula outside that fragment is **partial-converted** — its legal clauses lower
//! and the rest is carried as flagged unsupported residue, never silently narrowed (the
//! legalization rule of `design/LOGIC-IR.md`).
//!
//! The honest [`PreservationClaim`] is `{exact}` only when the whole formula set lowered,
//! else `{sound-under}` naming the residue.  The Horn+NAF sub-fragment authored as
//! [`gmeow_logic_compile::ir::LogicRule`]s is unaffected; this lowers only the richer
//! [`gmeow_logic_compile::ir::LogicProgram::formulas`].
//!
//! Floor of the supported fragment: a formula whose negation-normal form is a conjunction
//! of Horn clauses of **binary** atoms (`∀x̄. A ← B₁ ∧ … ∧ Bₙ`, optionally with a leading
//! existential prefix Skolemized to constants).  Beyond it — a disjunctive head, a
//! quantifier alternation (`∃` under `∀`, which would need a Skolem *function* the
//! relational term algebra cannot hold), a non-binary or sequence-marker atom, or a form
//! that would require full CNF distribution — is flagged unsupported, not mis-lowered.
//!
//! The lowering is live: [`crate::reason::reason_program`] consumes [`lower_formulas`] to
//! evaluate the full-FOL formula layer through the chase, rendering the Horn-expressible
//! fragment to Nemo `.rls` and disclosing the residue in the result's preservation claim.

use std::collections::BTreeSet;

use oxigraph::model::{Literal, NamedNode, Term as OxTerm};

use gmeow_logic_compile::ir::{Formula, LogicProgram, Term, LOGIC_NAMESPACE};

use crate::encode::sha1_hex;
use crate::result::PreservationClaim;
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm};

/// The outcome of lowering a program's full-FOL formulas to the relational core.
#[derive(Debug, Clone)]
pub(crate) struct RelationalCoreLowering {
    /// The relational-core rules the Horn-expressible formula fragment produced, in the
    /// order their source formulas appear in the (canonically sorted) program.
    pub(crate) rules: Vec<EvalRule>,
    /// An honest preservation claim: `{exact}` when every formula lowered, else
    /// `{sound-under}` carrying a description of each unsupported formula.
    pub(crate) preservation: PreservationClaim,
}

/// Lower every full-FOL formula in `program` to the relational core, partial-converting
/// the non-Horn fragment to flagged unsupported residue.
pub(crate) fn lower_formulas(program: &LogicProgram) -> RelationalCoreLowering {
    let mut rules: Vec<EvalRule> = Vec::new();
    let mut residue: BTreeSet<String> = BTreeSet::new();

    for formula in &program.formulas {
        let normalized = skolemize(nnf(formula));
        lower_top(&normalized, formula, &mut rules, &mut residue);
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

/// Lower a top-level (already NNF + Skolemized) formula: peel the universal closure,
/// flatten a top-level conjunction into independent clauses, and lower each clause —
/// pushing an [`EvalRule`] when it is a Horn clause of binary atoms, else recording an
/// honest residue entry keyed on the ORIGINAL formula (so the disclosure is stable).
fn lower_top(
    normalized: &Formula,
    source: &Formula,
    rules: &mut Vec<EvalRule>,
    residue: &mut BTreeSet<String>,
) {
    match normalized {
        // A universal binder merely closes the rule's variables.
        Formula::Forall { body, .. } => lower_top(body, source, rules, residue),
        // A conjunction at the top is independent clauses / assertions.
        Formula::And(fs) => {
            for f in fs {
                lower_top(f, source, rules, residue);
            }
        }
        // A clause (disjunction of literals) or a bare atom.
        clause => match lower_clause(clause) {
            Ok(rule) => rules.push(rule),
            Err(reason) => {
                residue.insert(format!(
                    "{reason} [{}]",
                    &sha1_hex(&source.content_key())[..12]
                ));
            }
        },
    }
}

/// Lower a single clause to a Horn [`EvalRule`].  A clause is a bare atom, a strong
/// negation of an atom, or a disjunction of those; Horn requires exactly one positive
/// literal (the head), the rest negative (clause `A ∨ ¬B ∨ ¬C` ≡ rule `A ← B ∧ C`).
fn lower_clause(clause: &Formula) -> Result<EvalRule, &'static str> {
    let literals: Vec<&Formula> = match clause {
        Formula::Or(fs) => fs.iter().collect(),
        other => vec![other],
    };

    let mut head: Option<&Formula> = None;
    let mut body_atoms: Vec<&Formula> = Vec::new();
    for lit in literals {
        match lit {
            Formula::Atom { .. } => {
                if head.is_some() {
                    // Two positive literals → not Horn (a disjunctive head).
                    return Err("disjunctive head: clause is not Horn (>1 positive literal)");
                }
                head = Some(lit);
            }
            // `¬B` in the clause becomes a positive body atom `B` in the rule.
            Formula::Not(inner) if matches!(**inner, Formula::Atom { .. }) => {
                body_atoms.push(inner);
            }
            _ => return Err("non-relational-core formula (not a Horn clause of binary atoms)"),
        }
    }

    let head = head.ok_or("headless clause (no positive literal; an integrity constraint)")?;
    let head_eval = atom_to_eval(head)?;
    let body: Result<Vec<EvalAtom>, &'static str> =
        body_atoms.iter().map(|a| atom_to_eval(a)).collect();
    let body = body?;

    let rule_iri = format!(
        "{LOGIC_NAMESPACE}formula-rule/{}",
        sha1_hex(&clause.content_key())
    );
    Ok(EvalRule {
        head: head_eval,
        body,
        rule_iri,
        distinct_pairs: Vec::new(),
    })
}

/// Convert a binary [`Formula::Atom`] to an [`EvalAtom`] (the relational core is binary:
/// subject / predicate / object).
fn atom_to_eval(atom: &Formula) -> Result<EvalAtom, &'static str> {
    let Formula::Atom { relation, args } = atom else {
        return Err("clause literal is not an atom");
    };
    if args.len() != 2 {
        return Err("non-binary atom (the relational core is binary; arity ≠ 2)");
    }
    let Term::Iri(pred) = relation else {
        return Err("non-IRI relation in atom");
    };
    let predicate = NamedNode::new(pred).map_err(|_| "invalid predicate IRI")?;
    let subject = term_to_eval(&args[0], false)?;
    let object = term_to_eval(&args[1], true)?;
    Ok(EvalAtom {
        subject,
        predicate,
        object,
        negated: false,
    })
}

/// Convert a [`Term`] to an [`EvalTerm`].  `is_object` gates a literal to the object slot.
fn term_to_eval(term: &Term, is_object: bool) -> Result<EvalTerm, &'static str> {
    match term {
        // EvalTerm::Var carries the `?` sigil (the surface convention Term drops).
        Term::Var(name) => Ok(EvalTerm::Var(format!("?{name}"))),
        Term::Iri(iri) => NamedNode::new(iri)
            .map(EvalTerm::ConstNamed)
            .map_err(|_| "invalid IRI term"),
        Term::Literal { lexical, datatype } => {
            if !is_object {
                return Err("literal in subject position (only an object may be a literal)");
            }
            let lit = match datatype {
                Some(dt) => Literal::new_typed_literal(
                    lexical,
                    NamedNode::new(dt).map_err(|_| "invalid literal datatype IRI")?,
                ),
                None => Literal::new_simple_literal(lexical),
            };
            Ok(EvalTerm::ConstLit(OxTerm::Literal(lit)))
        }
        Term::SequenceMarker(_) => {
            Err("sequence marker (variadic) is not representable in the relational core")
        }
    }
}

// --------------------------------------------------------------------------- //
// Negation-normal form
// --------------------------------------------------------------------------- //

/// Rewrite a formula into negation-normal form: eliminate `→` and `↔`, then push every
/// negation inward (De Morgan + quantifier duality) until it sits only on atoms.
fn nnf(formula: &Formula) -> Formula {
    nnf_inner(formula, false)
}

/// `neg` = an odd number of negations encloses this node; carry it inward.
fn nnf_inner(f: &Formula, neg: bool) -> Formula {
    match f {
        Formula::Atom { .. } => {
            if neg {
                Formula::Not(Box::new(f.clone()))
            } else {
                f.clone()
            }
        }
        Formula::Not(inner) => nnf_inner(inner, !neg),
        Formula::And(fs) => {
            let parts: Vec<Formula> = fs.iter().map(|x| nnf_inner(x, neg)).collect();
            if neg {
                Formula::Or(parts) // ¬(φ ∧ ψ) ≡ ¬φ ∨ ¬ψ
            } else {
                Formula::And(parts)
            }
        }
        Formula::Or(fs) => {
            let parts: Vec<Formula> = fs.iter().map(|x| nnf_inner(x, neg)).collect();
            if neg {
                Formula::And(parts) // ¬(φ ∨ ψ) ≡ ¬φ ∧ ¬ψ
            } else {
                Formula::Or(parts)
            }
        }
        Formula::Implies(a, b) => {
            // φ → ψ ≡ ¬φ ∨ ψ
            let rewritten = Formula::Or(vec![Formula::Not(a.clone()), (**b).clone()]);
            nnf_inner(&rewritten, neg)
        }
        Formula::Iff(a, b) => {
            // φ ↔ ψ ≡ (φ → ψ) ∧ (ψ → φ)
            let rewritten = Formula::And(vec![
                Formula::Implies(a.clone(), b.clone()),
                Formula::Implies(b.clone(), a.clone()),
            ]);
            nnf_inner(&rewritten, neg)
        }
        Formula::Forall { vars, body } => {
            let inner = Box::new(nnf_inner(body, neg));
            if neg {
                Formula::Exists {
                    vars: vars.clone(),
                    body: inner, // ¬∀x.φ ≡ ∃x.¬φ
                }
            } else {
                Formula::Forall {
                    vars: vars.clone(),
                    body: inner,
                }
            }
        }
        Formula::Exists { vars, body } => {
            let inner = Box::new(nnf_inner(body, neg));
            if neg {
                Formula::Forall {
                    vars: vars.clone(),
                    body: inner, // ¬∃x.φ ≡ ∀x.¬φ
                }
            } else {
                Formula::Exists {
                    vars: vars.clone(),
                    body: inner,
                }
            }
        }
    }
}

// --------------------------------------------------------------------------- //
// Existential Skolemization (constants only)
// --------------------------------------------------------------------------- //

/// Skolemize a leading existential prefix over a quantifier-free matrix, replacing each
/// `∃`-bound variable with a fresh Skolem-constant IRI derived deterministically from the
/// formula's alpha-normalized content key (so two alpha-equivalent formulas, however
/// constructed, get identical witnesses).  A formula with no leading `∃`, or whose matrix
/// still holds a quantifier (`∃` under `∀` ⇒ a Skolem *function*; or an inner binder ⇒ a
/// capture hazard), is returned unchanged — the lowering then flags the surviving `∃`.
fn skolemize(formula: Formula) -> Formula {
    if !matches!(formula, Formula::Exists { .. }) {
        return formula;
    }
    let seed = sha1_hex(&formula.content_key());
    let mut names: Vec<String> = Vec::new();
    let matrix = peel_exists(formula.clone(), &mut names);
    if has_quantifier(&matrix) {
        return formula;
    }
    let subs: Vec<(String, String)> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), format!("{LOGIC_NAMESPACE}skolem/{seed}-{i}")))
        .collect();
    subst_formula(matrix, &subs)
}

/// Peel a leading existential prefix, collecting its bound-variable names in order.
fn peel_exists(f: Formula, names: &mut Vec<String>) -> Formula {
    match f {
        Formula::Exists { vars, body } => {
            names.extend(vars);
            peel_exists(*body, names)
        }
        other => other,
    }
}

/// `true` if any quantifier appears anywhere in `f`.
fn has_quantifier(f: &Formula) -> bool {
    match f {
        Formula::Forall { .. } | Formula::Exists { .. } => true,
        Formula::Atom { .. } => false,
        Formula::Not(b) => has_quantifier(b),
        Formula::Implies(a, b) | Formula::Iff(a, b) => has_quantifier(a) || has_quantifier(b),
        Formula::And(fs) | Formula::Or(fs) => fs.iter().any(has_quantifier),
    }
}

/// Substitute each `(var → IRI)` binding into every atom term of a quantifier-free matrix.
fn subst_formula(f: Formula, subs: &[(String, String)]) -> Formula {
    match f {
        Formula::Atom { relation, args } => Formula::Atom {
            relation,
            args: args.into_iter().map(|t| subst_term(t, subs)).collect(),
        },
        Formula::Not(b) => Formula::Not(Box::new(subst_formula(*b, subs))),
        Formula::And(fs) => Formula::And(fs.into_iter().map(|x| subst_formula(x, subs)).collect()),
        Formula::Or(fs) => Formula::Or(fs.into_iter().map(|x| subst_formula(x, subs)).collect()),
        Formula::Implies(a, b) => Formula::Implies(
            Box::new(subst_formula(*a, subs)),
            Box::new(subst_formula(*b, subs)),
        ),
        Formula::Iff(a, b) => Formula::Iff(
            Box::new(subst_formula(*a, subs)),
            Box::new(subst_formula(*b, subs)),
        ),
        // Unreachable for a quantifier-free matrix, but total for safety.
        other => other,
    }
}

/// Replace a variable term with its Skolem IRI if bound by `subs`; else leave it.
///
/// `peel_exists` appends nested binders outer→inner, so a shadowed name
/// (`∃x ∃x . p(x)`) appears more than once in `subs`. The matrix occurrence is
/// bound by the *innermost* enclosing quantifier, so the search runs in reverse
/// — innermost binding wins — matching `resolve_binding`'s de-Bruijn semantics.
fn subst_term(t: Term, subs: &[(String, String)]) -> Term {
    if let Term::Var(name) = &t {
        for (var, iri) in subs.iter().rev() {
            if var == name {
                return Term::Iri(iri.clone());
            }
        }
    }
    t
}

#[cfg(test)]
mod tests;
