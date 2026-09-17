// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer-owned advisory control over the shared production shape profile.
//! The ontology is the selected bundle's already-imported native dataset.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_validate::advisory::{project_compliance_assessment, split_advisory_results};
use purrdf::{RdfDataset, TermRef};
use serde::{Deserialize, Serialize};

use super::production_shapes::PreparedProductionShapes;

/// Required compact action, including the actual emitted advisory claim wing.
pub const ARTIFACT: &str = "advice-wing-observation-v1.json";
const DEMO_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics-fixture-demo";

/// The original five-assertion control: bare Entity, proper Entity+Agent, and
/// Entity+Event. Their respective advisory counts must remain one, zero and two.
const INPUT: &str = concat!(
    "<https://ex.test/bareThing> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/gmeow/Entity> .\n",
    "<https://ex.test/goodThing> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/gmeow/Entity> .\n",
    "<https://ex.test/goodThing> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/gmeow/Agent> .\n",
    "<https://ex.test/badEvent> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/gmeow/Event> .\n",
    "<https://ex.test/badEvent> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/gmeow/Entity> .\n",
);

/// Exact source attribution and both advisory projection wings; retained hard
/// findings stay visible independently from the advisory count.
#[derive(Debug, Serialize, Deserialize)]
pub struct Observation {
    pub profile: String,
    pub shapes_digest: String,
    pub input_digest: String,
    pub shape_sources: BTreeMap<String, BTreeMap<String, BTreeSet<String>>>,
    pub retained_conforms: bool,
    pub retained_findings: Vec<RetainedFinding>,
    pub advisories: Vec<Advisory>,
    pub claims_nquads: String,
}

/// Preserve hard-finding severity and its exact shape-to-failure-class binding.
#[derive(Debug, Serialize, Deserialize)]
pub struct RetainedFinding {
    pub focus: String,
    pub component: String,
    pub source_shape: String,
    pub severity: String,
    pub failure_class: Option<String>,
}

/// The actual graded advisory and its claim, paired by one projection call.
#[derive(Debug, Serialize, Deserialize)]
pub struct Advisory {
    pub code: String,
    pub message: String,
    pub severity: gmeow_errors::Severity,
    pub suggestions: Vec<String>,
    pub subject_iri: Option<String>,
    pub claim_code: String,
    pub claim_subject_iri: Option<String>,
    pub modality_iri: String,
    pub standpoint_iri: String,
    pub confidence: f64,
    pub verdict_iri: String,
    pub advised_proposition: String,
}

/// Bind only the tiny explicit input to the shared shape analysis. Preserve the
/// original raw-report advisory split, including its own per-code deduplication.
pub fn observe(
    prepared: &PreparedProductionShapes,
    ontology: &RdfDataset,
) -> gmeow_errors::Result<Observation> {
    let data =
        purrdf::parse_dataset(INPUT.as_bytes(), "application/n-triples", None).map_err(fail)?;
    let report = prepared
        .shapes
        .bind_dataset(&data)
        .map_err(fail)?
        .validate()
        .map_err(fail)?;
    let (retained, advisories) = split_advisory_results(report, &prepared.dataset, ontology);
    let claims: Vec<_> = advisories
        .iter()
        .map(|advisory| advisory.project().claim)
        .collect();
    let claims_nquads = project_compliance_assessment(&claims, DEMO_GRAPH);
    Ok(Observation {
        profile: super::production_shapes::PROFILE.to_owned(),
        shapes_digest: prepared.digest.clone(),
        input_digest: gmeow_action_cache::bytes_digest(INPUT.as_bytes()),
        shape_sources: shape_sources(&prepared.dataset),
        retained_conforms: retained.conforms,
        retained_findings: retained
            .results
            .iter()
            .map(|result| RetainedFinding {
                focus: result.focus_node.to_string(),
                component: result.source_constraint_component.as_str().to_owned(),
                source_shape: result.source_shape.to_string(),
                severity: result.severity.iri().to_owned(),
                failure_class: prepared.classes.for_result(result).map(str::to_owned),
            })
            .collect(),
        advisories: advisories
            .iter()
            .zip(&claims)
            .map(|(advisory, claim)| Advisory {
                code: advisory.code.clone(),
                message: advisory.message.clone(),
                severity: advisory.severity,
                suggestions: advisory.suggestions.clone(),
                subject_iri: advisory.subject_iri.clone(),
                claim_code: claim.code.clone(),
                claim_subject_iri: claim.subject_iri.clone(),
                modality_iri: claim.modality_iri.clone(),
                standpoint_iri: claim.standpoint_iri.clone(),
                confidence: claim.confidence,
                verdict_iri: claim.verdict_iri.clone(),
                advised_proposition: claim.advised_proposition.clone(),
            })
            .collect(),
        claims_nquads,
    })
}

/// Capture each real advisory shape's formalization, severity and message under
/// its own subject identity. A value on an unrelated shape cannot satisfy this
/// provenance observation; no shape text is copied or reparsed for the consumer.
fn shape_sources(dataset: &RdfDataset) -> BTreeMap<String, BTreeMap<String, BTreeSet<String>>> {
    let mut sources: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
    let rows = dataset
        .quads()
        .map(|row| (row.s, row.p, row.o))
        .chain(dataset.annotations());
    for (subject, predicate, object) in rows {
        let (TermRef::Iri(subject), TermRef::Iri(predicate)) =
            (dataset.resolve(subject), dataset.resolve(predicate))
        else {
            continue;
        };
        if ![
            "BareEntitySortalAdviceConstraint",
            "EndurantAsEventAdviceConstraint",
        ]
        .iter()
        .any(|name| subject.contains(name))
        {
            continue;
        }
        if matches!(
            predicate,
            "https://blackcatinformatics.ca/logic/formalizes"
                | "http://www.w3.org/ns/shacl#severity"
                | "http://www.w3.org/ns/shacl#message"
        ) {
            sources
                .entry(subject.to_owned())
                .or_default()
                .entry(predicate.to_owned())
                .or_default()
                .insert(dataset.to_owned_term(object).to_string());
        }
    }
    sources
}

/// Preserve native validation failures as typed producer diagnostics.
fn fail(error: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::stage_err(&error.to_string())
}
