// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Foundation-corpus importer — the Rust port of the consumer
//! child `gmeow_tools.foundation_import`.
//!
//! Imports a Lillith_Foundation_Docs-shaped JSONL corpus into GMEOW instance data
//! over the `purrdf` IR, exercising every interior facility the two EPICs
//! landed: WEMI spine, claim-spine author facts Assessments for goal-score
//! vectors narrative positions seam links (FLAT BY DEFAULT — the
//! efficiency doctrine, budget-reported) arc samples scoped roles,
//! motifs, and provenance via ImportActivity.
//!
//! # Doctrine
//! - The efficiency doctrine is load-bearing: seam links emit as flat quads; only
//!   constructs whose vantage/score/mode is data reify. The [`BudgetReport`]
//!   records the split per link type.
//! - Tags are not promoted: `thematic_tags` stay unimported (budget-counted);
//!   explicit corpus concepts DO become Motifs.
//! - Privacy: this module never embeds corpus content in the repo; CI runs against
//!   a SYNTHETIC fixture.
//!
//! The six projections are lossy by design; each writer's docstring names its loss.

pub mod budget;
pub mod graphview;
pub mod importer;
pub mod model;
pub mod projections;
pub mod reconcile;
pub mod slug;

use std::path::Path;
use std::sync::Arc;

use purrdf::prelude::RdfDataset;

pub use budget::BudgetReport;
pub use graphview::GraphView;
pub use importer::{CORP_PREFIX, FoundationImporter, LANG, NAMESPACE};
pub use model::Record;
pub use projections::{PROJECTION_NAMES, project};
pub use reconcile::{NQ_PREDICATE_STATUS, reconcile_nq};

/// Read a JSONL corpus into a list of record structs.
///
/// Mirrors the Python `load_records`: strip each line, skip blanks, JSON-parse
/// each non-blank line. Streams the file line-by-line (`BufReader`) rather than
/// loading the whole corpus into memory.
pub fn load_records(path: &Path) -> std::io::Result<Vec<Record>> {
    use std::io::BufRead;
    let reader = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut records = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if !line.is_empty() {
            let record: Record = serde_json::from_str(line)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            records.push(record);
        }
    }
    Ok(records)
}

/// Run the full pipeline into `out_dir`.
///
/// Import, then write `foundation.ttl` + `budget-report.txt` + the six projections
/// (+ optional `.nq` reconciliation when `nq_path` is given and exists). Returns
/// the frozen dataset and the budget report.
pub fn run_import(
    jsonl_path: &Path,
    out_dir: &Path,
    nq_path: Option<&Path>,
) -> std::io::Result<(Arc<RdfDataset>, BudgetReport)> {
    let records = load_records(jsonl_path)?;
    let mut importer = FoundationImporter::new();
    importer.import_corpus(&records, &jsonl_path.to_string_lossy())?;
    let (dataset, budget) = importer.freeze()?;

    std::fs::create_dir_all(out_dir)?;

    // foundation.ttl (reference only — serialization differs from rdflib's).
    let prefixes = vec![
        ("gmeow".to_string(), NAMESPACE.to_string()),
        ("corp".to_string(), CORP_PREFIX.to_string()),
    ];
    let ttl = purrdf::turtle_normalize::render(&dataset, &prefixes);
    std::fs::write(out_dir.join("foundation.ttl"), ttl)?;

    // budget-report.txt (as_text() + "\n").
    std::fs::write(
        out_dir.join("budget-report.txt"),
        format!("{}\n", budget.as_text()),
    )?;

    // The six projections (byte-exact targets).
    let view = GraphView::from_dataset(&dataset);
    for name in PROJECTION_NAMES {
        let body = project(name, &view);
        std::fs::write(out_dir.join(name), body)?;
    }

    // Optional .nq reconciliation.
    if let Some(nq) = nq_path
        && nq.exists()
    {
        let report = reconcile_nq(nq, &NQ_PREDICATE_STATUS)?;
        std::fs::write(out_dir.join("nq-reconciliation.txt"), format!("{report}\n"))?;
    }

    Ok((dataset, budget))
}
