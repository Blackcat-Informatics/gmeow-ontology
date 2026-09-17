// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The source-declared well-founded phase plan, observed by the explicit producer.
//! This plan documents the native loop; it never schedules the runtime (Principle 12).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_action_cache::{ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef, TermValue};
use serde::{Deserialize, Serialize};

use crate::stages::parse_sources::SourceCatalog;

pub(super) const CHANNEL: &str = "pipeline/wellfounded-plan-observation.json";
pub(super) const SOURCE: &str = "slices/grounding/logic/module.ttl";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

#[derive(Serialize, Deserialize)]
pub(super) struct Observation {
    pub source_path: String,
    pub source_digest: String,
    pub plan_types: BTreeSet<String>,
    pub body_roots: Vec<String>,
    pub walk: Result<PhaseWalk, gmeow_errors::RecordedDiag>,
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct PhaseWalk {
    pub phases: Vec<String>,
    pub iterated: Vec<String>,
}

pub(super) fn input_files(root: &Path) -> Vec<PathBuf> {
    vec![root.join(SOURCE)]
}

pub(super) fn record(
    root: &Path,
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let fail = |error: String| super::stage_err(&error);
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(|error| fail(error.to_string()))?;
    let digest = catalog.document_digest(SOURCE)?.to_owned();
    let observed = super::execution::cached_inputs(
        &store,
        "wellfounded-phase-plan-v1:original-default-graph",
        vec![ActionInput::Raw {
            logical_path: SOURCE.to_owned(),
            file_kind: FileKind::File,
            executable: false,
            digest: digest.clone(),
        }],
        || {
            let dataset = catalog.document(SOURCE)?;
            let plan = iri("wellFoundedMaterializerPlan");
            let body_roots = named_objects(dataset, &plan, &iri("planBody"));
            let walk = one_object(dataset, &plan, &iri("planBody")).and_then(|body| {
                let mut walked = PhaseWalk::default();
                collect_phases(dataset, &body, &mut walked, &mut BTreeSet::new())?;
                Ok(walked)
            });
            Ok(Observation {
                source_path: SOURCE.to_owned(),
                source_digest: digest.clone(),
                plan_types: types(dataset, &plan),
                body_roots,
                walk: walk.map_err(super::record_failure),
            })
        },
    )?;
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observed).map_err(|error| fail(error.to_string()))?,
    );
    Ok(())
}

fn iri(local: &str) -> String {
    format!("{LOGIC}{local}")
}

fn local_name(iri: &str) -> String {
    iri.rsplit(['/', '#']).next().unwrap_or(iri).to_owned()
}

fn named_objects(dataset: &RdfDataset, subject: &str, predicate: &str) -> Vec<String> {
    let (Some(subject), Some(predicate)) = (
        dataset.term_id_by_value(&TermValue::iri(subject)),
        dataset.term_id_by_value(&TermValue::iri(predicate)),
    ) else {
        return Vec::new();
    };
    dataset
        .quads_for_pattern(Some(subject), Some(predicate), None, GraphMatch::Default)
        .filter_map(|quad| match dataset.resolve(quad.o) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
        .collect()
}

fn one_object(
    dataset: &RdfDataset,
    subject: &str,
    predicate: &str,
) -> Result<String, gmeow_errors::Diag> {
    let mut found = named_objects(dataset, subject, predicate);
    if found.len() != 1 {
        return Err(super::stage_err(&format!(
            "expected exactly one <{subject}> <{predicate}> ? edge, found {}",
            found.len()
        )));
    }
    Ok(found.remove(0))
}

fn types(dataset: &RdfDataset, node: &str) -> BTreeSet<String> {
    // Both surfaces name the same structural logic class; selection remains the
    // original document's default graph, never an imported or reasoned union.
    named_objects(dataset, node, RDF_TYPE)
        .into_iter()
        .chain(named_objects(dataset, node, &iri("instanceOf")))
        .collect()
}

fn collect_phases(
    dataset: &RdfDataset,
    node: &str,
    walked: &mut PhaseWalk,
    active: &mut BTreeSet<String>,
) -> Result<(), gmeow_errors::Diag> {
    if !active.insert(node.to_owned()) {
        return Err(super::stage_err(&format!(
            "cyclic transaction-program syntax at <{node}>"
        )));
    }
    let classes = types(dataset, node);
    if classes.contains(&iri("ActionSchema")) {
        walked.phases.push(local_name(node));
    } else if classes.contains(&iri("SerialConjunction")) {
        let left = one_object(dataset, node, &iri("leftOperand"))?;
        let right = one_object(dataset, node, &iri("rightOperand"))?;
        collect_phases(dataset, &left, walked, active)?;
        collect_phases(dataset, &right, walked, active)?;
    } else if classes.contains(&iri("Iteration")) {
        let body = one_object(dataset, node, &iri("iterationBody"))?;
        if types(dataset, &body).contains(&iri("ActionSchema")) {
            walked.iterated.push(local_name(&body));
        }
        collect_phases(dataset, &body, walked, active)?;
    } else {
        return Err(super::stage_err(&format!(
            "node <{node}> is not a recognised transaction-program combinator or ActionSchema"
        )));
    }
    active.remove(node);
    Ok(())
}
