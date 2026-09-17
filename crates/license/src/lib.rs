// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW license-token policy classifier — the RUST-FIRST single source of truth.
//!
//! A pure, dependency-free classifier over SPDX-ish license identifiers. One algorithm,
//! two named consumers:
//!   * `gmeow-conformance` — whether a third-party test corpus may be *vendored* into
//!     `cases/external/`.
//!   * the Python `gmeow_tools.config.LinkPolicy` surface — whether an external
//!     vocabulary's *axioms may be copied* into the CC-BY-published GMEOW ontology.
//!
//! Same algorithm, two named consumers; the Python side is a thin marshalling shim over
//! the PyO3 `license_policy_for` entrypoint (in `gmeow-validate`), which delegates here.
//!
//! The classifier is conservative: a restrictive marker (NC/ND/SA/GPL/…) anywhere in the
//! token forces [`LicensePolicy::ReferenceOnly`], even if a permissive substring is
//! present (e.g. `CC-BY-NC-SA`). An unknown license defaults to `ReferenceOnly` so a
//! mistake fails safe (vendoring refused / axiom-copying refused, linking still allowed).

/// Whether a license clears content reuse (vendoring / axiom copying).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicensePolicy {
    /// Compatibly licensed — the content may be reused (vendored / axioms copied).
    ImportOk,
    /// Restrictive / unknown — content reuse is refused; the source may still be
    /// referenced by IRI (which copies nothing).
    ReferenceOnly,
}

/// License-id tokens (uppercased) that block content reuse.
/// Non-commercial, no-derivatives, share-alike, and copyleft software licenses.
const REFERENCE_ONLY_MARKERS: [&str; 8] = [
    "NC",          // non-commercial
    "ND",          // no-derivatives
    "SA",          // share-alike
    "GPL",         // GPL / LGPL / AGPL copyleft
    "EUPL",        // European Union Public License (copyleft)
    "PROPRIETARY", //
    "INTERNAL",    //
    "ACADEMIC",    //
];

/// License-id tokens (uppercased) explicitly cleared for content reuse.
const IMPORT_OK_LICENSES: [&str; 22] = [
    "CC0",
    "CC0-1.0",
    "CC-BY",
    "CC-BY-1.0",
    "CC-BY-3.0",
    "CC-BY-4.0",
    "MIT",
    "APACHE-2.0",
    "BSD-2-CLAUSE",
    "BSD-3-CLAUSE",
    "PDDL-1.0",
    "PDDL",
    "ODC-BY-1.0",
    "ODC-BY",
    "PUBLIC-DOMAIN",
    "PUBLIC DOMAIN",
    "W3C",
    "W3C-DOCUMENT",
    "OGC",
    "NIST-PUBLIC-DOMAIN",
    "NIST PUBLIC DOMAIN",
    "UNLICENSE",
];

/// Classify a license identifier into a reuse policy.
///
/// Restrictive markers win over any permissive substring; the bare `CC-BY-<version>`
/// family is cleared when it carries no `SA`/`NC`; everything unrecognised fails safe
/// to [`LicensePolicy::ReferenceOnly`].
pub fn policy_for_license(license_id: &str) -> LicensePolicy {
    let token = license_id.trim().to_uppercase();
    // Restrictive markers win, regardless of any permissive substring.
    for marker in REFERENCE_ONLY_MARKERS {
        if has_marker_segment(&token, marker) {
            return LicensePolicy::ReferenceOnly;
        }
    }
    if IMPORT_OK_LICENSES.contains(&token.as_str()) {
        return LicensePolicy::ImportOk;
    }
    // Bare "CC-BY" with a version suffix not already listed (substring `SA`/`NC`
    // check mirrors the belt-and-suspenders guard).
    if token.starts_with("CC-BY-") && !token.contains("SA") && !token.contains("NC") {
        return LicensePolicy::ImportOk;
    }
    LicensePolicy::ReferenceOnly
}

/// Whether `marker` appears as a `-`/`_`/space-delimited segment of `token`.
///
/// Segment matching means `ND` does NOT spuriously match inside `PUBLIC-DOMAIN`.
/// The GPL family also matches as a suffix segment (`LGPL`, `AGPL-3.0`).
fn has_marker_segment(token: &str, marker: &str) -> bool {
    let normalized = token.replace(['_', ' '], "-");
    let segments: Vec<&str> = normalized.split('-').collect();
    if segments.contains(&marker) {
        return true;
    }
    marker == "GPL" && segments.iter().any(|seg| seg.ends_with("GPL"))
}

/// The descriptor of a vendored external corpus — the `corpus.json` fields that bear on the
/// reuse policy. The vendoring CATEGORY below is keyed off THESE descriptor fields, never off
/// a filesystem path, so it is reusable for any future vendored corpus with no policy churn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VendoredCorpus<'a> {
    /// SPDX license identifier of the vendored artifacts.
    pub spdx_license: &'a str,
    /// The upstream source URL the attribution points at (the provenance of the vendored
    /// bytes). Must be non-empty for the share-alike vendoring category.
    pub source_url: &'a str,
    /// The human attribution the vendored corpus credits (authors / project). Must be
    /// non-empty for the share-alike vendoring category.
    pub attribution: &'a str,
    /// Whether the corpus is RING-FENCED: committed as a clearly-separated, non-published
    /// vendored fragment that is NEVER merged into the CC-BY GMEOW vocabulary.
    pub ring_fenced: bool,
}

/// Classify a vendored corpus into a reuse policy, admitting the ONE additional category the
/// bare-token classifier cannot express: a **ring-fenced, fully-attributed CC BY-SA
/// share-alike corpus**.
///
/// Share-alike (`SA`) is [`LicensePolicy::ReferenceOnly`] as a bare token because copying such
/// content into the CC-BY-published GMEOW vocabulary would violate the copyleft. But a corpus
/// that is (a) ring-fenced — never merged into the published vocabulary — AND (b) fully
/// attributed — a credited source URL and attribution line — honours CC BY-SA 4.0's own terms,
/// so it clears vendoring as [`LicensePolicy::ImportOk`].
///
/// The exception is gated on the CC-BY-SA share-alike family SPECIFICALLY and does NOT loosen
/// any other restrictive license: a non-commercial (`NC`) or no-derivatives (`ND`) CC license,
/// a GPL/EUPL copyleft, or a proprietary/unknown token stays [`LicensePolicy::ReferenceOnly`]
/// regardless of ring-fencing or attribution.
pub fn policy_for_vendored_corpus(corpus: &VendoredCorpus) -> LicensePolicy {
    // A token the bare classifier already clears needs no exception.
    if policy_for_license(corpus.spdx_license) == LicensePolicy::ImportOk {
        return LicensePolicy::ImportOk;
    }
    // The share-alike vendoring exception: CC-BY-SA (attribution + share-alike, and NOT
    // NC/ND), ring-fenced, with a non-empty source URL and attribution.
    if is_cc_by_sa(corpus.spdx_license)
        && corpus.ring_fenced
        && !corpus.attribution.trim().is_empty()
        && !corpus.source_url.trim().is_empty()
    {
        return LicensePolicy::ImportOk;
    }
    LicensePolicy::ReferenceOnly
}

/// Whether the token is a Creative-Commons Attribution-ShareAlike license (`CC-BY-SA-*`):
/// a CC attribution + share-alike license carrying NO non-commercial (`NC`) or no-derivatives
/// (`ND`) restriction. These are exactly the licenses the ring-fenced-vendoring category
/// admits; a CC license bearing `NC`/`ND` is deliberately excluded.
fn is_cc_by_sa(license_id: &str) -> bool {
    let token = license_id.trim().to_uppercase().replace(['_', ' '], "-");
    let segments: Vec<&str> = token.split('-').collect();
    segments.first() == Some(&"CC")
        && segments.contains(&"BY")
        && segments.contains(&"SA")
        && !segments.contains(&"NC")
        && !segments.contains(&"ND")
}

#[path = "lib.tests.rs"]
#[cfg(test)]
mod tests;
