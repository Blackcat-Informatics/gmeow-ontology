// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit ontology-owned translation inputs. Slice ontologies retain their
//! actual manifest identity; the mapping DSL is a separate ontology, never a
//! fabricated slice. No other repository directory acquires catalog authority.

use purrdf::slice::{ArtifactRole, SliceCatalog};
use std::path::{Path, PathBuf};

pub const MAPPING_DSL_OWNER: &str = "https://blackcatinformatics.ca/gmeow/mapping-dsl";
pub const MAPPING_DSL_SOURCES: [&str; 2] = [
    "dsl/mappings/vocabulary.ttl",
    "dsl/mappings/catalog-families.ttl",
];

/// One original catalog artifact and its explicit owning ontology.
#[derive(Debug, Clone)]
pub struct OntologyCatalogSource {
    pub owner_iri: String,
    pub owner_path: PathBuf,
    pub path: PathBuf,
    pub content: Vec<u8>,
}

/// An explicitly supplied slice inventory selects only its actual slice owners.
pub fn from_slice_catalog(catalog: &SliceCatalog) -> Vec<OntologyCatalogSource> {
    let mut sources = Vec::new();
    for record in catalog.records() {
        for artifact in &record.artifacts {
            if artifact.role == ArtifactRole::TranslationCatalog {
                sources.push(OntologyCatalogSource {
                    owner_iri: record.manifest.slice_iri.clone(),
                    owner_path: record.slice_dir.clone(),
                    path: record.slice_dir.join(&artifact.logical_path),
                    content: artifact.content.clone(),
                });
            }
        }
    }
    sources.sort_by(|a, b| a.path.cmp(&b.path));
    sources
}

/// Admit the explicitly registered mapping-DSL ontology when it is part of the
/// selected repository. An existing owner requires both source files and its
/// catalog directory; absent files cannot silently turn it into slice-only mode.
pub fn mapping_dsl_sources(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let vocabulary = root.join(MAPPING_DSL_SOURCES[0]);
    let directory = root.join("dsl/mappings/i18n");
    match std::fs::metadata(&vocabulary) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if directory.try_exists()? || root.join(MAPPING_DSL_SOURCES[1]).try_exists()? {
                return Err(invalid(
                    "selected mapping-DSL sources or catalogs have no required vocabulary owner",
                ));
            }
            return Ok(Vec::new());
        }
        Err(error) => return Err(error.into()),
        Ok(metadata) if !metadata.is_file() => {
            return Err(invalid("mapping-DSL vocabulary is not a file"));
        }
        Ok(_) => {}
    }
    let bytes = std::fs::read(&vocabulary)?;
    let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)?;
    let owners: Vec<_> = dataset
        .owned_quads()
        .filter(|quad| {
            quad.predicate == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                && quad.object
                    == purrdf::RdfTerm::Iri(
                        "https://blackcatinformatics.ca/logic/Ontology".to_owned(),
                    )
        })
        .map(|quad| quad.subject)
        .collect();
    if owners != [purrdf::RdfTerm::Iri(MAPPING_DSL_OWNER.to_owned())] {
        return Err(invalid(
            "mapping-DSL vocabulary requires exactly its canonical ontology owner declaration",
        ));
    }
    let sources: Vec<_> = MAPPING_DSL_SOURCES
        .iter()
        .map(|path| root.join(path))
        .collect();
    for source in &sources {
        if !std::fs::metadata(source)?.is_file() {
            return Err(invalid(format!(
                "required mapping-DSL source {} is not a file",
                source.display()
            )));
        }
    }
    // Establish the selected catalog capability even if its current inventory is empty.
    let _catalog_directory = std::fs::read_dir(&directory).map_err(|error| {
        crate::i18n_compile::source_io_error(&directory, "read catalog directory", error)
    })?;
    Ok(sources)
}

/// Extend an existing repository inventory with the one registered non-slice
/// ontology. Every consumer uses these same owner/artifact records.
pub fn for_repository(
    root: &Path,
    catalog: &SliceCatalog,
) -> gmeow_errors::Result<Vec<OntologyCatalogSource>> {
    let mut sources = from_slice_catalog(catalog);
    append_mapping_dsl(root, &mut sources)?;
    sources.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(sources)
}

fn append_mapping_dsl(
    root: &Path,
    sources: &mut Vec<OntologyCatalogSource>,
) -> gmeow_errors::Result<()> {
    if !mapping_dsl_sources(root)?.is_empty() {
        let owner_path = root.join("dsl/mappings");
        for path in crate::i18n_compile::directory_entries(&owner_path.join("i18n"))? {
            if path.extension().and_then(|value| value.to_str()) == Some("po") {
                sources.push(OntologyCatalogSource {
                    owner_iri: MAPPING_DSL_OWNER.to_owned(),
                    owner_path: owner_path.clone(),
                    content: std::fs::read(&path).map_err(|error| {
                        crate::i18n_compile::source_io_error(&path, "read catalog", error)
                    })?,
                    path,
                });
            }
        }
    }
    Ok(())
}

/// Repository discovery is a controller, never a second PO parser.
pub fn discover(root: &Path) -> gmeow_errors::Result<Vec<OntologyCatalogSource>> {
    let mut sources = Vec::new();
    for owner_path in crate::i18n_compile::slice_directories(root)? {
        let directory = owner_path.join("i18n");
        if !directory.try_exists()? {
            continue;
        }
        let paths = crate::i18n_compile::directory_entries(&directory)?;
        let manifest = gmeow_validate::store::parse_file_dataset(&owner_path.join("manifest.ttl"))?;
        let owners: Vec<_> = manifest
            .owned_quads()
            .filter(|quad| {
                quad.predicate == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                    && quad.object
                        == purrdf::RdfTerm::Iri(
                            "https://blackcatinformatics.ca/gmeow/Slice".to_owned(),
                        )
            })
            .map(|quad| quad.subject)
            .collect();
        let [purrdf::RdfTerm::Iri(owner)] = owners.as_slice() else {
            return Err(invalid(format!(
                "{} must declare exactly one named translation-catalog ontology owner",
                owner_path.display()
            )));
        };
        for path in paths {
            if path.extension().and_then(|value| value.to_str()) == Some("po") {
                sources.push(OntologyCatalogSource {
                    owner_iri: owner.clone(),
                    owner_path: owner_path.clone(),
                    content: std::fs::read(&path).map_err(|error| {
                        crate::i18n_compile::source_io_error(&path, "read catalog", error)
                    })?,
                    path,
                });
            }
        }
    }
    append_mapping_dsl(root, &mut sources)?;
    sources.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(sources)
}

fn invalid(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::CatalogInconsistent {
        detail: detail.into(),
    })
}

#[cfg(test)]
#[path = "ontology_catalogs.tests.rs"]
mod tests;
