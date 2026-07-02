// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! DSL mapping-purity gate: alignment linkage flows from slices.
//!
//! A `gmeow:TermEquivalence` cell IS cross-ontology alignment linkage — the
//! authoring surface the correspondence floor derives from. Per the project's
//! data-flows-from-slices direction, that linkage must be authored in the slice
//! that owns the aligned term, never in the central `dsl/mappings/` tree. What
//! legitimately remains under `dsl/mappings/` is the irreducible *enrichment*
//! residue: the directional `projections/` (get/put legs, FnO transform bodies,
//! quantitative axes, standpoint), the foundational bridge, and the cross-vocab
//! `MappingSet` publication headers (set-level metadata, not linkage).
//!
//! This gate enforces the invariant structurally: any subject *typed*
//! `gmeow:TermEquivalence` authored anywhere under `dsl/mappings/` is a linkage
//! restatement in the wrong place and is a hard ERROR. The check parses each file
//! into the native [`Dataset`] IR and queries `?s rdf:type gmeow:TermEquivalence`
//! on the resolved type IRI — so it cannot be evaded by a prefix alias bound to
//! the gmeow namespace (`gm:TermEquivalence`) nor by a comma-typed list.
//!
//! Hard-fail, no warning-only (CONSTITUTION / no-optionality). Ontology-specific
//! (it names `gmeow:TermEquivalence` and the `dsl/mappings/` tree), so it stays in
//! gmeow rather than the namespace-neutral `purrdf::slice` carrier — it consumes
//! purrdf::slice's [`Dataset`]/[`ProjectionDiagnostic`]/[`SliceError`] primitives.

use std::path::{Path, PathBuf};

use purrdf::slice::rdf_query::Dataset;
use purrdf::slice::{ProjectionDiagnostic, SliceError};

/// The `gmeow:TermEquivalence` class IRI — the alignment-linkage cell type.
const GM_TERM_EQUIVALENCE: &str = "https://blackcatinformatics.ca/gmeow/TermEquivalence";

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
/// `dsl/mappings/**/*.ttl` file that authors one or more `gmeow:TermEquivalence`
/// linkage cells, sorted by file. An empty result means all alignment linkage is
/// authored in slices (the enforced invariant).
///
/// # Errors
///
/// Returns [`SliceError`] on a filesystem error reading the scanned tree (a
/// missing `dsl/mappings/` is not an error — it simply contributes no sources).
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
        let dataset = Dataset::parse_turtle(&bytes, &rel)?;
        let count = dataset.subject_terms_of_type(GM_TERM_EQUIVALENCE)?.len();
        if count == 0 {
            continue;
        }
        diagnostics.push(ProjectionDiagnostic {
            severity: "ERROR".to_owned(),
            check: "dsl-linkage-purity".to_owned(),
            code: "dsl-linkage-purity".to_owned(),
            message: format!(
                "{rel} authors {count} gmeow:TermEquivalence linkage cell(s) under dsl/mappings/. \
                 Alignment linkage flows from slices: move each cell to the mappings/equivalences.ttl \
                 of the slice that defines its alignSubject (preserve sssomFile/confidence/comment). \
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
