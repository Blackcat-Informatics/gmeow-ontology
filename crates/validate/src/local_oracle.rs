// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The loop-closure enrichment join — the wasm-clean core that turns a bare
//! validation [`Report`](gmeow_errors::model::Report) into a *teaching* envelope by
//! CORRESPONDING each finding to the bundle's own documentation surface.
//!
//! # Why this lives here (and not in the pipeline)
//!
//! The MCP consumer server (`gmeow mcp`) drives this join, but so does the
//! WASM-interactive-docs sibling (`gmeow-validate-wasm`, editor/browser). Both feed
//! the SAME structure: a validation `Report` plus three lookup maps extracted from
//! the `gmeow:graph/documentation` projection. So the *pure* join — "for each
//! finding, attach the corresponding counter-example, the positive exemplar, the
//! rule's help URI, and the term's entailments" — is factored here in `gmeow-validate`,
//! which is wasm-clean (no RDF store, no pipeline carrier, no reasoner). The RDF
//! *extraction* that builds the maps is the caller's job (native RDF store or the
//! wasm doc-graph reader); this module never touches a triple.
//!
//! # The correspondence invariant
//!
//! A counter-example is attached to a finding IFF the fixture's authored
//! `gmeow:docViolationCode` EQUALS the finding's emitted code — never a blanket
//! attach-the-first-fixture. `enrich_report` enforces this by keying the
//! counter-example map on the code; the caller MUST build that map keyed on the
//! authored violation code (see the pipeline's `tool_validate_local`). The
//! `enrich_report_attaches_by_correspondence` unit test proves a finding whose code
//! has no matching fixture gets `None`.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::rule_catalog::help_uri_for;

/// Namespace under which a *minted* (non-ledger) finding identity is anchored, so an
/// enriched envelope always carries a stable `finding_iri` even for a hand-built
/// finding that was never a ledger witness.
const MINTED_FINDING_BASE: &str = "https://blackcatinformatics.ca/gmeow/finding/local/";

/// A conformance fixture projected to the shape the enrichment envelope carries — a
/// self-contained teaching artifact (title + the full Turtle body + the authored
/// conformance metadata). Field order is fixed for deterministic serialization.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct FixtureView {
    /// The human label the docs projection annotates the fixture with
    /// (`rdfs:label`, e.g. `Conformance fixture: <title>`).
    pub title: String,
    /// The FULL Turtle body of the fixture (`gmeow:docFixtureText`) — runnable as-is.
    pub text: String,
    /// The authored expected outcome (`gmeow:docExpectedOutcome`), when the slice
    /// binds one.
    pub expected_outcome: Option<String>,
    /// The authored violation code (`gmeow:docViolationCode`) — the correspondence
    /// key for a counter-example. `None` on a well-formed exemplar (it violates
    /// nothing).
    pub violation_code: Option<String>,
    /// The authored conformance rationale (`gmeow:conformanceRationale`), when present.
    pub rationale: Option<String>,
}

/// One entailment (a single derivation step) projected for a documented term — the
/// rule that fires, its conclusion, and every premise in the derivation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct EntailmentView {
    /// The entailment rule name (`gmeow:entailmentRule`).
    pub rule: String,
    /// The derived conclusion (`gmeow:entailmentConclusion`).
    pub conclusion: String,
    /// Every premise the derivation rests on (`gmeow:entailmentPremise`), sorted for
    /// determinism.
    pub premises: Vec<String>,
}

/// One validation finding, enriched with the bundle's own teaching surface: the
/// rule's catalog help URI, the CORRESPONDING counter-example, a positive exemplar,
/// and the entailments of the term the finding concerns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EnrichedFinding {
    /// The finding code (the correspondence key).
    pub code: String,
    /// The finding's human message.
    pub message: String,
    /// The rule catalog's help URI for `code` (always present — `help_uri_for` is
    /// infallible).
    pub help_uri: String,
    /// The counter-example whose authored violation code EQUALS `code`, or `None`
    /// when the bundle documents no such fixture (honest absence).
    pub counter_example: Option<FixtureView>,
    /// The positive (well-formed) exemplar for the same shape/term, or `None`.
    pub wellformed_exemplar: Option<FixtureView>,
    /// The entailments of the term this finding concerns (empty when the finding's
    /// subject term is unknown or undocumented — never fabricated).
    pub entails: Vec<EntailmentView>,
    /// The finding's canonical fingerprint IRI, carried through when the finding was
    /// a ledger witness, else a deterministic minted identity so the envelope always
    /// has a stable handle.
    pub finding_iri: Option<String>,
    /// The finding's source-anchor IRI, carried through verbatim (`None` for a
    /// hand-built finding).
    pub anchor_iri: Option<String>,
}

/// The complete enriched validation envelope returned by `validate_local`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EnrichedReport {
    /// `true` iff the underlying report carries no `Error`-severity finding.
    pub ok: bool,
    /// The tool origin of the underlying report.
    pub tool: String,
    /// The enriched findings, in the report's existing (sorted) order.
    pub findings: Vec<EnrichedFinding>,
}

/// Enrich `report` by CORRESPONDENCE against the three documentation lookup maps.
///
/// * `counter_examples_by_code` — `finding-code → counter-example fixture`; a
///   finding gets a counter-example IFF its code is a key (the correspondence
///   invariant).
/// * `wellformed_by_code` — `finding-code → positive exemplar` for the same
///   shape/term (built best-effort by the caller via the shared referenced term).
/// * `entailments_by_term` — `term-IRI → entailments`; a finding's entailments are
///   the union over the terms it structurally concerns (its `documented_terms` plus
///   its primary focus node), sorted and deduplicated. Empty when none are
///   documented.
///
/// Findings keep the report's existing order; `entails` is sorted; a finding without
/// a carried `finding_iri` is given a deterministic minted one from a hash of
/// `(code, primary-location-display)`.
pub fn enrich_report(
    report: &gmeow_errors::model::Report,
    counter_examples_by_code: &BTreeMap<String, FixtureView>,
    wellformed_by_code: &BTreeMap<String, FixtureView>,
    entailments_by_term: &BTreeMap<String, Vec<EntailmentView>>,
) -> EnrichedReport {
    let findings = report
        .findings
        .iter()
        .map(|finding| {
            let help_uri = help_uri_for(&finding.code);
            let counter_example = counter_examples_by_code.get(&finding.code).cloned();
            let wellformed_exemplar = wellformed_by_code.get(&finding.code).cloned();
            let entails = entailments_for(finding, entailments_by_term);
            let finding_iri = finding
                .finding_iri
                .clone()
                .or_else(|| Some(mint_finding_iri(finding)));
            EnrichedFinding {
                code: finding.code.clone(),
                message: finding.message.clone(),
                help_uri,
                counter_example,
                wellformed_exemplar,
                entails,
                finding_iri,
                anchor_iri: finding.anchor_iri.clone(),
            }
        })
        .collect();
    EnrichedReport {
        ok: report.ok(),
        tool: report.tool.clone(),
        findings,
    }
}

/// The entailments a finding concerns: the union over every term the finding
/// structurally names — its `documented_terms` (e.g. a SHACL violation's
/// constrained `sh:path` property) plus its primary focus node — deduplicated and
/// sorted. Honest-empty when the finding names no documented term.
fn entailments_for(
    finding: &gmeow_errors::model::Finding,
    entailments_by_term: &BTreeMap<String, Vec<EntailmentView>>,
) -> Vec<EntailmentView> {
    let mut terms: Vec<&str> = finding
        .documented_terms
        .iter()
        .map(String::as_str)
        .collect();
    if let Some(logical) = finding.locations.first().and_then(|l| l.logical.as_deref()) {
        terms.push(logical);
    }
    terms.sort_unstable();
    terms.dedup();

    let mut out: Vec<EntailmentView> = terms
        .into_iter()
        .filter_map(|term| entailments_by_term.get(term))
        .flatten()
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

/// A deterministic minted `finding_iri` for a finding that carries none — a
/// wasm-clean [`FNV-1a`](fnv1a_64) hash over `(code, NUL, primary-location-display)`
/// so the same finding at the same source position always mints the same identity.
/// FNV-1a is a fixed, dependency-free algorithm (identical on native and wasm and
/// stable across toolchain versions), so the minted identity is reproducible
/// everywhere.
fn mint_finding_iri(finding: &gmeow_errors::model::Finding) -> String {
    let mut buf = finding.code.clone();
    buf.push('\u{0}');
    buf.push_str(&primary_location_display(finding));
    format!("{MINTED_FINDING_BASE}{:016x}", fnv1a_64(buf.as_bytes()))
}

/// The 64-bit FNV-1a hash — a fixed, dependency-free, byte-order-independent hash
/// (offset basis `0xcbf29ce484222325`, prime `0x100000001b3`). Deterministic and
/// identical on every target, so it is a stable minted-identity source that adds no
/// wasm dependency.
fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET_BASIS;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// A stable text rendering of a finding's primary location — the hash pre-image
/// paired with the code when minting an identity. Empty when the finding carries no
/// location.
fn primary_location_display(finding: &gmeow_errors::model::Finding) -> String {
    match finding.locations.first() {
        Some(loc) => format!(
            "{}:{}:{}:{}",
            loc.path.as_deref().unwrap_or(""),
            loc.line.map(|l| l.to_string()).unwrap_or_default(),
            loc.column.map(|c| c.to_string()).unwrap_or_default(),
            loc.logical.as_deref().unwrap_or(""),
        ),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::model::{Finding, Location, Report, Severity};

    const MIN_COUNT: &str = "shacl.MinCountConstraintComponent";

    fn fixture(title: &str, code: Option<&str>) -> FixtureView {
        FixtureView {
            title: title.to_string(),
            text: "<urn:s> <urn:p> <urn:o> .".to_string(),
            expected_outcome: Some("nonconforming".to_string()),
            violation_code: code.map(str::to_string),
            rationale: Some("min-count under the required floor".to_string()),
        }
    }

    /// The core correspondence proof: a finding whose code matches a counter-example
    /// gets it (with the help URI + a positive exemplar + the term's entailments); a
    /// finding whose code matches NOTHING gets a bare envelope. This proves the join
    /// is by-correspondence, not a blanket attach.
    #[test]
    fn enrich_report_attaches_by_correspondence() {
        let term = "https://ex/prop";
        let mut counter_examples = BTreeMap::new();
        counter_examples.insert(
            MIN_COUNT.to_string(),
            fixture("bad example", Some(MIN_COUNT)),
        );
        let mut wellformed = BTreeMap::new();
        wellformed.insert(MIN_COUNT.to_string(), fixture("good example", None));
        let mut entailments = BTreeMap::new();
        entailments.insert(
            term.to_string(),
            vec![EntailmentView {
                rule: "subClassOf".to_string(),
                conclusion: "x a C".to_string(),
                premises: vec!["x a D".to_string(), "D subClassOf C".to_string()],
            }],
        );

        // A finding whose code MATCHES the counter-example and whose documented term
        // MATCHES the entailment map.
        let mut matched = Finding::new(Severity::Error, MIN_COUNT, "min count violated")
            .with_documented_term(term);
        matched.add_location(Location::new(
            Some("mcp:validate_local".to_string()),
            None,
            None,
            Some("https://ex/focus".to_string()),
        ));
        // A finding whose code has NO matching fixture.
        let unmatched = Finding::new(
            Severity::Warning,
            "shacl.PatternConstraintComponent",
            "pattern mismatch",
        );

        let mut report = Report::new("mcp:validate_local");
        report.add_finding(matched);
        report.add_finding(unmatched);

        let enriched = enrich_report(&report, &counter_examples, &wellformed, &entailments);
        assert!(
            !enriched.ok,
            "an Error-severity finding makes the report not ok"
        );
        assert_eq!(enriched.findings.len(), 2);

        let m = &enriched.findings[0];
        assert_eq!(m.help_uri, help_uri_for(MIN_COUNT));
        assert_eq!(
            m.counter_example
                .as_ref()
                .and_then(|c| c.violation_code.as_deref()),
            Some(MIN_COUNT),
            "the matched finding gets the CORRESPONDING counter-example",
        );
        assert!(
            m.wellformed_exemplar.is_some(),
            "the matched finding gets a positive exemplar",
        );
        assert!(
            !m.entails.is_empty(),
            "the matched finding surfaces the documented term's entailments",
        );

        let u = &enriched.findings[1];
        assert_eq!(
            u.counter_example, None,
            "a finding whose code has no matching fixture gets NO counter-example (correspondence, not blanket-attach)",
        );
        assert!(
            u.entails.is_empty(),
            "an undocumented term surfaces no entailments"
        );
        assert_eq!(u.help_uri, help_uri_for("shacl.PatternConstraintComponent"));
    }

    /// A finding carrying no `finding_iri` is given a deterministic minted one, and
    /// the same finding always mints the same identity.
    #[test]
    fn enrich_report_mints_stable_identity_when_absent() {
        let f = Finding::new(Severity::Error, MIN_COUNT, "min count violated");
        let mut report = Report::new("mcp:validate_local");
        report.add_finding(f);
        let empty = BTreeMap::new();
        let entail_empty = BTreeMap::new();

        let a = enrich_report(&report, &empty, &empty, &entail_empty);
        let b = enrich_report(&report, &empty, &empty, &entail_empty);
        let iri = a.findings[0]
            .finding_iri
            .as_deref()
            .expect("minted identity");
        assert!(
            iri.starts_with(MINTED_FINDING_BASE),
            "minted under the local finding namespace"
        );
        assert_eq!(
            a.findings[0].finding_iri, b.findings[0].finding_iri,
            "minting is deterministic",
        );
    }
}
