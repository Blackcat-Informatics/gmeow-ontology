// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

#[path = "../src/extract.rs"]
mod extract;

#[path = "support/native_json.rs"]
mod native_json;

use purrdf_lex::json::{self, record::ToJson};

/// Pin actual CLI packets and require extracted source bodies to remain exact.
#[test]
fn module_extraction_preserves_exact_bodies_paths_and_module_names() {
    let root = tempfile::tempdir().unwrap();
    let source = "pub fn production() {}\n#[cfg(test)] mod tests {\n use super::*;\n #[test] fn original_name() { assert_eq!(include_str!(\"snapshot.txt\"), \"fixed\"); }\n}\n";
    std::fs::write(root.path().join("lib.rs"), source).unwrap();
    std::fs::write(root.path().join("snapshot.txt"), "fixed").unwrap();
    let plan = extract::plan(root.path(), &["lib.rs".into()]).unwrap();
    assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
    assert_eq!(plan.preservation.len(), 1);
    let sources = root.path().join("sources.json");
    std::fs::write(&sources, purrdf_lex::json::record::to_vec(&vec!["lib.rs"])).unwrap();
    let plan_path = root.path().join("plan.json");
    assert!(
        std::process::Command::new(env!("CARGO_BIN_EXE_extract-tests"))
            .arg("plan")
            .arg(root.path())
            .arg(&sources)
            .arg(&plan_path)
            .status()
            .unwrap()
            .success()
    );
    let plan_bytes = std::fs::read(&plan_path).unwrap();
    native_json::assert_reference(
        "extraction-plan",
        &plan_bytes,
        &gmeow_build_inputs::digest(&plan_bytes),
    );

    assert_eq!(plan.preservation[0].function_bodies[0].0, "original_name");
    assert_eq!(
        std::fs::read_to_string(root.path().join("lib.rs")).unwrap(),
        source,
        "planning is read-only"
    );
    let preservation_path = root.path().join("preservation.json");
    assert!(
        std::process::Command::new(env!("CARGO_BIN_EXE_extract-tests"))
            .arg("apply")
            .arg(root.path())
            .arg(&plan_path)
            .arg(&preservation_path)
            .status()
            .unwrap()
            .success()
    );
    let proof_bytes = std::fs::read(preservation_path).unwrap();
    native_json::assert_reference(
        "extraction-preservation",
        &proof_bytes,
        &gmeow_build_inputs::digest(&proof_bytes),
    );
    let parent = std::fs::read_to_string(root.path().join("lib.rs")).unwrap();
    assert!(parent.contains("mod tests"));
    assert!(parent.contains("#[path = \"lib.tests.rs\"]"));
    let moved = std::fs::read_to_string(root.path().join("lib.tests.rs")).unwrap();
    let proof = &plan.preservation[0];
    assert_eq!(
        &source[proof.original_range[0]..proof.original_range[1]],
        &moved[proof.extracted_range[0]..proof.extracted_range[1]]
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("snapshot.txt")).unwrap(),
        "fixed"
    );
}

/// Reject malformed evidence and changed plans before source modification.
#[test]
fn changing_the_plan_body_or_original_input_refuses_application() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("lib.rs");
    std::fs::write(
        &path,
        "#[cfg(test)] mod tests { #[test] fn original() { assert_eq!(1,1); } }",
    )
    .unwrap();
    let mut plan = extract::plan(root.path(), &["lib.rs".into()]).unwrap();
    for elements in [vec!["original"], vec!["original", "digest", "extra"]] {
        let mut invalid = plan.to_json();
        invalid["preservation"][0]["function_bodies"][0] = elements.to_json();
        let path = root.path().join("invalid-plan.json");
        std::fs::write(&path, purrdf_lex::json::record::to_vec(&invalid)).unwrap();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_extract-tests"))
            .arg("apply")
            .arg(root.path())
            .arg(&path)
            .arg(root.path().join("invalid-proof.json"))
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("function_bodies")
        );
        assert!(!root.path().join("lib.tests.rs").exists());
    }

    plan.replacements
        .iter_mut()
        .find(|replacement| replacement.before_sha256.is_none())
        .unwrap()
        .contents
        .push(' ');
    // An edit inside the claimed body cannot be authorized by a copied digest.
    let replacement = plan
        .replacements
        .iter_mut()
        .find(|replacement| replacement.before_sha256.is_none())
        .unwrap();
    replacement.contents = replacement
        .contents
        .replace("assert_eq!(1,1)", "assert_eq!(1,2)");
    assert!(extract::apply(root.path(), &plan).is_err());
    let plan = extract::plan(root.path(), &["lib.rs".into()]).unwrap();
    std::fs::write(&path, "pub fn concurrently_changed() {}").unwrap();
    assert!(extract::apply(root.path(), &plan).is_err());
}

#[test]
fn semantic_relocations_are_reviewed_explicitly_instead_of_rewritten_blindly() {
    for source in [
        "#[cfg(test)] fn helper() {}",
        "#[test] fn standalone() { assert!(true); }",
        "mod inline_parent { #[cfg(test)] mod tests { #[test] fn nested() {} } }",
        "struct S { #[cfg(test)] counter: usize }",
        "struct S; impl S { #[cfg(test)] fn helper(&self) {} }",
        "#[cfg(test)] mod tests { mod external; }",
        "#[path=\"original\"] #[cfg(test)] mod tests { #[test] fn body() {} }",
        "#[cfg_attr(unix,path=\"original\")] #[cfg(test)] mod tests { #[test] fn body() {} }",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("lib.rs"), source).unwrap();
        let plan = extract::plan(root.path(), &["lib.rs".into()]).unwrap();
        assert!(!plan.blockers.is_empty(), "{source}");
        assert!(extract::apply(root.path(), &plan).is_err());
        assert_eq!(
            std::fs::read_to_string(root.path().join("lib.rs")).unwrap(),
            source
        );
    }
}

#[test]
fn unicode_columns_preserve_the_original_utf8_body_bytes() {
    let root = tempfile::tempdir().unwrap();
    let source = r#"const SIGN: &str = "🐈 é"; #[cfg(test)] mod tests { #[test] fn original() { assert_eq!("élan 🐈", "élan 🐈"); } }"#;
    std::fs::write(root.path().join("lib.rs"), source).unwrap();
    let plan = extract::plan(root.path(), &["lib.rs".into()]).unwrap();
    assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
    extract::apply(root.path(), &plan).unwrap();
    let proof = &plan.preservation[0];
    let extracted = std::fs::read_to_string(root.path().join(&proof.destination)).unwrap();
    assert_eq!(
        &source[proof.original_range[0]..proof.original_range[1]],
        &extracted[proof.extracted_range[0]..proof.extracted_range[1]]
    );
}

#[test]
fn changing_the_parent_declaration_or_production_bytes_refuses_application() {
    for (before, after) in [
        ("#[cfg(test)]", "#[cfg(any())]"),
        ("mod tests", "mod renamed"),
        ("pub fn production() {}", ""),
    ] {
        let root = tempfile::tempdir().unwrap();
        let source = "pub fn production() {}\n#[cfg(test)] mod tests { #[test] fn required() { assert_eq!(2, 2); } }";
        std::fs::write(root.path().join("lib.rs"), source).unwrap();
        let mut plan = extract::plan(root.path(), &["lib.rs".into()]).unwrap();
        let parent = plan
            .replacements
            .iter_mut()
            .find(|replacement| replacement.before_sha256.is_some())
            .unwrap();
        assert!(parent.contents.contains(before));
        parent.contents = parent.contents.replace(before, after);
        assert!(extract::apply(root.path(), &plan).is_err());
        assert_eq!(
            std::fs::read_to_string(root.path().join("lib.rs")).unwrap(),
            source
        );
        assert!(!root.path().join("lib.tests.rs").exists());
    }
}

/// Preserve historical formatter records and reject changes beyond formatting.
#[test]
fn formatter_evidence_accepts_spacing_but_rejects_assertion_or_path_changes() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='format-proof'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("lib.rs"),
        "#[cfg(test)] mod tests { #[test] fn retained() { assert_eq!(1,1); } }",
    )
    .unwrap();
    let plan = extract::plan(root.path(), &["lib.rs".into()]).unwrap();
    extract::apply(root.path(), &plan).unwrap();
    let moved = root.path().join("lib.tests.rs");
    let initial = std::fs::read_to_string(&moved).unwrap();
    std::fs::write(
        &moved,
        initial.replace("assert_eq!(1,1)", "assert_eq!( 1, 1 )"),
    )
    .unwrap();
    let plan_path = root.path().join("format-plan.json");
    std::fs::write(&plan_path, json::write_pretty(&plan.to_json())).unwrap();
    let evidence_path = root.path().join("format-evidence.json");
    assert!(
        std::process::Command::new(env!("CARGO_BIN_EXE_extract-tests"))
            .arg("verify-format")
            .arg(root.path())
            .arg(&plan_path)
            .arg(&evidence_path)
            .status()
            .unwrap()
            .success()
    );
    let evidence_bytes = std::fs::read(evidence_path).unwrap();
    let historical: Vec<extract::FormattedPreservation> =
        purrdf_lex::json::record::from_slice(&native_json::original("extraction-format").1)
            .unwrap();
    let historical_bytes = json::write_pretty(&historical.to_json()).into_bytes();
    native_json::assert_reference(
        "extraction-format",
        &historical_bytes,
        &gmeow_build_inputs::digest(&historical_bytes),
    );
    let fresh: Vec<extract::FormattedPreservation> =
        purrdf_lex::json::record::from_slice(&evidence_bytes).unwrap();
    assert_eq!(fresh.len(), 2);
    std::fs::write(
        &moved,
        initial.replace("assert_eq!(1,1)", "assert_eq!(1,2)"),
    )
    .unwrap();
    assert!(extract::verify_format(root.path(), &plan).is_err());
    std::fs::write(&moved, initial).unwrap();
    let parent = root.path().join("lib.rs");
    let original = std::fs::read_to_string(&parent).unwrap();
    std::fs::write(&parent, original.replace("lib.tests.rs", "other.rs")).unwrap();
    assert!(extract::verify_format(root.path(), &plan).is_err());
}
