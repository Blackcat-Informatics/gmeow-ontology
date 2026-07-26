// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Acceptance test over the SHIPPED bundle: the advice dual-projection actually ships.
//!
//! Advice fires from a DATA MATCH: an advisory `logic:Constraint` at `logic:severity "Info"`
//! derives to a `sh:SPARQLConstraint` NodeShape carrying `logic:formalizes`, and produces a
//! `gmeow:ComplianceAssessment` / `gmeow:Finding` pair only when an individual in the validated
//! graph matches its guard (`crates/validate/src/advisory.rs::split_advisory_results`). The
//! shipped bundle's base graph folds every `slices/*/*/module.ttl`, and several of those DO
//! author bare `gmeow:Entity` A-Box individuals (e.g. `gmeow:procedureIngestionRawRoot`,
//! `gmeow:polymeterPattern`) — exactly the anti-pattern `gmeow:BareEntitySortalAdviceConstraint`
//! warns against — so the shipped `generated/dist/gmeow.gts` dogfoods the advisory tier: its
//! advice wing is NON-EMPTY.
//!
//! This test observes BOTH wings for a HARVESTED advisory match — the flat `graph/diagnostics`
//! Note finding AND the reified `graph/norm-claims` `gmeow:ComplianceAssessment` — keyed on the
//! data-dependent `advice.` FAMILY (`advice.<shape-local>.<focus-digest>`), never a hard-coded
//! code, because the code embeds a per-focus digest. It proves both wings SHIP, not merely that
//! the emitter code exists. The isolated, deterministic proof over a controlled fixture lives in
//! `advice_wing_fixture.rs`.
//!
//! Like `correspondence_laws_bundle.rs`, this test `.expect()`s the committed bundle — it FAILS
//! (never silently skips) if `generated/dist/gmeow.gts` is absent. It runs green only after
//! `make regen` materializes the bundle.

use std::path::{Path, PathBuf};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

const GRAPH_NORM_CLAIMS: &str = "https://blackcatinformatics.ca/gmeow/graph/norm-claims";
const GRAPH_DIAGNOSTICS: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";

/// The `advice.` family code prefix every harvested advisory-constraint match's code carries
/// (`crates/validate/src/codes.rs::ADVICE_FAMILY`) — the family this test proves SHIPS in the
/// bundle's advice wing (the full code is `advice.<shape-local>.<focus-digest>`, data-dependent).
const ADVICE_FAMILY: &str = "advice.";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// The ground triples (subject, predicate, object as IRI/label strings) of ONE named graph of
/// the committed `gmeow.gts`, read through the kernel GTS reader.
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

/// Objects `o` such that `(subject, predicate, o)` is present.
fn objects_of<'a>(
    triples: &'a [(String, String, String)],
    subject: &str,
    predicate: &str,
) -> Vec<&'a str> {
    triples
        .iter()
        .filter(|(s, p, _)| s == subject && p == predicate)
        .map(|(_, _, o)| o.as_str())
        .collect()
}

/// Wing 1 (`graph/norm-claims`): the shipped bundle carries at least one `gmeow:ComplianceAssessment`
/// whose IRI embeds an `advice.`-family code, and EVERY such assessment is well-formed — exactly one
/// `gmeow:vantage` = `gmeowBestPractice`, at least one `gmeow:complianceVerdict`, and a
/// `gmeow:assessedNorm` whose object carries `gmeow:deonticModality` = `deonticRecommendation` AND a
/// `gmeow:normIssuer` (the reified, standpoint-indexed advice claim — there is no ought, only
/// ought-according-to).
#[test]
fn shipped_bundle_norm_claims_carries_the_advisory_compliance_assessment() {
    let triples = graph_triples(GRAPH_NORM_CLAIMS);
    assert!(
        !triples.is_empty(),
        "the shipped gmeow.gts must carry a non-empty `graph/norm-claims` named graph \
         (the reified advice wing must ship — the base graph folds bare gmeow:Entity individuals \
         that match the advisory guard)"
    );

    let assessment_class = format!("{GMEOW}ComplianceAssessment");
    let assessment_subjects: Vec<&str> = triples
        .iter()
        .filter(|(_, p, o)| p == RDF_TYPE && o == &assessment_class)
        .map(|(s, _, _)| s.as_str())
        .filter(|s| s.contains(ADVICE_FAMILY))
        .collect();
    assert!(
        !assessment_subjects.is_empty(),
        "expected at least one gmeow:ComplianceAssessment subject in graph/norm-claims whose IRI \
         embeds an `{ADVICE_FAMILY}` code, found none (full graph has {} triples)",
        triples.len()
    );

    let vantage_pred = format!("{GMEOW}vantage");
    let best_practice = format!("{GMEOW}gmeowBestPractice");
    let verdict_pred = format!("{GMEOW}complianceVerdict");
    let assessed_norm_pred = format!("{GMEOW}assessedNorm");
    let modality_pred = format!("{GMEOW}deonticModality");
    let deontic_recommendation = format!("{GMEOW}deonticRecommendation");
    let issuer_pred = format!("{GMEOW}normIssuer");

    for assessment in &assessment_subjects {
        let vantages = objects_of(&triples, assessment, &vantage_pred);
        assert_eq!(
            vantages,
            vec![best_practice.as_str()],
            "advice ComplianceAssessment {assessment} must carry exactly one gmeow:vantage = \
             gmeowBestPractice, got {vantages:?}"
        );

        let verdicts = objects_of(&triples, assessment, &verdict_pred);
        assert_eq!(
            verdicts.len(),
            1,
            "advice ComplianceAssessment {assessment} must carry exactly one \
             gmeow:complianceVerdict, got {verdicts:?}"
        );

        let norms = objects_of(&triples, assessment, &assessed_norm_pred);
        assert_eq!(
            norms.len(),
            1,
            "advice ComplianceAssessment {assessment} must carry exactly one gmeow:assessedNorm, \
             got {norms:?}"
        );
        let norm = norms[0];

        let modalities = objects_of(&triples, norm, &modality_pred);
        assert_eq!(
            modalities,
            vec![deontic_recommendation.as_str()],
            "the assessedNorm {norm} must carry gmeow:deonticModality = deonticRecommendation, \
             got {modalities:?}"
        );

        let issuers = objects_of(&triples, norm, &issuer_pred);
        assert!(
            !issuers.is_empty(),
            "the assessedNorm {norm} must carry a gmeow:normIssuer (there is no ought, only \
             ought-according-to), found none"
        );
    }
}

/// Wing 2 (`graph/diagnostics`): the shipped bundle carries at least one `gmeow:Finding` with a
/// `gmeow:findingCode` in the `advice.` family, graded at the never-gating
/// `gmeow:standpointAdvisory` truth-axis — the flat projection of the same advice event.
#[test]
fn shipped_bundle_diagnostics_carries_the_advisory_finding() {
    let triples = graph_triples(GRAPH_DIAGNOSTICS);
    assert!(
        !triples.is_empty(),
        "the shipped gmeow.gts must carry a non-empty `graph/diagnostics` named graph"
    );

    let finding_code_pred = format!("{GMEOW}findingCode");
    // `graph_triples` resolves a literal object to its lexical VALUE (no surrounding quotes, no
    // datatype suffix) via the GTS term table, so a bare prefix match is correct here.
    let finding_subjects: Vec<&str> = triples
        .iter()
        .filter(|(_, p, o)| p == &finding_code_pred && o.starts_with(ADVICE_FAMILY))
        .map(|(s, _, _)| s.as_str())
        .collect();
    assert!(
        !finding_subjects.is_empty(),
        "expected at least one subject in graph/diagnostics carrying a gmeow:findingCode in the \
         `{ADVICE_FAMILY}` family, found none (searched {} triples)",
        triples.len()
    );

    let standpoint_pred = format!("{GMEOW}findingStandpoint");
    let standpoint_advisory = format!("{GMEOW}standpointAdvisory");
    let advisory_findings: Vec<&str> = finding_subjects
        .into_iter()
        .filter(|subject| {
            objects_of(&triples, subject, &standpoint_pred).contains(&standpoint_advisory.as_str())
        })
        .collect();
    assert!(
        !advisory_findings.is_empty(),
        "expected an advice.* finding to carry gmeow:findingStandpoint = standpointAdvisory (the \
         never-gate advisory tier), found none among the advice.*-codeded subjects"
    );
}

/// The finding<->claim pairing: the SAME advisory code appears BOTH as a `gmeow:findingCode`
/// literal in `graph/diagnostics` AND embedded in a `gmeow:ComplianceAssessment` subject IRI in
/// `graph/norm-claims` — the executable proof that these are two projections of ONE advice event.
#[test]
fn shipped_bundle_pairs_the_finding_and_the_norm_claim_by_advisory_code() {
    let diagnostics = graph_triples(GRAPH_DIAGNOSTICS);
    let norm_claims = graph_triples(GRAPH_NORM_CLAIMS);

    let finding_code_pred = format!("{GMEOW}findingCode");
    let advice_codes: Vec<&str> = diagnostics
        .iter()
        .filter(|(_, p, o)| p == &finding_code_pred && o.starts_with(ADVICE_FAMILY))
        .map(|(_, _, o)| o.as_str())
        .collect();
    assert!(
        !advice_codes.is_empty(),
        "graph/diagnostics must carry at least one gmeow:findingCode literal in the \
         `{ADVICE_FAMILY}` family — the flat wing of the paired advice event"
    );

    let assessment_class = format!("{GMEOW}ComplianceAssessment");
    // Every flat advice finding code must also be embedded in a norm-claims ComplianceAssessment IRI.
    for code in &advice_codes {
        let paired = norm_claims
            .iter()
            .any(|(s, p, o)| p == RDF_TYPE && o == &assessment_class && s.contains(code));
        assert!(
            paired,
            "graph/norm-claims must carry a gmeow:ComplianceAssessment whose IRI embeds the flat \
             finding code \"{code}\" — the reified wing of the paired advice event is missing"
        );
    }
}
