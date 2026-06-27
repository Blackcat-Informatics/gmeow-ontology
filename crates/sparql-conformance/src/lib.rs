// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]

//! Native W3C SPARQL 1.1 conformance harness (S6b #928).
//!
//! Discovers `mf:` test manifests, runs each case against the native
//! [`gmeow_sparql_eval`] engine (zero oxigraph Store), and diffs the result
//! against the expected SPARQL Results (SRX/SRJ) or canonical N-Quads. The
//! datatest-stable test harness ([`tests/sparql_conformance.rs`]) emits one
//! nextest case per `manifest.ttl`; each loops its entries via [`run_manifest`].
//!
//! Expected failures are recorded in [`xfail`] — never skipped — and the
//! per-manifest [`Summary`] prints a tally (`passed / xfail / unexpected-pass /
//! failed`). An xfail that unexpectedly PASSES is a hard error so the registry
//! cannot rot.

pub mod compare;
pub mod manifest;
pub mod paths;
pub mod run;
pub mod xfail;

use std::path::Path;

use manifest::SparqlTestCase;
use xfail::XfailReason;

/// Per-manifest run summary.
#[derive(Debug, Default)]
pub struct Summary {
    /// Cases that passed (and were not registered as xfail).
    pub passed: usize,
    /// Cases that failed as their xfail entry expected.
    pub xfail: usize,
    /// Registered-xfail cases that unexpectedly PASSED (a hard error: the entry
    /// is stale and must be removed). Carries the case IRI + reason label.
    pub unexpected_pass: Vec<String>,
    /// Cases that failed without an xfail entry: `(case IRI, message)`.
    pub failed: Vec<(String, String)>,
    /// Cases whose `rdf:type` the harness does not model (surfaced, not skipped).
    pub unmodeled: Vec<String>,
}

impl Summary {
    /// True when the manifest passed: no unexpected passes and no unexplained
    /// failures (xfails are allowed).
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.unexpected_pass.is_empty() && self.failed.is_empty()
    }

    /// A one-line tally for the run log.
    #[must_use]
    pub fn tally_line(&self) -> String {
        format!(
            "{} passed, {} xfail, {} unexpected-pass, {} failed, {} unmodeled",
            self.passed,
            self.xfail,
            self.unexpected_pass.len(),
            self.failed.len(),
            self.unmodeled.len(),
        )
    }

    /// A detailed failure report for the datatest error message.
    #[must_use]
    pub fn failure_report(&self) -> String {
        let mut lines = Vec::new();
        for iri in &self.unexpected_pass {
            lines.push(format!("  • UNEXPECTED PASS (remove xfail): {iri}"));
        }
        for (iri, msg) in &self.failed {
            lines.push(format!("  • FAIL {iri}: {msg}"));
        }
        lines.join("\n")
    }
}

/// The verdict for a single case before xfail accounting.
enum Verdict {
    Pass,
    Fail(String),
    Unmodeled,
}

/// Run every case declared by `manifest_path`, honoring the [`xfail`] registry.
///
/// # Errors
///
/// Returns a message if the manifest itself cannot be loaded/parsed.
pub fn run_manifest(manifest_path: &Path) -> Result<Summary, String> {
    let cases = manifest::load(manifest_path)?;
    let mut summary = Summary::default();
    for case in &cases {
        match verdict_of(case) {
            Verdict::Unmodeled => summary.unmodeled.push(case.iri.clone()),
            Verdict::Pass => match xfail::lookup(&case.iri) {
                Some(reason) => summary.unexpected_pass.push(format!(
                    "{} (xfail: {})",
                    case.iri,
                    reason.label()
                )),
                None => summary.passed += 1,
            },
            Verdict::Fail(msg) => match xfail::lookup(&case.iri) {
                Some(reason) => {
                    log_xfail(&case.iri, reason, &msg);
                    summary.xfail += 1;
                }
                None => summary.failed.push((case.iri.clone(), msg)),
            },
        }
    }
    Ok(summary)
}

/// Run + compare a single case into a [`Verdict`].
fn verdict_of(case: &SparqlTestCase) -> Verdict {
    if matches!(case.kind, manifest::TestKind::Unknown) {
        return Verdict::Unmodeled;
    }
    // SERVICE source wiring lands in Task 6b; until then a case with service data
    // resolves with no source (so a non-silent SERVICE fails and is recorded).
    let remote = service_source(case);
    match run::run(case, remote.as_deref()) {
        Ok(outcome) => match compare::compare(case, &outcome) {
            Ok(()) => Verdict::Pass,
            Err(msg) => Verdict::Fail(msg),
        },
        Err(msg) => Verdict::Fail(msg),
    }
}

/// Build an in-memory `SERVICE` source for `case`, if it declares service data.
///
/// Task 6a has no service fixtures, so this is `None`; Task 6b overrides it to map
/// `qt:serviceData` endpoints to in-memory datasets.
fn service_source(_case: &SparqlTestCase) -> Option<Box<dyn gmeow_sparql_eval::RemoteQuerySource>> {
    None
}

/// Log an expected failure (with its reason) so xfails are visible, not silent.
fn log_xfail(iri: &str, reason: XfailReason, msg: &str) {
    eprintln!("[xfail: {}] {iri} — {msg}", reason.label());
}
