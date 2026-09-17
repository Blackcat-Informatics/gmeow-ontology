// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::ir::{ContextualScope, Formula, LogicAxiom, LogicRule, Term};

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

fn iri(local: &str) -> String {
    format!("{LOGIC}{local}")
}

/// A program exercising an axiom, a rule (negated body atom + distinct pair), and a
/// full-FOL formula (quantifier + disjunction + strong negation + sequence marker) —
/// the union of constructs the three dialects each prove Exact in isolation.
fn fixture(subject: &str) -> LogicProgram {
    let axiom = LogicAxiom::ground(
        iri(subject),
        iri("knows"),
        crate::ir::AtomicTerm::resource(iri("b")),
    )
    .expect("axiom");

    let rule = LogicRule::new(
        LogicAxiom::new(
            "?x",
            iri("ancestor"),
            crate::ir::AtomicTerm::resource("?z"),
            false,
            ContextualScope::default(),
        )
        .expect("head"),
        vec![
            LogicAxiom::new(
                "?x",
                iri("parent"),
                crate::ir::AtomicTerm::resource("?y"),
                false,
                ContextualScope::default(),
            )
            .expect("b1"),
            LogicAxiom::new(
                "?y",
                iri("parent"),
                crate::ir::AtomicTerm::resource("?z"),
                true,
                ContextualScope::default(),
            )
            .expect("b2"),
        ],
        vec![("?x".to_owned(), "?z".to_owned())],
        ContextualScope::default(),
    );

    let inner_or = Formula::Or(vec![
        Formula::atom(
            Term::iri(iri("mortal")).unwrap(),
            vec![
                Term::var("p").unwrap(),
                Term::sequence_marker("rest").unwrap(),
            ],
        )
        .unwrap(),
        Formula::Not(Box::new(
            Formula::atom(
                Term::iri(iri("mortal")).unwrap(),
                vec![Term::var("p").unwrap()],
            )
            .unwrap(),
        )),
    ]);
    let formula = Formula::Forall {
        vars: vec!["p".to_owned()],
        body: Box::new(inner_or),
    };

    LogicProgram::new(
        vec![axiom],
        vec![rule],
        Vec::new(),
        Some("urn:test:cl".to_owned()),
    )
    .with_formulas(vec![formula])
}

#[test]
fn all_dialects_round_trip_and_agree() {
    assert_all_dialects_isomorphic(&fixture("socrates")).expect("all dialects isomorphic");
}

#[test]
fn cross_edge_flags_divergent_programs() {
    // Two programs that differ in one axiom subject must not pass a cross edge.
    let err = cross_edge("clif", &fixture("socrates"), "cgif", &fixture("plato"))
        .expect_err("divergent programs must fail the cross edge");
    assert!(
        err.message().contains("cross-dialect clif != cgif"),
        "{err}"
    );
}

#[test]
fn dialect_fixpoint_flags_non_idempotence() {
    // A dialect whose second round-trip returns a DIFFERENT program is caught as
    // non-idempotent. The parse closure returns socrates on the first leg and plato on
    // the second, so `assert_ir_isomorphic(fp1, fp2)` must fail.
    let calls = std::cell::Cell::new(0u8);
    let err = dialect_fixpoint(
        "clif",
        &fixture("socrates"),
        |_p| Ok(String::from("ignored-projection")),
        |_text| {
            let n = calls.get();
            calls.set(n + 1);
            Ok((
                if n == 0 {
                    fixture("socrates")
                } else {
                    fixture("plato")
                },
                Vec::new(),
            ))
        },
    )
    .expect_err("non-idempotent fixpoint must fail");
    assert!(
        err.message().contains("not idempotent at its fixpoint"),
        "{err}"
    );
}

#[test]
fn parse_checked_flags_error_diagnostic() {
    // A Severity::Error diagnostic from the re-parse is a hard failure (a lossy
    // round-trip must never be silently tolerated).
    let err = parse_checked("cgif", "ignored", |_text| {
        Ok((
            fixture("socrates"),
            vec![Diagnostic {
                severity: Severity::Error,
                code: "CL_TEST".to_owned(),
                message: "seeded error".to_owned(),
                subject: None,
            }],
        ))
    })
    .expect_err("error diagnostic must fail");
    assert!(err.message().contains("Severity::Error"), "{err}");
}
