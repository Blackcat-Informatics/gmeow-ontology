// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn acyclic_graph_certifies_complete_for_fragment() {
    // a -> b -> c, a -> c : a DAG (the shape of the build pipeline).
    let edges = [("a", "b"), ("b", "c"), ("a", "c")];
    let cert = certify_acyclic(edges.iter().copied());
    assert_eq!(cert, DagCertification::Certified);
    assert!(cert.is_certified());
    assert!(cert.witness().is_empty());
    assert_eq!(
        cert.result_status(),
        (
            EvaluationStatus::Completed,
            CompletenessStatus::CompleteForFragment
        )
    );
}

#[test]
fn multi_node_cycle_is_unsupported_and_names_the_offending_edge() {
    // b -> c -> b is a cycle; a -> b is acyclic context.
    let edges = [("a", "b"), ("b", "c"), ("c", "b")];
    let cert = certify_acyclic(edges.iter().copied());
    assert_eq!(cert, DagCertification::Cycle(vec!["b".into(), "c".into()]));
    // The witness names the offending cycle members — never silent truncation.
    assert_eq!(cert.witness(), vec!["b".to_string(), "c".to_string()]);
    // Cyclic under the DAG profile ⇒ the issue-mandated `unsupported` verdict.
    assert_eq!(
        cert.result_status(),
        (EvaluationStatus::Unsupported, CompletenessStatus::Unknown)
    );
}

#[test]
fn self_loop_is_the_minimal_cycle() {
    let edges = [("a", "b"), ("b", "b")];
    let cert = certify_acyclic(edges.iter().copied());
    assert_eq!(cert, DagCertification::SelfLoop("b".into()));
    assert_eq!(cert.witness(), vec!["b".to_string()]);
    assert_eq!(cert.result_status().0, EvaluationStatus::Unsupported);
}

#[test]
fn cycle_witness_is_deterministic_regardless_of_edge_order() {
    let forward = certify_acyclic([("c", "b"), ("b", "c"), ("a", "b")].iter().copied());
    let shuffled = certify_acyclic([("a", "b"), ("b", "c"), ("c", "b")].iter().copied());
    assert_eq!(forward, shuffled);
    assert_eq!(
        forward,
        DagCertification::Cycle(vec!["b".into(), "c".into()])
    );
}

#[test]
fn empty_graph_certifies() {
    let cert = certify_acyclic(std::iter::empty());
    assert_eq!(cert, DagCertification::Certified);
}

#[test]
fn certified_verdict_lowers_to_a_complete_for_fragment_reasoning_result() {
    let cert = certify_acyclic([("a", "b"), ("b", "c")].iter().copied());
    let result = cert.into_reasoning_result("contract:dag-test", "urn:world:test");
    // The typed result agrees with the RDF emitter's status mapping.
    assert_eq!(result.evaluation, EvaluationStatus::Completed);
    assert_eq!(result.completeness, CompletenessStatus::CompleteForFragment);
    assert_eq!(result.information, InformationState::Supported);
    // A certified plan carries an exact (loss-free) preservation claim and names
    // the fragment backing the complete-for-fragment claim.
    assert_eq!(result.preservation, PreservationClaim::exact());
    assert!(
        result
            .provenance
            .certified_fragment
            .as_deref()
            .is_some_and(|f| f.ends_with("DagWorkflowResource"))
    );
    assert!(result.validate().is_ok());
}

#[test]
fn cyclic_verdict_lowers_to_an_unsupported_reasoning_result_carrying_the_witness() {
    // A SYNTHETIC cyclic verdict (no cyclic pipeline needed): the DAG profile
    // refuses the loop, and the witness members are disclosed on the typed result.
    let cert = DagCertification::Cycle(vec!["stepProbe".into(), "stepRecover".into()]);
    let result = cert.into_reasoning_result("contract:dag-test", "urn:world:test");
    // The issue-mandated `unsupported` verdict, mirroring the RDF emitter.
    assert_eq!(result.evaluation, EvaluationStatus::Unsupported);
    assert_eq!(result.completeness, CompletenessStatus::Unknown);
    // The engine could not look — the unsupported-contract floor.
    assert_eq!(result.information, InformationState::NotEvaluated);
    // The offending cycle members are carried as the unsupported constructs the
    // DAG profile could not lower — never silently truncated.
    assert_eq!(
        result.preservation.unsupported_constructs,
        ["stepProbe".to_string(), "stepRecover".to_string()]
            .into_iter()
            .collect()
    );
    assert_eq!(
        result.preservation,
        PreservationClaim::unsupported_with(cert.witness())
    );
    assert!(result.provenance.certified_fragment.is_none());
    assert!(result.validate().is_ok());
}

#[test]
fn self_loop_verdict_also_lowers_to_unsupported_and_names_the_node() {
    let cert = DagCertification::SelfLoop("stepStuck".into());
    let result = cert.into_reasoning_result("contract:dag-test", "urn:world:test");
    assert_eq!(result.evaluation, EvaluationStatus::Unsupported);
    assert_eq!(
        result.preservation.unsupported_constructs,
        ["stepStuck".to_string()].into_iter().collect()
    );
    assert!(result.validate().is_ok());
}

/// The DAG-profile conformance case in executable form (mirrors the SHACL fixture
/// tests/conformance-fixtures/dag-strong-cyclic-plan.ttl): a strong-cyclic
/// plan with a retry loop is valid canonically, but the SAME plan under the
/// DAG-workflow profile resolves to `unsupported` with the offending back-edge
/// named (the recorded loss), while its acyclic projection — the plan with the
/// back-edge dropped — certifies as complete-for-fragment.
#[test]
fn strong_cyclic_plan_vs_its_acyclic_projection() {
    // The strong-cyclic plan's control flow: probe -> recover -> probe (the
    // retry loop / back-edge) and probe -> commit (success).
    let cyclic = [
        ("stepProbe", "stepRecover"),
        ("stepRecover", "stepProbe"), // the recovery back-edge
        ("stepProbe", "stepCommit"),
    ];
    let verdict = certify_acyclic(cyclic.iter().copied());
    // Under the DAG profile the loop is unsupported, and the loss is RECORDED:
    // the witness names exactly the cycle members (the dropped back-edge).
    assert_eq!(
        verdict,
        DagCertification::Cycle(vec!["stepProbe".into(), "stepRecover".into()])
    );
    assert_eq!(verdict.result_status().0, EvaluationStatus::Unsupported);
    assert_eq!(
        verdict.witness(),
        vec!["stepProbe".to_string(), "stepRecover".to_string()],
        "the recorded loss names the loop the DAG projection drops"
    );

    // The acyclic PROJECTION drops the back-edge; the rest certifies cleanly.
    let acyclic = [("stepProbe", "stepRecover"), ("stepProbe", "stepCommit")];
    let projected = certify_acyclic(acyclic.iter().copied());
    assert_eq!(projected, DagCertification::Certified);
    assert_eq!(
        projected.result_status(),
        (
            EvaluationStatus::Completed,
            CompletenessStatus::CompleteForFragment
        )
    );
}
