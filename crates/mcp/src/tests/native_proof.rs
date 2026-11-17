// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Caller-owned native proof publication without an authenticated corpus.

use super::*;

/// The shared producer/runtime explanation exit preserves genuine native proof
/// premises and refuses an absent target without starting another execution.
#[test]
fn explain_native_result_preserves_the_selected_proof_and_refuses_absent_target() {
    // gmeow-test-input: synthetic-only
    let edb = dataset_of(
        "<urn:proof:x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <urn:proof:A> .\n\
             <urn:proof:A> <https://blackcatinformatics.ca/logic/subClassOf> <urn:proof:B> .\n",
    );
    let input = prepare_reasoning_input(&edb).expect("synthetic native input");
    let domains = SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Default,
        DomainProfile::NonemptyObjectDomainV1,
        "gmeow.mcp.synthetic-explanation.v1".to_owned(),
        *input.ingress_contract(),
    )
    .expect("explicit default theory")])
    .expect("selected theory");
    let result = reason_all_budgeted(input, &domains, &governed_budget(Some(64), None))
        .expect("one governed native execution");
    let graph = gmeow_logic::reason::rl::DEFAULT_WORLD;
    let predicate = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let asserted =
        McpView::explain_native_result(&result, "urn:proof:x", predicate, "<urn:proof:A>", graph)
            .expect("asserted proof");
    assert_eq!(asserted["faithful"], true);
    assert_eq!(asserted["step_skeleton"].as_array().unwrap().len(), 1);
    assert_eq!(asserted["step_skeleton"][0]["is_asserted"], true);
    let derived =
        McpView::explain_native_result(&result, "urn:proof:x", predicate, "<urn:proof:B>", graph)
            .expect("derived proof");
    assert_eq!(derived["faithful"], true);
    let steps = derived["step_skeleton"].as_array().unwrap();
    assert!(!steps[0]["is_asserted"].as_bool().unwrap());
    assert!(
        steps.len() > 1,
        "a genuine derivation retains its premises: {derived}"
    );
    let artifact = purrdf::xsd::parse_base64(
        derived["artifact"]["output"]
            .as_str()
            .expect("the proof response carries its binary artifact envelope"),
    )
    .expect("the published artifact uses native canonical base64");
    let admitted = gmeow_logic::result_rdf::parse_reasoning_artifact(&artifact)
        .expect("the actual proof artifact passes complete native transport admission");
    assert_eq!(
        admitted
            .native_execution()
            .expect("admitted native evidence"),
        result.native_execution().expect("original native evidence"),
        "the binary proof artifact retains the caller-owned native execution"
    );
    assert_eq!(
        derived["judgment_nquads"].as_str().unwrap(),
        ResultProjection::reasoning(&result)
            .expect("project the original native result")
            .to_ntriples(),
    );
    let error = McpView::explain_native_result(
        &result,
        "urn:proof:absent",
        predicate,
        "<urn:proof:B>",
        graph,
    )
    .expect_err("unknown target must refuse");
    assert!(error.to_string().contains("not in closure"));
}
