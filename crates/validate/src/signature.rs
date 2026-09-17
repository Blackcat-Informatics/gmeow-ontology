// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GTS bundle signature and trust verification pre-gate.
//!
//! This module wraps [`purrdf::gts::verify::verify_file_with_options`] and maps the
//! cryptographic and policy-layer outcomes into canonical
//! [`gmeow_errors::Finding`] values. When the returned hard-failure flag is
//! true, [`ValidationRun::run`](crate::validate_all::ValidationRun::run) aborts
//! before the ontology validation phases.

use gmeow_errors::{Finding, FindingCategory, Severity};
use purrdf::gts::policy::TrustPolicy;
use purrdf::gts::verify::{VerifyOptions, verify_file_with_options};

use crate::codes;
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
) -> gmeow_errors::Result<(Vec<Finding>, bool)> {
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

    // Top-level verification errors from purrdf::gts. These cover conditions not
    // captured by the count-based fields below, especially key-loading failures.
    for error in &result.errors {
        let code = if error.starts_with("cannot load trusted key")
            || error.starts_with("cannot load embedded transport key")
            || error.contains("transportKey")
        {
            codes::SIGNATURE_INVALID
        } else {
            codes::SIGNATURE_VERIFY
        };
        findings.push(Finding::new(Severity::Error, code, error.clone()).with_tool("gts-verify"));
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
                codes::SIGNATURE_MISSING,
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
                codes::SIGNATURE_INVALID,
                format!("{} signature(s) cryptographically invalid", result.invalid),
            )
            .with_tool("gts-verify"),
        );
    }

    // Signatures whose key could not be resolved. Emitted as an error per the
    // design doc; the verification run aborts when unresolved signatures remain.
    if result.unverified > 0 {
        findings.push(
            Finding::new(
                Severity::Error,
                codes::SIGNATURE_UNVERIFIED,
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
    // purrdf::gts's profile policy evaluation does not run signature_trust.
    if result.signed > 0 && result.valid > 0 && result.trusted == 0 {
        let severity = if config.require_trusted_signer {
            Severity::Error
        } else {
            Severity::Warning
        };
        findings.push(
            Finding::new(
                severity,
                codes::SIGNATURE_UNTRUSTED,
                "no cryptographically valid signature from a deployment-trusted signer",
            )
            .with_tool("gts-verify"),
        );
    }

    // Profile and trust-policy findings from purrdf::gts. These cover
    // profile-specific rules (evidence/opaque) and may duplicate the generic
    // trust check above; duplicates are harmless because the canonical report
    // is normalized at serialization time.
    for finding in &result.profile_findings {
        let severity = match finding.severity {
            purrdf::gts::policy::Severity::Error => Severity::Error,
            purrdf::gts::policy::Severity::Warning => Severity::Warning,
            purrdf::gts::policy::Severity::Info => Severity::Info,
        };
        findings.push(
            Finding::new(
                severity,
                format!("{}{}", codes::SIGNATURE_FAMILY, finding.code),
                finding.detail.clone(),
            )
            .with_tool("gts-verify"),
        );
    }

    // Reader diagnostics produced while the verifier re-folded the bundle.
    // Their severity is inferred from the diagnostic code: structural integrity
    // failures are errors; missing-capacity or soft-degradation codes are warnings.
    for diagnostic in &result.diagnostics {
        let severity = reader_diagnostic_severity(&diagnostic.code);
        findings.push(
            Finding::new(
                severity,
                format!("{}{}", codes::GTS_FAMILY, diagnostic.code),
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
                codes::SIGNATURE_KEY,
                format!(
                    "resolved transport key kid={kid} fingerprint={}",
                    purrdf::gts::verify::format_fingerprint(fingerprint)
                ),
            )
            .with_tool("gts-verify"),
        );
    }

    // Tag every signature/trust finding as a policy advisory — these are
    // governance checks (signing requirements, trust anchors, key resolution)
    // orthogonal to ontology content correctness.
    let findings: Vec<Finding> = findings
        .into_iter()
        .map(|f| f.with_category(FindingCategory::PolicyWarning))
        .collect();

    // The purrdf::gts `ok` flag encodes cryptographic short-circuit rules, but
    // deployment-trust errors (e.g. an untrusted signer when one is required)
    // are surfaced as Error-level findings above. Abort the validation run
    // whenever any Error-level signature/trust finding is present.
    let hard_failures = !result.ok
        || findings
            .iter()
            .any(|finding| finding.severity == Severity::Error);

    Ok((findings, hard_failures))
}

/// Map a `purrdf::gts` reader diagnostic code to a canonical [`Severity`].
///
/// The reader does not attach severity to its diagnostics; the design doc
///  requires us to classify them. Structural integrity failures
/// (empty input, damaged frames, broken chain, torn/truncated logs, layout
/// violations) are treated as errors because they mean the bundle cannot be
/// reliably folded. Missing-capability and soft-degradation codes (unknown
/// frame types, missing decryption keys, forward references, conflicting
/// reifiers) remain warnings because the reader degrades gracefully to opaque
/// nodes or dropped quads.
fn reader_diagnostic_severity(code: &str) -> Severity {
    match code {
        "EmptyFile"
        | "DamagedFrame"
        | "BrokenChain"
        | "SegmentBoundary"
        | "TruncatedLog"
        | "TornAppendError"
        | "StreamableLayoutError"
        | "IndexMmrError"
        | "PositionConstraint" => Severity::Error,
        _ => Severity::Warning,
    }
}

/// Read an ASCII-armored OpenPGP key from `path`, or return the string as-is if
/// it already looks like an armored key block.
fn read_armored_key(path: &str) -> gmeow_errors::Result<String> {
    const ARMOR_BEGIN: &str = "-----BEGIN PGP PUBLIC KEY BLOCK-----";

    let trimmed = path.trim();
    if trimmed.starts_with(ARMOR_BEGIN) {
        return Ok(trimmed.to_owned());
    }

    std::fs::read_to_string(trimmed)
        .map(|s| s.trim().to_owned())
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Io {
                detail: format!("cannot read trusted key file {}: {e}", path),
            })
        })
}

#[path = "signature.tests.rs"]
#[cfg(test)]
mod tests;
