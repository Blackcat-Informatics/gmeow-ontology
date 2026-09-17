// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Cheap, DAG-free coverage of the tar-packing helper: the uncompressed
/// total is the sum of the raw file bytes (no archive overhead folded in),
/// and the archive itself carries real, non-empty bytes.
#[test]
fn rendered_format_sums_raw_bytes_and_packs_a_real_archive() {
    let mut tree = BTreeMap::new();
    tree.insert("a.md".to_string(), b"hello".to_vec());
    tree.insert("b/c.md".to_string(), b"world!!".to_vec());
    let format = rendered_format("demo", "docs", &tree).expect("pack a real tree");
    assert_eq!(format.format_name, "demo");
    assert_eq!(format.family, "docs");
    assert_eq!(format.uncompressed_bytes, 5 + 7);
    assert!(!format.archive.is_empty());
    assert_eq!(format.rep, "docs-measure/demo");
}

/// A format that renders no files is a hard failure, never a silently
/// zero-sized measurement (no-optionality).
#[test]
fn rendered_format_fails_closed_on_an_empty_tree() {
    let tree: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let result = rendered_format("empty", "docs", &tree);
    assert!(result.is_err());
}

/// The blob-row constructor is used twice per format (its own delta, then
/// again inside the combined Design B snapshot); both calls must yield the
/// exact same bytes so the two GTS framings are directly comparable.
#[test]
fn blob_row_is_stable_across_repeated_calls() {
    let mut tree = BTreeMap::new();
    tree.insert("x.md".to_string(), b"stable".to_vec());
    let format = rendered_format("stable", "serialization", &tree).expect("pack a real tree");
    let first = format.blob_row();
    let second = format.blob_row();
    assert_eq!(first.data, second.data);
    assert_eq!(first.media_type, second.media_type);
    assert_eq!(first.rep, second.rep);
}
