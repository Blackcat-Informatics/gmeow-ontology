// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The IR-isomorphism gate.
//!
//! [`assert_ir_isomorphic`] asserts that two [`LogicProgram`]s normalize to the same
//! canonical IR, raising [`IRIsomorphismError`] with a directional diff on mismatch. It
//! is the round-trip gate behind the CL/CLIF/CGIF/XCL projections
//! (`project_* → parse_*_str → assert_ir_isomorphic`) and the pipeline's compile-logic
//! re-derivation crosscheck.

use std::collections::HashSet;

use crate::ir::{ContentKey, Formula, LogicAxiom, LogicProgram, LogicRule, ReasoningContract};

// --------------------------------------------------------------------------- //
// IR isomorphism gate
// --------------------------------------------------------------------------- //

const SEP: char = '\u{0}';

/// Raised by [`assert_ir_isomorphic`] when two programs differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IRIsomorphismError(pub String);

impl std::fmt::Display for IRIsomorphismError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for IRIsomorphismError {}

fn py_bool(b: bool) -> &'static str {
    if b { "True" } else { "False" }
}

/// Stable diff key for an axiom (mirrors Python `_axiom_key`: subject, predicate,
/// obj, obj_is_literal — scope and negation are intentionally excluded).
fn axiom_key(a: &LogicAxiom) -> String {
    format!(
        "{}{SEP}{}{SEP}{}{SEP}{}",
        a.subject,
        a.predicate,
        a.obj,
        py_bool(a.obj_is_literal)
    )
}

/// Stable diff key for a rule (mirrors Python `_rule_key`).
fn rule_key(r: &LogicRule) -> String {
    let head = &r.head;
    let head_key = format!("{}{SEP}{}{SEP}{}", head.subject, head.predicate, head.obj);
    let mut body: Vec<String> = r
        .body
        .iter()
        .map(|b| format!("{}{SEP}{}{SEP}{}", b.subject, b.predicate, b.obj))
        .collect();
    body.sort();
    let mut base = format!("{head_key}{SEP}{}", body.join("|"));
    if !r.distinct_pairs.is_empty() {
        let distinct = r
            .distinct_pairs
            .iter()
            .map(|(a, b)| format!("{a}{SEP}{b}"))
            .collect::<Vec<_>>()
            .join("|");
        base.push(SEP);
        base.push_str(&distinct);
    }
    base
}

/// Stable diff key for a reasoning contract (previously `profile_key`).
fn contract_key(c: &ReasoningContract) -> String {
    c.sort_key()
}

/// Assert that two [`LogicProgram`]s are canonically equal, raising
/// [`IRIsomorphismError`] with a directional diff on mismatch (mirrors the Python
/// `assert_ir_isomorphic`).
pub fn assert_ir_isomorphic(
    prog_a: &LogicProgram,
    prog_b: &LogicProgram,
) -> Result<(), IRIsomorphismError> {
    if prog_a == prog_b {
        return Ok(());
    }

    let axioms_a: HashSet<String> = prog_a.axioms.iter().map(axiom_key).collect();
    let axioms_b: HashSet<String> = prog_b.axioms.iter().map(axiom_key).collect();
    let rules_a: HashSet<String> = prog_a.rules.iter().map(rule_key).collect();
    let rules_b: HashSet<String> = prog_b.rules.iter().map(rule_key).collect();
    let contracts_a: HashSet<String> = prog_a.contracts.iter().map(contract_key).collect();
    let contracts_b: HashSet<String> = prog_b.contracts.iter().map(contract_key).collect();
    let formulas_a: HashSet<ContentKey> =
        prog_a.formulas.iter().map(Formula::content_key).collect();
    let formulas_b: HashSet<ContentKey> =
        prog_b.formulas.iter().map(Formula::content_key).collect();

    // Generic over the key type so the formula leg keeps its `ContentKey` newtype rather
    // than being flattened to a bare `String` for the sake of one shared helper.
    fn diff<T: std::hash::Hash + Eq + Ord + Clone>(from: &HashSet<T>, to: &HashSet<T>) -> Vec<T> {
        let mut v: Vec<T> = from.difference(to).cloned().collect();
        v.sort();
        v
    }

    let mut lines: Vec<String> = Vec::new();
    for item in diff(&axioms_a, &axioms_b) {
        lines.push(format!("A has, B lacks (axiom):  {item}"));
    }
    for item in diff(&axioms_b, &axioms_a) {
        lines.push(format!("B has, A lacks (axiom):  {item}"));
    }
    for item in diff(&rules_a, &rules_b) {
        lines.push(format!("A has, B lacks (rule):   {item}"));
    }
    for item in diff(&rules_b, &rules_a) {
        lines.push(format!("B has, A lacks (rule):   {item}"));
    }
    for item in diff(&contracts_a, &contracts_b) {
        lines.push(format!("A has, B lacks (contract): {item}"));
    }
    for item in diff(&contracts_b, &contracts_a) {
        lines.push(format!("B has, A lacks (contract): {item}"));
    }
    for item in diff(&formulas_a, &formulas_b) {
        lines.push(format!("A has, B lacks (formula): {item}"));
    }
    for item in diff(&formulas_b, &formulas_a) {
        lines.push(format!("B has, A lacks (formula): {item}"));
    }

    if lines.is_empty() {
        if prog_a.source_iri != prog_b.source_iri {
            lines.push(format!(
                "source_iri differs: A={:?}  B={:?}",
                prog_a.source_iri, prog_b.source_iri
            ));
        } else {
            lines.push("programs differ (canonical mismatch — check nested scope)".to_owned());
        }
    }

    Err(IRIsomorphismError(format!(
        "IR isomorphism gate FAILED — programs do not normalize identically:\n  {}",
        lines.join("\n  ")
    )))
}

#[cfg(test)]
mod tests;
