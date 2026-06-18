// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Lowering the **one canonical AST** ([`crate::compile::ir`]) directly into the
//! evaluable rule IR ([`crate::rule_ir::EvalRule`]) — the AST-unification keystone
//! of issue #664.
//!
//! Before this, the evaluable IR could only be obtained by *re-parsing* the
//! `.rls` text the compiler had just produced
//! (`compile IR → project_nemo → .rls → Nemo parse → EvalRule`).  That round-trip
//! is the "the IR exists twice" duplication the issue targets.  [`lower_eval_rules`]
//! derives the evaluable rules **straight from the canonical source AST**, so the
//! canonical IR is the single definitional source and the runtime forms are mere
//! views of it.
//!
//! The lowering is proven equivalent to the `.rls`-reparse path by the parity test
//! in this module — the two paths produce identical [`EvalRule`]s for every
//! guard-free program (and the canonical lowering is strictly *more* faithful: it
//! preserves the `distinct_pairs` inequality guards that the Nemo-reparse path
//! currently drops).  The PyO3 boundary still accepts `.rls` text from Python; the
//! reparse there is retained only as a boundary adapter, proven to agree with this
//! authoritative lowering.
//!
//! Phase note: the lowering entry points are exercised by this module's parity
//! tests now; the non-test consumer is the PyO3 routing landed in the #664 PyO3
//! task (Task 6).  Until then the functions allow `dead_code` crate-internally
//! (the same phased-development posture as [`crate::rule_ir`]).
#![allow(dead_code)]

use oxigraph::model::{Literal, NamedNode, Term};

use super::ir::{LogicAxiom, LogicProgram, LogicRule, LOGIC_NAMESPACE};
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm};

/// Lower one canonical source atom to an [`EvalAtom`] (arity-3 world slot is not
/// part of the source IR, so nothing is dropped — subject/predicate/object map
/// 1:1).  `?`-prefixed terms become variables, a literal object becomes a plain
/// `xsd:string` `ConstLit` (matching `project_nemo`'s `"value"` encoding and the
/// reparse), every other term an IRI constant.
fn lower_atom(atom: &LogicAxiom, negated: bool) -> Result<EvalAtom, String> {
    let predicate = NamedNode::new(&atom.predicate)
        .map_err(|e| format!("invalid predicate IRI {:?}: {e}", atom.predicate))?;
    let subject = lower_term(&atom.subject, false, "subject")?;
    let object = lower_term(&atom.obj, atom.obj_is_literal, "object")?;
    Ok(EvalAtom {
        subject,
        predicate,
        object,
        negated,
    })
}

fn lower_term(value: &str, is_literal: bool, slot: &str) -> Result<EvalTerm, String> {
    if value.starts_with('?') {
        return Ok(EvalTerm::Var(value.to_owned()));
    }
    if is_literal {
        if slot != "object" {
            return Err(format!(
                "compile::lower: literal in {slot} position {value:?} — only an object may be a \
                 literal"
            ));
        }
        return Ok(EvalTerm::ConstLit(Term::Literal(
            Literal::new_simple_literal(value),
        )));
    }
    let nn = NamedNode::new(value).map_err(|e| format!("invalid {slot} IRI {value:?}: {e}"))?;
    Ok(EvalTerm::ConstNamed(nn))
}

/// Lower a single canonical [`LogicRule`] to an [`EvalRule`], with the same body
/// ordering the reparse yields (positive atoms first, then negated), the same
/// `rule_iri` (`scope.provenance` or the synthesized anonymous IRI), and — unlike
/// the reparse — the rule's `distinct_pairs` preserved.
pub(crate) fn lower_rule(rule: &LogicRule) -> Result<EvalRule, String> {
    let head = lower_atom(&rule.head, false)?;
    let mut body: Vec<EvalAtom> = Vec::new();
    for atom in rule.body.iter().filter(|a| !a.negated) {
        body.push(lower_atom(atom, false)?);
    }
    for atom in rule.body.iter().filter(|a| a.negated) {
        body.push(lower_atom(atom, true)?);
    }
    let rule_iri = rule
        .scope
        .provenance
        .clone()
        .unwrap_or_else(|| format!("{LOGIC_NAMESPACE}rule/anonymous"));
    Ok(EvalRule {
        head,
        body,
        rule_iri,
        distinct_pairs: rule.distinct_pairs.clone(),
    })
}

/// Lower every rule in a canonical [`LogicProgram`] to the evaluable IR — the
/// canonical-AST-authoritative alternative to `rule_ir::parse_eval_rules`.
pub(crate) fn lower_eval_rules(program: &LogicProgram) -> Result<Vec<EvalRule>, String> {
    program.rules.iter().map(lower_rule).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::frontend::parse_logic_str;
    use crate::compile::projections::text::project_nemo;
    use crate::rule_ir::parse_eval_rules;

    fn prog(ttl: &str) -> LogicProgram {
        let prefixes = "\
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
";
        parse_logic_str(&format!("{prefixes}{ttl}"), None)
            .expect("parse ok")
            .0
    }

    /// The unification proof: lowering the canonical AST yields the *same*
    /// evaluable rules as re-parsing the projected `.rls` (for a guard-free rule).
    #[test]
    fn canonical_lowering_equals_rls_reparse() {
        let program = prog(
            "ex:r a logic:Rule ;
                logic:head [ rdf:subject \"?x\" ; rdf:predicate logic:ancestor ; rdf:object \"?z\" ] ;
                logic:body [ rdf:subject \"?x\" ; rdf:predicate logic:parent ; rdf:object \"?y\" ] ;
                logic:body [ rdf:subject \"?y\" ; rdf:predicate logic:ancestor ; rdf:object \"?z\" ] .",
        );
        let rls = project_nemo(&program).unwrap().content;
        let from_rls = parse_eval_rules(&rls).expect("reparse");
        let from_canonical = lower_eval_rules(&program).expect("lower");
        assert_eq!(
            from_canonical, from_rls,
            "canonical lowering must equal the .rls reparse"
        );
    }

    #[test]
    fn lowering_preserves_distinct_guards() {
        // The reparse path drops inequality guards; the canonical lowering keeps them.
        let program = prog(
            "ex:r a logic:Rule ;
                logic:head [ rdf:subject \"?x\" ; rdf:predicate logic:rel ; rdf:object \"?y\" ] ;
                logic:body [ rdf:subject \"?x\" ; rdf:predicate logic:rel ; rdf:object \"?y\" ] ;
                logic:distinctBody [ rdf:subject \"?x\" ; rdf:object \"?y\" ] .",
        );
        let lowered = lower_eval_rules(&program).unwrap();
        assert_eq!(lowered.len(), 1);
        assert_eq!(
            lowered[0].distinct_pairs,
            vec![("?x".to_owned(), "?y".to_owned())]
        );
    }

    /// The certifier has a canonical-AST front door that agrees with certifying
    /// the program's own projected `.rls` rules section.
    #[test]
    fn certify_program_matches_projected_rls() {
        let program = prog(
            "ex:r a logic:Rule ;
                logic:head [ rdf:subject \"?x\" ; rdf:predicate logic:ancestor ; rdf:object \"?z\" ] ;
                logic:body [ rdf:subject \"?x\" ; rdf:predicate logic:parent ; rdf:object \"?y\" ] ;
                logic:body [ rdf:subject \"?y\" ; rdf:predicate logic:ancestor ; rdf:object \"?z\" ] .",
        );
        let via_canonical =
            crate::certify::certify_program(&program, "PositiveHornProfile").unwrap();
        let rls = project_nemo(&program).unwrap().content;
        let section = crate::compile::projections::text::extract_nemo_rules_section(&rls).unwrap();
        let via_rls = crate::certify::certify(&section, "PositiveHornProfile").unwrap();
        assert_eq!(via_canonical.profile_id, "PositiveHornProfile");
        assert_eq!(via_canonical.certified, via_rls.certified);
        assert_eq!(via_canonical.violations, via_rls.violations);
    }
}
