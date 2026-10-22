// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Inert authored source trees exercise the real fixture input key only.
//! No implementation discovery, model construction or corpus loader is invoked.

use super::*;

const VOCABULARY: &str = "<https://blackcatinformatics.ca/gmeow/test-dsl> a <https://blackcatinformatics.ca/logic/Ontology> .\n";

fn authored_root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("dsl/tests/i18n")).unwrap();
    fs::write(root.path().join("dsl/tests/vocabulary.ttl"), VOCABULARY).unwrap();
    for language in ["fr", "zh"] {
        fs::write(
            root.path().join(format!("dsl/tests/i18n/{language}.po")),
            format!("msgid \"\"\nmsgstr \"\"\n\"Language: {language}\\n\"\n"),
        )
        .unwrap();
    }
    root
}

#[test]
fn cq_rationale_fixture_key_binds_test_vocabulary_and_both_catalogs() {
    let root = authored_root();
    let original = cache_key_with_implementation(root.path(), "authored-implementation");
    for path in [
        "dsl/tests/vocabulary.ttl",
        "dsl/tests/i18n/fr.po",
        "dsl/tests/i18n/zh.po",
    ] {
        let path = root.path().join(path);
        let bytes = fs::read(&path).unwrap();
        let mut changed = bytes.clone();
        changed.extend_from_slice(b"# independently authored byte change\n");
        fs::write(&path, changed).unwrap();
        assert_ne!(
            original,
            cache_key_with_implementation(root.path(), "authored-implementation"),
            "{}",
            path.display()
        );
        fs::write(&path, bytes).unwrap();
        assert_eq!(
            original,
            cache_key_with_implementation(root.path(), "authored-implementation")
        );
    }
}

#[test]
fn cq_rationale_fixture_key_refuses_partial_or_wrong_selected_dsl_owner() {
    let root = authored_root();
    let vocabulary = root.path().join("dsl/tests/vocabulary.ttl");
    fs::remove_file(&vocabulary).unwrap();
    assert!(
        std::panic::catch_unwind(|| cache_key_with_implementation(
            root.path(),
            "authored-implementation"
        ))
        .is_err()
    );
    fs::write(
        &vocabulary,
        VOCABULARY.replace("gmeow/test-dsl", "gmeow/foreign-dsl"),
    )
    .unwrap();
    assert!(
        std::panic::catch_unwind(|| cache_key_with_implementation(
            root.path(),
            "authored-implementation"
        ))
        .is_err()
    );
    fs::write(&vocabulary, VOCABULARY).unwrap();
    fs::remove_dir_all(root.path().join("dsl/tests/i18n")).unwrap();
    assert!(
        std::panic::catch_unwind(|| cache_key_with_implementation(
            root.path(),
            "authored-implementation"
        ))
        .is_err()
    );
}

#[test]
fn cq_rationale_fixture_key_ignores_foreign_catalog_and_allows_fully_absent_dsl() {
    let root = tempfile::tempdir().unwrap();
    let absent = cache_key_with_implementation(root.path(), "authored-implementation");
    fs::create_dir_all(root.path().join("dsl/foreign/i18n")).unwrap();
    fs::write(
        root.path().join("dsl/foreign/i18n/fr.po"),
        b"foreign authored catalog",
    )
    .unwrap();
    assert_eq!(
        absent,
        cache_key_with_implementation(root.path(), "authored-implementation")
    );
}
