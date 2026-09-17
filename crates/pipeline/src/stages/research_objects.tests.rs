// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

#[test]
fn scores_ttl_rides_the_evals_product_not_disk() {
    // The scores bytes are threaded from the (consumed) evals product, never read off
    // the git-ignored generated/evals/scores.ttl: a sentinel passed as the scores bytes
    // appears verbatim as the LAST example input, labelled by the producer's SCORES_PATH.
    let tmp = tempfile::tempdir().expect("synthetic research-object root");
    for (rel, _) in AUTHORED_EXAMPLE_INPUTS {
        let path = tmp.path().join(rel);
        std::fs::create_dir_all(path.parent().expect("input parent")).unwrap();
        std::fs::write(&path, format!("# synthetic {rel}\n")).unwrap();
    }
    let sentinel = b"# sentinel scores\n".as_slice();
    let inputs = example_inputs(tmp.path(), sentinel).expect("example inputs");
    assert_eq!(inputs.len(), 6, "five authored inputs + scores.ttl");
    let (label, name, bytes) = inputs.last().expect("scores input present");
    assert_eq!(*label, crate::stages::evals::SCORES_PATH);
    assert_eq!(*label, "generated/evals/scores.ttl");
    assert_eq!(*name, "scores.ttl");
    assert_eq!(bytes.as_slice(), sentinel);
}

#[test]
fn input_files_omit_generated_scores_and_the_dag_edge_binds() {
    let stage = ResearchObjectsStage::default();
    // The DAG edge binds: the stage consumes both producers, in sorted order.
    assert_eq!(
        stage.consumes(),
        &[
            "stage-export-evals".to_string(),
            "stage-mappings".to_string()
        ]
    );
}

#[test]
fn authenticated_research_objects_have_the_complete_family() {
    let arts = crate::fixture::stage_artifacts(&repo_root(), 1, "stage-export-research-objects")
        .expect("load authenticated research-object product without rebuilding corpus");

    // Pin the family by its member-name SET, not a bare count: a count of 13 cannot catch a
    // silent membership swap (a top-level artifact migrating under `ro-crate/` while a new
    // member appears leaves the count unchanged). The four purrdf codecs + the untouched DCAT
    // CONSTRUCT project exactly these 13 logical paths.
    let base = RESEARCH_OBJECTS_DIR;
    let expected: BTreeSet<String> = [
        "lillith.croissant.jsonld",
        "lillith.datacite.xml",
        "datapackage.json",
        "lillith.dcat.ttl",
        "ro-crate/ro-crate-metadata.json",
        "ro-crate/ro-crate-preview.html",
        "ro-crate/corpus.ttl",
        "ro-crate/grounded-claim.ttl",
        "ro-crate/lillith-dataset.ttl",
        "ro-crate/lillith-pipeline.ttl",
        "ro-crate/rubric.ttl",
        "ro-crate/scores.ttl",
        "ro-crate/lillith.croissant.jsonld",
    ]
    .into_iter()
    .map(|member| format!("{base}/{member}"))
    .collect();
    let actual: BTreeSet<String> = arts.keys().cloned().collect();
    assert_eq!(
        actual, expected,
        "research-objects family membership drifted"
    );

    let text = |member: &str| {
        String::from_utf8(
            arts.get(&format!("{base}/{member}"))
                .unwrap_or_else(|| panic!("missing authenticated member {member}"))
                .clone(),
        )
        .unwrap_or_else(|error| panic!("authenticated member {member} is not UTF-8: {error}"))
    };
    assert!(text("lillith.croissant.jsonld").contains(CROISSANT_CONFORMS_TO));
    assert!(text("lillith.datacite.xml").contains(DATACITE_NS));
    assert!(text("datapackage.json").contains(purrdf::FRICTIONLESS_PROFILE));
    assert!(text("ro-crate/ro-crate-metadata.json").contains(RO_CRATE_PROFILE));
}
