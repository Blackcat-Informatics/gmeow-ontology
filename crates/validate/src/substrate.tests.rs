// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use sha2::{Digest, Sha256};

fn package(name: &str, version: &str, checksum: &str) -> String {
    let checksum = format!("{:x}", Sha256::digest(checksum.as_bytes()));
    format!(
        "[[package]]\nname = \"{name}\"\nversion = \"{version}\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"{checksum}\"\n"
    )
}

fn lock(version: &str, checksum: &str) -> String {
    format!(
        "version = 4\n{}{}",
        package("purrdf", version, checksum),
        package("purrdf-core", version, "core-release")
    )
}

fn fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("fuzz")).unwrap();
    for (file, text) in [
        (
            "Cargo.toml",
            "[workspace.dependencies]\npurrdf = \"3\"\npurrdf-core = \"3\"\n".to_owned(),
        ),
        (
            "fuzz/Cargo.toml",
            "[workspace]\n[dependencies]\npurrdf = \"3\"\n".to_owned(),
        ),
        ("Cargo.lock", lock("3.0.0", "same-release")),
        ("fuzz/Cargo.lock", lock("3.0.0", "same-release")),
    ] {
        std::fs::write(temp.path().join(file), text).unwrap();
    }
    temp
}

#[test]
fn compatibility_range_retains_the_exact_locked_release() {
    let temp = fixture();
    let root = temp.path();
    let resolution = resolve_purrdf(&root.join("Cargo.toml"), &root.join("Cargo.lock")).unwrap();
    assert_eq!(resolution.requirement, "^3");
    assert_eq!(resolution.version, "3.0.0");
    verify_fuzz_substrate(root).unwrap();
    std::fs::write(root.join("Cargo.lock"), lock("4.0.0", "outside-range")).unwrap();
    assert!(resolve_purrdf(&root.join("Cargo.toml"), &root.join("Cargo.lock")).is_err());
}

#[test]
fn matching_ranges_do_not_hide_fuzz_release_or_source_drift() {
    let temp = fixture();
    let root = temp.path();
    for changed in [
        lock("3.0.1", "other-release"),
        lock("3.0.0", "other-checksum"),
        format!(
            "{}{}",
            lock("3.0.0", "same-release"),
            package("purrdf", "3.0.1", "duplicate")
        ),
    ] {
        std::fs::write(root.join("fuzz/Cargo.lock"), changed).unwrap();
        assert!(verify_fuzz_substrate(root).is_err());
    }
    std::fs::remove_file(root.join("fuzz/Cargo.lock")).unwrap();
    assert!(verify_fuzz_substrate(root).is_err());
}

#[test]
fn identical_umbrella_versions_do_not_hide_a_different_core() {
    let temp = fixture();
    let root = temp.path();
    let changed = format!(
        "version = 4\n{}{}",
        package("purrdf", "3.0.0", "same-release"),
        package("purrdf-core", "3.0.0", "different-core")
    );
    std::fs::write(root.join("Cargo.lock"), &changed).unwrap();
    assert!(verify_fuzz_substrate(root).is_err());
    std::fs::write(root.join("fuzz/Cargo.lock"), changed).unwrap();
    verify_fuzz_substrate(root).unwrap();
}

#[test]
fn direct_core_and_fuzz_requirements_are_the_same_registry_contract() {
    let temp = fixture();
    let root = temp.path();
    for declaration in [
        "",
        "purrdf-core = '4'",
        "purrdf-core = { version = '3', path = '../purrdf' }",
        "purrdf-core = { version = '3', package = 'different-core' }",
        "purrdf-core = { version = '3', git = 'https://example.org/purrdf', rev = '0123456789abcdef0123456789abcdef01234567' }",
    ] {
        std::fs::write(
            root.join("Cargo.toml"),
            format!("[workspace.dependencies]\npurrdf = '3'\n{declaration}\n"),
        )
        .unwrap();
        assert!(verify_fuzz_substrate(root).is_err(), "{declaration}");
    }
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace.dependencies]\npurrdf = { version = '3' }\npurrdf-core = '3'\n",
    )
    .unwrap();
    verify_fuzz_substrate(root).unwrap();
    std::fs::write(
        root.join("fuzz/Cargo.toml"),
        "[workspace]\n[dependencies]\npurrdf = '>=3, <4'\n",
    )
    .unwrap();
    assert!(verify_fuzz_substrate(root).is_err());
}

#[test]
fn each_linked_component_has_one_authenticated_registry_release() {
    let temp = fixture();
    let root = temp.path();
    let valid = lock("3.0.0", "same-release");
    let invalid = [
        package("purrdf", "3.0.0", "same-release"),
        format!("{valid}{}", package("purrdf-gts", "3.0.1", "wrong-version")),
        valid.replace(
            "registry+https://github.com/rust-lang/crates.io-index",
            "git+https://example.org/purrdf",
        ),
        valid.replace(
            &format!("{:x}", Sha256::digest(b"core-release")),
            "incomplete",
        ),
    ];
    for changed in invalid {
        std::fs::write(root.join("Cargo.lock"), &changed).unwrap();
        std::fs::write(root.join("fuzz/Cargo.lock"), changed).unwrap();
        assert!(verify_fuzz_substrate(root).is_err());
    }
}

#[test]
fn no_requirement_may_admit_a_release_below_the_floor() {
    let temp = fixture();
    let root = temp.path();
    for requirement in ["2", ">=2", "*", "<4", ">2.5", "~2.9", ">=2.0.0, <4"] {
        std::fs::write(
            root.join("Cargo.toml"),
            format!(
                "[workspace.dependencies]\npurrdf = '{requirement}'\npurrdf-core = '{requirement}'\n"
            ),
        )
        .unwrap();
        assert!(
            resolve_purrdf(&root.join("Cargo.toml"), &root.join("Cargo.lock")).is_err(),
            "{requirement}"
        );
    }
    for requirement in ["3", "^3.0", ">=3", ">2", "=3.0.0", ">=3, <5"] {
        let parsed = semver::VersionReq::parse(requirement).unwrap();
        assert!(excludes_below_floor(&parsed), "{requirement}");
    }
}

#[test]
fn every_declared_component_shares_the_floor_bounded_requirement() {
    let temp = fixture();
    let root = temp.path();
    for component in ["purrdf-lex = '2'", "purrdf-ed25519 = '>=3, <5'"] {
        std::fs::write(
            root.join("Cargo.toml"),
            format!("[workspace.dependencies]\npurrdf = '3'\npurrdf-core = '3'\n{component}\n"),
        )
        .unwrap();
        assert!(verify_fuzz_substrate(root).is_err(), "{component}");
    }
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace.dependencies]\npurrdf = '3'\npurrdf-core = '3'\npurrdf-lex = '3'\n",
    )
    .unwrap();
    verify_fuzz_substrate(root).unwrap();
}

#[test]
fn a_single_locked_component_below_the_floor_is_refused() {
    let temp = fixture();
    let root = temp.path();
    let stray = format!(
        "version = 4\n{}{}",
        package("purrdf", "2.0.0", "same-release"),
        package("purrdf-core", "2.0.0", "core-release")
    );
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace.dependencies]\npurrdf = '>=2'\npurrdf-core = '>=2'\n",
    )
    .unwrap();
    std::fs::write(root.join("Cargo.lock"), &stray).unwrap();
    let error = locked_purrdf_packages(&root.join("Cargo.lock")).unwrap_err();
    assert!(error.to_string().contains("floor"), "{error}");
    let mixed = format!(
        "{}{}",
        lock("3.0.0", "same-release"),
        package("purrdf-lex", "2.0.0", "old-lex")
    );
    std::fs::write(root.join("Cargo.lock"), mixed).unwrap();
    assert!(locked_purrdf_packages(&root.join("Cargo.lock")).is_err());
}
