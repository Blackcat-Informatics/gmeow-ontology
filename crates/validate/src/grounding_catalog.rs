// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository controller for the explicit grounding-artifact authority input.
//! Logical namespaces and isDefinedBy assertions cannot promote an off-owner
//! mapping into this input collection.

use gmeow_logic_compile::projections::grounding_catalog::OwnedGroundingSource;
use purrdf::slice::{ArtifactRole, SliceCatalog};
use std::path::Path;

/// Reuse the actual discovered slice inventory and its original artifact bytes.
pub fn owned_sources(
    root: &Path,
    catalog: &SliceCatalog,
) -> gmeow_errors::Result<Vec<OwnedGroundingSource>> {
    let mut sources = Vec::new();
    for record in catalog.records() {
        let owner = record.manifest.slice_iri.as_str();
        if !matches!(
            owner,
            "https://blackcatinformatics.ca/gmeow/slices/lang"
                | "https://blackcatinformatics.ca/gmeow/slices/math"
                | "https://blackcatinformatics.ca/gmeow/slices/logic"
        ) {
            continue;
        }
        for artifact in &record.artifacts {
            if artifact.role != ArtifactRole::Mapping {
                continue;
            }
            let path = record.slice_dir.join(&artifact.logical_path);
            let relative = path.strip_prefix(root).map_err(|why| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    detail: format!(
                        "grounding catalog artifact {} is outside repository: {why}",
                        path.display()
                    ),
                })
            })?;
            sources.push(OwnedGroundingSource::new(
                owner,
                &relative.to_string_lossy().replace('\\', "/"),
                &artifact.content,
            )?);
        }
    }
    Ok(sources)
}

/// Standalone repository callers must discover the same physical slice roles;
/// they cannot substitute a surface-local DSL union for a required catalog.
pub fn discover(root: &Path) -> gmeow_errors::Result<Vec<OwnedGroundingSource>> {
    let catalog = SliceCatalog::discover(&root.join("slices"), gmeow_ns::gmeow_slice_vocab())
        .map_err(|why| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                detail: format!("required grounding slice catalog: {why}"),
            })
        })?;
    owned_sources(root, &catalog)
}

/// Load the same canonical module-role facts and owned mapping artifacts for a
/// repository consumer. Domain DSL files and example A-Boxes are not authority.
pub fn load(
    root: &Path,
) -> gmeow_errors::Result<gmeow_logic_compile::projections::grounding_catalog::GroundingCatalog> {
    let catalog = SliceCatalog::discover(&root.join("slices"), gmeow_ns::gmeow_slice_vocab())
        .map_err(|why| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                detail: format!("required grounding slice catalog: {why}"),
            })
        })?;
    from_catalog(root, &catalog)
}

/// Reuse an existing actual slice inventory rather than rediscovering it per
/// measured surface. Every source module retains its normal native parse.
pub fn from_catalog(
    root: &Path,
    catalog: &SliceCatalog,
) -> gmeow_errors::Result<gmeow_logic_compile::projections::grounding_catalog::GroundingCatalog> {
    let owned = owned_sources(root, catalog)?;
    let mut modules = vec![crate::store::parse_file_dataset(
        &root.join("ontology/gmeow.ttl"),
    )?];
    for record in catalog.records() {
        for artifact in &record.artifacts {
            if artifact.role == ArtifactRole::Module && artifact.logical_path == "module.ttl" {
                modules.push(purrdf::parse_dataset(
                    &artifact.content,
                    "text/turtle",
                    None,
                )?);
            }
        }
    }
    let refs: Vec<_> = modules.iter().map(AsRef::as_ref).collect();
    let ontology = purrdf::RdfDataset::union(&refs)?;
    gmeow_logic_compile::projections::grounding_catalog::GroundingCatalog::from_sources(
        &owned,
        &gmeow_logic_compile::ingest::DslView::new(&ontology),
    )
}
