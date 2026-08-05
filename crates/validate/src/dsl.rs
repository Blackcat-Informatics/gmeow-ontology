// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Merged native dataset plus focus→file provenance for the DSL SHACL path.
//!
//! PyO3-free engine core. The legacy Python DSL validation seam used to
//! build an rdflib graph AND a `node_to_file` map (the first `.ttl` file each
//! named subject appears in) so a SHACL violation could be attributed to its
//! source cell. That provenance walk is net-new Rust here: each file is
//! parsed in document order, every named (IRI) subject is recorded against the
//! first file it is seen in, and all triples are merged into one frozen native
//! [`purrdf::RdfDataset`] for the (native) SHACL validator.
//!
//! The merge is order-sensitive *only* for the provenance map (first-seen wins),
//! exactly matching the legacy Python `for path in sorted(...): ... if subject
//! not in node_to_file` loop.

use std::path::PathBuf;
use std::sync::Arc;

use gmeow_errors::Diag;
use purrdf::{
    DatasetView, GraphMatch, RdfDataset, RdfDatasetBuilder, SerializeGraph, TermRef, parse_dataset,
    serialize_dataset,
};

/// The merged DSL graph plus the focus→file provenance pairs.
pub struct DslMerge {
    /// The merged graph as a frozen native dataset (the SHACL data graph).
    pub dataset: Arc<RdfDataset>,
    /// `(named_subject_iri, source_file_path)` — first-seen file per named
    /// subject, in first-seen order.
    pub focus_to_file: Vec<(String, String)>,
}

impl DslMerge {
    /// Serialize the merged dataset's default graph to canonical N-Triples — the
    /// legacy `data_nt` surface the PyO3 `dsl_merge_with_provenance` returns. Uses the
    /// N-Quads writer over the default-graph projection (byte-lenient on private-use
    /// `@x-gmeow-*` tags; default-graph-only output is exactly N-Triples).
    ///
    /// # Errors
    ///
    /// Returns `Err` if serialization fails.
    pub fn data_ntriples(&self) -> gmeow_errors::Result<String> {
        let bytes = serialize_dataset(
            &self.dataset,
            "application/n-quads",
            SerializeGraph::DefaultGraph,
        )
        .map_err(|e| {
            Diag::of_kind(crate::error::Serialize {
                detail: format!("N-Triples serialization failed: {e}"),
            })
        })?;
        String::from_utf8(bytes).map_err(|e| {
            Diag::of_kind(crate::error::Serialize {
                detail: format!("N-Triples serialization failed: {e}"),
            })
        })
    }
}

/// Build the merged dataset plus the focus→file map over `paths`.
///
/// `paths` is processed in the order given (the Python caller sorts them); each
/// named-IRI subject is mapped to the FIRST path it appears in. Blank-node
/// subjects carry no file mapping (they have no stable cross-file identity),
/// matching the legacy `isinstance(subject, URIRef)` guard.
///
/// Each file is merged under a fresh blank scope ([`RdfDatasetBuilder::push_dataset`])
/// so anonymous blanks across DSL/competency files stay disjoint (e.g. two
/// `[ a ExpectedCell ; … ]` blanks never fuse) — the native twin of the old per-source
/// blank-prefix scoping (C0.2).
///
/// # Errors
///
/// Returns `Err` if any file fails to read or parse.
pub fn merge_with_provenance(paths: &[PathBuf]) -> gmeow_errors::Result<DslMerge> {
    let mut builder = RdfDatasetBuilder::new();
    let mut focus_to_file: Vec<(String, String)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for path in paths {
        let path_str = path.display().to_string();
        let bytes = std::fs::read(path).map_err(|e| {
            Diag::of_kind(crate::error::Io {
                detail: format!("failed to read {path_str}: {e}"),
            })
        })?;
        let dataset = parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
            Diag::of_kind(crate::error::Parse {
                detail: format!("syntax error in {path_str}: {e}"),
            })
        })?;
        // Record the first source file for each named-IRI subject, in document order
        // (the parsed per-file dataset preserves source order in its quad table).
        for q in dataset.quads_for_pattern(None, None, None, GraphMatch::Any) {
            if let TermRef::Iri(iri) = dataset.resolve(q.s)
                && seen.insert(iri.to_owned())
            {
                focus_to_file.push((iri.to_owned(), path_str.clone()));
            }
        }
        builder.push_dataset(&dataset);
    }

    let dataset = builder.freeze().map_err(|e| {
        Diag::of_kind(crate::error::Serialize {
            detail: format!("dataset freeze failed: {e}"),
        })
    })?;
    Ok(DslMerge {
        dataset,
        focus_to_file,
    })
}

/// Build the merged dataset over `paths` (no provenance), serialized to canonical
/// N-Triples — a legacy/test-only seam (`merge_to_ntriples` PyO3 surface).
///
/// The N-Quads writer is requested over the default-graph projection: it is
/// byte-lenient on the GMEOW ontology's private-use `@x-gmeow-*` language tags (it
/// writes the lexical tag verbatim) and a default-graph-only document is exactly
/// N-Triples.
///
/// # Errors
///
/// Returns `Err` if any file fails to read or parse, or serialization fails.
pub fn merge_to_ntriples(paths: &[PathBuf]) -> gmeow_errors::Result<String> {
    let dataset = crate::store::dataset_from_paths(paths)?;
    let bytes = serialize_dataset(
        &dataset,
        "application/n-quads",
        SerializeGraph::DefaultGraph,
    )
    .map_err(|e| {
        Diag::of_kind(crate::error::Serialize {
            detail: format!("N-Triples serialization failed: {e}"),
        })
    })?;
    String::from_utf8(bytes).map_err(|e| {
        Diag::of_kind(crate::error::Serialize {
            detail: format!("N-Triples serialization failed: {e}"),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write `contents` to `name` inside a fresh RAII temp directory.
    ///
    /// The returned [`tempfile::TempDir`] owns the directory: it is removed on
    /// drop, including on panic and early return. Bind it to a named `_tmp`
    /// (never a bare `_`, which would drop it immediately) so it outlives the
    /// path. The file *name* is preserved because the provenance map is keyed
    /// by file path and the assertions match on the file name.
    fn write_tmp(name: &str, contents: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join(name);
        std::fs::write(&path, contents).unwrap();
        (dir, path)
    }

    #[test]
    fn provenance_maps_each_named_subject_to_first_file() {
        let (_tmp_a, a) = write_tmp(
            "gmeow_validate_dsl_prov_a.ttl",
            "@prefix ex: <https://example.org/> .\n\
             ex:alice ex:p ex:b .\n\
             ex:shared ex:p ex:x .\n",
        );
        let (_tmp_b, b) = write_tmp(
            "gmeow_validate_dsl_prov_b.ttl",
            "@prefix ex: <https://example.org/> .\n\
             ex:bob ex:p ex:c .\n\
             ex:shared ex:p ex:y .\n",
        );
        let merge = merge_with_provenance(&[a.clone(), b.clone()]).expect("merge must succeed");

        let map: std::collections::HashMap<String, String> =
            merge.focus_to_file.iter().cloned().collect();
        // alice came from file a; bob from file b; shared first-seen in a.
        assert!(map["https://example.org/alice"].ends_with("gmeow_validate_dsl_prov_a.ttl"));
        assert!(map["https://example.org/bob"].ends_with("gmeow_validate_dsl_prov_b.ttl"));
        assert!(map["https://example.org/shared"].ends_with("gmeow_validate_dsl_prov_a.ttl"));
        // Both files' triples are in the merged data (4 distinct triples).
        assert_eq!(merge.dataset.quad_count(), 4);
    }

    #[test]
    fn merge_to_ntriples_unions_all_triples() {
        let (_tmp_a, a) = write_tmp(
            "gmeow_validate_dsl_merge_a.ttl",
            "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
        );
        let (_tmp_b, b) = write_tmp(
            "gmeow_validate_dsl_merge_b.ttl",
            "@prefix ex: <https://example.org/> .\nex:c ex:p ex:d .\n",
        );
        let nt = merge_to_ntriples(&[a.clone(), b.clone()]).expect("merge must succeed");
        let ds = crate::store::dataset_from_nt(&nt).unwrap();
        assert_eq!(ds.quad_count(), 2);
        for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
            let ok = matches!(ds.resolve(q.o), TermRef::Iri(n)
                if n == "https://example.org/b" || n == "https://example.org/d");
            assert!(ok);
        }
    }

    #[test]
    fn merge_propagates_parse_error_with_path() {
        let (_tmp, bad) = write_tmp("gmeow_validate_dsl_bad.ttl", "this is not turtle @@@ <<<");
        let err = merge_to_ntriples(std::slice::from_ref(&bad)).unwrap_err();
        assert!(err.message().contains("gmeow_validate_dsl_bad.ttl"));
    }
}
