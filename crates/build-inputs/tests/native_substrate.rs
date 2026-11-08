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

/// Build a small registry inventory with independently mutable package checksums.
fn registry_lock(facade: &str, lex: &str, leaf: &str) -> String {
    let source = "registry+https://github.com/rust-lang/crates.io-index";
    [("purrdf", facade), ("purrdf-lex", lex), ("purrdf-hash", leaf)]
        .into_iter()
        .map(|(name, checksum)| format!(
            "[[package]]\nname='{name}'\nversion='4.0.0'\nsource='{source}'\nchecksum='{checksum}'\n"
        ))
        .collect()
}

/// Every substrate leaf affects identity, while malformed source proof is refused.
#[test]
fn registry_substrate_binds_every_leaf_and_rejects_unproven_resolution() {
    let root = tempfile::tempdir().unwrap();
    let manifest = "[workspace.dependencies]\npurrdf='=4'\n";
    std::fs::write(root.path().join("Cargo.toml"), manifest).unwrap();
    let facade = "a".repeat(64);
    let lex = "b".repeat(64);
    let leaf = "c".repeat(64);
    let lock = registry_lock(&facade, &lex, &leaf);
    let identity = |lock: &str| {
        std::fs::write(root.path().join("Cargo.lock"), lock).unwrap();
        gmeow_build_inputs::native_substrate_source(root.path())
    };
    let first = identity(&lock).unwrap();
    assert!(first.contains("#purrdf@4.0.0;checksum="));
    assert!(first.contains(&facade));
    assert_ne!(
        first,
        identity(&registry_lock(&facade, &lex, &"d".repeat(64))).unwrap()
    );
    assert_ne!(
        first,
        identity(&registry_lock(&facade, &"d".repeat(64), &leaf)).unwrap()
    );
    let mut records: Vec<_> = lock
        .split("[[package]]")
        .filter(|row| !row.is_empty())
        .collect();
    records.reverse();
    let reordered: String = records
        .iter()
        .map(|row| format!("[[package]]{row}"))
        .collect();
    assert_eq!(first, identity(&reordered).unwrap());
    for invalid in [
        registry_lock(&facade, &lex, &leaf).replace(&leaf, "bad"),
        registry_lock(&facade, &lex, &leaf).replace(&leaf, &"A".repeat(64)),
        registry_lock(&facade, &lex, &leaf).replace("name='purrdf-lex'", "name='other'"),
        registry_lock(&facade, &lex, &leaf).replace("name='purrdf'", "name='other'"),
        format!("{lock}{lock}"),
        lock.replacen("version='4.0.0'", "version='3.0.0'", 1),
        lock.replace("version='4.0.0'", "version='3.0.0'"),
        lock.replace(
            "registry+https://github.com/rust-lang/crates.io-index",
            "registry+https://example.invalid/index",
        ),
    ] {
        assert!(identity(&invalid).is_err(), "{invalid}");
    }
    identity(&lock).unwrap();
    for declaration in [
        "purrdf={path='vendor'}",
        "purrdf={version='=4',package='other'}",
        "purrdf={version='=4',registry='other'}",
        "purrdf={version='=5'}",
    ] {
        std::fs::write(
            root.path().join("Cargo.toml"),
            format!("[workspace.dependencies]\n{declaration}\n"),
        )
        .unwrap();
        assert!(gmeow_build_inputs::native_substrate_source(root.path()).is_err());
    }
    std::fs::write(root.path().join("Cargo.toml"), manifest).unwrap();
    std::fs::create_dir(root.path().join(".cargo")).unwrap();
    std::fs::write(
        root.path().join(".cargo/config.toml"),
        "[source.crates-io]\nreplace-with='other'\n",
    )
    .unwrap();
    assert!(gmeow_build_inputs::native_substrate_source(root.path()).is_err());
}
