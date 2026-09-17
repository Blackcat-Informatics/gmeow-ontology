// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source-admission observations are shipped separately from semantic comparisons.

use gmeow_conformance::native_observation::SourceAdmissionObservation;
use gmeow_logic::reason::refute::ClassAdmissionObservation;
use purrdf::TermValue;
use std::fmt::Write as _;
use std::path::Path;

/// One producer-authenticated observation with independent external attribution.
pub(super) struct GradedAdmission {
    corpus: String,
    case: String,
    published: String,
    observation: SourceAdmissionObservation,
}

/// Check the exact live producer observation against this selected operation's
/// frozen expectation. Native source RDF is neither parsed nor evaluated again.
pub(super) fn grade(
    corpus: &str,
    case: &str,
    directory: &Path,
    published: String,
    observation: &SourceAdmissionObservation,
) -> gmeow_errors::Result<GradedAdmission> {
    observation.admission.validate()?;
    let bytes = std::fs::read(directory.join("input.nq"))
        .map_err(|error| super::stage_err(&error.to_string()))?;
    if observation.input_blake3 != *blake3::hash(&bytes).as_bytes() {
        return Err(super::stage_err(
            "source-admission producer observed different input bytes",
        ));
    }
    let expected: ClassAdmissionObservation = serde_json::from_slice(
        &std::fs::read(directory.join("expected/verdicts.json"))
            .map_err(|error| super::stage_err(&error.to_string()))?,
    )
    .map_err(|error| super::stage_err(&error.to_string()))?;
    expected.validate()?;
    if observation.admission != expected {
        return Err(super::stage_err(
            "source-admission observation differs from its exact selected expectation",
        ));
    }
    Ok(GradedAdmission {
        corpus: corpus.to_owned(),
        case: case.to_owned(),
        published,
        observation: observation.clone(),
    })
}

const RECEIPT_SCHEMA: &str = "gmeow-source-admission-observation-v1";
const RECEIPT_LIMIT: usize = 1024 * 1024;

struct Receipt(Vec<u8>);
impl std::io::Write for Receipt {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > RECEIPT_LIMIT.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other(
                "source-admission receipt exceeds its one-MiB terminal profile",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn literal(value: &str, datatype: &str) -> String {
    gmeow_logic::provenance::term_display(&TermValue::Literal {
        lexical_form: value.to_owned(),
        datatype: format!("http://www.w3.org/2001/XMLSchema#{datatype}"),
        language: None,
        direction: None,
    })
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut out, "{byte:02x}").expect("String writes are infallible");
    }
    out
}

/// Terminal RDF projection carries complete bounded native evidence once per
/// source case, plus queryable operation/provenance fields. No semantic verdict
/// or agreement finding is fabricated for an admission observation.
pub(super) fn emit(observations: &[GradedAdmission]) -> gmeow_errors::Result<String> {
    let mut out = String::new();
    for record in observations {
        record.observation.admission.validate()?;
        let mut receipt = Receipt(Vec::new());
        ciborium::into_writer(
            &(
                RECEIPT_SCHEMA,
                &record.corpus,
                &record.case,
                &record.published,
                &record.observation,
            ),
            &mut receipt,
        )
        .map_err(|error| super::stage_err(&error.to_string()))?;
        let subject = format!(
            "urn:gmeow:source-admission:{}",
            blake3::hash(&receipt.0).to_hex()
        );
        let graph = gmeow_conformance::divergence::CONFORMANCE_GRAPH;
        let logic = gmeow_ns::LOGIC_NS;
        let mut row = |predicate: &str, object: String| {
            writeln!(&mut out, "<{subject}> <{predicate}> {object} <{graph}> .")
                .expect("String writes are infallible");
        };
        row(
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            format!("<{logic}SourceAdmissionObservation>"),
        );
        row(
            "http://purl.org/dc/terms/identifier",
            literal(&format!("{}/{}", record.corpus, record.case), "string"),
        );
        row(
            &format!("{logic}observedSourceAdmission"),
            format!("<{logic}{}>", record.observation.admission.contract),
        );
        row(
            &format!("{logic}sourceAdmissionSelected"),
            literal(
                if record.observation.admission.outside_selection() {
                    "false"
                } else {
                    "true"
                },
                "boolean",
            ),
        );
        row(
            &format!("{logic}sourceAdmissionInputDigest"),
            literal(&hex(&record.observation.input_blake3), "hexBinary"),
        );
        row(
            &format!("{logic}sourceAdmissionPublishedToken"),
            literal(&record.published, "string"),
        );
        let count = record
            .observation
            .admission
            .source_worlds
            .values()
            .try_fold(0u64, |count, world| count.checked_add(world.assertions))
            .ok_or_else(|| super::stage_err("source-admission assertion census overflow"))?;
        row(
            &format!("{logic}sourceAdmissionAssertionCount"),
            literal(&count.to_string(), "nonNegativeInteger"),
        );
        for world in record.observation.admission.source_worlds.keys() {
            row(
                &format!("{logic}sourceAdmissionWorldKey"),
                literal(world, "string"),
            );
        }
        row(
            &format!("{logic}sourceAdmissionEvidence"),
            literal(&hex(&receipt.0), "hexBinary"),
        );
    }
    Ok(out)
}

#[path = "source_admission.tests.rs"]
#[cfg(test)]
mod tests;
