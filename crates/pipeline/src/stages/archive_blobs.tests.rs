// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

#[test]
fn declared_blob_reps_match_the_static_archive_contract() {
    let mut declared: Vec<String> = ArchiveBlobsStage::new().attaches_blob_reps().to_vec();
    declared.sort();
    let mut expected: Vec<String> = ARCHIVE_REPS.iter().map(|rep| (*rep).to_string()).collect();
    expected.sort();
    assert_eq!(declared, expected);
}

#[test]
fn input_files_cover_every_authored_source_the_fold_reads() {
    let root = repo_root();
    let declared: std::collections::BTreeSet<PathBuf> = ArchiveBlobsStage::new()
        .input_files(&root)
        .expect("input files")
        .into_iter()
        .collect();
    assert!(
        !declared.is_empty(),
        "the fold reads authored trees; the declaration must not be empty"
    );
    for probe in [
        slice_files(&root, "tests").expect("slice tests"),
        slice_files(&root, "mappings").expect("slice mappings"),
        list_files(&root.join("shapes"), "ttl").expect("authored shapes"),
        slice_named_files(&root, "shapes.ttl").expect("slice shapes"),
    ] {
        for path in probe {
            assert!(
                declared.contains(&path),
                "input_files must declare the authored source {}",
                path.display()
            );
        }
    }
}
