// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The single proof-carrying enrichment entry point over a consumer diagnostic
//! [`Report`] — attaches rule identity (catalog help URIs), registry-authored
//! remediation, and per-term usage guidance to every finding.
//!
//! Both the CLI validate/verify path
//! ([`crate::data_validate::run`](crate::data_validate::run)) and the pipeline
//! validate stage (`crates/pipeline/src/stages/validate.rs`) call
//! [`enrich_findings`] over their respective reports so the two surfaces cannot
//! drift: neither one is allowed to run only part of the enrichment pass.

use gmeow_errors::{GuidanceSource, Report};
use purrdf::RdfDataset;

use crate::rule_catalog::catalog_anchor_uri;

/// The single proof-carrying enrichment pass over a consumer diagnostic report:
/// attaches rule identity (help URIs), registry-authored remediation, and per-term
/// usage guidance to every finding. Reused by the CLI validate/verify path and the
/// pipeline validate stage so the two surfaces cannot drift. Honest absence: a
/// code/term with no authored remediation/guidance carries none.
///
/// `bundle` is the dataset carrying the constraint-catalog `gmeow:ValidationRule`
/// nodes (the rule-governing-term key) and/or authored per-term guidance prose;
/// `subject` is the graph the finding's own `documented_terms` are drawn from. Both
/// are scanned for guidance so a term authored in either resolves (see
/// [`crate::guidance::GuidanceIndex::term_guidance`]).
pub fn enrich_findings(report: &mut Report, bundle: &RdfDataset, subject: &RdfDataset) {
    crate::rule_catalog::populate_rules(report);
    crate::remediation::attach_remediations(report);
    attach_guidance(report, bundle, subject);
}

/// Join per-term usage guidance (`howToUse`/`useWhen`/`avoidWhen`) onto every
/// finding, from BOTH honest keys:
///
/// * the finding's rule's governing term(s)
///   ([`crate::guidance::GuidanceIndex::governing_terms`]), resolved from
///   `bundle`'s constraint-catalog `gmeow:ValidationRule` nodes and stamped
///   with the rule's own catalog help URI ([`catalog_anchor_uri`]); and
/// * the finding's own `documented_terms` — no natural rule-code → help-URI
///   mapping exists for a bare documented term, so those claims carry no help URI
///   (honest absence, never fabricated).
///
/// A claim resolved by both keys for the SAME `(modality, term_iri, text)` is kept
/// once (per-finding dedup; the ledger merge separately dedups identical guidance
/// ACROSS findings). Honest absence: a finding whose rule has no governing term
/// and whose documented terms author no guidance gets an empty `guidance` vec.
///
/// Builds the [`crate::guidance::GuidanceIndex`] ONCE for the whole report (a
/// single pass over `bundle` and `subject`), then does an O(1) lookup per
/// finding — replacing the old per-finding full-bundle scans that made this
/// pass O(findings × bundle).
fn attach_guidance(report: &mut Report, bundle: &RdfDataset, subject: &RdfDataset) {
    let graphs = [bundle, subject];
    let index = crate::guidance::GuidanceIndex::build(&graphs);
    for finding in &mut report.findings {
        let mut claims = Vec::new();

        for term in index.governing_terms(&finding.code) {
            claims.extend(index.term_guidance(
                term,
                GuidanceSource::RuleGoverningTerm,
                Some(catalog_anchor_uri(&finding.code)),
            ));
        }
        for term in &finding.documented_terms {
            claims.extend(index.term_guidance(term, GuidanceSource::DocumentedTerm, None));
        }

        claims.sort_by(|a, b| {
            (a.modality as u8, &a.term_iri, &a.text).cmp(&(b.modality as u8, &b.term_iri, &b.text))
        });
        claims.dedup_by(|a, b| {
            a.modality == b.modality && a.term_iri == b.term_iri && a.text == b.text
        });

        for claim in claims {
            finding.push_guidance(claim);
        }
    }
}
