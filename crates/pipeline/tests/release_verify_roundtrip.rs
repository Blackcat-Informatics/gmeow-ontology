// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Gpg-free release-attestation round-trip — the named "release verification
//! path" consumer.
//!
//! Exercises the EXISTING native fold/verify pair
//! ([`gmeow_pipeline::stages::release::fold_release_bundle`] /
//! [`gmeow_pipeline::stages::release::verify_release_bundle`]) end to end over
//! the committed `generated/dist/gmeow.gts`, with an ephemeral, in-process,
//! raw-Ed25519 signing key — never GPG, never an external process, never the
//! network. `release.rs` already unit-tests this pair against a tiny synthetic
//! snapshot; this integration test pins the SAME contract against the REAL
//! shipped bundle, so a regression in the release path is caught even though
//! `make release-sign-gts` itself is a maintainer-only, GPG-gated lane this
//! repo's tests must never invoke.
//!
//! Off the on-gate default/ci nextest profile (see `.config/nextest.toml`):
//! `fold_release_bundle`/`verify_release_bundle` each replay the WHOLE
//! committed ~48 MB bundle, the identical "whole committed bundle" cost class
//! as `fold_parity`/`fanout_parity`/`end_to_end` (irreducibly O(bundle size),
//! not fixture cost). Runs on `maint-heavy`. The real crypto-through-the-
//! built-binary requirement stays on-gate via `crates/gmeow-cli/tests/
//! bundle_smoke.rs`'s ephemeral-signed positive/negative checks, which build a
//! small in-process `.gts` rather than replaying the shipped bundle.

use std::path::PathBuf;

use ed25519_dalek::SigningKey;
use gmeow_pipeline::stages::release::{
    build_coherence_evidence, fold_release_bundle, verify_release_bundle,
};

/// The committed `generated/dist/gmeow.gts` snapshot bytes — mirrors the
/// `committed_snapshot` helper already used by `tests/bundle_integrity.rs` and
/// `bundle_blobs.rs`'s own unit tests (a private helper this separate
/// compilation unit cannot import).
fn committed_snapshot() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("generated/dist/gmeow.gts");
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Deterministic Ed25519 signing key from a seed — mirrors
/// `release.rs`'s own `deterministic_signing_key` test helper (private, so
/// replicated here against only public `ed25519-dalek` API).
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

/// Build a minimal ASCII-armored OpenPGP v4 Ed25519 public-key certificate the
/// bundle's transport-key meta frame carries — mirrors `release.rs`'s own
/// `fake_public_armor` test helper (private, hand-rolled OpenPGP-armor
/// construction with no GPG process involved, replicated here byte-for-byte
/// since this integration test is a separate compilation unit).
fn fake_public_armor(verify_key: &[u8; 32]) -> String {
    // Tag-6 public-key packet body: v4, ctime=0, algo=22, OID, 0x40-MPI.
    let mut body = vec![4u8, 0, 0, 0, 0, 22];
    body.push(9); // OID length
    body.extend_from_slice(&[0x2b, 0x06, 0x01, 0x04, 0x01, 0xda, 0x47, 0x0f, 0x01]);
    // MPI: 0x40 prefix marker || 32-byte key => 263 bits.
    let mut mpi = vec![0x40u8];
    mpi.extend_from_slice(verify_key);
    let bits = (mpi.len() * 8 - 1) as u16; // high bit of 0x40 is clear
    body.extend_from_slice(&bits.to_be_bytes());
    body.extend_from_slice(&mpi);
    // New-format tag-6 packet header.
    let mut packet = vec![0xc6u8, body.len() as u8];
    packet.extend_from_slice(&body);
    let b64 = base64_encode(&packet);
    let mut wrapped = String::new();
    for line in b64.as_bytes().chunks(64) {
        wrapped.push_str(std::str::from_utf8(line).unwrap());
        wrapped.push('\n');
    }
    format!("-----BEGIN PGP PUBLIC KEY BLOCK-----\n\n{wrapped}-----END PGP PUBLIC KEY BLOCK-----\n")
}

/// Mint an ephemeral signer's `(raw 32-byte secret, kid, armored public key)`
/// triple from a seed. The `kid` is the FINGERPRINT of the just-built armor —
/// matching the convention `verify_release_bundle`'s out-of-band-key branch
/// actually checks against (it resolves the candidate signing kid from the
/// SUPPLIED armor's own fingerprint, never `fold_release_bundle`'s caller-chosen
/// `signer_kid` string) — so signing under this kid and later verifying with
/// `Some(&armor)` agree.
fn signer_material(seed: u8) -> ([u8; 32], String, String) {
    let signer = deterministic_signing_key(seed);
    let secret = signer.to_bytes();
    let armor = fake_public_armor(&signer.verifying_key().to_bytes());
    let transport =
        purrdf::gts::openpgp::parse_transport_key(&armor).expect("test armor must parse");
    (secret, transport.fingerprint, armor)
}

/// Fold the REAL committed ~48MB bundle with a signed coherence-evidence
/// attestation under signer seed 42. `fold_release_bundle` replays the whole
/// snapshot into a fresh builder — irreducibly O(bundle size); see the module
/// doc for why this test file runs on `maint-heavy` rather than the on-gate
/// default/ci profile.
fn fold_signed_bundle() -> (Vec<u8>, String) {
    let snapshot = committed_snapshot();
    let issued_at = "2026-01-01T00:00:00Z";
    let (secret, kid, armor) = signer_material(42);
    let evidence = build_coherence_evidence(&snapshot, issued_at)
        .expect("coherence evidence must be derivable from the committed bundle");
    let signed_bundle = fold_release_bundle(
        &snapshot,
        vec![evidence],
        "https://blackcatinformatics.ca/gmeow/agent/release-verify-roundtrip-test",
        issued_at,
        "https://blackcatinformatics.ca/gmeow/release/gmeow.gts",
        secret,
        &kid,
        &armor,
    )
    .expect("fold_release_bundle must succeed over the committed snapshot");
    (signed_bundle, armor)
}

/// The full signed release-attestation walk — coherence evidence folded and
/// signed over the REAL committed bundle, then verified — round-trips
/// cleanly: an ephemeral Ed25519 key, no GPG binary, no external process, no
/// network.
#[test]
fn release_bundle_with_coherence_evidence_round_trips_natively() {
    let (signed_bundle, armor) = fold_signed_bundle();
    assert!(
        !signed_bundle.is_empty(),
        "the folded, signed release bundle must be non-empty"
    );

    let report = verify_release_bundle(&signed_bundle, Some(&armor))
        .expect("a well-formed, validly-signed release bundle must verify");
    assert!(
        report.valid >= 1,
        "the release bundle must carry at least one cryptographically valid signature: \
         signed={} valid={} artifacts_verified={}",
        report.signed,
        report.valid,
        report.artifacts_verified
    );
    assert!(
        report.artifacts_verified > 0,
        "at least one attested evidence artifact (the coherence certificate) must resolve to \
         a present blob: signed={} valid={} artifacts_verified={}",
        report.signed,
        report.valid,
        report.artifacts_verified
    );
}

/// The trust leg genuinely discriminates: verifying the SAME signed bundle
/// against a DIFFERENT out-of-band key must fail — proving this round-trip
/// exercises real cryptographic verification, not an always-pass stub.
#[test]
fn release_bundle_rejects_an_untrusted_out_of_band_key() {
    let (signed_bundle, _armor) = fold_signed_bundle();
    let (_other_secret, _other_kid, wrong_armor) = signer_material(43);

    assert!(
        verify_release_bundle(&signed_bundle, Some(&wrong_armor)).is_err(),
        "verifying against an untrusted out-of-band key must fail, never silently pass"
    );
}
