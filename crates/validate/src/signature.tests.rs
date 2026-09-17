// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use ed25519_dalek::SigningKey;
use purrdf::gts::model::{Term, TermKind};

fn minimal_unsigned_gts_bytes() -> Vec<u8> {
    let mut graph = purrdf::gts::model::Graph::default();
    graph.terms.push(Term {
        kind: TermKind::Iri,
        value: Some("https://example.org/a".to_string()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
        triple: None,
    });
    graph.terms.push(Term {
        kind: TermKind::Iri,
        value: Some("https://example.org/p".to_string()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
        triple: None,
    });
    graph.terms.push(Term {
        kind: TermKind::Iri,
        value: Some("https://example.org/b".to_string()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
        triple: None,
    });
    graph.quads.push((0, 1, 2, None));

    let writer = purrdf::gts::writer::Writer::deterministic(&graph, "gmeow-validate-test")
        .expect("deterministic GTS writer must succeed");
    writer.to_bytes()
}

fn deterministic_signing_key(seed: u8) -> SigningKey {
    let mut bytes = [0u8; 32];
    bytes[0] = seed;
    // Mix the seed throughout so keys for different seeds differ.
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

/// Build a minimal ASCII-armored OpenPGP v4 Ed25519 public-key certificate
/// from a raw 32-byte public key. The format matches the one GPG emits and
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

fn minimal_signed_gts_bytes(signing_key: &SigningKey, kid: &str) -> Vec<u8> {
    let mut writer = purrdf::gts::writer::Writer::new("gmeow-validate-test");
    writer.sign_with(signing_key.clone(), kid);
    writer.add_terms(&[
        Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/a".to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
            triple: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/p".to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
            triple: None,
        },
        Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/b".to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
            triple: None,
        },
    ]);
    writer.add_quads(&[(0, 1, 2, None)]);
    writer.to_bytes()
}

#[test]
fn unsigned_bundle_with_required_signatures_is_hard_failure() {
    let bytes = minimal_unsigned_gts_bytes();
    let config = SignatureConfig {
        require_signatures: true,
        ..SignatureConfig::default()
    };

    let (findings, hard) = verify_gts_bundle(&bytes, &config).expect("verification must run");

    assert!(hard, "missing required signatures must be a hard failure");
    assert!(
        findings
            .iter()
            .any(|f| f.code == "signature.missing" && f.severity == Severity::Error)
    );
}

#[test]
fn unsigned_bundle_without_required_signatures_is_warning_only() {
    let bytes = minimal_unsigned_gts_bytes();
    let config = SignatureConfig::default();

    let (findings, hard) = verify_gts_bundle(&bytes, &config).expect("verification must run");

    assert!(!hard, "missing optional signatures must not hard-fail");
    assert!(
        findings
            .iter()
            .any(|f| f.code == "signature.missing" && f.severity == Severity::Warning)
    );
}

#[test]
fn signed_bundle_with_untrusted_signer_is_hard_failure() {
    let signer = deterministic_signing_key(1);
    let armor = ed25519_public_key_armor(&signer.verifying_key().to_bytes());
    let transport =
        purrdf::gts::openpgp::parse_transport_key(&armor).expect("test armor must parse");
    let bytes = minimal_signed_gts_bytes(&signer, &transport.fingerprint);

    // The signer is cryptographically valid, but the policy trusts a different fingerprint.
    let other_fingerprint = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let config = SignatureConfig {
        require_trusted_signer: true,
        trusted_key: Some(armor),
        trusted_signers: vec![other_fingerprint.to_string()],
        ..SignatureConfig::default()
    };

    let (findings, hard) = verify_gts_bundle(&bytes, &config).expect("verification must run");

    assert!(
        hard,
        "untrusted signer when required must be a hard failure"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.code == "signature.untrusted" && f.severity == Severity::Error)
    );
}

#[test]
fn signed_bundle_with_trusted_signer_passes() {
    let signer = deterministic_signing_key(2);
    let armor = ed25519_public_key_armor(&signer.verifying_key().to_bytes());
    let transport =
        purrdf::gts::openpgp::parse_transport_key(&armor).expect("test armor must parse");
    let bytes = minimal_signed_gts_bytes(&signer, &transport.fingerprint);

    let config = SignatureConfig {
        require_trusted_signer: true,
        trusted_key: Some(armor),
        trusted_signers: vec![transport.fingerprint],
        ..SignatureConfig::default()
    };

    let (findings, hard) = verify_gts_bundle(&bytes, &config).expect("verification must run");

    assert!(!hard, "trusted signer must not hard-fail");
    assert!(
        !findings.iter().any(|f| f.severity == Severity::Error),
        "no error-level findings expected"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.code == "signature.key" && f.severity == Severity::Info)
    );
}

#[test]
fn reader_diagnostic_codes_classified_by_severity() {
    // Structural integrity failures are errors.
    assert_eq!(reader_diagnostic_severity("EmptyFile"), Severity::Error);
    assert_eq!(reader_diagnostic_severity("DamagedFrame"), Severity::Error);
    assert_eq!(reader_diagnostic_severity("BrokenChain"), Severity::Error);
    assert_eq!(
        reader_diagnostic_severity("SegmentBoundary"),
        Severity::Error
    );
    assert_eq!(reader_diagnostic_severity("TruncatedLog"), Severity::Error);
    assert_eq!(
        reader_diagnostic_severity("TornAppendError"),
        Severity::Error
    );
    assert_eq!(
        reader_diagnostic_severity("StreamableLayoutError"),
        Severity::Error
    );
    assert_eq!(reader_diagnostic_severity("IndexMmrError"), Severity::Error);
    assert_eq!(
        reader_diagnostic_severity("PositionConstraint"),
        Severity::Error
    );

    // Soft-degradation / missing-capability codes remain warnings.
    assert_eq!(reader_diagnostic_severity("MissingKey"), Severity::Warning);
    assert_eq!(
        reader_diagnostic_severity("UnknownFrameType"),
        Severity::Warning
    );
    assert_eq!(
        reader_diagnostic_severity("UnknownCodec"),
        Severity::Warning
    );
    assert_eq!(
        reader_diagnostic_severity("ForwardReference"),
        Severity::Warning
    );
    assert_eq!(
        reader_diagnostic_severity("ConflictingReifier"),
        Severity::Warning
    );

    // Unknown future codes default to warning so we do not invent errors.
    assert_eq!(
        reader_diagnostic_severity("FutureDiagnostic"),
        Severity::Warning
    );
}

#[test]
fn empty_gts_bundle_emits_error_diagnostic() {
    let config = SignatureConfig::default();

    let (findings, hard) = verify_gts_bundle(&[], &config).expect("verification must run");

    assert!(
        hard,
        "empty GTS bundle must hard-fail because the reader reports an error-level diagnostic"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.code == "gts.EmptyFile" && f.severity == Severity::Error)
    );
}

#[test]
fn armored_key_string_passes_through() {
    let armored =
        "-----BEGIN PGP PUBLIC KEY BLOCK-----\n\ntest\n-----END PGP PUBLIC KEY BLOCK-----";
    let result = read_armored_key(armored);
    assert_eq!(result.unwrap(), armored);
}

#[test]
fn missing_key_file_returns_error() {
    let result = read_armored_key("/nonexistent/key.asc");
    assert!(result.is_err());
}

/// Every finding emitted by `verify_gts_bundle` must carry the
/// `PolicyWarning` category — signature/trust checks are governance
/// policy, orthogonal to ontology content correctness.
#[test]
fn every_signature_finding_carries_policy_warning_category() {
    use gmeow_errors::FindingCategory;

    // An unsigned bundle with required signatures: produces at least one finding.
    let config = SignatureConfig {
        require_signatures: true,
        require_trusted_signer: false,
        trusted_signers: vec![],
        trusted_key: None,
    };
    let bytes = minimal_unsigned_gts_bytes();
    let (findings, _hard) = verify_gts_bundle(&bytes, &config).expect("verification must run");

    assert!(
        !findings.is_empty(),
        "an unsigned bundle with required signatures must emit at least one finding"
    );
    for finding in &findings {
        assert_eq!(
            finding.category,
            Some(FindingCategory::PolicyWarning),
            "finding '{}' must carry PolicyWarning; got {:?}",
            finding.code,
            finding.category
        );
    }
}
