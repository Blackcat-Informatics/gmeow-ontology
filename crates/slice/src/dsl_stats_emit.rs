// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native DSL surface-count emission — GMEOW's committed, drift-gated
//! `generated/mappings/dsl-stats.json` (#861).
//!
//! Ports `mapping_compile.emit_dsl_stats`: a counts summary over the SAME merged
//! mapping-DSL source set every other emitter loads (the shared
//! `dsl/mappings/**/*.ttl` tree + the slice [`crate::artifact::ArtifactRole::Mapping`]
//! artifacts — exactly `mapping_dsl.load_dsl`'s sources). The counts are:
//!   * `equivalences` — every `gmeow:TermEquivalence` cell.
//!   * `functions` — every `gmeow:ProjectionFunction`.
//!   * `mapping_sets` — every `gmeow:MappingSet`.
//!   * `projections` — every `gmeow:ProjectionMapping`.
//!   * `cells_by_set` — per `gmeow:sssomFile`, the equivalence-cell count.
//!
//! The JSON text is **byte-identical** to the historical Python emitter:
//! `json.dumps(stats, indent=1, sort_keys=True) + "\n"` — sorted keys, a 1-space
//! indent per nesting level, and a trailing newline.

use std::collections::BTreeMap;
use std::path::Path;

use oxigraph::model::NamedNode;

use crate::error::SliceError;
use crate::sparql_emit::{collect_dsl_store, object_literal, subjects_of_type};

// gmeow class IRIs the stats counts (full IRIs derived from the Python `GM.<name>`).
const GM_TERM_EQUIVALENCE: &str = "https://blackcatinformatics.ca/gmeow/TermEquivalence";
const GM_PROJECTION_FUNCTION: &str = "https://blackcatinformatics.ca/gmeow/ProjectionFunction";
const GM_MAPPING_SET: &str = "https://blackcatinformatics.ca/gmeow/MappingSet";
const GM_PROJECTION_MAPPING: &str = "https://blackcatinformatics.ca/gmeow/ProjectionMapping";
const GM_SSSOM_FILE: &str = "https://blackcatinformatics.ca/gmeow/sssomFile";

/// Emit the DSL surface-count summary as committed JSON text.
///
/// All inputs are sourced natively from `root` (the shared mapping-DSL tree + the
/// slice mapping artifacts — the same merged store [`emit_sparql_sets`] parses).
/// The text is byte-identical to the historical Python `emit_dsl_stats`.
///
/// [`emit_sparql_sets`]: crate::sparql_emit::emit_sparql_sets
///
/// # Errors
///
/// Returns [`SliceError`] on any missing/unparsable required source (no degraded
/// fallback): a `gmeow:TermEquivalence` cell with no `gmeow:sssomFile` is a hard
/// error, mirroring `mapping_dsl._equivalences`.
pub fn emit_dsl_stats(root: &Path) -> Result<String, SliceError> {
    let store = collect_dsl_store(root)?;

    // cells_by_set + equivalences: every gmeow:TermEquivalence keyed by sssomFile.
    let mut cells_by_set: BTreeMap<String, u64> = BTreeMap::new();
    let mut equivalences: u64 = 0;
    for cell_iri in subjects_of_type(&store, GM_TERM_EQUIVALENCE)? {
        equivalences += 1;
        let cell = NamedNode::new(&cell_iri)
            .map_err(|e| SliceError::Parse(format!("invalid cell IRI {cell_iri}: {e}")))?;
        let Some(sssom_file) = object_literal(&store, &cell, GM_SSSOM_FILE)? else {
            return Err(SliceError::Parse(format!(
                "term equivalence {cell_iri} missing sssomFile"
            )));
        };
        *cells_by_set.entry(sssom_file).or_insert(0) += 1;
    }

    let functions = subjects_of_type(&store, GM_PROJECTION_FUNCTION)?.len() as u64;
    let projections = subjects_of_type(&store, GM_PROJECTION_MAPPING)?.len() as u64;

    // mapping_sets: Python's `_mapping_sets` keys a dict by `gmeow:sssomFile`, so
    // two `gmeow:MappingSet` nodes targeting the same file (e.g. the music slice +
    // the shared DSL both declaring `gmeow-music.sssom.tsv`) collapse to ONE entry
    // (last-write-wins). Count the DISTINCT target files, not the subjects.
    let mut mapping_set_files: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    for set_iri in subjects_of_type(&store, GM_MAPPING_SET)? {
        let set = NamedNode::new(&set_iri)
            .map_err(|e| SliceError::Parse(format!("invalid mapping set IRI {set_iri}: {e}")))?;
        let Some(file) = object_literal(&store, &set, GM_SSSOM_FILE)? else {
            return Err(SliceError::Parse(format!(
                "mapping set {set_iri} missing sssomFile"
            )));
        };
        mapping_set_files.insert(file);
    }
    let mapping_sets = mapping_set_files.len() as u64;

    Ok(render_stats(
        &cells_by_set,
        equivalences,
        functions,
        mapping_sets,
        projections,
    ))
}

/// Render the stats dict as `json.dumps(stats, indent=1, sort_keys=True) + "\n"`.
///
/// Python's `indent=1` adds one space per nesting level, uses `": "` as the
/// key/value separator and `,` (newline-suffixed) as the item separator, and sorts
/// the keys at every level. The top-level keys sort to `cells_by_set`,
/// `equivalences`, `functions`, `mapping_sets`, `projections`; the `cells_by_set`
/// inner keys (the `*.sssom.tsv` file names) are already sorted by the
/// [`BTreeMap`]. The whole document ends with a trailing newline.
fn render_stats(
    cells_by_set: &BTreeMap<String, u64>,
    equivalences: u64,
    functions: u64,
    mapping_sets: u64,
    projections: u64,
) -> String {
    let mut out = String::new();
    out.push_str("{\n");

    // "cells_by_set": { … } — nested object at indent depth 2.
    out.push_str(" \"cells_by_set\": {");
    if cells_by_set.is_empty() {
        // Python renders an empty dict as `{}` with no inner newlines.
        out.push('}');
    } else {
        out.push('\n');
        let mut first = true;
        for (file, count) in cells_by_set {
            if !first {
                out.push_str(",\n");
            }
            first = false;
            out.push_str(&format!("  {}: {count}", json_string(file)));
        }
        out.push_str("\n }");
    }
    out.push_str(",\n");

    out.push_str(&format!(" \"equivalences\": {equivalences},\n"));
    out.push_str(&format!(" \"functions\": {functions},\n"));
    out.push_str(&format!(" \"mapping_sets\": {mapping_sets},\n"));
    out.push_str(&format!(" \"projections\": {projections}\n"));
    out.push_str("}\n");
    out
}

/// Render a string as a JSON string literal, mirroring Python's `json.dumps` of a
/// `str` (the file names here are plain ASCII `*.sssom.tsv`, but escape defensively
/// for `"` and `\`).
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn dsl_stats_matches_committed() {
        let root = repo_root();
        let got = emit_dsl_stats(&root).expect("emit dsl stats");
        let committed_path = root
            .join("generated")
            .join("mappings")
            .join("dsl-stats.json");
        let want = std::fs::read_to_string(&committed_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", committed_path.display()));
        if got != want {
            for (i, (g, w)) in got.lines().zip(want.lines()).enumerate() {
                assert_eq!(g, w, "first diff at line {}", i + 1);
            }
            assert_eq!(
                got.lines().count(),
                want.lines().count(),
                "line counts differ"
            );
            // Trailing-newline / byte-level difference with matching lines.
            assert_eq!(got, want, "dsl-stats.json byte mismatch");
        }
    }

    #[test]
    fn render_stats_empty_cells() {
        let empty: BTreeMap<String, u64> = BTreeMap::new();
        let text = render_stats(&empty, 0, 0, 0, 0);
        assert_eq!(
            text,
            "{\n \"cells_by_set\": {},\n \"equivalences\": 0,\n \"functions\": 0,\n \"mapping_sets\": 0,\n \"projections\": 0\n}\n"
        );
    }
}
