// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Post-build binary smoke test for the shipped `gmeow` CLI — the native residue
//! of the retired `scripts/pypi_smoke.py`.
//!
//! The built binary loads the shipped bundle and answers a basic query/verify.
//! The same surface proves that `gmeow verify` is wired onto the native
//! `purrdf::gts::verify` primitive (never the external `gts` binary), so the
//! verify leg is genuinely hermetic
//! (runs correctly with `gts` absent from `PATH`) AND that it performs REAL
//! Ed25519 cryptographic verification (accepts a validly-signed, trusted
//! ephemeral bundle; rejects the same bundle under a different trusted key) —
//! not just a liveness check over the unsigned, committed dev bundle.

use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use ed25519_dalek::SigningKey;
use predicates::prelude::*;
use purrdf::gts::model::{Term, TermKind};

// ── shared helpers (mirrors the `cli.rs` / `self_sufficiency.rs` conventions) ─

/// The repo-root path of a committed validate fixture.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/validate")
        .join(name)
}

/// The repo root (this crate's manifest dir, two levels up) — used to locate
/// the committed `generated/dist/gmeow.gts`.
fn repo_root() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    path.canonicalize()
        .unwrap_or_else(|e| panic!("canonicalize repo root {}: {e}", path.display()))
}

/// The built `gmeow` binary, as an `assert_cmd::Command`.
fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

/// The built `gmeow` binary's absolute path, for raw `std::process::Command`
/// invocations that need a fully-cleared environment.
fn gmeow_bin() -> PathBuf {
    assert_cmd::cargo::cargo_bin("gmeow")
}

// ── 1. Shipped bundle loads (query) ──────────────────────────────────────────

#[test]
fn info_loads_the_shipped_bundle() {
    gmeow()
        .arg("info")
        .assert()
        .success()
        .stdout(predicate::str::contains("terms").and(predicate::str::contains("quads")));
}

#[test]
fn describe_names_a_known_term() {
    // `Entity` is the same stable kernel term `cli.rs::describe_known_term_renders_prose`
    // already proves live against the embedded bundle.
    gmeow()
        .args(["describe", "Entity"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Entity"));
}

#[test]
fn validate_clean_fixture_passes() {
    gmeow()
        .arg("validate")
        .arg(fixture("clean.ttl"))
        .assert()
        .success()
        .stdout(predicate::str::contains("validation passed"));
}

// ── 2. Verify leg on the shipped (unsigned) bundle: liveness + integrity ────

/// The committed dev bundle carries zero signatures, so this leg proves the
/// binary loads/answers and the blob-DAG integrity law holds — NOT that it
/// cryptographically verified anything (that is item 3 below). Run with a fully
/// cleared environment (mirrors `cli.rs::gts_shim_hard_fails_when_binary_missing`)
/// to empirically prove the re-wired verify leg needs no `gts` binary on `PATH`
/// at all — the native verify path is self-contained.
#[test]
fn verify_allow_unsigned_needs_no_external_gts_on_path() {
    let output = StdCommand::new(gmeow_bin())
        .args(["verify", "--allow-unsigned"])
        .current_dir(repo_root())
        .env_clear()
        .env("PATH", "/nonexistent-path-for-tests")
        .output()
        .expect("run gmeow verify --allow-unsigned");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "verify --allow-unsigned must succeed with no `gts` binary reachable on PATH: \
         stdout={stdout}\nstderr={stderr}"
    );
    assert!(
        stdout.contains("verification passed"),
        "stdout must report overall success: {stdout}"
    );
}

// ── 3. Real cryptographic verification through the BUILT BINARY ─────────────

/// Deterministic Ed25519 signing key from a seed — mirrors
/// `crates/validate/src/signature.rs`'s test helper of the same name (a
/// private helper this integration test, a separate compilation unit, cannot
/// import).
fn deterministic_signing_key(seed: u8) -> SigningKey {
    let mut bytes = [0u8; 32];
    bytes[0] = seed;
    for i in 1..32 {
        bytes[i] = bytes[i - 1].wrapping_mul(31).wrapping_add(seed);
    }
    SigningKey::from_bytes(&bytes)
}

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(ALPHABET[(b0 >> 2) as usize] as char);
        out.push(ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Build a minimal ASCII-armored OpenPGP v4 Ed25519 public-key certificate from
/// a raw 32-byte public key — mirrors `signature.rs`'s test helper of the same
/// name (private, in a different crate, so replicated here against only public
/// APIs). The format matches the one GPG emits and
/// `purrdf::gts::openpgp::parse_transport_key` accepts.
fn ed25519_public_key_armor(raw_public: &[u8; 32]) -> String {
    const ED25519_ALGO: u8 = 22;
    const ED25519_OID: &[u8] = &[0x2b, 0x06, 0x01, 0x04, 0x01, 0xda, 0x47, 0x0f, 0x01];

    let mut body = Vec::new();
    body.push(0x04); // OpenPGP v4
    body.extend_from_slice(&0u32.to_be_bytes()); // creation time
    body.push(ED25519_ALGO);
    body.push(ED25519_OID.len() as u8);
    body.extend_from_slice(ED25519_OID);
    let mpi_len = 1 + raw_public.len(); // 0x40 marker + raw key
    body.extend_from_slice(&(mpi_len as u16 * 8).to_be_bytes());
    body.push(0x40);
    body.extend_from_slice(raw_public);

    // Old-format packet: tag 6, one-octet length.
    let mut packet = Vec::new();
    packet.push(0x98);
    packet.push(body.len() as u8);
    packet.extend_from_slice(&body);

    let b64 = base64_encode(&packet);
    let mut wrapped = String::new();
    for line in b64.as_bytes().chunks(64) {
        wrapped.push_str(std::str::from_utf8(line).unwrap());
        wrapped.push('\n');
    }
    format!("-----BEGIN PGP PUBLIC KEY BLOCK-----\n\n{wrapped}-----END PGP PUBLIC KEY BLOCK-----\n")
}

/// Mint a tiny GTS bundle, signed with `signing_key` under the kid that
/// `--trusted-key` verification actually checks against: the FINGERPRINT of the
/// out-of-band armored key (`purrdf::gts::verify::verify_file_with_options`'s
/// out-of-band-key branch resolves the candidate kid from the supplied armor's
/// own fingerprint, never a caller-chosen string — mirrors
/// `signature.rs::minimal_signed_gts_bytes` signing under `transport.fingerprint`).
///
/// `gmeow verify` doesn't just check the signature: it also runs the
/// source-free "Bundled Ontology Checks" (a non-empty, labeled, defined GMEOW
/// term catalog) over WHATEVER bundle is handed to it — that is the documented
/// contract of the command (`Verify GTS signatures and the source-free ontology
/// checks`), not something specific to the shipped bundle. So this fixture
/// carries one real `https://blackcatinformatics.ca/gmeow/`-namespaced
/// `owl:Class`, fully labeled and defined, so the crypto-positive case below
/// exercises the WHOLE command (signature leg AND ontology-checks leg), not
/// just the signature leg in isolation.
fn minimal_signed_gts_bytes(signing_key: &SigningKey, kid: &str) -> Vec<u8> {
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
    const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
    const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
    const SMOKE_CLASS: &str = "https://blackcatinformatics.ca/gmeow/SmokeTestClass";

    fn iri(value: &str) -> Term {
        Term {
            kind: TermKind::Iri,
            value: Some(value.to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
            triple: None,
        }
    }
    fn en_literal(text: &str) -> Term {
        Term {
            kind: TermKind::Literal,
            value: Some(text.to_string()),
            datatype: None,
            lang: Some("en".to_string()),
            direction: None,
            reifier: None,
            triple: None,
        }
    }

    let mut writer = purrdf::gts::writer::Writer::new("gmeow-cli-bundle-smoke-test");
    writer.sign_with(signing_key.clone(), kid);
    writer.add_terms(&[
        iri(SMOKE_CLASS),               // 0
        iri(RDF_TYPE),                  // 1
        iri(OWL_CLASS),                 // 2
        iri(RDFS_LABEL),                // 3
        en_literal("Smoke Test Class"), // 4
        iri(SKOS_DEFINITION),           // 5
        en_literal("A synthetic GMEOW class minted only to exercise gmeow verify's crypto path."), // 6
    ]);
    writer.add_quads(&[(0, 1, 2, None), (0, 3, 4, None), (0, 5, 6, None)]);
    writer.to_bytes()
}

/// A signed bundle + its signer's armored public key, written to fresh scratch
/// files inside a freshly-created `TempDir`: `(scratch_dir, bundle_path,
/// signer_armor_path, armor)`. The caller must hold the returned `TempDir`
/// alive for as long as the paths are needed — its `Drop` impl removes the
/// directory (and everything under it) on ANY exit path, including a panic,
/// with no manual cleanup required.
fn write_signed_bundle_and_key(
    _tag: &str,
    seed: u8,
) -> (tempfile::TempDir, PathBuf, PathBuf, String) {
    let signer = deterministic_signing_key(seed);
    let armor = ed25519_public_key_armor(&signer.verifying_key().to_bytes());
    let transport =
        purrdf::gts::openpgp::parse_transport_key(&armor).expect("test armor must parse");
    let bundle_bytes = minimal_signed_gts_bytes(&signer, &transport.fingerprint);

    let dir = tempfile::TempDir::new().expect("create temp dir");
    let bundle_path = dir.path().join("signed.gts");
    let armor_path = dir.path().join("key.asc");
    std::fs::write(&bundle_path, &bundle_bytes).expect("write signed bundle");
    std::fs::write(&armor_path, &armor).expect("write armored key");
    (dir, bundle_path, armor_path, armor)
}

/// Positive case: `gmeow verify --trusted-key <armor>` against a bundle signed
/// by the SAME key exits 0 and reports a passing, cryptographically-checked
/// signature — a REAL Ed25519 signature was verified through the CLI entry
/// point, not merely a liveness check.
#[test]
fn verify_accepts_a_validly_signed_trusted_bundle() {
    let (_scratch_dir, bundle_path, armor_path, _armor) =
        write_signed_bundle_and_key("positive", 11);

    let output = gmeow()
        .args(["verify", "--trusted-key"])
        .arg(&armor_path)
        .arg(&bundle_path)
        .output()
        .expect("run gmeow verify (positive case)");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "a validly-signed bundle verified against its own trusted key must pass: \
         stdout={stdout}\nstderr={stderr}"
    );
    assert!(
        stdout.contains("verification passed"),
        "stdout must report overall success: {stdout}"
    );
    // The signature/trust findings now fold into the unified report and render on
    // the product channel (stdout) via `render::to_text`, so the resolved-key
    // finding surfaces there rather than on the reporter's stderr channel.
    assert!(
        stdout.contains("resolved transport key"),
        "the resolved-key finding must be surfaced in the unified report: \
         stdout={stdout}\nstderr={stderr}"
    );
}

/// Negative case: the SAME signed bundle verified against a DIFFERENT ephemeral
/// key's armored public key exits non-zero — the binary's crypto path actually
/// discriminates a wrong trusted key, not merely always-passes.
#[test]
fn verify_rejects_the_same_bundle_under_a_different_trusted_key() {
    let (_scratch_dir, bundle_path, _armor_path, _armor) =
        write_signed_bundle_and_key("negative-signed", 11);
    let (_wrong_scratch_dir, _other_bundle, wrong_armor_path, _wrong_armor) =
        write_signed_bundle_and_key("negative-wrong-key", 99);

    let output = gmeow()
        .args(["verify", "--trusted-key"])
        .arg(&wrong_armor_path)
        .arg(&bundle_path)
        .output()
        .expect("run gmeow verify (negative case)");
    assert!(
        !output.status.success(),
        "a bundle signed by key A, verified against key B's trusted-key, must be rejected: \
         stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── 4. Embedded == committed (load-liveness closure) ─────────────────────────

#[test]
fn embedded_bundle_equals_the_committed_snapshot() {
    let committed_bytes = gmeow_bundle_import::load_authenticated_source_bytes(&repo_root())
        .expect("authenticated producer bundle; tests never produce it");
    assert_eq!(
        gmeow_cli::BUNDLE_GTS,
        committed_bytes.as_slice(),
        "the shipped binary's embedded bundle must equal the committed \
         generated/dist/gmeow.gts byte-for-byte"
    );
}
