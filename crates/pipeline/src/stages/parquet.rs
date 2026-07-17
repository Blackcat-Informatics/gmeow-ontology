// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `parquet` export leaf (P4): columnar gts_db tables (dist/, gitignored).
//!
//! Projects the WHOLE composed carrier — terms/quads/reifiers/annotations/blobs
//! of every named graph, never scoped to one graph — into five Parquet files via
//! purrdf 0.7.0's native columnar codec (`purrdf::columnar::write`), the columnar
//! interchange form for DataFrame/SQL consumers (DuckDB, pandas, polars, Spark)
//! who should not need an RDF parser. LOSSLESS: the five tables jointly carry
//! every term, quad, reifier binding, statement annotation, and blob purrdf's
//! codec is given.
//!
//! purrdf owns the dictionary encoding, the Parquet Data Page V2 physical
//! layout, and the compression; gmeow owns only the carrier→`DatasetView` hookup,
//! the (always-empty) blob store — the carrier transport is RDF only, blob
//! payloads live in the gts archive by reference, never in the in-memory
//! carrier — and the `dist/parquet/<table>.parquet` path mapping. Retires
//! gmeow's hand-rolled Arrow record-batch builders and `parquet`-crate writer
//! (the former genuine port of the gts_db table dump).
//!
//! Outputs live under git-ignored `dist/parquet/`, so the bar is structural
//! validity + round-trip fidelity + determinism (purrdf's codec is
//! byte-deterministic by construction — fixed dictionary order, fixed row
//! order), not byte-parity across library versions.

use std::collections::BTreeMap;

use purrdf::columnar::{Compression, Table};
use purrdf::{ContentStore, RdfDataset};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::export::read_fold_upstream;

/// The generator's output directory (under the git-ignored dist/ tree).
pub const PARQUET_DIR: &str = "dist/parquet";

fn err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-export-parquet".into(),
        message: message.into(),
    })
}

/// Report purrdf's runtime [`purrdf::LossLedger`] as ONE tracing event (target
/// `parquet_loss`) per distinct `(code, note)` so no RDF→columnar projection
/// loss is ever silently dropped. Columnar projection is lossless by
/// construction (see [`purrdf::columnar::write`]'s doc), so this currently
/// never fires — it exists so a future loss is observable rather than silent.
fn report_parquet_losses(ledger: &purrdf::LossLedger) {
    let mut grouped: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();
    for loss in ledger.entries() {
        let subject = loss
            .location
            .as_deref()
            .and_then(|location| location.subject.as_deref())
            .unwrap_or("<unlocated>");
        grouped
            .entry((loss.code.as_ref(), loss.note.as_ref()))
            .or_default()
            .push(subject);
    }
    for ((construct, reason), mut subjects) in grouped {
        subjects.sort_unstable();
        subjects.dedup();
        let examples = subjects
            .iter()
            .take(5)
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if subjects.len() > 5 {
            format!(" (+{} more)", subjects.len() - 5)
        } else {
            String::new()
        };
        tracing::info!(
            target: "parquet_loss",
            construct = construct,
            subjects = subjects.len(),
            reason = reason,
            examples = %format!("{examples}{suffix}"),
            "lossy drop projecting the carrier RDF to the columnar Parquet surface",
        );
    }
}

/// Render the five-table Parquet projection of the WHOLE carrier `dataset` via
/// purrdf's columnar codec. All five `dist/parquet/<table>.parquet` files are
/// always produced (including valid zero-row files, e.g. `blobs.parquet` — the
/// carrier is RDF-only in memory, so blobs is always empty today) — purrdf's
/// closed `Table::ALL` set never omits an empty table.
pub(crate) fn render_parquet(
    dataset: &RdfDataset,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    // The carrier transport is RDF only: blob payloads live in the gts archive
    // by reference, never in the in-memory dataset — an empty content store
    // reproduces that today, and purrdf hard-fails digest verification rather
    // than silently degrading if that ever stops being true.
    let blobs = ContentStore::new();
    let written = purrdf::columnar::write(dataset, &blobs, Compression::Zstd)
        .map_err(|e| err(format!("columnar::write: {e}")))?;
    report_parquet_losses(&written.losses);

    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (table, bytes) in written.files.iter() {
        out.insert(
            format!("{PARQUET_DIR}/{}", table.file_name()),
            bytes.to_vec(),
        );
    }
    debug_assert_eq!(out.len(), Table::ALL.len());
    Ok(out)
}

// ── Stage impl ───────────────────────────────────────────────────────────────────

/// The `stage-export-parquet` export-leaf stage.
pub struct ParquetStage {
    consumes: Vec<String>,
}

impl ParquetStage {
    /// Construct the stage; it consumes THIS run's snapshot fold.
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-snapshot".to_string()],
        }
    }
}

impl Default for ParquetStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for ParquetStage {
    fn id(&self) -> &str {
        "stage-export-parquet"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "parquet.v1-purrdf"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // Consume THIS run's snapshot carrier dataset DIRECTLY off the product bundle —
        // no re-parse of the gmeow.gts bytes (GTS is exit-only).
        let dataset = read_fold_upstream(input.upstream)?;
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            render_parquet(dataset.as_ref())?,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::datasets_isomorphic;
    use std::path::Path;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    /// Every table is present, byte-deterministic across two renders, and the
    /// round trip (`purrdf::columnar::read` over the written files) reconstructs
    /// a dataset isomorphic to the source carrier — over the committed
    /// `generated/dist/gmeow.gts` carrier, the same production input this stage
    /// projects.
    #[test]
    fn parquet_tables_present_deterministic_and_round_trip() {
        let root = repo_root();
        let dataset = crate::stages::export::read_fold(&root).expect("read fold");
        let arts = render_parquet(dataset.as_ref()).expect("render");

        // All five tables are always produced (purrdf's closed Table::ALL set).
        for table in Table::ALL {
            let path = format!("{PARQUET_DIR}/{}", table.file_name());
            assert!(arts.contains_key(&path), "{path} must be produced");
            assert!(!arts[&path].is_empty(), "{path} must be non-empty bytes");
        }
        assert_eq!(arts.len(), Table::ALL.len());

        // Determinism: a second render is byte-identical per table.
        let arts2 = render_parquet(dataset.as_ref()).expect("render2");
        assert_eq!(arts, arts2, "parquet render is not deterministic");

        // Round trip: read the written files back and assert dataset isomorphism
        // with the source carrier — a non-tautological check that the codec did
        // not silently drop or corrupt terms/quads/reifiers/annotations.
        let files: [Vec<u8>; 5] =
            Table::ALL.map(|table| arts[&format!("{PARQUET_DIR}/{}", table.file_name())].clone());
        let read_back = purrdf::columnar::read(&purrdf::columnar::ParquetFiles::from_array(files))
            .expect("columnar::read round trip");
        assert!(
            read_back.losses.is_empty(),
            "round-trip read reported unexpected losses: {:?}",
            read_back.losses
        );
        assert_eq!(
            read_back.dataset.quad_count(),
            dataset.quad_count(),
            "round-tripped quad count must match the source carrier"
        );
        assert!(
            datasets_isomorphic(dataset.as_ref(), &read_back.dataset),
            "round-tripped dataset must be isomorphic to the source carrier"
        );
    }
}
