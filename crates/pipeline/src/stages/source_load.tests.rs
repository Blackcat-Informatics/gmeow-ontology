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
fn authored_files_includes_root_and_modules() {
    let root = repo_root();
    let files = authored_files(&root).unwrap();
    assert!(files.iter().any(|p| p.ends_with("ontology/gmeow.ttl")));
    assert!(
        files
            .iter()
            .any(|p| p.ends_with("slices/core/pipeline/module.ttl"))
    );
    assert!(
        files.len() > 50,
        "expected 50+ authored files, got {}",
        files.len()
    );
}

/// Fixed span policy, asserted against the span table read back from the FOLDED
/// product bundle (not just the live index): a RootOntology + Source file contribute
/// their subjects; the imports/ (Import) file is SUPPRESSED. Mirrors
/// `bundle_carries_the_consumer_archives` in reading through the fold accessor.
#[test]
fn fixed_policy_emits_source_and_root_suppresses_imports_through_the_fold() {
    use std::sync::Arc;
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    // RootOntology: ontology/gmeow.ttl.
    std::fs::create_dir_all(root.join("ontology")).unwrap();
    std::fs::write(
        root.join("ontology/gmeow.ttl"),
        "@prefix ex: <https://example.test/> .\nex:rootSubject a ex:Root .\n",
    )
    .unwrap();
    // Source: a slice module.ttl.
    std::fs::create_dir_all(root.join("slices/g/n")).unwrap();
    std::fs::write(
        root.join("slices/g/n/module.ttl"),
        "@prefix ex: <https://example.test/> .\nex:sourceSubject a ex:Thing .\n",
    )
    .unwrap();
    // Import: imports/foo.ttl — must be SUPPRESSED.
    std::fs::create_dir_all(root.join("imports")).unwrap();
    std::fs::write(
        root.join("imports/foo.ttl"),
        "@prefix ex: <https://example.test/> .\nex:importSubject a ex:Imported .\n",
    )
    .unwrap();

    // Fold the built index into a product bundle exactly as `run` does, then read it
    // back through the `span_index()` accessor (the folded product, not the live index).
    let index = build_source_span_index(root).expect("build span index");
    let blob = serde_json::to_vec(&index).expect("encode");
    let bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
        Arc::new(RdfDataset::union(&[])),
        BTreeMap::new(),
        purrdf::provenance::DatasetProvenance::new(),
        crate::stages::carrier::REP_SPAN_TABLE,
        "application/json",
        blob,
    );
    let product = StageProduct::from_bundle("stage-source-load", Arc::new(bundle));
    let folded = product
        .span_index()
        .expect("read span table back from the fold");

    assert!(
        folded.lookup("https://example.test/rootSubject").is_some(),
        "RootOntology subject must be tracked"
    );
    assert!(
        folded
            .lookup("https://example.test/sourceSubject")
            .is_some(),
        "Source subject must be tracked"
    );
    assert!(
        folded
            .lookup("https://example.test/importSubject")
            .is_none(),
        "Import subject must be SUPPRESSED by the fixed policy"
    );
}

/// EVERY slice's demonstrator corpus is admitted, not one grounding slice's. The floor is
/// stated across all three slice groups, and named witnesses are asserted in `core/` and
/// `extensions/` as well as `grounding/`: a regression that quietly narrowed the sweep back
/// to `slices/grounding/math` would still satisfy a bare non-empty count.
#[test]
fn example_files_admits_every_slice_group_not_just_math() {
    let root = repo_root();
    let files = example_files(&root).expect("list every slice's examples");
    assert!(
        files.windows(2).all(|pair| pair[0] < pair[1]),
        "files must be sorted"
    );
    assert!(
        files
            .iter()
            .all(|path| path.extension().is_some_and(|ext| ext == "ttl")),
        "only .ttl demonstrators are admitted"
    );
    let group_count = |group: &str| {
        files
            .iter()
            .filter(|path| {
                path.to_string_lossy()
                    .contains(&format!("/slices/{group}/"))
            })
            .count()
    };
    for group in ["core", "extensions", "grounding"] {
        assert!(
            group_count(group) > 0,
            "slices/{group}/*/examples must be admitted, saw none"
        );
    }
    assert!(
        group_count("core") > group_count("grounding") / 2,
        "the core slices' corpus is a first-class member, not a rounding error: \
             core={} grounding={}",
        group_count("core"),
        group_count("grounding")
    );
    for witness in [
        "slices/core/inference/examples",
        "slices/extensions/finance/examples",
        "slices/grounding/math/examples/alpha-equivalent-twins.ttl",
    ] {
        assert!(
            files
                .iter()
                .any(|path| path.to_string_lossy().contains(witness)),
            "the corpus must reach {witness}"
        );
    }
}

#[test]
fn missing_directory_listings_are_empty_not_errors() {
    // `sorted_dirs` / `ttl_files_in` treat an absent directory as an empty listing
    // (NotFound → Ok(empty)), so the discovery helpers on a root with no `slices`/
    // `imports` tree return empty rather than erroring.
    let empty = tempfile::tempdir().unwrap();
    let root = empty.path();
    assert!(sorted_dirs(&root.join("slices")).unwrap().is_empty());
    assert!(ttl_files_in(&root.join("imports")).unwrap().is_empty());
    assert!(module_files(root).unwrap().is_empty());
    assert!(all_manifest_files(root).unwrap().is_empty());
}

/// A grounding-surface demonstrator (a slice example authoring a `logic:GroundingCorrespondence`)
/// is owned by `graph/correspondence-laws`, NOT the object-level `graph/examples` corpus. Its
/// representative out-of-fragment `logic:`-native constructs (`logic:inverseFunctionalProperty`,
/// `logic:oneOf`, …) are grounding DEMONSTRATIONS, not production object-level axioms, so they
/// must never enter the reasoned object-level EDB — where the native DL path would honestly but
/// uselessly WITHHOLD on them and red `reason-verify`. Regression for the `owl:`→`logic:`
/// authoring migration (the flip made these constructs visible to `scan_coverage`).
#[test]
fn grounding_surface_demonstrator_stays_out_of_object_level_examples() {
    // Unit: the recognizer keys on the authored `logic:GroundingCorrespondence` type.
    let demonstrator = turtle_bytes_to_dataset(
        b"@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
              @prefix ex: <https://blackcatinformatics.ca/gmeow/examples/grounding-bridge/> .\n\
              ex:bridge a logic:GroundingCorrespondence .\n\
              ex:identifiedBy a logic:inverseFunctionalProperty .\n",
        "test-grounding-surface-demonstrator",
    )
    .expect("parse demonstrator fixture");
    assert!(is_grounding_surface_demonstrator(demonstrator.as_ref()));
    assert!(
        !belongs_in_object_level_examples(demonstrator.as_ref()),
        "a grounding-surface demonstrator is correspondence-law material"
    );
    let ordinary = turtle_bytes_to_dataset(
        b"@prefix ex: <https://blackcatinformatics.ca/gmeow/examples/> .\n\
              ex:a ex:knows ex:b .\n",
        "test-ordinary-demonstrator",
    )
    .expect("parse ordinary fixture");
    assert!(!is_grounding_surface_demonstrator(ordinary.as_ref()));
    assert!(
        belongs_in_object_level_examples(ordinary.as_ref()),
        "an ordinary positive demonstrator belongs in the object-level graph"
    );
}
