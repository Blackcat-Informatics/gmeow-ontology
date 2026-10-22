// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit ontology-owned translation inputs. Slice ontologies retain their
//! actual manifest identity; each registered DSL is a separate ontology, never a
//! fabricated slice. No other repository directory acquires catalog authority.

use purrdf::slice::{ArtifactRole, SliceCatalog};
use std::path::{Path, PathBuf};

pub const MAPPING_DSL_OWNER: &str = "https://blackcatinformatics.ca/gmeow/mapping-dsl";
pub const MAPPING_DSL_SOURCES: [&str; 2] = [
    "dsl/mappings/vocabulary.ttl",
    "dsl/mappings/catalog-families.ttl",
];

pub const TEST_DSL_OWNER: &str = "https://blackcatinformatics.ca/gmeow/test-dsl";
pub const TEST_DSL_SOURCES: [&str; 1] = ["dsl/tests/vocabulary.ttl"];

/// One explicitly registered ontology source and catalog directory.
pub struct DslCatalogUnit {
    /// Human-readable owner name for input diagnostics.
    pub name: &'static str,
    /// Canonical ontology identity admitted from the vocabulary.
    pub owner_iri: &'static str,
    /// Repository-relative directory holding the ontology and its catalogs.
    pub directory: &'static str,
    /// All mandatory authored sources of this ontology.
    pub sources: &'static [&'static str],
}

/// The complete selected non-slice catalog owner inventory.
pub const DSL_CATALOG_UNITS: &[DslCatalogUnit] = &[
    DslCatalogUnit {
        name: "mapping-DSL",
        owner_iri: MAPPING_DSL_OWNER,
        directory: "dsl/mappings",
        sources: &MAPPING_DSL_SOURCES,
    },
    DslCatalogUnit {
        name: "test-DSL",
        owner_iri: TEST_DSL_OWNER,
        directory: "dsl/tests",
        sources: &TEST_DSL_SOURCES,
    },
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

/// Resolve authored inputs of every explicitly selected non-slice ontology.
/// Fully absent owners are outside the operation. Any partial owner refuses.
pub fn dsl_sources(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for unit in DSL_CATALOG_UNITS {
        paths.extend(admit_dsl_unit(root, unit)?);
    }
    Ok(paths)
}

/// Exact authored sources and PO catalogs of every selected registered DSL.
/// Fully absent owners contribute nothing; partial or malformed owners refuse.
pub fn dsl_input_files(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for unit in DSL_CATALOG_UNITS {
        let sources = admit_dsl_unit(root, unit)?;
        if sources.is_empty() {
            continue;
        }
        paths.extend(sources);
        paths.extend(dsl_catalog_paths(root, unit)?);
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn dsl_catalog_paths(root: &Path, unit: &DslCatalogUnit) -> gmeow_errors::Result<Vec<PathBuf>> {
    Ok(
        crate::i18n_compile::directory_entries(&root.join(unit.directory).join("i18n"))?
            .into_iter()
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("po"))
            .collect(),
    )
}

fn admit_dsl_unit(root: &Path, unit: &DslCatalogUnit) -> gmeow_errors::Result<Vec<PathBuf>> {
    let vocabulary = root.join(unit.sources[0]);
    let directory = root.join(unit.directory).join("i18n");
    match std::fs::metadata(&vocabulary) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut selected = directory.try_exists()?;
            for source in &unit.sources[1..] {
                selected |= root.join(source).try_exists()?;
            }
            if selected {
                return Err(invalid(format!(
                    "selected {} sources or catalogs have no required vocabulary owner",
                    unit.name
                )));
            }
            return Ok(Vec::new());
        }
        Err(error) => return Err(error.into()),
        Ok(metadata) if !metadata.is_file() => {
            return Err(invalid(format!("{} vocabulary is not a file", unit.name)));
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
    if owners != [purrdf::RdfTerm::Iri(unit.owner_iri.to_owned())] {
        return Err(invalid(format!(
            "{} vocabulary requires exactly its canonical ontology owner declaration",
            unit.name
        )));
    }
    let sources: Vec<_> = unit.sources.iter().map(|path| root.join(path)).collect();
    for source in &sources {
        if !std::fs::metadata(source)?.is_file() {
            return Err(invalid(format!(
                "required {} source {} is not a file",
                unit.name,
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

/// Extend an existing repository inventory with registered non-slice
/// ontologies. Every consumer uses these same owner/artifact records.
pub fn for_repository(
    root: &Path,
    catalog: &SliceCatalog,
) -> gmeow_errors::Result<Vec<OntologyCatalogSource>> {
    let mut sources = from_slice_catalog(catalog);
    append_dsl_catalogs(root, &mut sources)?;
    sources.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(sources)
}

fn append_dsl_catalogs(
    root: &Path,
    sources: &mut Vec<OntologyCatalogSource>,
) -> gmeow_errors::Result<()> {
    for unit in DSL_CATALOG_UNITS {
        if admit_dsl_unit(root, unit)?.is_empty() {
            continue;
        }
        let owner_path = root.join(unit.directory);
        for path in dsl_catalog_paths(root, unit)? {
            sources.push(OntologyCatalogSource {
                owner_iri: unit.owner_iri.to_owned(),
                owner_path: owner_path.clone(),
                content: std::fs::read(&path).map_err(|error| {
                    crate::i18n_compile::source_io_error(&path, "read catalog", error)
                })?,
                path,
            });
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
    append_dsl_catalogs(root, &mut sources)?;
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
