// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fixture registry selection, bindings and their authored rationale terms.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use purrdf::{
    RdfLiteral,
    slice::{ArtifactRecord, ArtifactRole},
};
use serde::{Deserialize, Serialize};

use crate::i18n::Translations;
use crate::i18n_compile::source_io_error;
use crate::store::Store;

use super::{DocsError, extract_example, local_name};

const REGISTRY_PATH: &str = "tests/example-conformance.ttl";
const CELL_TYPE: &str = "https://blackcatinformatics.ca/gmeow/ExampleConformance";
const EXAMPLE_FILE: &str = "https://blackcatinformatics.ca/gmeow/exampleFile";
const EXPECTED_OUTCOME: &str = "https://blackcatinformatics.ca/gmeow/expectedOutcome";
const EXPECTED_VIOLATION_CODE: &str = "https://blackcatinformatics.ca/gmeow/expectedViolationCode";
pub(crate) const RATIONALE: &str = "https://blackcatinformatics.ca/gmeow/conformanceRationale";
const ENGLISH_TAG: &str = "x-gmeow-english";

/// Select each slice's authored conformance registry for catalog admission and
/// documentation input keys.
///
/// # Errors
///
/// Reports source I/O failures during physical selection.
pub fn conformance_registry_files(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut directories = vec![root.join("slices")];
    while let Some(dir) = directories.pop() {
        // SliceCatalog stops at a manifest and walks real directory entries.
        // Select only the registry path without loading the other artifacts.
        let manifest = dir.join("manifest.ttl");
        if manifest
            .try_exists()
            .map_err(|error| source_io_error(&manifest, "locate slice manifest", error))?
        {
            if physical_file_type(&dir.join("tests"))?.is_some_and(|ty| ty.is_dir()) {
                let registry = dir.join(REGISTRY_PATH);
                if physical_file_type(&registry)?.is_some_and(|ty| ty.is_file()) {
                    files.push(registry);
                }
            }
            continue;
        }
        let entries = std::fs::read_dir(&dir)
            .map_err(|error| source_io_error(&dir, "read slice directory", error))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| source_io_error(&dir, "read slice directory entry", error))?;
            let path = entry.path();
            let kind = entry
                .file_type()
                .map_err(|error| source_io_error(&path, "read source type", error))?;
            if kind.is_dir() {
                directories.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn physical_file_type(path: &Path) -> gmeow_errors::Result<Option<std::fs::FileType>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata.file_type())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(source_io_error(path, "read source type", error)),
    }
}

pub(crate) fn is_registry(artifact: &ArtifactRecord) -> bool {
    artifact.role == ArtifactRole::TestDsl && artifact.logical_path == REGISTRY_PATH
}

/// The canonical English text of an undirected rationale is a gettext source.
fn english_text(literal: &RdfLiteral) -> Option<&str> {
    (literal.language.as_deref() == Some(ENGLISH_TAG) && literal.direction.is_none())
        .then_some(literal.lexical_form.as_str())
}

pub(crate) fn english_rationales(store: &Store) -> Result<Vec<(String, String)>, DocsError> {
    let mut values = Vec::new();
    for cell in store.subjects_of_type(CELL_TYPE) {
        for literal in store.rdf_literals(&cell, RATIONALE)? {
            if let Some(english) = english_text(&literal) {
                values.push((cell.clone(), english.to_owned()));
            }
        }
    }
    Ok(values)
}

/// Whether a fixture is a conforming instance or a counter-example.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DocFixtureKind {
    /// A fixture whose cell expects a conforming outcome.
    Wellformed,
    /// A fixture whose cell expects a violation.
    CounterExample,
}

/// A conformance fixture and the descriptive fields joined from its registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocFixture {
    /// Slice IRI that owns the fixture and its catalog.
    pub slice: String,
    /// Path within the slice directory.
    pub logical_path: String,
    /// Authored label or a title derived from the filename.
    pub title: String,
    /// Complete Turtle source of the fixture.
    pub text: String,
    /// Conforming instance or counter-example.
    pub kind: DocFixtureKind,
    /// GMEOW CURIEs referenced in the fixture.
    pub terms_referenced: Vec<String>,
    /// Local name of the declared `gmeow:expectedOutcome`, when bound.
    pub expected_outcome: Option<String>,
    /// Declared `gmeow:expectedViolationCode`, when supplied.
    pub violation_code: Option<String>,
    /// Authored rationale terms with their source cell identities.
    pub rationales: Vec<DocFixtureRationale>,
    /// Slug of the matching constraint-catalog rule, when resolved.
    pub catalog_slug: Option<String>,
}

/// A complete authored rationale literal from a named conformance cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocFixtureRationale {
    /// IRI of the source `gmeow:ExampleConformance` cell.
    pub cell: String,
    /// Exact authored RDF literal.
    #[serde(with = "super::literal_values::single")]
    pub literal: RdfLiteral,
}

impl DocFixtureRationale {
    pub(crate) fn translation<'a>(
        &self,
        slice: &str,
        translations: &'a Translations,
        language: &str,
    ) -> Option<&'a str> {
        translations.fixture_rationale(slice, &self.cell, english_text(&self.literal)?, language)
    }

    /// The reviewed locale text, or the authored lexical form for this cell.
    #[must_use]
    pub fn display_text<'a>(
        &'a self,
        slice: &str,
        translations: &'a Translations,
        language: &str,
    ) -> &'a str {
        self.translation(slice, translations, language)
            .unwrap_or(self.literal.lexical_form.as_str())
    }
}

pub(super) struct FixtureBinding {
    expected_outcome: Option<String>,
    violation_code: Option<String>,
    rationales: Vec<DocFixtureRationale>,
}

/// Join conformance cells by their slice-relative specimen path. Keep each
/// rationale and its cell together even when several cells cite one specimen.
pub(super) fn extract_bindings(
    store: &Store,
) -> Result<BTreeMap<String, FixtureBinding>, DocsError> {
    let mut out = BTreeMap::<String, FixtureBinding>::new();
    for cell in store.subjects_of_type(CELL_TYPE) {
        let Some(file) = store.first_literal(&cell, EXAMPLE_FILE) else {
            continue;
        };
        let expected_outcome = store
            .named_objects(&cell, EXPECTED_OUTCOME)
            .into_iter()
            .min()
            .map(|iri| local_name(&iri).to_owned());
        let violation_code = store.first_literal(&cell, EXPECTED_VIOLATION_CODE);
        let binding = out.entry(file).or_insert_with(|| FixtureBinding {
            expected_outcome: None,
            violation_code: None,
            rationales: Vec::new(),
        });
        binding.expected_outcome = expected_outcome;
        binding.violation_code = violation_code;
        binding
            .rationales
            .extend(
                store
                    .rdf_literals(&cell, RATIONALE)?
                    .into_iter()
                    .map(|literal| DocFixtureRationale {
                        cell: cell.clone(),
                        literal,
                    }),
            );
    }
    Ok(out)
}

pub(super) fn extract_fixture(
    artifact: &ArtifactRecord,
    owner_slice: &str,
    kind: DocFixtureKind,
    binding: Option<&FixtureBinding>,
) -> DocFixture {
    let example = extract_example(artifact, owner_slice);
    let (expected_outcome, violation_code, rationales) = match binding {
        Some(binding) => (
            binding.expected_outcome.clone(),
            binding.violation_code.clone(),
            binding.rationales.clone(),
        ),
        None => (None, None, Vec::new()),
    };
    DocFixture {
        slice: example.slice,
        logical_path: example.logical_path,
        title: example.title,
        text: example.text,
        kind,
        terms_referenced: example.terms_referenced,
        expected_outcome,
        violation_code,
        rationales,
        catalog_slug: None,
    }
}
