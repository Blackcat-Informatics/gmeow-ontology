// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! DSL mapping-purity gate: alignment linkage flows from slices.
//!
//! A native alignment cell IS cross-ontology alignment linkage — the authoring
//! surface the correspondence floor derives from. Per the project's
//! data-flows-from-slices direction, that linkage must be authored in the slice
//! that owns the aligned term, never in the central `dsl/mappings/` tree. What
//! legitimately remains under `dsl/mappings/` is the irreducible *enrichment*
//! residue: the directional `projections/` (get/put legs, FnO transform bodies,
//! quantitative axes, standpoint), the foundational bridge, and the cross-vocab
//! `MappingSet` publication headers (set-level metadata, not linkage).
//!
//! This gate enforces the invariant structurally: any native alignment cell — a
//! reified `skos:*Match` / `owl:equivalent*` / `owl:sameAs` / `rdfs:sub*Of`
//! statement carrying the `gmeow:sssomFile` discriminator — authored anywhere
//! under `dsl/mappings/` is a linkage restatement in the wrong place and is a hard
//! ERROR. The check reuses the CANONICAL native reader
//! ([`equivalence_cells`]) over the same [`DslView`] the correspondence frontend
//! reads, so it cannot drift from what the pipeline treats as an alignment cell,
//! and a bare (un-annotated) `skos:*Match` A-Box coreference or a `MappingSet`
//! publication header (which carries `gmeow:sssomFile` but is not a reified match
//! cell) never trips it.
//!
//! Hard-fail, no warning-only (CONSTITUTION / no-optionality). Ontology-specific
//! (it names the alignment cell shape and the `dsl/mappings/` tree), so it stays
//! in gmeow rather than the namespace-neutral `purrdf::slice` carrier — it
//! consumes purrdf::slice's [`ProjectionDiagnostic`]/[`SliceError`] primitives.

use std::path::{Path, PathBuf};

use gmeow_logic_compile::ingest::DslView;
use gmeow_logic_compile::projections::sssom::equivalence_cells;
use purrdf::slice::{ProjectionDiagnostic, SliceError};

/// Recursively collect `.ttl` files under `dir` (a missing root yields nothing;
/// a transient FS error surfaces — no silent drop).
fn collect_ttl(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), SliceError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_ttl(&path, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("ttl") {
            out.push(path);
        }
    }
    Ok(())
}

/// Run the DSL mapping-purity gate over `dsl/mappings/` under `root`.
///
/// Returns one `dsl-linkage-purity` ERROR [`ProjectionDiagnostic`] per
/// `dsl/mappings/**/*.ttl` file that authors one or more native alignment cells,
/// sorted by file. An empty result means all alignment linkage is authored in
/// slices (the enforced invariant).
///
/// # Errors
///
/// Returns [`SliceError`] on a filesystem error reading the scanned tree (a
/// missing `dsl/mappings/` is not an error — it simply contributes no sources), or
/// on a malformed alignment cell / unparsable Turtle surfaced by the canonical
/// native reader.
pub fn lint_dsl_mapping_purity(root: &Path) -> Result<Vec<ProjectionDiagnostic>, SliceError> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_ttl(&root.join("dsl").join("mappings"), &mut files)?;
    files.sort();

    let mut diagnostics: Vec<ProjectionDiagnostic> = Vec::new();
    for file in &files {
        let rel = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .into_owned();
        let bytes = std::fs::read(file)?;
        // Reuse the canonical native alignment-cell reader over the SAME DslView the
        // correspondence frontend reads, so the gate cannot drift from what the pipeline
        // treats as an alignment cell (a reified skos:*Match / owl: / rdfs: statement
        // carrying gmeow:sssomFile).
        let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
            .map_err(|e| SliceError::Parse(format!("{rel}: {e}")))?;
        let view = DslView::new(&dataset);
        let count = equivalence_cells(&view)
            .map_err(|e| {
                SliceError::Parse(format!("{rel}: malformed alignment cell: {}", e.message()))
            })?
            .len();
        if count == 0 {
            continue;
        }
        diagnostics.push(ProjectionDiagnostic {
            severity: "ERROR".to_owned(),
            check: "dsl-linkage-purity".to_owned(),
            code: "dsl-linkage-purity".to_owned(),
            message: format!(
                "{rel} authors {count} native alignment cell(s) (a reified skos:*Match / owl: / \
                 rdfs: statement carrying gmeow:sssomFile) under dsl/mappings/. Alignment linkage \
                 flows from slices: move each cell to the mappings/equivalences.ttl of the slice \
                 that defines its subject term (preserve sssomFile/confidence/comment). \
                 dsl/mappings/ carries only enrichment residue (projections/ legs + FnO bodies, the \
                 foundational bridge, and MappingSet publication headers)."
            ),
            instance: Some(rel),
            subject_id: None,
            predicate_id: None,
            object_id: None,
        });
    }
    diagnostics.sort_by(|a, b| a.cmp_severity_check_instance(b));
    Ok(diagnostics)
}

#[path = "mapping_purity.tests.rs"]
#[cfg(test)]
mod tests;
