// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer-owned negative control for the shipped flagship wiring constraint.
//! The exact archived production shape profile is prepared only by the explicit
//! fixture producer; tests consume the recorded findings without loading shapes.

use serde::{Deserialize, Serialize};

use super::production_shapes::PreparedProductionShapes;

/// Required action over the selected bundle's complete production shape archive.
pub const ARTIFACT: &str = "flagship-unwired-observation-v1.json";
/// Exact profile used by the original missing-producer regression.
pub const PROFILE: &str = "archived-production-shapes:flagship-deduplicated-v1";

/// Every required wiring link except the producer. This is deliberately the
/// original control, including the language-owned failure-class reference.
const UNWIRED_SCENARIO: &str = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang:  <https://blackcatinformatics.ca/lang/> .
@prefix cq:    <https://blackcatinformatics.ca/gmeow/examples/lang/tests/> .

<https://blackcatinformatics.ca/gmeow/examples/unwired/missingProducer>
    a gmeow:FlagshipScenario ;
    gmeow:demonstratedByExample "tests/conformance-fixtures/example.ttl" ;
    gmeow:demonstratedByCompetency cq:cqSome ;
    gmeow:guardedByCounterExample "tests/counter-examples/counter.ttl" ;
    gmeow:enforcesFailureClass lang:UnhashableSurface .
"#;

/// Compact validation evidence tied to the producer's exact shape and control
/// bytes. All findings are retained, so consumer counts cannot hide extra hits.
#[derive(Debug, Serialize, Deserialize)]
pub struct Observation {
    pub profile: String,
    pub shapes_digest: String,
    pub input_digest: String,
    pub conforms: bool,
    pub findings: Vec<Finding>,
}

/// The owning shape and its resolved failure class stay paired on each finding;
/// a blank property shape is resolved through `FailureClassIndex`, never guessed
/// from a named parent or the result path.
#[derive(Debug, Serialize, Deserialize)]
pub struct Finding {
    pub focus: String,
    pub path: Option<String>,
    pub component: String,
    pub source_shape: String,
    pub severity: String,
    pub failure_class: Option<String>,
    pub message: Option<String>,
}

/// Bind the tiny control to the exact prepared production profile in the
/// explicit producer. The bundle action receipt authenticates
/// this source, the shape archive, and the resulting compact observation.
pub fn observe(prepared: &PreparedProductionShapes) -> gmeow_errors::Result<Observation> {
    let data =
        purrdf::parse_dataset(UNWIRED_SCENARIO.as_bytes(), "text/turtle", None).map_err(fail)?;
    let mut report = prepared
        .shapes
        .bind_dataset(&data)
        .map_err(fail)?
        .validate()
        .map_err(fail)?;
    gmeow_validate::store::dedupe_validation_results(&mut report);
    Ok(Observation {
        profile: PROFILE.to_owned(),
        shapes_digest: prepared.digest.clone(),
        input_digest: gmeow_action_cache::bytes_digest(UNWIRED_SCENARIO.as_bytes()),
        conforms: report.conforms,
        findings: report
            .results
            .iter()
            .map(|result| Finding {
                focus: result.focus_node.to_string(),
                path: result.result_path.as_ref().map(ToString::to_string),
                component: result.source_constraint_component.as_str().to_owned(),
                source_shape: result.source_shape.to_string(),
                severity: result.severity.iri().to_owned(),
                failure_class: prepared.classes.for_result(result).map(str::to_owned),
                message: result.message.clone(),
            })
            .collect(),
    })
}

/// Keep producer failures on the conformance stage's typed diagnostic surface.
fn fail(error: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::stage_err(&error.to_string())
}
