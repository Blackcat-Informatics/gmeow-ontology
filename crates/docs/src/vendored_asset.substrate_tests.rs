// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const OK: Result<&str, &str> = Ok("purrdf 0.12.0; wasm-bindgen 0.2.125; binaryen version_130");

#[test]
fn refresh_hashes_the_new_substrate_stamp() {
    // Entirely synthetic asset metadata; no repository corpus is produced.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let asset = VendoredWasmAsset {
        name: "synthetic",
        emitted_files: &[],
        vendored_files: &["module.wasm", SUBSTRATE_RECORD],
        wasm_file: "module.wasm",
        min_wasm_len: 7,
        export_checks: &[],
        refresh_target: "synthetic-refresh",
        witness_attestation: None,
    };
    let dir = asset.asset_dir(root);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("module.wasm"), b"\0asm\x01\0\0\0").unwrap();
    std::fs::write(dir.join(SUBSTRATE_RECORD), "old substrate\n").unwrap();
    std::fs::write(root.join("Makefile"), "BINARYEN_VER := version_130\n").unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace.dependencies]\npurrdf = '2'\npurrdf-core = '2'\n",
    )
    .unwrap();
    let mut lock =
        "version = 4\n[[package]]\nname = 'wasm-bindgen'\nversion = '0.2.125'\n".to_owned();
    for name in ["purrdf", "purrdf-core"] {
        lock.push_str(&format!("[[package]]\nname = '{name}'\nversion = '2.0.0'\nsource = 'registry+https://github.com/rust-lang/crates.io-index'\nchecksum = '{}'\n", "a".repeat(64)));
    }
    std::fs::write(root.join("Cargo.lock"), lock).unwrap();
    // gmeow-test-input: synthetic-only
    asset.refresh_manifest(root);
    assert_eq!(
        std::fs::read_to_string(dir.join(SUBSTRATE_RECORD)).unwrap(),
        "purrdf 2.0.0; wasm-bindgen 0.2.125; binaryen version_130\n"
    );
    let manifest = std::fs::read(dir.join(DIGEST_MANIFEST)).unwrap();
    // Read-only verification cannot rewrite the final publication.
    asset.verify(root);
    assert_eq!(std::fs::read(dir.join(DIGEST_MANIFEST)).unwrap(), manifest);
}

#[test]
fn agreeing_substrate_is_current() {
    assert!(
        substrate_verdict("query", "maint-refresh-query-asset", OK, OK).is_none(),
        "matching records must report current"
    );
}

#[test]
fn a_missing_stamp_is_a_failure() {
    let out = substrate_verdict::<&str, &str>(
        "query",
        "maint-refresh-query-asset",
        Err("No such file or directory"),
        OK,
    )
    .expect("a missing stamp must not report current");
    assert!(out.contains("has no SUBSTRATE.txt"), "{out}");
    assert!(out.contains("maint-refresh-query-asset"), "{out}");
}

#[test]
fn a_mismatched_stamp_is_a_failure() {
    let out = substrate_verdict::<&str, &str>(
        "query",
        "maint-refresh-query-asset",
        Ok("purrdf 0.11.0; wasm-bindgen 0.2.125; binaryen version_130"),
        OK,
    )
    .expect("a stale stamp must not report current");
    assert!(out.contains("0.11.0"), "{out}");
    assert!(out.contains("DIFFERENT substrate"), "{out}");
}

#[test]
fn an_unreadable_workspace_pin_is_a_failure_not_agreement() {
    let out = substrate_verdict::<&str, &str>(
        "query",
        "maint-refresh-query-asset",
        OK,
        Err("Cargo.lock: cannot read"),
    )
    .expect("an unreadable pin must not report current");
    assert!(
        out.contains("failed comparison, not agreement"),
        "the unreadable-pin branch must say so plainly: {out}"
    );
}
