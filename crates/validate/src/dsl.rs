// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Merged-graph N-Triples plus focus→file provenance for the DSL SHACL path.
//!
//! PyO3-free engine core. The DSL validation seam (`dsl_validate.py`) used to
//! build an rdflib graph AND a `node_to_file` map (the first `.ttl` file each
//! named subject appears in) so a SHACL violation could be attributed to its
//! source cell. That provenance walk is net-new Rust here (#579): each file is
//! parsed in document order, every named (IRI) subject is recorded against the
//! first file it is seen in, and all triples are merged into one store dumped to
//! canonical N-Triples for the SHACL validator.
//!
//! The merge is order-sensitive *only* for the provenance map (first-seen wins),
//! exactly matching the legacy Python `for path in sorted(...): ... if subject
//! not in node_to_file` loop.

use std::path::PathBuf;

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::NamedOrBlankNode;
use oxigraph::store::Store;

use crate::store::dump_store_to_ntriples;

/// The merged DSL graph as N-Triples plus the focus→file provenance pairs.
pub struct DslMerge {
    /// The merged graph serialized to canonical N-Triples.
    pub data_nt: String,
    /// `(named_subject_iri, source_file_path)` — first-seen file per named
    /// subject, in first-seen order.
    pub focus_to_file: Vec<(String, String)>,
}

/// Build the merged N-Triples plus the focus→file map over `paths`.
///
/// `paths` is processed in the order given (the Python caller sorts them); each
/// named-IRI subject is mapped to the FIRST path it appears in. Blank-node
/// subjects carry no file mapping (they have no stable cross-file identity),
/// matching the legacy `isinstance(subject, URIRef)` guard.
///
/// # Errors
///
/// Returns `Err(message)` if any file fails to read or parse.
pub fn merge_with_provenance(paths: &[PathBuf]) -> Result<DslMerge, String> {
    let store = Store::new().map_err(|e| format!("store creation failed: {e}"))?;
    let mut focus_to_file: Vec<(String, String)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for path in paths {
        let path_str = path.display().to_string();
        let bytes = std::fs::read(path).map_err(|e| format!("failed to read {path_str}: {e}"))?;
        for triple in RdfParser::from_format(RdfFormat::Turtle)
            .lenient()
            .for_reader(bytes.as_slice())
        {
            let triple = triple.map_err(|e| format!("syntax error in {path_str}: {e}"))?;
            if let NamedOrBlankNode::NamedNode(n) = &triple.subject {
                let iri = n.as_str().to_owned();
                if seen.insert(iri.clone()) {
                    focus_to_file.push((iri, path_str.clone()));
                }
            }
            store
                .insert(&triple)
                .map_err(|e| format!("store insert failed for {path_str}: {e}"))?;
        }
    }

    Ok(DslMerge {
        data_nt: dump_store_to_ntriples(&store)
            .map_err(|e| format!("N-Triples serialization failed: {e}"))?,
        focus_to_file,
    })
}

/// Build the merged N-Triples over `paths` (no provenance), for the plain SHACL
/// data path (`run_shacl` / `check_examples`).
///
/// # Errors
///
/// Returns `Err(message)` if any file fails to read or parse.
pub fn merge_to_ntriples(paths: &[PathBuf]) -> Result<String, String> {
    let store = build_merged_store(paths)?;
    dump_store_to_ntriples(&store).map_err(|e| format!("N-Triples serialization failed: {e}"))
}

/// Build one merged store from the Turtle `paths` (lenient parsing).
fn build_merged_store(paths: &[PathBuf]) -> Result<Store, String> {
    let store = Store::new().map_err(|e| format!("store creation failed: {e}"))?;
    for path in paths {
        let path_str = path.display().to_string();
        let bytes = std::fs::read(path).map_err(|e| format!("failed to read {path_str}: {e}"))?;
        for triple in RdfParser::from_format(RdfFormat::Turtle)
            .lenient()
            .for_reader(bytes.as_slice())
        {
            let triple = triple.map_err(|e| format!("syntax error in {path_str}: {e}"))?;
            store
                .insert(&triple)
                .map_err(|e| format!("store insert failed for {path_str}: {e}"))?;
        }
    }
    Ok(store)
}

#[cfg(test)]
mod tests {
    use oxigraph::model::Term;

    use super::*;

    fn write_tmp(name: &str, contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn provenance_maps_each_named_subject_to_first_file() {
        let a = write_tmp(
            "gmeow_validate_dsl_prov_a.ttl",
            "@prefix ex: <https://example.org/> .\n\
             ex:alice ex:p ex:b .\n\
             ex:shared ex:p ex:x .\n",
        );
        let b = write_tmp(
            "gmeow_validate_dsl_prov_b.ttl",
            "@prefix ex: <https://example.org/> .\n\
             ex:bob ex:p ex:c .\n\
             ex:shared ex:p ex:y .\n",
        );
        let merge = merge_with_provenance(&[a.clone(), b.clone()]).expect("merge must succeed");
        std::fs::remove_file(&a).ok();
        std::fs::remove_file(&b).ok();

        let map: std::collections::HashMap<String, String> =
            merge.focus_to_file.iter().cloned().collect();
        // alice came from file a; bob from file b; shared first-seen in a.
        assert!(map["https://example.org/alice"].ends_with("gmeow_validate_dsl_prov_a.ttl"));
        assert!(map["https://example.org/bob"].ends_with("gmeow_validate_dsl_prov_b.ttl"));
        assert!(map["https://example.org/shared"].ends_with("gmeow_validate_dsl_prov_a.ttl"));
        // Both files' triples are in the merged data (4 distinct triples).
        let store = crate::store::build_store_from_nt(&merge.data_nt).unwrap();
        assert_eq!(store.len().unwrap(), 4);
    }

    #[test]
    fn merge_to_ntriples_unions_all_triples() {
        let a = write_tmp(
            "gmeow_validate_dsl_merge_a.ttl",
            "@prefix ex: <https://example.org/> .\nex:a ex:p ex:b .\n",
        );
        let b = write_tmp(
            "gmeow_validate_dsl_merge_b.ttl",
            "@prefix ex: <https://example.org/> .\nex:c ex:p ex:d .\n",
        );
        let nt = merge_to_ntriples(&[a.clone(), b.clone()]).expect("merge must succeed");
        std::fs::remove_file(&a).ok();
        std::fs::remove_file(&b).ok();
        let store = crate::store::build_store_from_nt(&nt).unwrap();
        assert_eq!(store.len().unwrap(), 2);
        for quad in store.iter() {
            let q = quad.unwrap();
            let ok = matches!(&q.object, Term::NamedNode(n)
                if n.as_str() == "https://example.org/b" || n.as_str() == "https://example.org/d");
            assert!(ok);
        }
    }

    #[test]
    fn merge_propagates_parse_error_with_path() {
        let bad = write_tmp("gmeow_validate_dsl_bad.ttl", "this is not turtle @@@ <<<");
        let err = merge_to_ntriples(std::slice::from_ref(&bad)).unwrap_err();
        std::fs::remove_file(&bad).ok();
        assert!(err.contains("gmeow_validate_dsl_bad.ttl"));
    }
}
