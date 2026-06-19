// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

//! End-to-end tests of the `gts` binary against the frozen corpus —
//! pinning the §14.1 composition-tooling contract (refuse-don't-trust).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn vectors() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vectors")
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

#[cfg(feature = "policy-config-yaml")]
#[test]
fn verify_policy_file_trusts_did_style_signer() {
    use ed25519_dalek::SigningKey;
    use gmeow_gts::model::{Term, TermKind};
    use gmeow_gts::wire::hex;
    use gmeow_gts::writer::Writer;

    const KID: &str = "did:example:issuer";

    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let gts_path = tmp.join("signed.gts");
    let policy_path = tmp.join("policy.yaml");

    let key = SigningKey::from_bytes(&[9u8; 32]);
    let verifier = key.verifying_key();
    let mut writer = Writer::new("evidence");
    writer.sign_with(key, KID);
    writer.add_terms(&[
        Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/claim".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/says".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Literal,
            value: Some("signed".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
    ]);
    writer.add_quads(&[(0, 1, 2, None)]);
    std::fs::write(&gts_path, writer.to_bytes()).unwrap();
    std::fs::write(
        &policy_path,
        "\
trusted_signers:
  - did:example:issuer
require_trusted_signer: true
",
    )
    .unwrap();

    let key_spec = format!("{KID}:{}", hex(&verifier.to_bytes()));
    let out = gts(&[
        "verify",
        "--key",
        &key_spec,
        "--policy",
        policy_path.to_str().unwrap(),
        gts_path.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8(out.stdout)
        .unwrap()
        .contains("signature did:example:issuer: valid"));
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
    std::env::temp_dir().join(format!("gts-cli-test-{}-{n}", std::process::id()))
}

fn make_tree(root: &Path) {
    std::fs::create_dir_all(root.join("subdir")).unwrap();
    std::fs::write(root.join("a.txt"), "hello").unwrap();
    std::fs::write(root.join("subdir").join("b.txt"), "world").unwrap();
}

#[test]
fn prove_and_verify_proof_round_trip() {
    use gmeow_gts::model::{Term, TermKind};
    use gmeow_gts::wire::hex;
    use gmeow_gts::writer::Writer;

    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let path = tmp.join("indexed.gts");
    let proof_path = tmp.join("proof.json");
    let bad_path = tmp.join("bad-proof.json");

    let mut w = Writer::new("generic");
    let target = w.add_terms(&[
        Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/Cat".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("http://www.w3.org/2000/01/rdf-schema#label".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Literal,
            value: Some("Cat".to_string()),
            datatype: None,
            lang: Some("en".to_string()),
            reifier: None,
        },
    ]);
    w.add_quads(&[(0, 1, 2, None)]);
    w.add_index_with_mmr();
    std::fs::write(&path, w.to_bytes()).unwrap();

    let target_hex = hex(&target);
    let out = gts(&["prove", path.to_str().unwrap(), &target_hex]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let proof_json = String::from_utf8(out.stdout).unwrap();
    let proof: serde_json::Value = serde_json::from_str(&proof_json).unwrap();
    assert_eq!(proof["schema"], "gts-mmr-proof-v1");
    assert_eq!(proof["frame_id"], target_hex);
    std::fs::write(&proof_path, &proof_json).unwrap();

    let out = gts(&["verify-proof", proof_path.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("proof ok"), "{text}");

    let mut bad = proof;
    let root = bad["root"].as_str().unwrap().to_string();
    let replacement = if root.starts_with('0') { "1" } else { "0" };
    bad["root"] = serde_json::Value::String(format!("{replacement}{}", &root[1..]));
    std::fs::write(&bad_path, serde_json::to_string_pretty(&bad).unwrap()).unwrap();
    let out = gts(&["verify-proof", bad_path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn replication_verbs_report_ranges_and_resume_bytes() {
    use gmeow_gts::reader::read;
    use gmeow_gts::wire::hex;

    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let first_path = vectors().join("01-minimal.gts");
    let second_path = vectors().join("14-bnode-label.gts");
    let first = std::fs::read(&first_path).unwrap();
    let second = std::fs::read(&second_path).unwrap();
    let mut data = first.clone();
    data.extend_from_slice(&second);
    let path = tmp.join("multi.gts");
    std::fs::write(&path, &data).unwrap();
    let first_head = hex(read(&first, true, None).segment_heads.last().unwrap());

    let out = gts(&["heads", path.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let heads: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(heads["schema"], "gts-replication-heads-v1");
    assert_eq!(heads["clean"], true);
    assert_eq!(heads["segment_heads"].as_array().unwrap().len(), 2);
    assert_eq!(heads["aggregate"]["count"], 2);

    let out = gts(&["segments", path.to_str().unwrap()]);
    assert!(out.status.success());
    let segments: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = segments["segments"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["byte_range"]["start"], 0);
    assert_eq!(rows[0]["byte_range"]["end"], first.len());
    assert_eq!(rows[1]["byte_range"]["start"], first.len());
    assert!(rows[0]["frame_count"].as_u64().unwrap() > 0);

    let out = gts(&[
        "missing",
        "--from-head",
        &first_head,
        path.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    let missing: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(missing["status"], "ranges");
    assert_eq!(missing["ranges"][0]["start"], first.len());
    assert_eq!(missing["ranges"][0]["end"], data.len());
    assert_eq!(missing["scan_required"], false);

    let out = gts(&["resume", "--after", &first_head, path.to_str().unwrap()]);
    assert!(out.status.success());
    assert_eq!(out.stdout, second);
}

#[test]
fn replication_verbs_handle_streamable_unknown_and_torn_inputs() {
    use gmeow_gts::reader::read;
    use gmeow_gts::wire::hex;

    let streamable = vectors().join("25b-streamable-compacted.gts");
    let out = gts(&["segments", streamable.to_str().unwrap()]);
    assert!(out.status.success());
    let segments: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let row = &segments["segments"][0];
    assert_eq!(row["layout"]["claimed"], true);
    assert!(row["layout"]["covered"].as_u64().unwrap() > 0);
    assert!(row["frame_count"].as_u64().unwrap() >= row["layout"]["covered"].as_u64().unwrap());

    let data = std::fs::read(&streamable).unwrap();
    let full_head = hex(read(&data, true, None).segment_heads.last().unwrap());
    let out = gts(&[
        "missing",
        "--from-head",
        &full_head,
        streamable.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    let complete: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(complete["status"], "complete");
    assert!(complete["ranges"].as_array().unwrap().is_empty());

    let unknown = "0000000000000000000000000000000000000000000000000000000000000000";
    let out = gts(&[
        "missing",
        "--from-head",
        unknown,
        streamable.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    let missing: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(missing["status"], "unknown");
    assert_eq!(missing["scan_required"], true);

    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let torn_path = tmp.join("torn.gts");
    let mut torn = std::fs::read(vectors().join("01-minimal.gts")).unwrap();
    torn.pop();
    std::fs::write(&torn_path, torn).unwrap();

    let out = gts(&["heads", torn_path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let heads: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(heads["torn_at"].is_number());

    let out = gts(&["resume", "--after", unknown, torn_path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
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
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let dst = tmp.join("dst");
    let out = gts(&[
        "unpack",
        archive.to_str().unwrap(),
        "-C",
        dst.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read_to_string(dst.join("a.txt")).unwrap(), "hello");
    assert_eq!(
        std::fs::read_to_string(dst.join("subdir").join("b.txt")).unwrap(),
        "world"
    );

    // Re-packing the extracted tree yields the same bytes.
    let archive2 = tmp.join("out2.gts");
    let out = gts(&[
        "pack",
        dst.to_str().unwrap(),
        "-o",
        archive2.to_str().unwrap(),
    ]);
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
    let out = gts(&[
        "pack",
        src.to_str().unwrap(),
        "-o",
        archive.to_str().unwrap(),
    ]);
    assert!(out.status.success());

    use gmeow_gts::reader::read;
    let data = std::fs::read(&archive).unwrap();
    let g = read(&data, true, None);
    assert_eq!(
        g.blobs.len(),
        1,
        "two files with identical content -> one blob"
    );
}

fn files_archive_with_path(archive_path: &str) -> Vec<u8> {
    use gmeow_gts::writer::{digest_string, Writer};

    let payload = b"path-test";
    let digest = digest_string(payload);

    let mut w = Writer::new("files");
    w.add_terms(&[
        gmeow_gts::model::Term {
            kind: gmeow_gts::model::TermKind::Iri,
            value: Some("https://w3id.org/gts/files#FileEntry".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        gmeow_gts::model::Term {
            kind: gmeow_gts::model::TermKind::Iri,
            value: Some("https://w3id.org/gts/files#path".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        gmeow_gts::model::Term {
            kind: gmeow_gts::model::TermKind::Iri,
            value: Some("https://w3id.org/gts/files#digest".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        gmeow_gts::model::Term {
            kind: gmeow_gts::model::TermKind::Iri,
            value: Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        gmeow_gts::model::Term {
            kind: gmeow_gts::model::TermKind::Bnode,
            value: Some("e0".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        gmeow_gts::model::Term {
            kind: gmeow_gts::model::TermKind::Literal,
            value: Some(archive_path.to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        gmeow_gts::model::Term {
            kind: gmeow_gts::model::TermKind::Literal,
            value: Some(digest.clone()),
            datatype: None,
            lang: None,
            reifier: None,
        },
    ]);
    w.add_quads(&[(4, 3, 0, None), (4, 1, 5, None), (4, 2, 6, None)]);
    w.add_blob(payload, None, None);
    w.to_bytes()
}

#[test]
fn unpack_refuses_traversal() {
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let archive = tmp.join("traversal.gts");
    std::fs::write(&archive, files_archive_with_path("../escape.txt")).unwrap();

    let dst = tmp.join("dst");
    let out = gts(&[
        "unpack",
        archive.to_str().unwrap(),
        "-C",
        dst.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("traversal") || stderr.contains("escapes"),
        "expected traversal refusal, got: {stderr}"
    );
}

#[test]
fn unpack_refuses_windows_style_paths() {
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    for (idx, (archive_path, want)) in [
        ("..\\..\\etc\\passwd", "traversal"),
        ("C:\\secret.txt", "drive-relative"),
    ]
    .iter()
    .enumerate()
    {
        let archive = tmp.join(format!("unsafe-{idx}.gts"));
        std::fs::write(&archive, files_archive_with_path(archive_path)).unwrap();
        let dst = tmp.join(format!("dst-{idx}"));
        let out = gts(&[
            "unpack",
            archive.to_str().unwrap(),
            "-C",
            dst.to_str().unwrap(),
        ]);
        assert_eq!(out.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains(want), "stderr: {stderr}");
    }
}

#[cfg(unix)]
#[test]
fn unpack_refuses_destination_symlink_escape() {
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    let dest = tmp.join("dst");
    let outside = tmp.join("outside");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, dest.join("link")).unwrap();
    let archive = tmp.join("symlink-escape.gts");
    std::fs::write(&archive, files_archive_with_path("link/escape.txt")).unwrap();

    let out = gts(&[
        "unpack",
        archive.to_str().unwrap(),
        "-C",
        dest.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("escapes"), "stderr: {stderr}");
    assert!(!outside.join("escape.txt").exists());
}

#[cfg(unix)]
#[test]
fn pack_refuses_symlink_entry() {
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    make_tree(&tmp.join("src"));
    std::os::unix::fs::symlink(
        tmp.join("src").join("a.txt"),
        tmp.join("src").join("linked.txt"),
    )
    .unwrap();
    let archive = tmp.join("out.gts");
    let out = gts(&[
        "pack",
        tmp.join("src").to_str().unwrap(),
        "-o",
        archive.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("symlink"), "stderr: {stderr}");
}

#[cfg(unix)]
#[test]
fn diff_refuses_symlink_entry() {
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
    std::os::unix::fs::symlink(
        tmp.join("src").join("a.txt"),
        tmp.join("src").join("linked.txt"),
    )
    .unwrap();
    let out = gts(&[
        "diff",
        archive.to_str().unwrap(),
        tmp.join("src").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("symlink"), "stderr: {stderr}");
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

    let out = gts(&[
        "diff",
        archive.to_str().unwrap(),
        tmp.join("src").to_str().unwrap(),
    ]);
    assert!(out.status.success(), "identical tree -> exit 0");
    assert!(out.stdout.is_empty());

    std::fs::write(tmp.join("src").join("a.txt"), "changed").unwrap();
    let out = gts(&[
        "diff",
        archive.to_str().unwrap(),
        tmp.join("src").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("modified: a.txt"));
}

#[test]
fn cat_refuses_suppress_everything_composition() {
    // Vector 21: the second segment suppresses every prior quad. Raw byte
    // concatenation is structurally valid GTS, but gts cat refuses it (§14.1).
    let v21 = vectors().join("21-degenerate-composition.gts");
    let out = gts(&["cat", v21.to_str().unwrap(), v21.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("hide every quad"), "stderr: {err}");
}

// ------------------------------------------------------------------------- //
// gts compact --streamable (§10.1, §14.1) + layout reporting (§3.3)
// ------------------------------------------------------------------------- //

/// An accretive source: a blob delivered before any description, then graph
/// content — mirrors the Python CLI test fixture.
fn accretive_file(path: &Path) {
    use gmeow_gts::model::{Term, TermKind};
    use gmeow_gts::writer::Writer;

    let mut w = Writer::new("generic");
    w.add_blob(&[b'Z'; 64], Some("application/octet-stream"), None);
    w.add_terms(&[
        Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/Cat".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("http://www.w3.org/2000/01/rdf-schema#label".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Literal,
            value: Some("Cat".to_string()),
            datatype: None,
            lang: Some("en".to_string()),
            reifier: None,
        },
    ]);
    w.add_quads(&[(0, 1, 2, None)]);
    std::fs::write(path, w.to_bytes()).unwrap();
}

#[test]
fn compact_requires_streamable_flag() {
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let src = tmp.join("accretive.gts");
    accretive_file(&src);
    let out = gts(&[
        "compact",
        src.to_str().unwrap(),
        "-o",
        tmp.join("x.gts").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("compact requires --streamable"),
        "stderr: {err}"
    );
}

#[test]
fn compact_verify_info_round_trip() {
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let src = tmp.join("accretive.gts");
    accretive_file(&src);
    let dst = tmp.join("streamable.gts");
    let out = gts(&[
        "compact",
        src.to_str().unwrap(),
        "-o",
        dst.to_str().unwrap(),
        "--streamable",
        "--timestamp",
        "2026-01-01T00:00:00Z",
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = gts(&["verify", dst.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("layout: streamable through frame"), "{text}");
    assert!(!text.contains("accretive tail"), "{text}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("warning"), "stderr: {err}");

    let out = gts(&["info", dst.to_str().unwrap()]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("layout: streamable through frame"), "{text}");
}

#[test]
fn compact_is_reproducible_with_fixed_timestamp() {
    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let src = tmp.join("accretive.gts");
    accretive_file(&src);
    let (a, b) = (tmp.join("a.gts"), tmp.join("b.gts"));
    for out_path in [&a, &b] {
        let out = gts(&[
            "compact",
            src.to_str().unwrap(),
            "--streamable",
            "--timestamp",
            "2026-01-01T00:00:00Z",
            "-o",
            out_path.to_str().unwrap(),
        ]);
        assert!(out.status.success());
    }
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
}

#[test]
fn verify_refuses_streamable_lie() {
    // Frozen vector 26: a covered blob delivered before its stream:digest
    // description — the claimed layout the bytes contradict (§3.3).
    let v = vectors().join("26-streamable-lie.gts");
    let out = gts(&["verify", v.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("StreamableLayoutError"), "{text}");
}

#[test]
fn info_reports_accretive_tail() {
    // Frozen vector 27: frames appended after the index footer are the legal
    // accretive tail — boundary info, never a diagnostic (§3.3).
    let v = vectors().join("27-streamable-tail.gts");
    let out = gts(&["info", v.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("layout: streamable through frame"), "{text}");
    assert!(text.contains("accretive tail 2 frame(s)"), "{text}");
}

#[test]
fn verify_warns_on_stream_vocab_without_claim() {
    // §13.3: stream# provenance in an unclaimed segment is a warning, never
    // an error — it legitimately survives nq → gts round trips.
    use gmeow_gts::model::{Term, TermKind};
    use gmeow_gts::writer::Writer;

    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let path = tmp.join("unclaimed-stream.gts");
    let mut w = Writer::new("generic");
    w.add_terms(&[
        Term {
            kind: TermKind::Bnode,
            value: Some("c".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some(gmeow_gts::stream::COMPACTION.to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Literal,
            value: Some(gmeow_gts::stream::COMPACT_AGENT.to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
    ]);
    w.add_quads(&[(0, 1, 2, None)]);
    std::fs::write(&path, w.to_bytes()).unwrap();

    let out = gts(&["verify", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "warning, exit stays 0");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("layout warning"), "stderr: {err}");
}

#[test]
fn compact_refuses_evidence_without_seal_then_seals() {
    use gmeow_gts::model::{Term, TermKind};
    use gmeow_gts::wire::digest_str;
    use gmeow_gts::writer::Writer;

    let tmp = tmpdir();
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let path = tmp.join("evidence.gts");
    let mut w = Writer::new("evidence");
    w.add_terms(&[
        Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/Cat".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("http://www.w3.org/2000/01/rdf-schema#label".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Literal,
            value: Some("Cat".to_string()),
            datatype: None,
            lang: Some("en".to_string()),
            reifier: None,
        },
    ]);
    w.add_quads(&[(0, 1, 2, None)]);
    std::fs::write(&path, w.to_bytes()).unwrap();
    let dst = tmp.join("out.gts");

    let out = gts(&[
        "compact",
        path.to_str().unwrap(),
        "-o",
        dst.to_str().unwrap(),
        "--streamable",
    ]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("seal-original"), "stderr: {err}");

    let out = gts(&[
        "compact",
        path.to_str().unwrap(),
        "-o",
        dst.to_str().unwrap(),
        "--streamable",
        "--seal-original",
    ]);
    assert_eq!(out.status.code(), Some(0));
    let out = gts(&["verify", dst.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    let extracted = tmp.join("source.gts");
    let digest = digest_str(&std::fs::read(&path).unwrap());
    let out = gts(&[
        "extract",
        dst.to_str().unwrap(),
        &digest,
        "-o",
        extracted.to_str().unwrap(),
        "--mt",
        "application/vnd.blackcat.gts+cbor-seq",
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        std::fs::read(extracted).unwrap(),
        std::fs::read(path).unwrap()
    );
}

#[test]
fn verify_flags_undeclared_files_profile() {
    use gmeow_gts::model::{Term, TermKind};
    use gmeow_gts::writer::Writer;

    let tmp = std::env::temp_dir().join("gts-verify-profile-test.gts");
    let _ = std::fs::remove_file(&tmp);
    let mut w = Writer::new("generic");
    w.add_terms(&[
        Term {
            kind: TermKind::Iri,
            value: Some("https://w3id.org/gts/files#FileEntry".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("https://w3id.org/gts/files#path".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Literal,
            value: Some("x.txt".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
    ]);
    w.add_quads(&[(0, 1, 2, None)]);
    std::fs::write(&tmp, w.to_bytes()).unwrap();

    let out = gts(&["verify", tmp.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("profile error"), "stderr: {err}");
}

#[test]
fn verify_flags_undeclared_files_profile_object_only() {
    // Regression: profile vocabulary in ordinary object position must be
    // detected, not only rdf:type objects (§14.1).
    use gmeow_gts::model::{Term, TermKind};
    use gmeow_gts::writer::Writer;

    let tmp = std::env::temp_dir().join("gts-verify-profile-obj-test.gts");
    let _ = std::fs::remove_file(&tmp);
    let mut w = Writer::new("generic");
    w.add_terms(&[
        Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/Thing".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/relatedTo".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("https://w3id.org/gts/files#FileEntry".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
    ]);
    w.add_quads(&[(0, 1, 2, None)]);
    std::fs::write(&tmp, w.to_bytes()).unwrap();

    let out = gts(&["verify", tmp.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("profile error"), "stderr: {err}");
}

#[test]
fn verify_declared_files_profile_object_only_is_not_unused() {
    // A declared profile whose term appears only as an object IRI must not
    // trigger the "declared but unused" warning.
    use gmeow_gts::model::{Term, TermKind};
    use gmeow_gts::writer::Writer;

    let tmp = std::env::temp_dir().join("gts-verify-profile-declared-obj-test.gts");
    let _ = std::fs::remove_file(&tmp);
    let mut w = Writer::new("files");
    w.add_terms(&[
        Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/Thing".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/relatedTo".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("https://w3id.org/gts/files#FileEntry".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        },
    ]);
    w.add_quads(&[(0, 1, 2, None)]);
    std::fs::write(&tmp, w.to_bytes()).unwrap();

    let out = gts(&["verify", tmp.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("profile warning"), "stderr: {err}");
}

#[test]
fn extract_key_matches_frozen_stdout() {
    // §9.2: `gts extract-key` prints kid, fingerprint, emojihash, and the
    // armored key — byte-identical to the Python-generated vector.
    let raw = std::fs::read_to_string(vectors().join("openpgp/extract-key.json")).unwrap();
    let case: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let bytes: Vec<u8> = (0..case["gts"].as_str().unwrap().len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&case["gts"].as_str().unwrap()[i..i + 2], 16).unwrap())
        .collect();
    let tmp = std::env::temp_dir().join("gts-extract-key-test.gts");
    std::fs::write(&tmp, &bytes).unwrap();

    let out = gts(&["extract-key", tmp.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        case["stdout"].as_str().unwrap()
    );
}

#[test]
fn extract_key_missing_exits_1() {
    let v = vectors().join("01-minimal.gts");
    let out = gts(&["extract-key", v.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
}
