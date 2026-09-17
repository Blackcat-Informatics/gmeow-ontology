// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The expression-identity contracts grade the producer's actual reasoned substrate.
//! In particular the missing-operator control must witness a DL-invented filler while
//! the asserted grammar still prevents publication of a false structural identity.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use gmeow_errors::{Finding, Severity};

use super::super::expression_substrate::{
    CHANNEL, CLOSED_FORMS, MATH, OPERATOR_CONTROL, Observations, Profile, REFERENCE, Reasoned,
    Scene, TWINS, WRAPPER_CONTROL,
};

fn scene(path: &str, profile: Profile) -> &'static Scene {
    struct Selected {
        selector: String,
        observed: Observations,
    }
    static OBSERVED: OnceLock<Result<Selected, gmeow_errors::Diag>> = OnceLock::new();
    let selector = std::env::var(gmeow_action_cache::selection::MANIFEST_SHA256_ENV)
        .expect("expression substrate contracts require the exact producer selector");
    let selected = OBSERVED
        .get_or_init(|| {
            let root = gmeow_conformance::paths::repo_root();
            let bytes = gmeow_action_cache::selection::source_artifacts::load(
                &root,
                "stage-conformance",
                CHANNEL,
            )
            .map_err(gmeow_errors::Diag::from)?;
            let observed = serde_json::from_slice(&bytes).map_err(gmeow_errors::Diag::from)?;
            Ok(Selected {
                selector: selector.clone(),
                observed,
            })
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("authenticated expression substrate: {error}"));
    assert_eq!(
        selected.selector, selector,
        "expression observations cannot cross selector identities"
    );
    assert_eq!(
        selected.observed.context_path, MATH,
        "the native math TBox is a required part of this substrate"
    );
    assert_eq!(selected.observed.context_source_digest.len(), 64);
    assert_eq!(selected.observed.reasoning_projection_digest.len(), 64);
    assert_eq!(
        selected.observed.scenes.len(),
        5,
        "three shipped scenes and both authored-TBox controls"
    );
    let observed = selected
        .observed
        .scenes
        .get(path)
        .unwrap_or_else(|| panic!("required expression scene {path} absent"));
    assert_eq!(observed.source_path, path);
    assert_eq!(
        observed.profile, profile,
        "selected reasoned/multigraph scope"
    );
    assert_eq!(
        observed.source_digest.len(),
        64,
        "exact original scene digest"
    );
    observed
}

fn reasoned(path: &str, profile: Profile) -> &'static Reasoned {
    scene(path, profile)
        .reasoned
        .as_ref()
        .expect("selected scene requires native reasoned materialization")
}

fn reference_findings() -> &'static [Finding] {
    reasoned(REFERENCE, Profile::Reference)
        .expression_findings
        .as_deref()
        .expect("reference scene requires actual expression-identity findings")
}

fn the_shipped_reference_example_raises_no_expression_identity_finding_when_reasoned() {
    let findings: Vec<_> = reference_findings()
        .iter()
        .filter(|finding| finding.severity != Severity::Note)
        .collect();
    assert!(
        findings.is_empty(),
        "the shipped reference example must be clean through the reasoned substrate, got: {:?}",
        findings
            .iter()
            .map(|finding| &finding.message)
            .collect::<Vec<_>>()
    );
}

fn structurally_identical_expressions_under_different_iris_share_one_key() {
    let declarations = &scene(REFERENCE, Profile::Reference).declared_keys;
    let declared: Vec<_> = declarations.values().flatten().collect();
    assert!(
        declarations.len() >= 2 && declared.len() >= 2,
        "the example must declare a key on BOTH structurally identical expressions, saw {declarations:?}"
    );
    assert!(
        declared.windows(2).all(|pair| pair[0] == pair[1]),
        "structurally identical expressions under different IRIs must share one key, saw {declared:?}"
    );
    assert!(
        reference_findings()
            .iter()
            .all(|finding| finding.severity == Severity::Note),
        "the shared key must survive the reasoned substrate"
    );
}

fn conforming_alpha_equivalent_expressions_share_one_materialized_class_node() {
    let by_root = &reasoned(REFERENCE, Profile::Reference).alpha_classes;
    assert!(
        by_root.len() >= 2,
        "the gate must materialize an alpha-equivalence class for each lowered root, saw {by_root:?}"
    );
    let distinct: BTreeSet<_> = by_root.values().collect();
    assert_eq!(
        distinct.len(),
        1,
        "the example's two structurally identical expressions must resolve to ONE class node, saw {by_root:?}"
    );
}

fn the_gate_decides_a_non_empty_population_over_the_shipped_example_corpus() {
    let decided = reasoned(REFERENCE, Profile::Reference).alpha_classes.len();
    assert!(
        decided > 0,
        "the expression-identity gate must DECIDE at least one root over the shipped example corpus; a zero population makes every clean run vacuous and indistinguishable from a passing one"
    );
}

fn independently_authored_twins_over_shared_symbols_share_one_key_when_reasoned() {
    let classes = &reasoned(WRAPPER_CONTROL, Profile::Control).alpha_classes;
    assert_eq!(
        classes.len(),
        2,
        "both twins must be decided by the gate, saw {classes:?}"
    );
    let distinct: BTreeSet<_> = classes.values().collect();
    assert_eq!(
        distinct.len(),
        1,
        "independently authored twins over the same symbols must share ONE alpha-equivalence class; two classes means the digest is keyed on occurrence-wrapper IRIs and is a label, not a content key: {classes:?}"
    );
}

fn the_shipped_twin_example_resolves_both_authorings_to_one_class() {
    let classes = &reasoned(TWINS, Profile::Twins).alpha_classes;
    assert_eq!(
        classes.len(),
        2,
        "both shipped twins must be decided by the gate, saw {classes:?}"
    );
    let distinct: BTreeSet<_> = classes.values().collect();
    assert_eq!(
        distinct.len(),
        1,
        "the shipped twins must share ONE alpha-equivalence class: {classes:?}"
    );
}

fn a_root_the_grammar_refutes_gets_no_materialized_identity_edge() {
    let observed = reasoned(OPERATOR_CONTROL, Profile::Control);
    assert!(
        observed.operator_less_root_operator_edges > 0,
        "precondition: the DL chase must invent the omitted math:operator on the closure, else this test cannot observe the defect it exists to pin"
    );
    let published: Vec<_> = observed
        .alpha_classes
        .keys()
        .filter(|root| root.contains("refuted/noOperator"))
        .collect();
    assert!(
        published.is_empty(),
        "an expression the grammar refutes must carry NO materialized identity edge; the materializer published {published:?}, which means it lowered the CLOSURE (where the chase invented the missing operator) rather than the asserted graph"
    );
}

fn shipped_examples_are_clean_over_a_multi_graph_substrate() {
    for (path, profile) in [
        (TWINS, Profile::Twins),
        (REFERENCE, Profile::Reference),
        (CLOSED_FORMS, Profile::MultiGraph),
    ] {
        let findings = scene(path, profile)
            .multi_graph_findings
            .as_ref()
            .expect("selected scene requires the native multigraph probe");
        let errors: Vec<_> = findings
            .iter()
            .filter(|finding| finding.severity == Severity::Error)
            .map(|finding| format!("{} {}", finding.code, finding.message))
            .collect();
        assert!(
            errors.is_empty(),
            "{path} is a SHIPPED conforming example, but the expression-identity gate errors on it once its triples appear in more than one graph — the substrate gmeow validate --deep actually builds: {errors:?}"
        );
    }
}

#[test]
fn expression_substrate_contracts_share_one_authenticated_source_action() {
    let contracts: [(&str, fn()); 8] = [
        (
            "the_shipped_reference_example_raises_no_expression_identity_finding_when_reasoned",
            the_shipped_reference_example_raises_no_expression_identity_finding_when_reasoned,
        ),
        (
            "structurally_identical_expressions_under_different_iris_share_one_key",
            structurally_identical_expressions_under_different_iris_share_one_key,
        ),
        (
            "conforming_alpha_equivalent_expressions_share_one_materialized_class_node",
            conforming_alpha_equivalent_expressions_share_one_materialized_class_node,
        ),
        (
            "the_gate_decides_a_non_empty_population_over_the_shipped_example_corpus",
            the_gate_decides_a_non_empty_population_over_the_shipped_example_corpus,
        ),
        (
            "independently_authored_twins_over_shared_symbols_share_one_key_when_reasoned",
            independently_authored_twins_over_shared_symbols_share_one_key_when_reasoned,
        ),
        (
            "the_shipped_twin_example_resolves_both_authorings_to_one_class",
            the_shipped_twin_example_resolves_both_authorings_to_one_class,
        ),
        (
            "a_root_the_grammar_refutes_gets_no_materialized_identity_edge",
            a_root_the_grammar_refutes_gets_no_materialized_identity_edge,
        ),
        (
            "shipped_examples_are_clean_over_a_multi_graph_substrate",
            shipped_examples_are_clean_over_a_multi_graph_substrate,
        ),
    ];
    assert_eq!(
        contracts
            .iter()
            .map(|(name, _)| *name)
            .collect::<BTreeSet<_>>()
            .len(),
        8,
        "all original named expression contracts remain distinct"
    );
    let mut failures = Vec::new();
    for (name, contract) in contracts {
        if let Err(payload) = std::panic::catch_unwind(contract) {
            let detail = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| {
                    payload
                        .downcast_ref::<&str>()
                        .map(|text| (*text).to_owned())
                })
                .unwrap_or_else(|| "non-string assertion panic".to_owned());
            failures.push(format!("{name}: {detail}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of 8 expression substrate contracts failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
