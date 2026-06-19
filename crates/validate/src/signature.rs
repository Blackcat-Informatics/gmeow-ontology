// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GTS bundle signature and trust verification pre-gate (#646).
//!
//! This module wraps [`gmeow_gts::verify::verify_file_with_options`] and maps the
//! cryptographic and policy-layer outcomes into canonical
//! [`gmeow_diagnostics::Finding`] values. When the returned hard-failure flag is
//! true, [`ValidationRun::run`](crate::validate_all::ValidationRun::run) aborts
//! before the ontology validation phases.

use gmeow_diagnostics::{Finding, Severity};
use gmeow_gts::policy::TrustPolicy;
use gmeow_gts::verify::{verify_file_with_options, VerifyOptions};

use crate::validate_all::SignatureConfig;

/// Verify a GTS byte bundle's embedded signatures and optional trust policy.
///
/// Returns the diagnostic findings and a flag indicating whether hard failures
/// occurred. A hard failure means the signature/trust phase believes the bundle
/// should not proceed to ontology validation (e.g. missing required signatures,
/// cryptographic invalidity, unresolved signatures, or an untrusted signer when
/// one is required).
pub fn verify_gts_bundle(
    bytes: &[u8],
    config: &SignatureConfig,
) -> Result<(Vec<Finding>, bool), String> {
    let armored_key = if let Some(path) = &config.trusted_key {
        Some(read_armored_key(path)?)
    } else {
        None
    };

    let trust_policy = TrustPolicy::new(
        config.trusted_signers.iter().cloned(),
        config.require_trusted_signer,
    );

    let options = VerifyOptions {
        armored_key,
        require_signatures: config.require_signatures,
        trust_policy,
    };

    let result = verify_file_with_options(bytes, &options);
    let mut findings = Vec::new();

    // Top-level verification errors from gmeow_gts. Most of these correspond to
    // the count-based conditions below, but key-loading failures only appear
    // here, so emit them explicitly as cryptographic/key failures.
    for error in &result.errors {
        if is_key_loading_error(error) {
            findings.push(
                Finding::new(Severity::Error, "signature.invalid", error.clone())
                    .with_tool("gts-verify"),
            );
        }
    }

    // Missing signatures.
    if result.signed == 0 {
        let (severity, hard) = if config.require_signatures {
            (Severity::Error, true)
        } else {
            (Severity::Warning, false)
        };
        findings.push(
            Finding::new(
                severity,
                "signature.missing",
                "no signed frames found in GTS bundle",
            )
            .with_tool("gts-verify"),
        );
        // `hard` is unused for the boolean here because the overall hard-failure
        // flag is driven by `result.ok`, but keeping the branch explicit makes
        // the mapping obvious.
        let _ = hard;
    }

    // Cryptographically invalid signatures.
    if result.invalid > 0 {
        findings.push(
            Finding::new(
                Severity::Error,
                "signature.invalid",
                format!("{} signature(s) cryptographically invalid", result.invalid),
            )
            .with_tool("gts-verify"),
        );
    }

    // Signatures whose key could not be resolved. Emitted as a warning per the
    // task mapping, but still treated as a hard failure by the short-circuit
    // flag (`result.ok` is false when unverified signatures remain).
    if result.unverified > 0 {
        findings.push(
            Finding::new(
                Severity::Warning,
                "signature.unverified",
                format!(
                    "{} signature(s) unverified (key unavailable)",
                    result.unverified
                ),
            )
            .with_tool("gts-verify"),
        );
    }

    // Deployment-trust evaluation for the bundle as a whole. This covers
    // generic bundles that do not declare an evidence/opaque profile, where
    // gmeow_gts's profile policy evaluation does not run signature_trust.
    if result.signed > 0 && result.valid > 0 && result.trusted == 0 {
        let severity = if config.require_trusted_signer {
            Severity::Error
        } else {
            Severity::Warning
        };
        findings.push(
            Finding::new(
                severity,
                "signature.untrusted",
                "no cryptographically valid signature from a deployment-trusted signer",
            )
            .with_tool("gts-verify"),
        );
    }

    // Profile and trust-policy findings from gmeow_gts. These cover
    // profile-specific rules (evidence/opaque) and may duplicate the generic
    // trust check above; duplicates are harmless because the canonical report
    // is normalized at serialization time.
    for finding in &result.profile_findings {
        let severity = match finding.severity {
            gmeow_gts::policy::Severity::Error => Severity::Error,
            gmeow_gts::policy::Severity::Warning => Severity::Warning,
            gmeow_gts::policy::Severity::Info => Severity::Info,
        };
        findings.push(
            Finding::new(
                severity,
                format!("signature.{}", finding.code),
                finding.detail.clone(),
            )
            .with_tool("gts-verify"),
        );
    }

    // Reader diagnostics produced while the verifier re-folded the bundle.
    for diagnostic in &result.diagnostics {
        findings.push(
            Finding::new(
                Severity::Warning,
                format!("gts.{}", diagnostic.code),
                format!("{} (frame {:?})", diagnostic.detail, diagnostic.frame_index),
            )
            .with_tool("gts-verify"),
        );
    }

    // Informational note surfacing the resolved key id / fingerprint for
    // transparency, when one was resolved.
    if let (Some(kid), Some(fingerprint)) = (&result.kid, &result.fingerprint) {
        findings.push(
            Finding::new(
                Severity::Info,
                "signature.key",
                format!(
                    "resolved transport key kid={kid} fingerprint={}",
                    gmeow_gts::verify::format_fingerprint(fingerprint)
                ),
            )
            .with_tool("gts-verify"),
        );
    }

    // The gmeow_gts `ok` flag already encodes the approved short-circuit rule:
    // false when signatures are missing (if required), invalid, unverified, or
    // when a profile/trust policy error is present.
    let hard_failures = !result.ok;

    Ok((findings, hard_failures))
}

/// Read an ASCII-armored OpenPGP key from `path`, or return the string as-is if
/// it already looks like an armored key block.
fn read_armored_key(path: &str) -> Result<String, String> {
    const ARMOR_BEGIN: &str = "-----BEGIN PGP PUBLIC KEY BLOCK-----";

    let trimmed = path.trim();
    if trimmed.starts_with(ARMOR_BEGIN) {
        return Ok(trimmed.to_owned());
    }

    std::fs::read_to_string(trimmed)
        .map(|s| s.trim().to_owned())
        .map_err(|e| format!("cannot read trusted key file {}: {e}", path))
}

fn is_key_loading_error(error: &str) -> bool {
    error.starts_with("cannot load trusted key")
        || error.starts_with("cannot load embedded transport key")
        || error.contains("transportKey")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_gts::model::{Term, TermKind};

    fn minimal_unsigned_gts_bytes() -> Vec<u8> {
        let mut graph = gmeow_gts::model::Graph::default();
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/a".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        });
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/p".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        });
        graph.terms.push(Term {
            kind: TermKind::Iri,
            value: Some("https://example.org/b".to_string()),
            datatype: None,
            lang: None,
            reifier: None,
        });
        graph.quads.push((0, 1, 2, None));

        let writer = gmeow_gts::writer::Writer::deterministic(&graph, "gmeow-validate-test")
            .expect("deterministic GTS writer must succeed");
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
        assert!(findings
            .iter()
            .any(|f| f.code == "signature.missing" && f.severity == Severity::Error));
    }

    #[test]
    fn unsigned_bundle_without_required_signatures_is_warning_only() {
        let bytes = minimal_unsigned_gts_bytes();
        let config = SignatureConfig::default();

        let (findings, hard) = verify_gts_bundle(&bytes, &config).expect("verification must run");

        assert!(!hard, "missing optional signatures must not hard-fail");
        assert!(findings
            .iter()
            .any(|f| f.code == "signature.missing" && f.severity == Severity::Warning));
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
}
