// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authenticated native scene findings and the original pinned modal product.
//! Tests inspect complete typed evidence without source parsing or evaluation.

use std::sync::{Arc, OnceLock};

use gmeow_errors::{Finding, Severity};
use gmeow_logic::reasoning_graphs::GRAPH_EXAMPLES;
use gmeow_logic::result_rdf::GRAPH_REASONING;

use super::super::native_scene::{
    ALPHA_DRIFT, AuxiliaryObservation, CHANNEL, DIMENSION, DimensionFindings, MATH_MODULE, MODAL,
    Scene,
};

fn scene(path: &str) -> &'static Scene {
    super::gmn_grounding::observations()
        .sources
        .get(path)
        .unwrap_or_else(|| panic!("required native scene {path} is absent"))
        .native_scene
        .as_ref()
        .unwrap_or_else(|| panic!("selected native scene {path} was not observed"))
        .as_ref()
        .unwrap_or_else(|error| panic!("native scene {path}: {error}"))
}

fn dimension(path: &str) -> &'static DimensionFindings {
    let Scene::Dimension(observed) = scene(path) else {
        panic!("selected dimension scene {path} has the wrong observation kind");
    };
    observed
}

fn has_class(findings: &[Finding], needle: &str) -> bool {
    findings
        .iter()
        .any(|finding| finding.severity == Severity::Error && finding.message.contains(needle))
}

fn gmn_dimension_roundtrip_scene_is_clean() {
    let f = &dimension(DIMENSION).findings;
    assert!(
        !has_class(f, "math:DimensionalInhomogeneity") && !has_class(f, "math:MalformedDimension"),
        "the force = ∫ a dm round-trip scene must pass the ℚ⁷ dimension gate cleanly: {f:?}"
    );
}

fn real_math_module_force_dimension_is_clean_under_the_reasoned_gate() {
    let observed = dimension(MATH_MODULE);
    // Native source evidence strengthens the former disconnected string search:
    // the exact canonical subject carries the required class itself.
    assert!(
        observed
            .force_dimension_types
            .contains("https://blackcatinformatics.ca/math/DerivedDimension"),
        "math:forceDimension must be authored as a math:DerivedDimension in the real math module"
    );
    let f = &observed.findings;
    assert!(
        f.iter().all(|x| x.severity != Severity::Error),
        "the real math module (incl. the canonical math:forceDimension) must pass the ℚ⁷ \
         dimension gate cleanly in the production read substrate: {f:?}"
    );
}

fn alpha_drift() -> &'static AuxiliaryObservation {
    static OBSERVED: OnceLock<
        gmeow_errors::Result<
            super::source_artifact::Selected<
                Result<AuxiliaryObservation, gmeow_errors::RecordedDiag>,
            >,
        >,
    > = OnceLock::new();
    let observed = super::source_artifact::get(&OBSERVED, CHANNEL)
        .as_ref()
        .expect("native alpha-equivalence scene evaluation");
    assert_eq!(observed.source_path, ALPHA_DRIFT);
    observed
}

fn alpha_equivalent_drifted_expressions_cite_the_same_alpha_class_iri() {
    let f = &alpha_drift().findings;
    let drift: Vec<&Finding> = f
        .iter()
        .filter(|finding| finding.message.contains("math:StructuralKeyDrift"))
        .collect();
    assert_eq!(
        drift.len(),
        2,
        "both alpha-equivalent binder expressions must drift: {f:?}"
    );
    for finding in &drift {
        assert_eq!(
            finding.cited_iris.len(),
            1,
            "each drift finding cites exactly one α-equivalence-class IRI: {finding:?}"
        );
        assert!(
            finding.cited_iris[0].starts_with("https://blackcatinformatics.ca/math/alphaClass/"),
            "the cited IRI is minted under the math: alpha-class namespace: {finding:?}"
        );
    }
    assert_eq!(
        drift[0].cited_iris[0], drift[1].cited_iris[0],
        "two alpha-equivalent expressions' drift findings cite the SAME α-equivalence-class \
         IRI — a consumer can join on it: {f:?}"
    );
}

fn shipped_modal_example_is_a_complete_reason_product_frame() {
    let Scene::Modal(product) = scene(MODAL) else {
        panic!("graph/reasoning must carry an observed native Reasoning product");
    };
    assert_eq!(product.stage_id, "stage-reason");
    assert_eq!(product.graph_iri, GRAPH_REASONING);
    assert_eq!(product.pinned_graph_digest, product.graph_digest);
    let result = &product.result;
    assert_eq!(
        product.payload_digest,
        crate::handle_identity::handle_payload_digest(&crate::bundle::PipelineHandle::Reasoning(
            Arc::new(result.clone()),
        )),
        "the observed product retains its complete pinned native payload"
    );
    // C owns the modal verdict; w0 is its explicit evaluation/accessibility
    // source. The example's body remains in C, so self-access at the distinct
    // w0 does not make the body true there. Do not project the verdict into w0.
    for (formula, predicate) in [
        (
            "https://blackcatinformatics.ca/gmeow/examples/logic/necessarilyReliable",
            "https://blackcatinformatics.ca/logic/modalNecessityFails",
        ),
        (
            "https://blackcatinformatics.ca/gmeow/examples/logic/possiblyReliable",
            "https://blackcatinformatics.ca/logic/modalPossibilityFails",
        ),
    ] {
        let verdict = result
            .inferred()
            .iter()
            .find(|axiom| axiom.subject == formula && axiom.predicate == predicate)
            .unwrap_or_else(|| panic!("missing production modal verdict for {formula}"));
        assert_eq!(verdict.world, GRAPH_EXAMPLES);
        assert_eq!(
            verdict.rule_name.as_deref(),
            Some("https://blackcatinformatics.ca/logic/rule/modal-evaluation")
        );
        assert!(
            result.inferred().iter().any(|axiom| {
                axiom.subject == formula
                    && axiom.predicate == "https://blackcatinformatics.ca/logic/modalEvalWorld"
                    && axiom.object.as_iri()
                        == Some("https://blackcatinformatics.ca/gmeow/examples/logic/modalWorld")
                    && axiom.world == GRAPH_EXAMPLES
            }),
            "the asserting context retains its explicit evaluation-world binding"
        );
    }
    let counterexample = result
        .inferred()
        .iter()
        .find(|axiom| {
            axiom.subject
                == "https://blackcatinformatics.ca/gmeow/examples/logic/necessarilyReliable"
                && axiom.predicate
                    == "https://blackcatinformatics.ca/logic/modalCounterexampleWorld"
        })
        .expect("shipped necessity verdict carries its exact counterexample world");
    assert_eq!(
        counterexample.object.as_iri(),
        Some("https://blackcatinformatics.ca/gmeow/examples/logic/modalWorld")
    );
    assert_eq!(counterexample.world, GRAPH_EXAMPLES);
    assert_eq!(
        counterexample.rule_name.as_deref(),
        Some("https://blackcatinformatics.ca/logic/rule/modal-evaluation")
    );
}

#[test]
fn authored_native_scene_contracts() {
    gmn_dimension_roundtrip_scene_is_clean();
    real_math_module_force_dimension_is_clean_under_the_reasoned_gate();
    alpha_equivalent_drifted_expressions_cite_the_same_alpha_class_iri();
    shipped_modal_example_is_a_complete_reason_product_frame();
}
