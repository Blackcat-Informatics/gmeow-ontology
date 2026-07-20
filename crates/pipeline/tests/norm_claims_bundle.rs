// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Acceptance tests over the SHIPPED bundle: the advice dual-projection.
//!
//! Observe BOTH advice wings in the materialized
//! `generated/dist/gmeow.gts` — the flat `graph/diagnostics` Note finding AND the
//! reified `graph/norm-claims` `gmeow:ComplianceAssessment` claim — for a HARVESTED
//! advisory rule: `advice.candAdviceAvoidBareEntity`, the soft rule harvested from
//! gmeow:Entity's `avoidWhen` prose. One advisory event, two projections
//! (dual-projection-always, P4/P17); this test proves both actually SHIP, not merely
//! that the emitter code exists.
//!
//! Like `correspondence_laws_bundle.rs`, this test `.expect()`s the committed bundle —
//! it FAILS (never silently skips) if `generated/dist/gmeow.gts` is absent. It runs
//! green only after `make sync` materializes the bundle.

use std::path::{Path, PathBuf};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

const GRAPH_NORM_CLAIMS: &str = "https://blackcatinformatics.ca/gmeow/graph/norm-claims";
const GRAPH_DIAGNOSTICS: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";

/// A harvested advisory rule's code both wings project — `advice.` family prefix +
/// the `logic:candAdviceAvoidBareEntity` candidate's local name — embedded in the
/// `graph/norm-claims` claim's content-addressed IRIs (`NORM_CLAIMS_BASE_IRI`). The
/// candidate harvests gmeow:Entity's `avoidWhen` prose, which ships in the bundle.
const ADVICE_CODE: &str = "advice.candAdviceAvoidBareEntity";

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

/// Wing 1 (`graph/norm-claims`): the shipped bundle carries a `gmeow:ComplianceAssessment`
/// whose IRI embeds the harvested advisory code, with exactly one `gmeow:vantage` =
/// `gmeowBestPractice`, exactly one `gmeow:complianceVerdict`, and a `gmeow:assessedNorm`
/// whose object carries `gmeow:deonticModality` = `deonticRecommendation` AND a
/// `gmeow:normIssuer` — the reified, standpoint-indexed advice claim.
#[test]
fn shipped_bundle_norm_claims_carries_the_advisory_compliance_assessment() {
    let triples = graph_triples(GRAPH_NORM_CLAIMS);
    assert!(
        !triples.is_empty(),
        "the shipped gmeow.gts must carry a non-empty `graph/norm-claims` named graph \
         (missing the reified advice wing entirely)"
    );

    let assessment_class = format!("{GMEOW}ComplianceAssessment");
    let assessment_subjects: Vec<&str> = triples
        .iter()
        .filter(|(_, p, o)| p == RDF_TYPE && o == &assessment_class)
        .map(|(s, _, _)| s.as_str())
        .filter(|s| s.contains(ADVICE_CODE))
        .collect();
    assert_eq!(
        assessment_subjects.len(),
        1,
        "expected exactly one gmeow:ComplianceAssessment subject in graph/norm-claims whose \
         IRI embeds `{ADVICE_CODE}`, got {assessment_subjects:?} (full graph has {} triples)",
        triples.len()
    );
    let assessment = assessment_subjects[0];

    let vantage_pred = format!("{GMEOW}vantage");
    let best_practice = format!("{GMEOW}gmeowBestPractice");
    let vantages = objects_of(&triples, assessment, &vantage_pred);
    assert_eq!(
        vantages,
        vec![best_practice.as_str()],
        "the {ADVICE_CODE} ComplianceAssessment must carry exactly one gmeow:vantage = \
         gmeowBestPractice, got {vantages:?}"
    );

    let verdict_pred = format!("{GMEOW}complianceVerdict");
    let verdicts = objects_of(&triples, assessment, &verdict_pred);
    assert_eq!(
        verdicts.len(),
        1,
        "the {ADVICE_CODE} ComplianceAssessment must carry exactly one gmeow:complianceVerdict, \
         got {verdicts:?}"
    );

    let assessed_norm_pred = format!("{GMEOW}assessedNorm");
    let norms = objects_of(&triples, assessment, &assessed_norm_pred);
    assert_eq!(
        norms.len(),
        1,
        "the {ADVICE_CODE} ComplianceAssessment must carry exactly one gmeow:assessedNorm, \
         got {norms:?}"
    );
    let norm = norms[0];

    let modality_pred = format!("{GMEOW}deonticModality");
    let deontic_recommendation = format!("{GMEOW}deonticRecommendation");
    let modalities = objects_of(&triples, norm, &modality_pred);
    assert_eq!(
        modalities,
        vec![deontic_recommendation.as_str()],
        "the assessedNorm {norm} must carry gmeow:deonticModality = deonticRecommendation, \
         got {modalities:?}"
    );

    let issuer_pred = format!("{GMEOW}normIssuer");
    let issuers = objects_of(&triples, norm, &issuer_pred);
    assert!(
        !issuers.is_empty(),
        "the assessedNorm {norm} must carry a gmeow:normIssuer (there is no ought, only \
         ought-according-to), found none"
    );
}

/// Wing 2 (`graph/diagnostics`): the shipped bundle carries a `gmeow:Finding` with
/// `gmeow:findingCode` = the harvested advisory code, graded at the never-gating
/// `gmeow:standpointAdvisory` truth-axis — the flat projection of the same advice event.
#[test]
fn shipped_bundle_diagnostics_carries_the_advisory_finding() {
    let triples = graph_triples(GRAPH_DIAGNOSTICS);
    assert!(
        !triples.is_empty(),
        "the shipped gmeow.gts must carry a non-empty `graph/diagnostics` named graph \
         (missing the flat advice wing entirely)"
    );

    let finding_code_pred = format!("{GMEOW}findingCode");
    // `graph_triples` resolves a literal object to its lexical VALUE (no surrounding
    // quotes, no datatype suffix) via the GTS term table, so match the bare code.
    let code_literal = ADVICE_CODE.to_string();
    let finding_subjects: Vec<&str> = triples
        .iter()
        .filter(|(_, p, o)| p == &finding_code_pred && o == &code_literal)
        .map(|(s, _, _)| s.as_str())
        .collect();
    assert!(
        !finding_subjects.is_empty(),
        "expected at least one subject in graph/diagnostics carrying \
         gmeow:findingCode = \"{ADVICE_CODE}\", found none (searched {} triples)",
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
        "expected the {ADVICE_CODE} finding to carry gmeow:findingStandpoint = \
         standpointAdvisory (the never-gate advisory tier), found none among the \
         findingCode-matching subjects"
    );
}

/// The finding<->claim pairing (Completion-Adversary F3): the SAME advisory code appears
/// BOTH as a `gmeow:findingCode` literal in `graph/diagnostics` AND embedded in the
/// `gmeow:ComplianceAssessment` subject IRI in `graph/norm-claims` — the executable proof
/// that these are two projections of ONE advice event, not two unrelated pieces of content.
#[test]
fn shipped_bundle_pairs_the_finding_and_the_norm_claim_by_advisory_code() {
    let diagnostics = graph_triples(GRAPH_DIAGNOSTICS);
    let norm_claims = graph_triples(GRAPH_NORM_CLAIMS);

    let finding_code_pred = format!("{GMEOW}findingCode");
    // `graph_triples` resolves a literal object to its lexical VALUE (no surrounding
    // quotes, no datatype suffix) via the GTS term table, so match the bare code.
    let code_literal = ADVICE_CODE.to_string();
    let has_matching_finding_code = diagnostics
        .iter()
        .any(|(_, p, o)| p == &finding_code_pred && o == &code_literal);
    assert!(
        has_matching_finding_code,
        "graph/diagnostics must carry a gmeow:findingCode literal \"{ADVICE_CODE}\" — the \
         flat wing of the paired advice event is missing"
    );

    let assessment_class = format!("{GMEOW}ComplianceAssessment");
    let has_matching_claim_iri = norm_claims
        .iter()
        .any(|(s, p, o)| p == RDF_TYPE && o == &assessment_class && s.contains(ADVICE_CODE));
    assert!(
        has_matching_claim_iri,
        "graph/norm-claims must carry a gmeow:ComplianceAssessment whose IRI embeds \
         \"{ADVICE_CODE}\" — the reified wing of the paired advice event is missing"
    );
}
