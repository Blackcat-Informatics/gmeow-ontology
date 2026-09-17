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

#[path = "dsl.tests.rs"]
#[cfg(test)]
mod tests;
