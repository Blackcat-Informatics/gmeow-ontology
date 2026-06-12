// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests of the `gts` binary against the frozen corpus —
//! pinning the §14.1 composition-tooling contract (refuse-don't-trust).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn vectors() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../generated/gts-vectors")
}

fn gts(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gts"))
        .args(args)
        .output()
        .expect("gts binary runs")
}

#[test]
fn fold_emits_nquads() {
    let v = vectors().join("01-minimal.gts");
    let out = gts(&["fold", v.to_str().unwrap()]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        text,
        "<https://example.org/Cat> <http://www.w3.org/2000/01/rdf-schema#label> \
         \"Cat\"@en .\n"
    );
}

#[test]
fn verify_flags_damage_with_exit_1() {
    let v = vectors().join("04-damaged-frame.gts");
    let out = gts(&["verify", v.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("DamagedFrame"), "ledger lists the diagnostic");
}

#[test]
fn cat_composes_clean_inputs_as_raw_concatenation() {
    let a = vectors().join("01-minimal.gts");
    let b = vectors().join("14-bnode-label.gts");
    let out = gts(&["cat", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert!(out.status.success());
    let mut raw = std::fs::read(&a).unwrap();
    raw.extend(std::fs::read(&b).unwrap());
    // §3.1: a validating composer adds checks, never transformation —
    // the output IS the byte concatenation.
    assert_eq!(out.stdout, raw);
}

#[test]
fn cat_refuses_a_damaged_input() {
    let a = vectors().join("01-minimal.gts");
    let b = vectors().join("04-damaged-frame.gts");
    let out = gts(&["cat", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("refusing"), "stderr names the refusal");
}

#[test]
fn cat_refuses_a_composition_whose_suppressions_hide_everything() {
    // 09's suppress targets its own term 0 (the Cat IRI); after the
    // value-union that hides 01-minimal's only quad too (§11) — a
    // composition that suppresses the whole graph is refused (§14.1).
    let a = vectors().join("01-minimal.gts");
    let b = vectors().join("09-suppression.gts");
    let out = gts(&["cat", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("hide every quad"), "stderr names the reason");
}

#[test]
fn cat_refuses_fewer_than_two_inputs() {
    let a = vectors().join("01-minimal.gts");
    let out = gts(&["cat", a.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn ls_lists_digest_size_and_media_type() {
    let v = vectors().join("22-inline-blob.gts");
    let out = gts(&["ls", v.to_str().unwrap()]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("blake3:"), "digest listed");
    assert!(text.contains("21"), "size listed");
    assert!(text.contains("image/webp"), "declared media type listed");
}

#[test]
fn extract_verifies_and_asserts_media_type() {
    // the frozen expectation carries the digest — read it from the corpus
    let expected: serde_json::Value = serde_json::from_slice(
        &std::fs::read(vectors().join("22-inline-blob.expected.json")).unwrap(),
    )
    .unwrap();
    let digest = expected["blobs"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    let v = vectors().join("22-inline-blob.gts");

    let out = gts(&["extract", v.to_str().unwrap(), &digest]);
    assert!(out.status.success());
    assert_eq!(out.stdout, b"not really webp bytes");

    // --mt is an assertion, never a conversion: mismatch refuses
    let out = gts(&["extract", v.to_str().unwrap(), &digest, "--mt", "image/png"]);
    assert_eq!(out.status.code(), Some(1));
    let out = gts(&[
        "extract",
        v.to_str().unwrap(),
        &digest,
        "--mt",
        "image/webp",
    ]);
    assert!(out.status.success());
}

#[test]
fn fold_exits_nonzero_on_diagnostics() {
    // damaged corpus vector: partial fold emitted, exit 1 — pipelines
    // (`gts fold … && publish`) must fail on damage
    let v = vectors().join("04-damaged-frame.gts");
    let out = gts(&["fold", v.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
}

fn tmpdir() -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "gts-cli-test-{}-{n}",
        std::process::id()
    ))
}

fn make_tree(root: &Path) {
    std::fs::create_dir_all(root.join("subdir")).unwrap();
    std::fs::write(root.join("a.txt"), "hello").unwrap();
    std::fs::write(root.join("subdir").join("b.txt"), "world").unwrap();
}

#[test]
fn pack_unpack_round_trip_bit_for_bit() {
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    make_tree(&tmp.join("src"));
    let archive = tmp.join("out.gts");
    let out = gts(&[
        "pack",
        tmp.join("src").to_str().unwrap(),
        "-o",
        archive.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let dst = tmp.join("dst");
    let out = gts(&["unpack", archive.to_str().unwrap(), "-C", dst.to_str().unwrap()]);
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(std::fs::read_to_string(dst.join("a.txt")).unwrap(), "hello");
    assert_eq!(
        std::fs::read_to_string(dst.join("subdir").join("b.txt")).unwrap(),
        "world"
    );

    // Re-packing the extracted tree yields the same bytes.
    let archive2 = tmp.join("out2.gts");
    let out = gts(&["pack", dst.to_str().unwrap(), "-o", archive2.to_str().unwrap()]);
    assert!(out.status.success());
    assert_eq!(
        std::fs::read(&archive).unwrap(),
        std::fs::read(&archive2).unwrap()
    );
}

#[test]
fn pack_deduplicates_identical_content() {
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    let src = tmp.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("a.txt"), "shared").unwrap();
    std::fs::write(src.join("b.txt"), "shared").unwrap();
    let archive = tmp.join("out.gts");
    let out = gts(&["pack", src.to_str().unwrap(), "-o", archive.to_str().unwrap()]);
    assert!(out.status.success());

    use gts::reader::read;
    let data = std::fs::read(&archive).unwrap();
    let g = read(&data, true, None);
    assert_eq!(g.blobs.len(), 1, "two files with identical content -> one blob");
}

#[test]
fn unpack_refuses_traversal() {
    use gts::writer::Writer;

    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let archive = tmp.join("traversal.gts");

    let mut w = Writer::new("files");
    w.add_terms(&[
        gts::model::Term {
            kind: gts::model::TermKind::Iri,
            value: Some("https://w3id.org/gts/files#FileEntry".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        gts::model::Term {
            kind: gts::model::TermKind::Iri,
            value: Some("https://w3id.org/gts/files#path".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        gts::model::Term {
            kind: gts::model::TermKind::Iri,
            value: Some("https://w3id.org/gts/files#digest".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        gts::model::Term {
            kind: gts::model::TermKind::Iri,
            value: Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        gts::model::Term {
            kind: gts::model::TermKind::Bnode,
            value: Some("e0".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        gts::model::Term {
            kind: gts::model::TermKind::Literal,
            value: Some("../escape.txt".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        gts::model::Term {
            kind: gts::model::TermKind::Literal,
            value: Some("blake3:0000000000000000000000000000000000000000000000000000000000000000".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
    ]);
    w.add_quads(&[(4, 3, 0, None), (4, 1, 5, None), (4, 2, 6, None)]);
    std::fs::write(&archive, w.to_bytes()).unwrap();

    let dst = tmp.join("dst");
    let out = gts(&["unpack", archive.to_str().unwrap(), "-C", dst.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn diff_reports_changes() {
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    make_tree(&tmp.join("src"));
    let archive = tmp.join("out.gts");
    let out = gts(&[
        "pack",
        tmp.join("src").to_str().unwrap(),
        "-o",
        archive.to_str().unwrap(),
    ]);
    assert!(out.status.success());

    let out = gts(&["diff", archive.to_str().unwrap(), tmp.join("src").to_str().unwrap()]);
    assert!(out.status.success(), "identical tree -> exit 0");
    assert!(out.stdout.is_empty());

    std::fs::write(tmp.join("src").join("a.txt"), "changed").unwrap();
    let out = gts(&["diff", archive.to_str().unwrap(), tmp.join("src").to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("modified: a.txt"));
}
