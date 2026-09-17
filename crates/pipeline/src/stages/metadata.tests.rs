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

fn authenticated_native_metadata(path: &str) -> std::sync::Arc<RdfDataset> {
    let native_path = NATIVE_PRODUCTS
        .iter()
        .find_map(|(public, native)| (*public == path).then_some(*native))
        .expect("every metadata document has a native product");
    let bytes =
        crate::fixture::authenticated_artifact(&repo_root(), "stage-export-metadata", native_path)
            .expect("authenticated native metadata; tests never regenerate or reparse it");
    purrdf::restore_pack(&bytes).expect("restore the producer's exact metadata dataset")
}

/// Read a `void:<key>` integer literal from the authenticated producer
/// artifact so this semantic check never consults the materialized tree.
fn authenticated_void_stat(dataset: &RdfDataset, key: &str) -> u64 {
    let predicate = format!("{VOID}{key}");
    let mut found: Option<u64> = None;
    for q in dataset.owned_quads() {
        if term_iri(&q.subject) != Some(VOID_DATASET_IRI) || q.predicate != predicate {
            continue;
        }
        if let RdfTerm::Literal(RdfLiteral { lexical_form, .. }) = &q.object {
            found = Some(
                lexical_form
                    .parse()
                    .unwrap_or_else(|_| panic!("void:{key} not an integer: {lexical_form}")),
            );
        }
    }
    found.unwrap_or_else(|| panic!("authenticated void.ttl lacks void:{key} on dataset"))
}

#[test]
fn authenticated_metadata_stats_are_non_zero() {
    let dataset = authenticated_native_metadata(VOID_PATH);
    for key in ["triples", "entities", "classes", "properties"] {
        assert!(
            authenticated_void_stat(&dataset, key) > 0,
            "authenticated void:{key} census must be non-zero"
        );
    }
}

#[test]
fn metadata_census_counts_canonical_logic_typing() {
    let dataset = purrdf::parse_dataset(
        br#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
                @prefix ex: <https://blackcatinformatics.ca/gmeow/test/> .
                ex:Class a logic:Class .
                ex:object a logic:ObjectProperty .
                ex:data a logic:DatatypeProperty .
                ex:annotation a logic:AnnotationProperty ."#,
        "text/turtle",
        None,
    )
    .expect("canonical logic typing fixture parses");
    let stats = fold_stats(&dataset).expect("metadata census succeeds");
    assert_eq!(stats.classes, 1);
    assert_eq!(stats.properties, 3);
}

#[test]
/// Generated VoID/DCAT prose exposes only public `@en` language tags.
fn authenticated_external_metadata_uses_only_public_english_tags() {
    let root = repo_root();
    for path in [VOID_PATH, DCAT_PATH] {
        let artifact = crate::fixture::authenticated_artifact(&root, "stage-export-metadata", path)
            .unwrap_or_else(|error| {
                panic!("authenticated {path}; tests never produce it: {error}")
            });
        let dataset = authenticated_native_metadata(path);
        let languages: Vec<String> = dataset
            .owned_quads()
            .filter_map(|quad| match quad.object {
                RdfTerm::Literal(RdfLiteral { language, .. }) => language,
                _ => None,
            })
            .collect();
        assert!(
            !languages.is_empty(),
            "{path} carries language-tagged prose"
        );
        assert!(
            languages.iter().all(|language| language == "en"),
            "{path} publishes only public English language tags: {languages:?}"
        );
        assert!(
            !String::from_utf8_lossy(&artifact).contains("@x-gmeow-"),
            "{path} must not leak internal carrier language tags"
        );
    }
}
