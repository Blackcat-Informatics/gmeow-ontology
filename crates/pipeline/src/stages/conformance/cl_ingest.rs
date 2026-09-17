// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Selected external CLIF ingestion, with one parse shared by both executions.
//! Compact output retains the complete world-scoped quad and annotation lineage;
//! the consumer grades it without loading or compiling the authored fixtures.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_action_cache::{
    ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits, bytes_digest,
};
use gmeow_logic::annotation::{
    AnnotatedQuad, AnnotationCertification, AnnotationContract, AnnotationFactRef,
    AnnotationRequest,
};
use gmeow_logic::materialize::{
    MaterializationLimits, materialize_program, materialize_program_annotated,
};
use gmeow_logic::provenance::{ZWeightSemiring, term_display};
use gmeow_logic::seam::DerivedQuad;
use gmeow_logic_compile::{clif::parse_clif_str, frontend::Diagnostic};
use serde::{Deserialize, Serialize};

pub(crate) const CHANNEL: &str = "pipeline/cl-ingest-observations.json";
const SOURCES: [&str; 2] = [
    "conformance/logic/cl-ingest/sample-kb.clif",
    "conformance/logic/cl-ingest/sample-kb.edb.nq",
];
const GENEALOGY: &str = "https://example.org/cl-ingest/genealogy/";

#[derive(Serialize, Deserialize)]
pub(super) struct Observation {
    pub diagnostics: Vec<Diagnostic>,
    pub plain: Result<Vec<DerivedQuad>, gmeow_errors::RecordedDiag>,
    pub annotated: Result<Annotated, gmeow_errors::RecordedDiag>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Annotated {
    pub certification: AnnotationCertification,
    pub quads: Vec<AnnotatedQuad<i64>>,
}

pub(super) fn input_files(root: &Path) -> Vec<PathBuf> {
    SOURCES.iter().map(|path| root.join(path)).collect()
}

pub(super) fn record(
    root: &Path,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let fail = |error: &dyn std::fmt::Display| super::stage_err(&error.to_string());
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(|error| fail(&error))?;
    let sources = SOURCES
        .iter()
        .map(|path| {
            let bytes = std::fs::read(root.join(path)).map_err(|error| fail(&error))?;
            let input = ActionInput::Raw {
                logical_path: (*path).to_owned(),
                file_kind: FileKind::File,
                executable: false,
                digest: bytes_digest(&bytes),
            };
            Ok((input, bytes))
        })
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    let observed = super::execution::cached_inputs(
        &store,
        "clif-ingest-native-v1",
        sources.iter().map(|(input, _)| input.clone()).collect(),
        || observe(&sources[0].1, &sources[1].1),
    )
    .map_err(super::record_failure);
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observed).map_err(|error| fail(&error))?,
    );
    Ok(())
}

fn observe(clif: &[u8], edb: &[u8]) -> Result<Observation, gmeow_errors::Diag> {
    let clif = std::str::from_utf8(clif).map_err(gmeow_errors::Diag::from)?;
    let (program, diagnostics) =
        parse_clif_str(clif, Some(format!("{GENEALOGY}kb"))).map_err(gmeow_errors::Diag::from)?;
    let dataset = purrdf::parse_dataset(edb, purrdf::NativeRdfFormat::NQuads.media_type(), None)
        .map_err(gmeow_errors::Diag::from)?;
    let limits = MaterializationLimits::default();
    let plain = materialize_program(&program, &dataset, limits, None)
        .map(|result| result.quads)
        .map_err(super::record_failure);
    let parent = format!("{GENEALOGY}parent");
    let alice = format!("<{GENEALOGY}alice>");
    let bob = format!("<{GENEALOGY}bob>");
    let carol = format!("<{GENEALOGY}carol>");
    let annotated = materialize_program_annotated(
        &program,
        &dataset,
        limits,
        None,
        AnnotationRequest::new(
            &ZWeightSemiring,
            &AnnotationContract::exact(),
            |fact: AnnotationFactRef<'_>| {
                if fact.predicate != parent {
                    return None;
                }
                match (term_display(fact.subject), term_display(fact.object)) {
                    (s, o) if s == alice && o == bob => Some(2),
                    (s, o) if s == bob && o == carol => Some(3),
                    _ => None,
                }
            },
        ),
    )
    .map(|result| Annotated {
        certification: result.certification,
        quads: result.quads,
    })
    .map_err(super::record_failure);
    Ok(Observation {
        diagnostics,
        plain,
        annotated,
    })
}
