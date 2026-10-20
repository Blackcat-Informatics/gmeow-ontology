// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn glossary_table_cells_escape_structural_characters() {
    assert_eq!(cell("a | b"), "a \\| b");
    assert_eq!(cell("line1\nline2\tx"), "line1 line2 x");
    assert_eq!(
        localname("https://blackcatinformatics.ca/gmeow/EntityExistence"),
        "EntityExistence"
    );
}

#[test]
fn tbx_text_and_attributes_escape_xml() {
    assert_eq!(xml_text("a & b < c > d"), "a &amp; b &lt; c &gt; d");
    assert_eq!(xml_attr("x\"y"), "x&quot;y");
}

#[test]
fn glossary_stage_inputs_share_strict_catalog_discovery() {
    let root = tempfile::tempdir().unwrap();
    let slice = root.path().join("slices/core/example");
    let i18n = slice.join("i18n");
    std::fs::create_dir_all(&i18n).unwrap();
    let module = slice.join("module.ttl");
    let catalog = i18n.join("fr.po");
    std::fs::write(&module, "# synthetic source\n").unwrap();
    std::fs::write(&catalog, "# synthetic catalog\n").unwrap();
    let stage = GlossaryTableStage;
    let mut expected = vec![module, catalog.clone()];
    expected.sort();
    assert_eq!(stage.input_files(root.path()).unwrap(), expected);
    std::fs::remove_file(catalog).unwrap();
    std::fs::remove_dir(&i18n).unwrap();
    std::fs::write(&i18n, "not a directory").unwrap();
    let failure = stage.input_files(root.path()).unwrap_err();
    assert!(failure.downcast_ref::<std::io::Error>().is_some());
    assert_eq!(
        failure.inner().source_ctx.location.path.as_deref(),
        i18n.to_str()
    );
}
