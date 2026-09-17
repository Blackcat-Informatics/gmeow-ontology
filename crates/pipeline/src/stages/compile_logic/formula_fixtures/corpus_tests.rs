// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authenticated consumers; no authored source parsing or lowering occurs here.

use super::{CATS, CHANNEL, FormulaObservation, Observations, TYPED};
use gmeow_logic_compile::ir::{Formula, LOGIC_NAMESPACE as LOGIC, PreservationKind, Term};
use gmeow_logic_compile::relational_core::RcTerm;

fn observation(source: &str) -> &'static FormulaObservation {
    static OBSERVATIONS: std::sync::OnceLock<Observations> = std::sync::OnceLock::new();
    OBSERVATIONS
        .get_or_init(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let bytes =
                crate::fixture::authenticated_artifact(&root, "stage-compile-logic", CHANNEL)
                    .expect("authenticated native formula observations");
            serde_json::from_slice(&bytes).expect("decode formula observations")
        })
        .get(source)
        .expect("producer selected the exact source")
        .as_ref()
        .expect("source compiled")
}

/// The authored flagship reaches the real native adapter in the explicit producer.
/// Its authenticated observation must retain exact Horn shape and preservation.
#[test]
fn flagship_cats_chase_mice_lowers_to_evaluable_rules_with_exact_preservation() {
    let observation = observation(CATS);
    let diagnostics = &observation.diagnostics;
    assert!(
        !diagnostics.iter().any(|d| d.code == "MALFORMED_FORMULA"),
        "the flagship formula AST must reconstruct cleanly, got: {diagnostics:?}"
    );

    // The front-end lifted the sentence's denoted formula as a top-level assertion.
    assert_eq!(
        observation.formulas.len(),
        1,
        "exactly the one top-level flagship formula is lifted: {:?}",
        observation.formulas
    );

    // The native reasoner CONSUMES it: the full-FOL AST is clausified to an evaluable rule.
    assert_eq!(observation.selected_formulas, 1);
    let out = observation
        .lowering
        .as_ref()
        .expect("native formula adapter observation");
    assert_eq!(
        out.rules.len(),
        1,
        "the flagship formula lowers to exactly one evaluable Horn rule: {:?}",
        out.rules
    );

    // Evaluation shape: head is the binary chase relation, body is the two type memberships.
    let rule = &out.rules[0];
    assert_eq!(rule.head.len(), 1);
    assert!(
        rule.head[0].predicate.ends_with("typeChase"),
        "the derived head is the chase predication, got: {}",
        rule.head[0].predicate
    );
    assert_eq!(
        rule.body.len(),
        2,
        "two body atoms (the cat and mouse type memberships): {:?}",
        rule.body
    );
    assert!(
        rule.body
            .iter()
            .all(|a| a.predicate.ends_with("instanceOf")),
        "each body atom is a HiLog type membership: {:?}",
        rule.body
    );

    // Per-stage preservation: the `logic:` → relational-core evaluation is EXACT (no residue),
    // matching the fixture's asserted `logic:ExactPreservation` on the denotation composition.
    assert!(
        out.preservation.unsupported_constructs.is_empty(),
        "nothing is carried as residue: {:?}",
        out.preservation.unsupported_constructs
    );
    assert!(
        out.preservation
            .polarities
            .contains(&PreservationKind::Exact),
        "the flagship formula lowers with exact preservation: {:?}",
        out.preservation.polarities
    );
}

/// The producer selects the authored ternary atom and records the real adapter.
/// The read-only observation retains argument order and the shared tuple witness.
#[test]
fn typed_ir_fixture_ternary_formula_legalizes_through_the_real_adapter() {
    const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/logic/";
    let observation = observation(TYPED);
    let diagnostics = &observation.diagnostics;
    assert!(
        !diagnostics.iter().any(|d| d.code == "MALFORMED_FORMULA"),
        "every authored typed-IR formula must reconstruct cleanly: {diagnostics:?}"
    );

    let relation = Term::Iri(format!("{EX}between"));
    let between = observation
        .formulas
        .iter()
        .find(|formula| {
            matches!(
                formula,
                Formula::Atom {
                    relation: candidate,
                    ..
                } if candidate == &relation
            )
        })
        .expect("the source fixture carries its between formula")
        .clone();
    let Formula::Atom {
        relation: parsed_relation,
        args,
    } = &between
    else {
        unreachable!("selected by Formula::Atom relation")
    };
    assert_eq!(parsed_relation, &relation, "the relation IRI is preserved");
    assert_eq!(
        args,
        &vec![
            Term::Iri(format!("{EX}Alice")),
            Term::Iri(format!("{EX}Bob")),
            Term::Iri(format!("{EX}Carol")),
        ],
        "termIndex reconstructs the exact Alice/Bob/Carol argument order"
    );

    assert_eq!(observation.selected_formulas, 1);
    let out = observation
        .lowering
        .as_ref()
        .expect("native formula adapter observation");
    assert!(
        out.rules.is_empty(),
        "a ternary derivation belongs to the conjunctive-head chase lane"
    );
    assert_eq!(
        out.existential_rules.len(),
        1,
        "one source ternary atom yields one existential tuple rule"
    );
    assert!(
        out.preservation.unsupported_constructs.is_empty(),
        "the fixed-arity ternary formula is legal, not residue: {:?}",
        out.preservation.unsupported_constructs
    );
    assert!(
        out.preservation
            .polarities
            .contains(&PreservationKind::Exact),
        "fixed-arity reification is exact"
    );

    let lowered = &out.existential_rules[0];
    assert!(lowered.body.is_empty(), "a ground assertion has no body");
    assert_eq!(
        lowered.head.len(),
        4,
        "the tuple has one relation-typing edge plus three positional edges"
    );
    let reifier = lowered.head[0].subject.clone();
    assert!(
        matches!(reifier, RcTerm::Var(ref name) if name.starts_with("?naryH")),
        "the shared tuple node is the content-addressed existential reifier: {reifier:?}"
    );
    assert!(
        lowered.head.iter().all(|atom| atom.subject == reifier),
        "all four edges must share one tuple reifier: {:?}",
        lowered.head
    );
    assert_eq!(lowered.head[0].predicate, format!("{LOGIC}instanceOf"));
    assert_eq!(lowered.head[0].object, RcTerm::Iri(format!("{EX}between")));
    for (index, expected) in ["Alice", "Bob", "Carol"].iter().enumerate() {
        let atom = &lowered.head[index + 1];
        assert_eq!(atom.predicate, format!("{LOGIC}naryArg{index}"));
        assert_eq!(atom.object, RcTerm::Iri(format!("{EX}{expected}")));
    }
}
