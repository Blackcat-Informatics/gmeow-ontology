// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Original glossary coordinates for independent executable/spec agreement.

mod json;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_action_cache::{ActionStore, STORE_FORMAT_VERSION, StoreLimits};
use purrdf::{NativeRdfFormat, TermRef, parse_dataset};

#[derive(Clone, Debug)]
pub(super) struct Triple(pub String, pub String, pub purrdf::TermValue);

/// Complete source outcomes, including failed parses retained by the producer.
#[derive(Debug)]
pub(super) struct Observations(
    pub BTreeMap<String, Result<Vec<Triple>, gmeow_errors::RecordedDiag>>,
);

pub(crate) const CHANNEL: &str = "pipeline/glossary-source-observations.json";
const SOURCES: &[&str] = &[
    "slices/grounding/lang/tests/conformance-fixtures/glossary-term-consistent.ttl",
    "slices/grounding/lang/tests/counter-examples/glossary-term-inconsistent.ttl",
    "slices/grounding/lang/tests/conformance-fixtures/glossary-declared-homograph.ttl",
];

pub(super) fn input_files(root: &Path) -> Vec<PathBuf> {
    SOURCES.iter().map(|source| root.join(source)).collect()
}

pub(super) fn record(
    root: &Path,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(|error| super::stage_err(&error.to_string()))?;
    let mut observations = BTreeMap::new();
    for source in SOURCES {
        let observed = super::execution::cached_native_observation(
            root,
            &root.join(source),
            &store,
            "native-glossary-source-v1",
            |bytes| {
                let dataset = parse_dataset(bytes, NativeRdfFormat::Turtle.media_type(), None)
                    .map_err(gmeow_errors::Diag::from)?;
                Ok(dataset
                    .quads()
                    .filter_map(|quad| {
                        let (TermRef::Iri(subject), TermRef::Iri(predicate)) =
                            (dataset.resolve(quad.s), dataset.resolve(quad.p))
                        else {
                            return None;
                        };
                        Some(Triple(
                            subject.to_owned(),
                            predicate.to_owned(),
                            dataset.term_value(quad.o),
                        ))
                    })
                    .collect::<Vec<_>>())
            },
        )
        .map_err(super::record_failure);
        observations.insert((*source).to_owned(), observed);
    }
    artifacts.insert(
        CHANNEL.to_owned(),
        purrdf_lex::json::record::to_vec(&Observations(observations)),
    );
    Ok(())
}
