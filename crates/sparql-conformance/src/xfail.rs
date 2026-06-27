// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The explicit expected-failure registry.
//!
//! Per the project's "no silent skips" doctrine, every conformance case the
//! native engine cannot yet pass is recorded HERE with a reason. The harness:
//!
//! * runs every discovered case (nothing is skipped at discovery time);
//! * for an `XFAIL` case, treats a real failure as the *expected* outcome but a
//!   surprise PASS as a HARD ERROR (so a stale xfail is caught and removed);
//! * prints an end-of-run tally (`N passed, M xfail, K unexpected-pass, …`).
//!
//! Entries are matched on the test-case IRI's local name (the fragment after the
//! manifest base), which is stable across vendored manifests.

/// Why a conformance case is expected to fail today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XfailReason {
    /// Uses a construct the native engine deliberately does not support yet.
    UnsupportedConstruct,
    /// A federated `SERVICE` shape the harness cannot resolve offline (e.g. a
    /// variable endpoint, which needs the lateral seam).
    PendingService,
    /// The result is format-/order-/blank-node-nondeterministic in a way this
    /// harness does not normalize.
    NonDeterministic,
    /// Known upstream erratum in the vendored fixture.
    UpstreamErratum,
}

impl XfailReason {
    /// A short human-readable label for the tally / logs.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            XfailReason::UnsupportedConstruct => "unsupported-construct",
            XfailReason::PendingService => "pending-service",
            XfailReason::NonDeterministic => "non-deterministic",
            XfailReason::UpstreamErratum => "upstream-erratum",
        }
    }
}

/// One registered expected failure: a case-IRI local-name suffix plus its reason.
pub struct Xfail {
    /// Match when the case IRI ends with this string (usually its local name).
    pub iri_suffix: &'static str,
    /// Why it is expected to fail.
    pub reason: XfailReason,
}

/// The registry. Each entry is justified inline. Vendored W3C cases that the
/// native engine cannot yet pass are added here (Task 7) rather than skipped.
pub const XFAIL: &[Xfail] = &[
    // (Populated as vendored W3C groups are enabled — see Task 7. The GMEOW-owned
    //  smoke suite passes outright and registers nothing.)
];

/// The registered [`XfailReason`] for `case_iri`, if any.
#[must_use]
pub fn lookup(case_iri: &str) -> Option<XfailReason> {
    XFAIL
        .iter()
        .find(|x| case_iri.ends_with(x.iri_suffix))
        .map(|x| x.reason)
}
