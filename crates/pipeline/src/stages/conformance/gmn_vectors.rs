// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete frozen GMN vector execution, sharing the producer's native codebook.
//! Per-source actions exclude goldens; authenticated consumers own their comparison.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_action_cache::{
    ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits, bytes_digest,
};
use gmeow_lang_bridge::gmn_conformance::{PositiveObservation, observe_positive};
use gmeow_lang_bridge::{Gmn0Model, Gmn1Document, codebook_digest, gmn1_read, gmn1_write};
use purrdf::{NativeRdfFormat, RdfTerm, parse_dataset};
use serde::{Deserialize, Serialize};

use crate::stages::parse_sources::SourceCatalog;

pub(crate) const CHANNEL: &str = "pipeline/gmn-vector-observations.json";
pub(super) const ROOT: &str = "slices/grounding/lang/tests/gmn1-vectors";
const LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const DIGEST: &str = "https://blackcatinformatics.ca/gmeow/gmnCodebookDigest";
const CLASS: &str = "https://blackcatinformatics.ca/gmeow/enforcesFailureClass";

#[derive(Serialize, Deserialize)]
pub(super) struct Observation {
    pub codebook_digest: String,
    pub declared_digests: BTreeSet<String>,
    pub manifest_stems: BTreeSet<String>,
    pub positives: BTreeMap<String, Result<PositiveObservation, gmeow_errors::RecordedDiag>>,
    pub expected_negatives: BTreeMap<String, String>,
    pub negatives: BTreeMap<String, Result<Option<String>, gmeow_errors::RecordedDiag>>,
}

pub(super) fn input_files(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    super::execution::collect_files(&root.join(ROOT), &mut files)?;
    files.sort();
    Ok(files)
}

fn fail(error: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::stage_err(&error.to_string())
}

fn raw(path: &str, digest: String) -> ActionInput {
    ActionInput::Raw {
        logical_path: path.to_owned(),
        file_kind: FileKind::File,
        executable: false,
        digest,
    }
}

pub(super) fn record(
    root: &Path,
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let language = catalog.language()?;
    let dictionary = &language.dictionary;
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(fail)?;
    let manifest = std::fs::read(root.join(ROOT).join("vector-manifest.ttl")).map_err(fail)?;
    let manifest =
        parse_dataset(&manifest, NativeRdfFormat::Turtle.media_type(), None).map_err(fail)?;
    let mut manifest_stems = BTreeSet::new();
    let mut declared_digests = BTreeSet::new();
    for quad in manifest.owned_quads() {
        if let RdfTerm::Literal(literal) = quad.object {
            if quad.predicate == DIGEST {
                declared_digests.insert(literal.lexical_form.clone());
            }
            if quad.predicate == LABEL
                && let Some((stem, rest)) = literal.lexical_form.split_once(".in.ttl -> ")
                && rest == format!("{stem}.gmn")
            {
                manifest_stems.insert(stem.to_owned());
            }
        }
    }
    let expected =
        std::fs::read(root.join(ROOT).join("negative-codec/expected.ttl")).map_err(fail)?;
    let expected =
        parse_dataset(&expected, NativeRdfFormat::Turtle.media_type(), None).map_err(fail)?;
    let mut labels = BTreeMap::new();
    let mut classes = BTreeMap::new();
    for quad in expected.owned_quads() {
        let RdfTerm::BlankNode(subject) = quad.subject else {
            continue;
        };
        match (quad.predicate.as_str(), quad.object) {
            (LABEL, RdfTerm::Literal(literal)) => {
                labels.insert(subject, literal.lexical_form);
            }
            (CLASS, RdfTerm::Iri(class)) => {
                classes.insert(subject, class);
            }
            _ => {}
        }
    }
    let expected_negatives: BTreeMap<_, _> = labels
        .into_iter()
        .map(|(subject, filename)| {
            let class = classes
                .get(&subject)
                .ok_or_else(|| fail(format!("negative {filename} has no expected class")))?;
            Ok((filename, class.clone()))
        })
        .collect::<gmeow_errors::Result<_>>()?;
    if manifest_stems.is_empty() || expected_negatives.is_empty() {
        return Err(fail(
            "frozen GMN vectors require nonempty positive and negative inventories",
        ));
    }
    let language_source = gmeow_lang_bridge::gmn1_codec::native::SOURCE_PATH;
    let language_input = raw(
        language_source,
        catalog.document_digest(language_source)?.to_owned(),
    );
    let mut positives = BTreeMap::new();
    for path in input_files(root)? {
        if path.parent() != Some(root.join(ROOT).as_path()) {
            continue;
        }
        let Some(stem) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".in.ttl"))
        else {
            continue;
        };
        let bytes = std::fs::read(&path).map_err(fail)?;
        let observed = super::execution::cached_inputs(
            &store,
            "gmn-vector-positive-v1",
            vec![
                raw(
                    &path.strip_prefix(root).map_err(fail)?.to_string_lossy(),
                    bytes_digest(&bytes),
                ),
                language_input.clone(),
            ],
            || {
                let dataset = parse_dataset(&bytes, NativeRdfFormat::Turtle.media_type(), None)
                    .map_err(gmeow_errors::Diag::from)?;
                observe_positive(&Gmn0Model::from_dataset(&dataset), dictionary)
            },
        )
        .map_err(super::record_failure);
        positives.insert(stem.to_owned(), observed);
    }
    let mut negatives = BTreeMap::new();
    for filename in expected_negatives.keys() {
        let path = format!("{ROOT}/negative-codec/{filename}");
        let bytes = std::fs::read(root.join(&path)).map_err(fail)?;
        let observed = super::execution::cached_inputs(
            &store,
            "gmn-vector-negative-v1",
            vec![raw(&path, bytes_digest(&bytes)), language_input.clone()],
            || {
                let failure = if filename.ends_with(".gmn") {
                    let text = std::str::from_utf8(&bytes).map_err(gmeow_errors::Diag::from)?;
                    gmn1_read(&Gmn1Document::from_text(text), dictionary).err()
                } else {
                    let dataset = parse_dataset(&bytes, NativeRdfFormat::Turtle.media_type(), None)
                        .map_err(gmeow_errors::Diag::from)?;
                    gmn1_write(&Gmn0Model::from_dataset(&dataset), dictionary).err()
                };
                Ok(failure.map(|error| error.failure_class().to_owned()))
            },
        )
        .map_err(super::record_failure);
        negatives.insert(filename.clone(), observed);
    }
    let observed = Observation {
        codebook_digest: codebook_digest(&language.codebook, dictionary),
        declared_digests,
        manifest_stems,
        positives,
        expected_negatives,
        negatives,
    };
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observed).map_err(fail)?,
    );
    Ok(())
}
