// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `parquet` export leaf (#861 P4): columnar gts_db tables (dist/, gitignored).
//!
//! A genuine Rust port of `src/gmeow_tools/parquet_gen.py` + the relational
//! projection in `src/gmeow_tools/gts_db.py` (#377, #12). Projects the folded gts
//! `Graph` into one Parquet file per non-empty table of the dictionary-encoded
//! integer-id schema — `terms`, `quads`, `reifiers`, `annotations`, `blobs` — the
//! columnar interchange form for DataFrame/SQL consumers (DuckDB, pandas, polars,
//! Spark) who should not need an RDF parser. LOSSLESS: the tables jointly carry
//! every term, quad, reifier binding, statement annotation, and inline blob.
//!
//! The Python path loads the rows into an in-memory DuckDB and `COPY`-exports; this
//! port builds Arrow record batches in the SAME enumerate-order (`graph.terms` /
//! `graph.quads` / …) so ids are stable, and serializes them with the `parquet`
//! crate's writer. Outputs live under git-ignored `dist/parquet/`, so the bar is
//! structural validity (re-reads with the expected row counts) + determinism
//! (fixed row order), not byte-parity — Parquet bytes embed writer metadata that
//! varies across library versions (the Python `compare` gates SEMANTICALLY).

use std::collections::BTreeMap;
use std::sync::Arc;

use arrow_array::{ArrayRef, BinaryArray, Int64Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use gmeow_gts::model::{Graph, TermKind};
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};
use crate::stages::export::read_fold_upstream;

/// The generator's output directory (under the git-ignored dist/ tree).
pub const PARQUET_DIR: &str = "dist/parquet";

/// The five relational tables, in dependency-free (gts_db `_INSERTS`) order.
const TABLES: &[&str] = &["terms", "quads", "reifiers", "annotations", "blobs"];

fn term_kind_int(kind: TermKind) -> i64 {
    match kind {
        TermKind::Iri => 0,
        TermKind::Literal => 1,
        TermKind::Bnode => 2,
        TermKind::Triple => 3,
    }
}

/// Build the `terms` record batch: `(id, kind, lex, datatype, lang, reifier)`
/// in `graph.terms` enumerate order (mirror gts_db `_rows["terms"]`).
fn terms_batch(graph: &Graph) -> Result<RecordBatch, PipelineError> {
    let mut id: Vec<i64> = Vec::with_capacity(graph.terms.len());
    let mut kind: Vec<i64> = Vec::with_capacity(graph.terms.len());
    let mut lex: Vec<Option<String>> = Vec::with_capacity(graph.terms.len());
    let mut datatype: Vec<Option<i64>> = Vec::with_capacity(graph.terms.len());
    let mut lang: Vec<Option<String>> = Vec::with_capacity(graph.terms.len());
    let mut reifier: Vec<Option<i64>> = Vec::with_capacity(graph.terms.len());
    for (i, t) in graph.terms.iter().enumerate() {
        id.push(i as i64);
        kind.push(term_kind_int(t.kind));
        lex.push(t.value.clone());
        datatype.push(t.datatype.map(|d| d as i64));
        lang.push(t.lang.clone());
        reifier.push(t.reifier.map(|r| r as i64));
    }
    let schema = Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("kind", DataType::Int64, false),
        Field::new("lex", DataType::Utf8, true),
        Field::new("datatype", DataType::Int64, true),
        Field::new("lang", DataType::Utf8, true),
        Field::new("reifier", DataType::Int64, true),
    ]);
    let cols: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from(id)),
        Arc::new(Int64Array::from(kind)),
        Arc::new(StringArray::from(lex)),
        Arc::new(Int64Array::from(datatype)),
        Arc::new(StringArray::from(lang)),
        Arc::new(Int64Array::from(reifier)),
    ];
    RecordBatch::try_new(Arc::new(schema), cols)
        .map_err(|e| PipelineError::Parse(format!("terms batch: {e}")))
}

/// Build the `quads` record batch: `(s, p, o, g)` in `graph.quads` order.
fn quads_batch(graph: &Graph) -> Result<RecordBatch, PipelineError> {
    let mut s: Vec<i64> = Vec::with_capacity(graph.quads.len());
    let mut p: Vec<i64> = Vec::with_capacity(graph.quads.len());
    let mut o: Vec<i64> = Vec::with_capacity(graph.quads.len());
    let mut g: Vec<Option<i64>> = Vec::with_capacity(graph.quads.len());
    for &(qs, qp, qo, qg) in &graph.quads {
        s.push(qs as i64);
        p.push(qp as i64);
        o.push(qo as i64);
        g.push(qg.map(|x| x as i64));
    }
    let schema = Schema::new(vec![
        Field::new("s", DataType::Int64, false),
        Field::new("p", DataType::Int64, false),
        Field::new("o", DataType::Int64, false),
        Field::new("g", DataType::Int64, true),
    ]);
    let cols: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from(s)),
        Arc::new(Int64Array::from(p)),
        Arc::new(Int64Array::from(o)),
        Arc::new(Int64Array::from(g)),
    ];
    RecordBatch::try_new(Arc::new(schema), cols)
        .map_err(|e| PipelineError::Parse(format!("quads batch: {e}")))
}

/// Build the `reifiers` record batch: `(reifier, s, p, o)` in insertion order.
fn reifiers_batch(graph: &Graph) -> Result<RecordBatch, PipelineError> {
    let mut reifier: Vec<i64> = Vec::with_capacity(graph.reifiers.len());
    let mut s: Vec<i64> = Vec::with_capacity(graph.reifiers.len());
    let mut p: Vec<i64> = Vec::with_capacity(graph.reifiers.len());
    let mut o: Vec<i64> = Vec::with_capacity(graph.reifiers.len());
    for &(r, (rs, rp, ro)) in &graph.reifiers {
        reifier.push(r as i64);
        s.push(rs as i64);
        p.push(rp as i64);
        o.push(ro as i64);
    }
    let schema = Schema::new(vec![
        Field::new("reifier", DataType::Int64, false),
        Field::new("s", DataType::Int64, false),
        Field::new("p", DataType::Int64, false),
        Field::new("o", DataType::Int64, false),
    ]);
    let cols: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from(reifier)),
        Arc::new(Int64Array::from(s)),
        Arc::new(Int64Array::from(p)),
        Arc::new(Int64Array::from(o)),
    ];
    RecordBatch::try_new(Arc::new(schema), cols)
        .map_err(|e| PipelineError::Parse(format!("reifiers batch: {e}")))
}

/// Build the `annotations` record batch: `(reifier, predicate, value)` in order.
fn annotations_batch(graph: &Graph) -> Result<RecordBatch, PipelineError> {
    let mut reifier: Vec<i64> = Vec::with_capacity(graph.annotations.len());
    let mut predicate: Vec<i64> = Vec::with_capacity(graph.annotations.len());
    let mut value: Vec<i64> = Vec::with_capacity(graph.annotations.len());
    for &(r, p, v) in &graph.annotations {
        reifier.push(r as i64);
        predicate.push(p as i64);
        value.push(v as i64);
    }
    let schema = Schema::new(vec![
        Field::new("reifier", DataType::Int64, false),
        Field::new("predicate", DataType::Int64, false),
        Field::new("value", DataType::Int64, false),
    ]);
    let cols: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from(reifier)),
        Arc::new(Int64Array::from(predicate)),
        Arc::new(Int64Array::from(value)),
    ];
    RecordBatch::try_new(Arc::new(schema), cols)
        .map_err(|e| PipelineError::Parse(format!("annotations batch: {e}")))
}

/// Build the `blobs` record batch: `(digest, bytes)` in insertion order.
/// Lazy (undecoded) blob payloads surface as empty bytes — the by-reference loss
/// is intentional (blobs can be multi-TB); the digest stays the join key.
fn blobs_batch(graph: &Graph) -> Result<RecordBatch, PipelineError> {
    let mut digest: Vec<String> = Vec::with_capacity(graph.blobs.len());
    let mut bytes: Vec<Vec<u8>> = Vec::with_capacity(graph.blobs.len());
    for (d, entry) in &graph.blobs {
        digest.push(d.clone());
        bytes.push(entry.cached_bytes().unwrap_or(&[]).to_vec());
    }
    let schema = Schema::new(vec![
        Field::new("digest", DataType::Utf8, false),
        Field::new("bytes", DataType::Binary, false),
    ]);
    let byte_refs: Vec<&[u8]> = bytes.iter().map(|b| b.as_slice()).collect();
    let cols: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(digest)),
        Arc::new(BinaryArray::from(byte_refs)),
    ];
    RecordBatch::try_new(Arc::new(schema), cols)
        .map_err(|e| PipelineError::Parse(format!("blobs batch: {e}")))
}

/// Serialize a record batch to Parquet bytes (snappy, deterministic layout).
fn write_parquet(batch: &RecordBatch) -> Result<Vec<u8>, PipelineError> {
    let mut buf: Vec<u8> = Vec::new();
    let props = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .build();
    let mut writer = ArrowWriter::try_new(&mut buf, batch.schema(), Some(props))
        .map_err(|e| PipelineError::Parse(format!("parquet writer: {e}")))?;
    writer
        .write(batch)
        .map_err(|e| PipelineError::Parse(format!("parquet write: {e}")))?;
    writer
        .close()
        .map_err(|e| PipelineError::Parse(format!("parquet close: {e}")))?;
    Ok(buf)
}

/// Render the per-table Parquet projection of a folded gts graph. Only non-empty
/// tables are written, keyed by their logical `dist/parquet/<table>.parquet` path.
pub(crate) fn render_parquet(graph: &Graph) -> Result<BTreeMap<String, Vec<u8>>, PipelineError> {
    let batches: BTreeMap<&str, RecordBatch> = {
        let mut m: BTreeMap<&str, RecordBatch> = BTreeMap::new();
        m.insert("terms", terms_batch(graph)?);
        m.insert("quads", quads_batch(graph)?);
        m.insert("reifiers", reifiers_batch(graph)?);
        m.insert("annotations", annotations_batch(graph)?);
        m.insert("blobs", blobs_batch(graph)?);
        m
    };
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for table in TABLES {
        let batch = &batches[table];
        if batch.num_rows() == 0 {
            continue; // duckdb path skips empty tables
        }
        out.insert(
            format!("{PARQUET_DIR}/{table}.parquet"),
            write_parquet(batch)?,
        );
    }
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
    fn kind(&self) -> StageKind {
        StageKind::ExportLeaf
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "parquet.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let graph = read_fold_upstream(input.upstream)?;
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), render_parquet(graph.as_ref())?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use std::path::Path;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    /// Read a Parquet blob back and return its total row count.
    fn parquet_row_count(bytes: &[u8]) -> usize {
        let reader = ParquetRecordBatchReaderBuilder::try_new(bytes::Bytes::copy_from_slice(bytes))
            .expect("parquet reader")
            .build()
            .expect("parquet batches");
        reader.map(|b| b.expect("batch").num_rows()).sum()
    }

    #[test]
    fn parquet_tables_reread_with_expected_row_counts() {
        let root = repo_root();
        let graph = crate::stages::export::read_fold(&root).expect("read fold");
        let arts = render_parquet(&graph).expect("render");

        // terms and quads are always non-empty for the committed fold.
        assert!(
            arts.contains_key(&format!("{PARQUET_DIR}/terms.parquet")),
            "terms.parquet must be produced"
        );
        assert!(
            arts.contains_key(&format!("{PARQUET_DIR}/quads.parquet")),
            "quads.parquet must be produced"
        );

        // Each produced file re-reads via the parquet reader with the row count
        // that matches the source model (enumerate-order parity).
        let expected: BTreeMap<&str, usize> = BTreeMap::from([
            ("terms", graph.terms.len()),
            ("quads", graph.quads.len()),
            ("reifiers", graph.reifiers.len()),
            ("annotations", graph.annotations.len()),
            ("blobs", graph.blobs.len()),
        ]);
        for table in TABLES {
            let path = format!("{PARQUET_DIR}/{table}.parquet");
            match arts.get(&path) {
                Some(bytes) => {
                    assert!(!bytes.is_empty(), "{path} is empty");
                    let n = parquet_row_count(bytes);
                    assert_eq!(n, expected[table], "{path} row count mismatch");
                    assert!(n > 0, "{path} written but empty — empties must be skipped");
                }
                None => {
                    assert_eq!(expected[table], 0, "{table} non-empty but not written");
                }
            }
        }

        // Determinism: a second render is byte-identical per table.
        let arts2 = render_parquet(&graph).expect("render2");
        assert_eq!(arts, arts2, "parquet render is not deterministic");
    }
}
