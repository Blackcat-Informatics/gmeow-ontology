// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
