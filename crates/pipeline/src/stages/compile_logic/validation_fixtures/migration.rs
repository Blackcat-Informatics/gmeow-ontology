// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer-owned differential evidence for retiring the diagnostics up-set shape.
//! This finite boundary check complements the migration oracle; it is not a
//! universal equivalence certificate or permission to delete a legacy shape.

use std::collections::BTreeSet;
use std::path::Path;

use purrdf::RdfDataset;
use purrdf::shapes::engine::{parse_shapes, validate_dataset};
mod json;

pub(super) const MODULE: &str = "slices/core/diagnostics/module.ttl";
pub(super) const LEGACY_PATH: &str = "slices/core/diagnostics/shapes.ttl";
const LEGACY_SHAPE: &str = "https://blackcatinformatics.ca/gmeow/GateFatalUpsetShape";
const PROJECTED_SHAPE: &str =
    "https://blackcatinformatics.ca/gmeow/GateFatalUpsetConstraintProceduralConstraintShape";
const EX: &str = "https://blackcatinformatics.ca/gmeow/testing/diagnostic-migration/";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Finding {
    pub(super) focus: String,
    path: Option<String>,
    value: Option<String>,
    component: String,
    severity: String,
    messages: Vec<String>,
}

#[derive(Debug)]
pub(super) struct Agreement {
    pub(super) legacy_shape: String,
    pub(super) projected_shape: String,
    pub(super) checked_focuses: BTreeSet<String>,
    pub(super) expected_violations: BTreeSet<String>,
    pub(super) legacy: Vec<Finding>,
    pub(super) projected: Vec<Finding>,
}

struct Witnesses {
    dataset: std::sync::Arc<RdfDataset>,
    checked: BTreeSet<String>,
    expected: BTreeSet<String>,
}

fn failure(message: impl Into<String>) -> gmeow_errors::Diag {
    super::super::stage_err(format!("diagnostics migration parity: {}", message.into()))
}

/// Compare the exact selected generated shape with the still-live legacy shape.
pub(super) fn record(
    root: &Path,
    module: &super::ModuleProduct,
) -> gmeow_errors::Result<Agreement> {
    if module
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "MALFORMED_CONSTRAINT")
    {
        return Err(failure(
            "canonical diagnostics module has malformed constraints",
        ));
    }
    let bytes = std::fs::read(root.join(LEGACY_PATH))?;
    let legacy = std::str::from_utf8(&bytes)
        .map_err(|error| failure(format!("legacy shape is not UTF-8: {error}")))?;
    let projected = purrdf::parse_dataset(module.procedural_shacl.as_bytes(), "text/turtle", None)?;
    let projected = gmeow_validate::shape_oracle::shape_subgraph_ttl(
        &projected,
        &[PROJECTED_SHAPE.to_owned()],
        "diagnostic-migration",
    );
    compare(
        legacy,
        &projected,
        &population()?,
        LEGACY_SHAPE,
        PROJECTED_SHAPE,
    )
}

fn finding_set(
    turtle: &str,
    data: &RdfDataset,
    source_shape: &str,
) -> gmeow_errors::Result<Vec<Finding>> {
    let shapes = parse_shapes(turtle, None)?;
    let mut report = validate_dataset(data, &shapes)?;
    gmeow_validate::store::dedupe_validation_results(&mut report);
    let mut findings = Vec::new();
    for result in report.results {
        if result.source_shape.to_string() != format!("<{source_shape}>") {
            return Err(failure(format!(
                "unexpected source shape {}; expected <{source_shape}>",
                result.source_shape
            )));
        }
        findings.push(Finding {
            focus: result.focus_node.to_string(),
            path: result.result_path.as_ref().map(ToString::to_string),
            value: result.value.as_ref().map(ToString::to_string),
            component: result.source_constraint_component.as_str().to_owned(),
            severity: format!("{:?}", result.severity),
            messages: result
                .messages
                .iter()
                .map(|message| purrdf::shapes::term::Term::Literal(message.clone()).to_string())
                .collect(),
        });
    }
    findings.sort();
    Ok(findings)
}

fn compare(
    legacy_ttl: &str,
    projected_ttl: &str,
    witnesses: &Witnesses,
    legacy_shape: &str,
    projected_shape: &str,
) -> gmeow_errors::Result<Agreement> {
    if witnesses.expected.is_empty()
        || !witnesses.expected.is_subset(&witnesses.checked)
        || witnesses.expected.len() == witnesses.checked.len()
    {
        return Err(failure(
            "witness population must contain both violating and conforming focuses",
        ));
    }
    let legacy = finding_set(legacy_ttl, &witnesses.dataset, legacy_shape)?;
    let projected = finding_set(projected_ttl, &witnesses.dataset, projected_shape)?;
    let observed: BTreeSet<_> = legacy.iter().map(|finding| finding.focus.clone()).collect();
    if observed != witnesses.expected {
        return Err(failure(format!(
            "legacy verdict differs from independent boundary expectations: expected {:?}, got {observed:?}",
            witnesses.expected
        )));
    }
    if legacy != projected {
        return Err(failure(format!(
            "generated findings differ: legacy={legacy:?}, projected={projected:?}"
        )));
    }
    Ok(Agreement {
        legacy_shape: legacy_shape.to_owned(),
        projected_shape: projected_shape.to_owned(),
        checked_focuses: witnesses.checked.clone(),
        expected_violations: witnesses.expected.clone(),
        legacy,
        projected,
    })
}

/// Independent Cartesian boundary population, not observations derived from either validator.
fn population() -> gmeow_errors::Result<Witnesses> {
    let mut turtle = format!(
        "@prefix ex: <{EX}> .\n\
         @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         ex:DerivedFinding rdfs:subClassOf ex:IntermediateFinding .\n\
         ex:IntermediateFinding rdfs:subClassOf gmeow:Finding .\n\
         ex:Blocking gmeow:categoryBlocking gmeow:blockingBlocking .\n\
         ex:Advisory gmeow:categoryBlocking gmeow:blockingNonBlocking .\n"
    );
    for (kind, class) in [
        Some("gmeow:Finding"),
        Some("ex:DerivedFinding"),
        Some("ex:Other"),
        None,
    ]
    .into_iter()
    .enumerate()
    {
        for (severity, grade) in [
            Some("gmeow:severityError"),
            Some("gmeow:severityNote"),
            None,
        ]
        .into_iter()
        .enumerate()
        {
            for (category, category_value) in [Some("ex:Blocking"), Some("ex:Advisory"), None]
                .into_iter()
                .enumerate()
            {
                for (standpoint, standpoint_value) in [
                    Some("gmeow:standpointBinding"),
                    Some("gmeow:standpointPerspectival"),
                    None,
                ]
                .into_iter()
                .enumerate()
                {
                    for (verdict, verdict_value) in
                        [Some("gmeow:gateFatal"), Some("gmeow:gatePass"), None]
                            .into_iter()
                            .enumerate()
                    {
                        let subject =
                            format!("ex:f{kind}{severity}{category}{standpoint}{verdict}");
                        add_case(
                            &mut turtle,
                            &subject,
                            [
                                class,
                                grade,
                                category_value,
                                standpoint_value,
                                verdict_value,
                            ],
                            kind < 2
                                && severity == 0
                                && category == 0
                                && standpoint == 0
                                && verdict != 0,
                        );
                    }
                }
            }
        }
    }
    let guarded = [
        Some("gmeow:Finding"),
        Some("gmeow:severityError"),
        Some("ex:Blocking"),
        Some("gmeow:standpointBinding"),
        None,
    ];
    add_case(&mut turtle, "ex:multipleGuards", guarded, true);
    turtle.push_str("ex:multipleGuards gmeow:findingSeverity gmeow:severityNote ; gmeow:findingCategory ex:Advisory ; gmeow:findingStandpoint gmeow:standpointPerspectival .\n");
    add_case(&mut turtle, "ex:multipleVerdicts", guarded, false);
    turtle.push_str(
        "ex:multipleVerdicts gmeow:findingGateVerdict gmeow:gatePass, gmeow:gateFatal .\n",
    );
    add_case(
        &mut turtle,
        "ex:disconnected",
        [
            Some("gmeow:Finding"),
            Some("gmeow:severityError"),
            None,
            None,
            None,
        ],
        false,
    );
    turtle.push_str("ex:separateSupport gmeow:findingCategory ex:Blocking ; gmeow:findingStandpoint gmeow:standpointBinding ; gmeow:findingGateVerdict gmeow:gateFatal .\n");
    add_case(&mut turtle, "_:missingVerdict", guarded, true);
    add_case(&mut turtle, "_:presentVerdict", guarded, false);
    turtle.push_str("_:presentVerdict gmeow:findingGateVerdict gmeow:gateFatal .\n");
    witnesses(&turtle)
}

fn add_case(turtle: &mut String, subject: &str, values: [Option<&str>; 5], expected: bool) {
    turtle.push_str(&format!(
        "{subject} ex:checked true ; ex:expected {expected} .\n"
    ));
    for (predicate, value) in [
        "a",
        "gmeow:findingSeverity",
        "gmeow:findingCategory",
        "gmeow:findingStandpoint",
        "gmeow:findingGateVerdict",
    ]
    .into_iter()
    .zip(values)
    {
        if let Some(value) = value {
            turtle.push_str(&format!("{subject} {predicate} {value} .\n"));
        }
    }
}

fn witnesses(turtle: &str) -> gmeow_errors::Result<Witnesses> {
    let dataset = purrdf::parse_dataset(turtle.as_bytes(), "text/turtle", None)?;
    let truth = purrdf::RdfTerm::literal(purrdf::RdfLiteral::typed(
        "true",
        "http://www.w3.org/2001/XMLSchema#boolean",
    ));
    let subjects = |predicate: &str| {
        dataset
            .owned_quads()
            .filter(|quad| quad.predicate == format!("{EX}{predicate}") && quad.object == truth)
            .map(|quad| quad.subject.to_string())
            .collect()
    };
    let checked = subjects("checked");
    let expected = subjects("expected");
    Ok(Witnesses {
        dataset,
        checked,
        expected,
    })
}

#[path = "migration.tests.rs"]
#[cfg(test)]
mod tests;
