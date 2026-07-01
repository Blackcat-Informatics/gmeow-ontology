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
//! This gate enforces the invariant structurally: a `gmeow:TermEquivalence`
//! subject authored anywhere under `dsl/mappings/` is a linkage restatement in
//! the wrong place and is a hard ERROR. `gmeow:MappingSet` headers,
//! `gmeow:ProjectionMapping` enrichment cells, and FnO function bodies are NOT
//! linkage and are unaffected.
//!
//! Hard-fail, no warning-only (CONSTITUTION / no-optionality). Surfaced as a
//! `mapping-compile.dsl-linkage-purity` [`ProjectionDiagnostic`] and enforced in
//! the `mappings` stage so `regenerate` / `check-generated` / `make check` all
//! reject a stray linkage cell.

use std::path::{Path, PathBuf};

use regex::Regex;

use crate::diagnostics::ProjectionDiagnostic;
use crate::error::SliceError;

/// A `gmeow:TermEquivalence` type assertion in either CURIE or full-IRI form,
/// as the object of an `a` / `rdf:type` predicate. The authoring convention is
/// `gmeow:eqX a gmeow:TermEquivalence ;`, but the full-IRI and `rdf:type` forms
/// are matched too so the gate cannot be evaded by spelling.
fn term_equivalence_re() -> Regex {
    // The object is delimited two ways: the CURIE by a following non-word char or
    // line end (so `gmeow:TermEquivalenceFoo` does not match — the `regex` crate has
    // no look-around, so the boundary char is consumed, which is fine for counting);
    // the full IRI by its closing `>`.
    Regex::new(
        r"(?m)(?:\ba\b|rdf:type|<http://www\.w3\.org/1999/02/22-rdf-syntax-ns#type>)\s+(?:gmeow:TermEquivalence(?:[^A-Za-z0-9_]|$)|<https://blackcatinformatics\.ca/gmeow/TermEquivalence>)",
    )
    .expect("static gmeow:TermEquivalence type-assertion regex")
}

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
    let re = term_equivalence_re();
    let mut files: Vec<PathBuf> = Vec::new();
    collect_ttl(&root.join("dsl").join("mappings"), &mut files)?;
    files.sort();

    let mut diagnostics: Vec<ProjectionDiagnostic> = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file)?;
        let count = re.find_iter(&text).count();
        if count == 0 {
            continue;
        }
        let rel = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .into_owned();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .expect("repo root")
    }

    /// The enforced invariant: no alignment linkage is authored under
    /// `dsl/mappings/` — every `gmeow:TermEquivalence` lives in a slice.
    #[test]
    fn dsl_mappings_hold_no_linkage_cells() {
        let found = lint_dsl_mapping_purity(&repo_root()).expect("scan dsl/mappings");
        assert!(
            found.is_empty(),
            "alignment linkage must be authored in slices, not dsl/mappings/: {found:#?}"
        );
    }

    /// Positive polarity: a stray `gmeow:TermEquivalence` under `dsl/mappings/`
    /// is caught (both CURIE and full-IRI type spellings), while a `MappingSet`
    /// header and a `ProjectionMapping` enrichment cell pass.
    #[test]
    fn catches_a_stray_linkage_cell_but_not_enrichment() {
        let tmp = std::env::temp_dir().join(format!("gmeow-purity-test-{}", std::process::id()));
        // Clean slate: a crashed prior run could leave a stray.ttl that would fail
        // the clean-set assertion below.
        std::fs::remove_dir_all(&tmp).ok();
        let dir = tmp.join("dsl").join("mappings");
        std::fs::create_dir_all(&dir).expect("mkdir");

        // Enrichment + publication metadata only — must PASS.
        std::fs::write(
            dir.join("mapping-sets.ttl"),
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             gmeow:mapsetX a gmeow:MappingSet ; gmeow:sssomFile \"x.sssom.tsv\" .\n\
             gmeow:pmX a gmeow:ProjectionMapping ; gmeow:getLeg gmeow:someLeg .\n",
        )
        .expect("write clean");
        assert!(
            lint_dsl_mapping_purity(&tmp).expect("scan").is_empty(),
            "MappingSet + ProjectionMapping must not trip the linkage gate"
        );

        // A stray linkage cell (CURIE + full-IRI type) — must RED (2 files).
        let curie = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n@prefix skos: <http://www.w3.org/2004/02/skos/core#> .\ngmeow:eq1 a gmeow:TermEquivalence ; gmeow:alignSubject gmeow:Foo .\n";
        std::fs::write(dir.join("stray.ttl"), curie).expect("write stray");
        let full = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\ngmeow:eq2 <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/gmeow/TermEquivalence> .\n";
        std::fs::write(dir.join("stray-full.ttl"), full).expect("write stray-full");
        let found = lint_dsl_mapping_purity(&tmp).expect("scan");
        assert_eq!(
            found.len(),
            2,
            "both stray-cell spellings must red: {found:#?}"
        );
        assert!(found.iter().all(|d| d.check == "dsl-linkage-purity"));

        std::fs::remove_dir_all(&tmp).ok();
    }
}
