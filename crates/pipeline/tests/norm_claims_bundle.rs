// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Acceptance test over the SHIPPED bundle: the advice dual-projection is honestly EMPTY.
//!
//! Advice fires only on a DATA MATCH: a `logic:Constraint` at `logic:severity "Info"`
//! (a "advice-constraint") derives to a `sh:SPARQLConstraint` NodeShape carrying
//! `logic:formalizes`, and that shape only produces a `gmeow:ComplianceAssessment` /
//! `gmeow:Finding` pair when an individual in the validated graph actually matches its
//! guard (`crates/validate/src/advisory.rs::split_advisory_results`). The SHIPPED bundle's
//! base graph is deliberately TBox-only — `slices/*/*/module.ttl` + imports, with NO
//! `examples/` individuals folded in — so no individual anywhere in `generated/dist/gmeow.gts`
//! ever matches an advisory guard, and the advice wing (`graph/norm-claims`
//! `gmeow:ComplianceAssessment`s embedding an `advice.` code, `graph/diagnostics`
//! `gmeow:findingCode` literals starting with `advice.`) is honestly EMPTY in the shipped
//! bundle. This test asserts exactly that absence, rather than asserting the presence of a
//! specific harvested code that nothing in the current architecture produces.
//!
//! The POSITIVE proof that the advisory dual-projection machinery actually works — the real
//! `gmeow:BareEntitySortalAdviceConstraint` fired end-to-end over a fixture individual that
//! DOES match its guard — lives in `advice_wing_fixture.rs`, which supplies its own
//! TEST-ONLY anti-pattern individual (deliberately absent from the shipped bundle) and drives
//! the full compile → validate → split → project pipeline over it.
//!
//! Like `correspondence_laws_bundle.rs`, this test `.expect()`s the committed bundle — it
//! FAILS (never silently skips) if `generated/dist/gmeow.gts` is absent. It runs green only
//! after `make sync` materializes the bundle.

use std::path::{Path, PathBuf};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

const GRAPH_NORM_CLAIMS: &str = "https://blackcatinformatics.ca/gmeow/graph/norm-claims";
const GRAPH_DIAGNOSTICS: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";

/// The `advice.` family code prefix every harvested advisory-constraint match's code carries
/// (`crates/validate/src/codes.rs::ADVICE_FAMILY`) — the string this test proves is ABSENT
/// from the shipped bundle's advice wing.
const ADVICE_FAMILY: &str = "advice.";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// The ground triples (subject, predicate, object as IRI/label strings) of ONE named graph of
/// the committed `gmeow.gts`, read through the kernel GTS reader. Returns an empty vector, not
/// an error, when the named graph is entirely absent from the bundle (no such graph-name term
/// interned) — an absent graph and an empty graph are both honest "no advice wing" states.
fn graph_triples(graph_iri: &str) -> Vec<(String, String, String)> {
    let bytes =
        std::fs::read(repo_root().join("generated/dist/gmeow.gts")).expect("committed gmeow.gts");
    let g = purrdf::gts::read_graph(&bytes, true).expect("read_graph");
    let term = |id: usize| -> String {
        g.terms
            .get(id)
            .and_then(|t| t.value.clone())
            .unwrap_or_else(|| format!("<term {id}>"))
    };
    let mut out = Vec::new();
    for &(s, p, o, gname) in &g.quads {
        let Some(gid) = gname else { continue };
        if term(gid) != graph_iri {
            continue;
        }
        out.push((term(s), term(p), term(o)));
    }
    out
}

/// Wing 1 (`graph/norm-claims`): the shipped bundle carries NO `gmeow:ComplianceAssessment`
/// whose subject IRI embeds an `advice.`-family code. A TBox-only bundle has no anti-pattern
/// individual for any data-matching advisory guard to fire on, so the reified advice wing is
/// honestly empty — an absent or empty `graph/norm-claims` graph both satisfy this.
#[test]
fn shipped_bundle_norm_claims_carries_no_advisory_compliance_assessment() {
    let triples = graph_triples(GRAPH_NORM_CLAIMS);

    let assessment_class = format!("{GMEOW}ComplianceAssessment");
    let advisory_assessment_subjects: Vec<&str> = triples
        .iter()
        .filter(|(_, p, o)| p == RDF_TYPE && o == &assessment_class)
        .map(|(s, _, _)| s.as_str())
        .filter(|s| s.contains(ADVICE_FAMILY))
        .collect();
    assert!(
        advisory_assessment_subjects.is_empty(),
        "the TBox-only shipped bundle must carry NO gmeow:ComplianceAssessment whose IRI \
         embeds an `{ADVICE_FAMILY}` code (no anti-pattern individual exists for a \
         data-matching advisory guard to fire on); found: {advisory_assessment_subjects:?}"
    );
}

/// Wing 2 (`graph/diagnostics`): the shipped bundle carries NO `gmeow:findingCode` literal
/// starting with the `advice.` family prefix — the flat wing is equally honestly empty.
#[test]
fn shipped_bundle_diagnostics_carries_no_advisory_finding_code() {
    let triples = graph_triples(GRAPH_DIAGNOSTICS);

    let finding_code_pred = format!("{GMEOW}findingCode");
    // `graph_triples` resolves a literal object to its lexical VALUE (no surrounding quotes,
    // no datatype suffix) via the GTS term table, so a bare prefix match is correct here.
    let advisory_finding_codes: Vec<&str> = triples
        .iter()
        .filter(|(_, p, o)| p == &finding_code_pred && o.starts_with(ADVICE_FAMILY))
        .map(|(_, _, o)| o.as_str())
        .collect();
    assert!(
        advisory_finding_codes.is_empty(),
        "the TBox-only shipped bundle must carry NO gmeow:findingCode literal starting with \
         `{ADVICE_FAMILY}` (no anti-pattern individual exists for a data-matching advisory \
         guard to fire on); found: {advisory_finding_codes:?}"
    );
}
