// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored input-path selection only; neither stage executes or produces data.

use super::docs_source_files;
use crate::node::Stage;
use crate::stages::mappings::MappingsStage;

fn authored_root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("slices")).unwrap();
    std::fs::create_dir_all(root.path().join("dsl/tests/i18n")).unwrap();
    std::fs::write(root.path().join("dsl/tests/vocabulary.ttl"),
        "<https://blackcatinformatics.ca/gmeow/test-dsl> a <https://blackcatinformatics.ca/logic/Ontology> .").unwrap();
    for language in ["fr", "zh"] {
        std::fs::write(
            root.path().join(format!("dsl/tests/i18n/{language}.po")),
            b"# independent catalog\n",
        )
        .unwrap();
    }
    root
}

#[test]
fn cq_rationale_pipeline_source_controllers_bind_registered_dsl_sources_and_catalogs() {
    let root = authored_root();
    std::fs::create_dir_all(root.path().join("dsl/foreign/i18n")).unwrap();
    let foreign = root.path().join("dsl/foreign/i18n/fr.po");
    std::fs::write(&foreign, b"# unrelated catalog\n").unwrap();
    for paths in [
        docs_source_files(root.path()).unwrap(),
        MappingsStage::default().input_files(root.path()).unwrap(),
    ] {
        for path in [
            "dsl/tests/vocabulary.ttl",
            "dsl/tests/i18n/fr.po",
            "dsl/tests/i18n/zh.po",
        ] {
            assert!(paths.contains(&root.path().join(path)), "missing {path}");
        }
        assert!(!paths.contains(&foreign));
    }
}

#[test]
fn cq_rationale_pipeline_source_controllers_refuse_partial_registered_dsl_owner() {
    let root = authored_root();
    std::fs::remove_file(root.path().join("dsl/tests/vocabulary.ttl")).unwrap();
    assert!(docs_source_files(root.path()).is_err());
    assert!(MappingsStage::default().input_files(root.path()).is_err());
}
