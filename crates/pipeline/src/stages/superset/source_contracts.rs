// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compact, source-bound fanout contracts from the already-parsed native catalog.
//! Consumers hydrate typed rule records; they never parse or compile authored RDF.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use purrdf::{RdfDataset, RdfTerm};
use serde::{Deserialize, Serialize};

use super::{FanoutFamily, FanoutRule, GraphForm};
use crate::stages::parse_sources::SourceCatalog;

pub(crate) const CHANNEL: &str = "pipeline/superset-source-contracts.json";
const SOURCE: &str = "slices/core/pipeline/module.ttl";
const VALUE_MARKER: &str = "are exactly:";

#[derive(Serialize, Deserialize)]
struct Observation {
    source_path: String,
    source_digest: String,
    fanout_rules: Vec<OwnedRule>,
    expected_outputs: BTreeSet<String>,
    definitions: BTreeMap<String, Definition>,
}

#[derive(Serialize, Deserialize)]
struct Definition {
    text: String,
    values: Result<BTreeSet<String>, gmeow_errors::RecordedDiag>,
}

#[derive(Serialize, Deserialize)]
struct OwnedRule {
    path: String,
    match_prefix: bool,
    suffix: Option<String>,
    family: FanoutFamily,
    form: OwnedForm,
}

/// Unlike the native dispatch enum, this wire record owns its graph IRI. Hydration
/// admits only the graph actually supported by the native N-Quads dispatch arm.
#[derive(Serialize, Deserialize)]
enum OwnedForm {
    Turtle,
    NTriples,
    NQuads(String),
    NQuadsSelf,
    Blob,
    HeaderDict,
}

impl From<FanoutRule> for OwnedRule {
    fn from(rule: FanoutRule) -> Self {
        let form = match rule.form {
            GraphForm::Turtle => OwnedForm::Turtle,
            GraphForm::NTriples => OwnedForm::NTriples,
            GraphForm::NQuads(graph) => OwnedForm::NQuads(graph.to_owned()),
            GraphForm::NQuadsSelf => OwnedForm::NQuadsSelf,
            GraphForm::Blob => OwnedForm::Blob,
            GraphForm::HeaderDict => OwnedForm::HeaderDict,
        };
        Self {
            path: rule.path,
            match_prefix: rule.match_prefix,
            suffix: rule.suffix,
            family: rule.family,
            form,
        }
    }
}

pub(crate) fn input_files(root: &Path) -> Vec<PathBuf> {
    vec![root.join(SOURCE)]
}

pub(crate) fn record(
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let dataset = catalog.document(SOURCE)?;
    let observed = Observation {
        source_path: SOURCE.to_owned(),
        source_digest: catalog.document_digest(SOURCE)?.to_owned(),
        fanout_rules: super::read_fanout_rules(dataset)?
            .into_iter()
            .map(OwnedRule::from)
            .collect(),
        expected_outputs: super::read_expected_outputs(dataset)?,
        definitions: ["extractsGraphFamily", "extractsForm"]
            .into_iter()
            .map(|local| {
                let definition = definition(dataset, local)?;
                Ok((local.to_owned(), definition))
            })
            .collect::<gmeow_errors::Result<_>>()?,
    };
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observed).map_err(|error| super::stage_err(&error.to_string()))?,
    );
    Ok(())
}

fn definition(dataset: &RdfDataset, local: &str) -> gmeow_errors::Result<Definition> {
    let subject = RdfTerm::iri(format!("https://blackcatinformatics.ca/gmeow/{local}"));
    let text = dataset
        .owned_quads()
        .filter(|quad| {
            quad.subject == subject
                && quad.predicate == "http://www.w3.org/2004/02/skos/core#definition"
        })
        .find_map(|quad| match quad.object {
            RdfTerm::Literal(literal) => Some(literal.lexical_form),
            _ => None,
        })
        .ok_or_else(|| super::stage_err(&format!("gmeow:{local} carries no skos:definition")))?;
    let values = text
        .split_once(VALUE_MARKER)
        .ok_or_else(|| {
            format!(
                "gmeow:{local}'s skos:definition must enumerate its values after {VALUE_MARKER:?}"
            )
        })
        .map(|(_, tail)| {
            let tail = tail.split_once('.').map_or(tail, |(head, _)| head);
            tail.split('|')
                .map(|part| part.trim().trim_matches('"').to_owned())
                .filter(|part| !part.is_empty())
                .collect()
        });
    Ok(Definition {
        text,
        values: values.map_err(|message| {
            gmeow_errors::DiagLedger::new().record(
                super::stage_err(&message),
                gmeow_errors::StageId::new("stage-conformance"),
            )
        }),
    })
}

#[cfg(test)]
mod test_support;
#[cfg(test)]
use super::{GRAPH_DIAGNOSTICS_IRI, RdfFanoutClasses};
#[cfg(test)]
pub(super) use test_support::{authored_expected, declared_value_set};
#[cfg(test)]
pub(crate) use test_support::{authored_fanout_classes, authored_fanout_rules};
