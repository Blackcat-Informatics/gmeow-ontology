// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

#[test]
fn native_substrate_binds_the_git_source_and_refuses_mutable_selectors() {
    let root = tempfile::tempdir().unwrap();
    let manifest = root.path().join("Cargo.toml");
    let source = |git: &str, selector: &str| {
        std::fs::write(
            &manifest,
            format!("[workspace.dependencies]\npurrdf = {{ git = {git:?}, {selector} }}\n"),
        )
        .unwrap();
        gmeow_build_inputs::native_substrate_source(root.path())
    };
    let first = source(
        "https://example.org/rdf",
        "rev = '1111111111111111111111111111111111111111'",
    )
    .unwrap();
    let second = source(
        "https://example.org/rdf",
        "rev = '2222222222222222222222222222222222222222'",
    )
    .unwrap();
    let different_repo = source(
        "https://example.org/other",
        "rev = '1111111111111111111111111111111111111111'",
    )
    .unwrap();
    assert_ne!(
        first, second,
        "implementation-only changes must move the identity"
    );
    assert_ne!(first, different_repo);
    for selector in [
        "branch = 'main'",
        "tag = 'v3'",
        "rev = '1234567'",
        "rev = 'zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz'",
    ] {
        assert!(
            source("https://example.org/rdf", selector).is_err(),
            "{selector}"
        );
    }
    std::fs::write(&manifest, "[workspace.dependencies]\npurrdf = '3'\n").unwrap();
    assert!(gmeow_build_inputs::native_substrate_source(root.path()).is_err());
}
