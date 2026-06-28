// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The caller for the oxigraph-free correspondence lowerings.
//!
//! The SSSOM / FnO / EDOAL / SPARQL alignment artifacts are now produced by the
//! wasm-clean `gmeow-logic-compile` correspondence lowerings, not by the historical
//! oxigraph-coupled `gmeow-slice` emitters. This module is the file-reading edge: it
//! natively parses the DSL + ontology + metadata sources into `RdfDataset`s (via the
//! oxigraph-free `gmeow-rdf` codecs) and drives the four lowerings. EDOAL + SPARQL
//! lower from one shared get-leg model, so the historical `spec-drift` invariant is
//! gone by construction.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_logic_compile::ingest::DslView;
use gmeow_logic_compile::projections::{edoal, fno, sparql, sssom, ProjectionResult};
use gmeow_rdf::dataset_view::{DatasetView, GraphMatch};
use gmeow_rdf::{
    parse_dataset, NativeRdfFormat, RdfDataset, RdfDatasetBuilder, TermRef, TermValue,
};
use gmeow_slice::{ArtifactRole, SliceCatalog, SliceError};

const GM_VERSION_FINGERPRINT: &str = "https://blackcatinformatics.ca/gmeow/versionFingerprint";
const GM_DATE_PUBLISHED: &str = "https://blackcatinformatics.ca/gmeow/datePublished";

/// All four alignment dialects' outputs, keyed by bare file name within each
/// generated directory.
pub struct CorrespondenceArtifacts {
    /// `<name>.sssom.tsv` → TSV.
    pub sssom: BTreeMap<String, String>,
    /// The single FnO catalog N-Triples text.
    pub fno: String,
    /// `<profile>.edoal.ttl` → Turtle.
    pub edoal: BTreeMap<String, String>,
    /// `<profile>.rq` → SPARQL CONSTRUCT.
    pub sparql: BTreeMap<String, String>,
    /// The per-correspondence loss ledger aggregated across all four dialects — one
    /// `ProjectionResult` per correspondence per dialect that drops something. The
    /// mappings stage unions this with the logic projection rows and serializes the
    /// final `generated/logic/projection-report.ttl` (the loss ledger is the residue
    /// set, per LOGIC-CORRESPONDENCE.md).
    pub ledger: Vec<ProjectionResult>,
}

/// Lower every alignment dialect from the sources under `root`.
pub fn lower_all(root: &Path) -> Result<CorrespondenceArtifacts, SliceError> {
    // Discover the slice catalog once and share it across both merges (the DSL
    // `Mapping` artifacts and the ontology `Module` artifacts), rather than
    // rescanning the `slices/` tree per role.
    let slices_dir = root.join("slices");
    let catalog = if slices_dir.is_dir() {
        Some(SliceCatalog::discover(&slices_dir)?)
    } else {
        None
    };
    let dsl = merge_dsl(root, catalog.as_ref())?;
    let onto = merge_ontology(root, catalog.as_ref())?;
    let dsl_view = DslView::new(&dsl);
    let onto_view = DslView::new(&onto);
    let (version, release_date) = read_self_metadata(root)?;

    let sssom =
        sssom::lower_sssom(&dsl_view, &version, &release_date).map_err(SliceError::Parse)?;
    let fno = fno::lower_fno(&dsl_view, &onto_view).map_err(SliceError::Parse)?;
    let edoal = edoal::lower_edoal(&dsl_view, &onto_view).map_err(SliceError::Parse)?;
    let sparql = sparql::lower_sparql(&dsl_view, &onto_view).map_err(SliceError::Parse)?;

    // Aggregate the per-correspondence ledger across all four dialects. Each dialect
    // already attributes its residue to the dropping (get) leg.
    let mut ledger: Vec<ProjectionResult> = Vec::new();
    ledger.extend(sssom.ledger);
    ledger.extend(fno.ledger);
    ledger.extend(edoal.ledger);
    ledger.extend(sparql.ledger);

    Ok(CorrespondenceArtifacts {
        sssom: sssom.sets,
        fno: fno.catalog,
        edoal: edoal.alignments,
        sparql: sparql.queries,
        ledger,
    })
}

fn parse_turtle(bytes: &[u8], context: &str) -> Result<Arc<RdfDataset>, SliceError> {
    parse_dataset(bytes, NativeRdfFormat::Turtle.media_type(), None)
        .map_err(|e| SliceError::Parse(format!("{context}: {e}")))
}

fn collect_ttl_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), SliceError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).map_err(SliceError::Io)? {
        let path = entry.map_err(SliceError::Io)?.path();
        if path.is_dir() {
            collect_ttl_files(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("ttl") {
            out.push(path);
        }
    }
    Ok(())
}

fn merge_slice_artifacts(
    catalog: Option<&SliceCatalog>,
    role: ArtifactRole,
    b: &mut RdfDatasetBuilder,
) -> Result<(), SliceError> {
    let Some(catalog) = catalog else {
        return Ok(());
    };
    // Borrow the artifact bytes (no clone): the catalog outlives this merge.
    let mut artifacts: Vec<(PathBuf, &[u8])> = Vec::new();
    for record in catalog.records() {
        for artifact in &record.artifacts {
            if artifact.role == role {
                artifacts.push((
                    record.slice_dir.join(&artifact.logical_path),
                    &artifact.content,
                ));
            }
        }
    }
    artifacts.sort_by(|a, c| a.0.cmp(&c.0));
    for (path, bytes) in &artifacts {
        let ds = parse_turtle(bytes, &path.display().to_string())?;
        b.push_dataset(&ds);
    }
    Ok(())
}

/// The DSL source set (functions + cells): the sorted `dsl/mappings/**/*.ttl` tree,
/// then the sorted slice `Mapping` artifacts — the same order the historical store
/// loaded them, so collisions resolve identically.
fn merge_dsl(root: &Path, catalog: Option<&SliceCatalog>) -> Result<Arc<RdfDataset>, SliceError> {
    let mut b = RdfDatasetBuilder::new();
    let mut files = Vec::new();
    collect_ttl_files(&root.join("dsl").join("mappings"), &mut files)?;
    files.sort();
    for path in &files {
        let bytes = std::fs::read(path).map_err(SliceError::Io)?;
        let ds = parse_turtle(&bytes, &path.display().to_string())?;
        b.push_dataset(&ds);
    }
    merge_slice_artifacts(catalog, ArtifactRole::Mapping, &mut b)?;
    b.freeze().map_err(|e| SliceError::Parse(e.to_string()))
}

/// The ontology source set (`rdfs:range` / suppression vocabulary / language tags):
/// `ontology/gmeow.ttl`, then the sorted slice `Module` artifacts.
fn merge_ontology(
    root: &Path,
    catalog: Option<&SliceCatalog>,
) -> Result<Arc<RdfDataset>, SliceError> {
    let mut b = RdfDatasetBuilder::new();
    let onto = root.join("ontology").join("gmeow.ttl");
    if onto.is_file() {
        let bytes = std::fs::read(&onto).map_err(SliceError::Io)?;
        let ds = parse_turtle(&bytes, "ontology/gmeow.ttl")?;
        b.push_dataset(&ds);
    }
    merge_slice_artifacts(catalog, ArtifactRole::Module, &mut b)?;
    b.freeze().map_err(|e| SliceError::Parse(e.to_string()))
}

/// Read `(version, release_date)` from `metadata/gmeow-self.ttl` (the Manifestation is
/// the subject of `gmeow:versionFingerprint`; its `gmeow:datePublished` is the date).
fn read_self_metadata(root: &Path) -> Result<(String, String), SliceError> {
    let bytes =
        std::fs::read(root.join("metadata").join("gmeow-self.ttl")).map_err(SliceError::Io)?;
    let ds = parse_turtle(&bytes, "metadata/gmeow-self.ttl")?;
    let vfp = ds
        .term_id_by_value(&TermValue::Iri(GM_VERSION_FINGERPRINT.to_owned()))
        .ok_or_else(|| {
            SliceError::Parse("gmeow-self.ttl: no gmeow:versionFingerprint predicate".to_owned())
        })?;
    let manifestation = ds
        .quads_for_pattern(None, Some(vfp), None, GraphMatch::Default)
        .next()
        .ok_or_else(|| {
            SliceError::Parse("gmeow-self.ttl: no manifestation with versionFingerprint".to_owned())
        })?
        .s;
    let subject_iri = match ds.resolve(manifestation) {
        TermRef::Iri(iri) => iri.to_owned(),
        _ => {
            return Err(SliceError::Parse(
                "gmeow-self.ttl: versionFingerprint subject is not an IRI".to_owned(),
            ))
        }
    };
    let view = DslView::new(&ds);
    let version = view
        .object_literal(&subject_iri, GM_VERSION_FINGERPRINT)
        .ok_or_else(|| {
            SliceError::Parse("gmeow-self.ttl: missing versionFingerprint".to_owned())
        })?;
    let release_date = view
        .object_literal(&subject_iri, GM_DATE_PUBLISHED)
        .ok_or_else(|| SliceError::Parse("gmeow-self.ttl: missing datePublished".to_owned()))?;
    Ok((version, release_date))
}
