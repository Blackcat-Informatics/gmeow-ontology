// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

const W: &str = "http://gmeow.example/w";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const A: &str = "http://gmeow.example/A";
const B: &str = "http://gmeow.example/B";
const C: &str = "http://gmeow.example/C";

fn selected_world(edb: &RdfDataset, world: &str) -> crate::physical::SelectedDomains {
    use crate::physical::{DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld};

    let input = crate::reason::prepare_reasoning_input(edb).expect("synthetic verification input");
    SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Named(TermValue::iri(world)),
        DomainProfile::NonemptyObjectDomainV1,
        "urn:test:verification:source-world".to_owned(),
        *input.ingress_contract(),
    )
    .expect("explicit verification world")])
    .expect("one verification world")
}

fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
}

fn store() -> std::sync::Arc<RdfDataset> {
    // A ⊑ B ⊑ C — the native EL closure derives A ⊑ C.
    let mut builder = RdfDatasetBuilder::new();
    for quad in [quad(A, SUBCLASS, B), quad(B, SUBCLASS, C)] {
        builder.push_owned_quad(&quad);
    }
    builder.freeze().expect("valid test dataset")
}

/// The embedded verify query set is non-empty, sorted by stem, contains a
/// known top-level query, and carries no empty sparql text — proving
/// `build.rs` actually walked `queries/verify/` + `slices/**/queries/verify/`
/// and embedded real content rather than an empty/degenerate set.
#[test]
fn embedded_verify_queries_are_sorted_nonempty_and_known() {
    let queries = embedded_verify_queries();
    assert!(
        !queries.is_empty(),
        "the embedded verify query set must not be empty"
    );
    for window in queries.windows(2) {
        let (prev, next) = (&window[0].0, &window[1].0);
        assert!(
            prev < next,
            "embedded_verify_queries() must be strictly sorted by stem: {prev:?} >= {next:?}"
        );
    }
    assert!(
        queries
            .iter()
            .any(|(stem, _)| stem == "notability-without-secondary"),
        "the known top-level query `notability-without-secondary` must be embedded: {:?}",
        queries.iter().map(|(s, _)| s).collect::<Vec<_>>()
    );
    for (stem, sparql) in &queries {
        assert!(
            !sparql.trim().is_empty(),
            "embedded query {stem:?} must carry non-empty sparql text"
        );
    }
}

#[test]
fn clean_query_yields_no_error_findings() {
    // No class is a subclass of itself → no rows → clean.
    let q = (
        "queries/verify/no-self-subclass.rq".to_owned(),
        format!("SELECT ?x WHERE {{ ?x <{SUBCLASS}> ?x }}"),
    );
    let dataset = store();
    let report = verify(
        dataset.as_ref(),
        std::slice::from_ref(&q),
        &selected_world(&dataset, W),
    )
    .expect("verify runs");
    assert!(report.ok(), "clean run must have no error findings");
    assert_eq!(report.error_count(), 0);
}

#[test]
fn violating_query_yields_error_finding_with_detail() {
    // Anything that is a subclass of C → A (asserted) and B (asserted) and,
    // crucially, the DERIVED A ⊑ C is also present, proving the closure layer.
    let q = (
        "queries/verify/subclass-of-c.rq".to_owned(),
        format!("SELECT ?x WHERE {{ ?x <{SUBCLASS}> <{C}> }}"),
    );
    let dataset = store();
    let report = verify(
        dataset.as_ref(),
        std::slice::from_ref(&q),
        &selected_world(&dataset, W),
    )
    .expect("verify runs");
    assert!(!report.ok(), "a returned row must fail the report");
    assert_eq!(report.error_count(), 1);
    let finding = report
        .findings
        .iter()
        .find(|f| f.severity == Severity::Error)
        .expect("error finding present");
    assert_eq!(finding.code, "verify.subclass-of-c");
    let detail = finding.detail.as_deref().unwrap_or("");
    // B ⊑ C is asserted; A ⊑ C is the derived edge — both must be caught,
    // which proves the native closure was layered onto the asserted graph.
    assert!(detail.contains(A), "derived A ⊑ C must be caught: {detail}");
    assert!(
        detail.contains(B),
        "asserted B ⊑ C must be caught: {detail}"
    );
}

#[test]
fn selected_closure_materialization_keeps_native_inference_without_global_gates() {
    let dataset = store();
    let input = crate::reason::prepare_reasoning_input(&dataset).expect("reasoning input");
    let result = crate::reason::reason_all(input, &selected_world(&dataset, W))
        .expect("native closure completes");
    let ReasonedGraphOutcome::Ready(graph) =
        materialize_reasoned_closure(&dataset, &result).expect("checked selected closure")
    else {
        panic!("the subclass scene has no DL coverage gap");
    };
    assert!(graph.dataset.owned_quads().any(|quad| {
        quad.subject == RdfTerm::iri(A)
            && quad.predicate == SUBCLASS
            && quad.object == RdfTerm::iri(C)
    }));
    assert!(graph.derived_predicates.contains(SUBCLASS));
}

#[test]
fn ask_query_is_rejected() {
    let q = (
        "queries/verify/bad.rq".to_owned(),
        "ASK { ?s ?p ?o }".to_owned(),
    );
    let dataset = store();
    let err = verify(
        dataset.as_ref(),
        std::slice::from_ref(&q),
        &selected_world(&dataset, W),
    )
    .unwrap_err();
    assert!(
        err.message().contains("SELECT"),
        "ASK must be rejected: {err}"
    );
}

/// A DL coverage gap makes `verify` hard-fail with an `error` Finding.
///
/// Use an unparsable `owl:maxCardinality` literal — the same case the
/// `unparseable_cardinality_bound_stays_unsupported_so_the_gate_can_fire`
/// test in `dl.rs` validates against the DL verdict directly. Here we prove
/// the gap propagates all the way through `verify` to an `error` Finding,
/// and that the summary note says "aborted" rather than listing a query count
/// (since we short-circuit before running any queries).
#[test]
fn dl_coverage_gap_makes_verify_fail() {
    const W2: &str = "http://gmeow.example/w2";
    const ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
    const MAX_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
    const P: &str = "http://gmeow.example/p";
    const R: &str = "http://gmeow.example/R";
    const X: &str = "http://gmeow.example/x";
    const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

    // R = ≤? p (maxCardinality with an unparsable bound) — the native handler
    // cannot act on it, so maxCardinality stays unsupported → a DL gap.
    let mut builder = RdfDatasetBuilder::new();
    for (s, p, o) in [(R, ON_PROPERTY, P), (X, TYPE, R)] {
        builder.push_owned_quad(
            &RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W2)),
        );
    }
    builder.push_owned_quad(
        &RdfQuad::new(
            RdfTerm::iri(R),
            MAX_CARDINALITY,
            RdfTerm::Literal(RdfLiteral::typed(
                "not-a-number",
                "http://www.w3.org/2001/XMLSchema#string",
            )),
        )
        .in_graph(RdfTerm::iri(W2)),
    );
    let dataset = builder.freeze().expect("valid gap-trigger dataset");

    // Pass an empty query slice — we want to prove the gap fires BEFORE any
    // query is evaluated, i.e. the short-circuit path works.
    let report = verify(dataset.as_ref(), &[], &selected_world(&dataset, W2))
        .expect("verify itself must not Err on a gap");

    assert!(
        !report.ok(),
        "a DL coverage gap must make verify fail (report.ok() == false)"
    );
    assert!(
        report.error_count() >= 1,
        "there must be at least one error Finding for the gap: {:?}",
        report.findings
    );

    // The error finding code must reference the gap.
    let gap_finding = report
        .findings
        .iter()
        .find(|f| f.severity == Severity::Error && f.code.contains("dl-gap"))
        .expect("an error finding with 'dl-gap' in the code must be present");
    assert!(
        gap_finding.code.contains("maxCardinality")
            || gap_finding.message.contains("maxCardinality"),
        "the finding must name the undecided construct: {:?}",
        gap_finding
    );

    // The summary note must say "aborted".
    let summary = report
        .findings
        .iter()
        .find(|f| f.code == "verify.native.summary")
        .expect("a summary note must be present");
    assert!(
        summary.message.contains("aborted"),
        "summary must say 'aborted' when gaps prevent closure: {:?}",
        summary
    );
}

// ── Typed formalization governance: Arm B + reviewer gate ───────────────────

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

// The committed `.rq` files ARE the queries under test (include_str! keeps the
// tests in lockstep with what `make verify` runs).
const COUNTERPART_Q: &str =
    include_str!("../../../slices/grounding/logic/queries/verify/non-entailment-counterpart.rq");
const REVIEWER_Q: &str =
    include_str!("../../../slices/grounding/logic/queries/verify/reviewer-gate.rq");

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}
fn lg(local: &str) -> String {
    format!("{LOGIC}{local}")
}

fn dataset(triples: &[(&str, &str, &str)]) -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for (s, p, o) in triples {
        builder.push_owned_quad(&quad(s, p, o));
    }
    builder.freeze().expect("valid test dataset")
}

fn has_violation(report: &Report, code: &str) -> bool {
    report
        .findings
        .iter()
        .any(|f| f.code == code && f.severity == Severity::Error)
}

#[test]
fn counterpart_non_transitivity_green_then_red() {
    let (a, b, c) = ("http://ex/a", "http://ex/b", "http://ex/c");
    let cp = gm("counterpartOf");
    let q = (
        "queries/verify/non-entailment-counterpart.rq".to_owned(),
        COUNTERPART_Q.to_owned(),
    );
    // Green: a chain A→B→C with no transitive A→C.
    let green = dataset(&[(a, &cp, b), (b, &cp, c)]);
    let report = verify(
        green.as_ref(),
        std::slice::from_ref(&q),
        &selected_world(&green, W),
    )
    .expect("verify runs");
    assert!(
        !has_violation(&report, "verify.non-entailment-counterpart"),
        "no transitive edge → obligation discharged"
    );
    // Red: add the forbidden transitive A→C; the obligation must fire.
    let red = dataset(&[(a, &cp, b), (b, &cp, c), (a, &cp, c)]);
    let report = verify(
        red.as_ref(),
        std::slice::from_ref(&q),
        &selected_world(&red, W),
    )
    .expect("verify runs");
    assert!(
        has_violation(&report, "verify.non-entailment-counterpart"),
        "transitive A→C in the closure → obligation violated"
    );
}

#[test]
fn asserted_deceptive_intent_is_not_a_derived_violation() {
    // Regression: an ASSERTED gmeow:deceptiveIntentClaim (the real-bundle shape —
    // an attributed assessment) is EDB, never a DERIVED edge, so the finite-closure
    // arm of the deception obligation must NOT fire on it. We assert the standing
    // obligation alongside an asserted, attributed intent on a held≠projected gap
    // and confirm no violation surfaces. (The red path — an actually-derived intent
    // — is unit-tested in `obligations::tests::arm_b_finite_closure_green_then_red`.)
    let intent_pred = "https://blackcatinformatics.ca/gmeow/deceptiveIntentClaim";
    let mut builder = RdfDatasetBuilder::new();
    // The standing obligation, declaring finite-closure discharge.
    builder.push_owned_quad(&quad(
        "http://ex/obl",
        RDF_TYPE,
        &lg("NonEntailmentObligation"),
    ));
    builder.push_owned_quad(&quad(
        "http://ex/obl",
        &lg("obligationDischargeCondition"),
        &lg("DischargeFiniteClosure"),
    ));
    builder.push_owned_quad(
        &RdfQuad::new(
            RdfTerm::iri("http://ex/obl"),
            lg("obligationForbiddenPredicate"),
            RdfTerm::Literal(RdfLiteral::typed(
                intent_pred,
                "http://www.w3.org/2001/XMLSchema#anyURI",
            )),
        )
        .in_graph(RdfTerm::iri(W)),
    );
    // An ASSERTED, attributed deceptive-intent claim on a held≠projected gap.
    for (s, p, o) in [
        ("http://ex/ev", gm("heldStandpoint"), "http://ex/held"),
        ("http://ex/ev", gm("projectedStandpoint"), "http://ex/proj"),
        (
            "http://ex/ev",
            gm("deceptiveIntentClaim"),
            "http://ex/intent",
        ),
        ("http://ex/intent", gm("accordingTo"), "http://ex/assessor"),
    ] {
        builder.push_owned_quad(&quad(s, &p, o));
    }
    let dataset = builder.freeze().expect("valid test dataset");
    // A no-op query (the obligation checks run regardless of the query list).
    let q = (
        "queries/verify/non-entailment-counterpart.rq".to_owned(),
        COUNTERPART_Q.to_owned(),
    );
    let report = verify(
        dataset.as_ref(),
        std::slice::from_ref(&q),
        &selected_world(&dataset, W),
    )
    .expect("verify runs");
    assert!(
        !has_violation(&report, "verify.non-entailment.derived"),
        "an asserted (EDB) attributed intent claim must not be read as a derived entailment"
    );
    assert!(
        !has_violation(&report, "verify.non-entailment.violated"),
        "deceptiveIntentClaim is not a foundation rule head → Arm A discharged"
    );
}

#[test]
fn reviewer_gate_green_then_red() {
    let (cand, reviewer) = ("http://ex/cand", "http://ex/reviewer");
    let (candidate, lifecycle, accepted, reviewed_by, category, deriv) = (
        lg("FormalizationCandidate"),
        lg("candidateLifecycle"),
        lg("CandidateAccepted"),
        lg("reviewedBy"),
        lg("candidateCategory"),
        lg("CategoryDerivationRule"),
    );
    let q = (
        "queries/verify/reviewer-gate.rq".to_owned(),
        REVIEWER_Q.to_owned(),
    );
    // Green: an accepted candidate WITH a recorded reviewer decision (and a
    // category, so the coverage check is also clean).
    let green = dataset(&[
        (cand, RDF_TYPE, &candidate),
        (cand, &lifecycle, &accepted),
        (cand, &reviewed_by, reviewer),
        (cand, &category, &deriv),
    ]);
    let report = verify(
        green.as_ref(),
        std::slice::from_ref(&q),
        &selected_world(&green, W),
    )
    .expect("verify runs");
    assert!(
        !has_violation(&report, "verify.reviewer-gate"),
        "a reviewed accepted candidate is canonical-legitimate"
    );
    // Red: an accepted candidate with NO reviewer decision — an extraction
    // promoted straight to canonical. The gate must fire.
    let red = dataset(&[
        (cand, RDF_TYPE, &candidate),
        (cand, &lifecycle, &accepted),
        (cand, &category, &deriv),
    ]);
    let report = verify(
        red.as_ref(),
        std::slice::from_ref(&q),
        &selected_world(&red, W),
    )
    .expect("verify runs");
    assert!(
        has_violation(&report, "verify.reviewer-gate"),
        "an unreviewed accepted candidate → reviewer-gate violation"
    );
}

/// A `logic:reviewedBy` whose object is a plain literal is not an auditable
/// reviewer node — the tightened gate must still fire.
///
/// This proves the inner `FILTER(isIRI(?reviewer) || isBlank(?reviewer))`
/// clause is load-bearing: the old query (bare `FILTER NOT EXISTS { ?candidate
/// logic:reviewedBy ?reviewer }`) would have passed this case because the
/// triple EXISTS; the new query correctly rejects it because the object is a
/// literal rather than a node.
#[test]
fn reviewer_gate_literal_reviewer_is_a_violation() {
    let cand = "http://ex/cand-lit";
    let (candidate, lifecycle, accepted, reviewed_by, category, deriv) = (
        lg("FormalizationCandidate"),
        lg("candidateLifecycle"),
        lg("CandidateAccepted"),
        lg("reviewedBy"),
        lg("candidateCategory"),
        lg("CategoryDerivationRule"),
    );
    let q = (
        "queries/verify/reviewer-gate.rq".to_owned(),
        REVIEWER_Q.to_owned(),
    );
    // Build the dataset manually so we can assert a literal-object triple.
    let mut builder = RdfDatasetBuilder::new();
    for (s, p, o) in [
        (cand, RDF_TYPE, candidate.as_str()),
        (cand, lifecycle.as_str(), accepted.as_str()),
        (cand, category.as_str(), deriv.as_str()),
    ] {
        builder.push_owned_quad(&quad(s, p, o));
    }
    // The only reviewedBy value is a plain string literal — not a node.
    builder.push_owned_quad(
        &RdfQuad::new(
            RdfTerm::iri(cand),
            reviewed_by.clone(),
            RdfTerm::Literal(RdfLiteral::typed(
                "alice",
                "http://www.w3.org/2001/XMLSchema#string",
            )),
        )
        .in_graph(RdfTerm::iri(W)),
    );
    let dataset = builder.freeze().expect("valid literal-reviewer dataset");
    let report = verify(
        dataset.as_ref(),
        std::slice::from_ref(&q),
        &selected_world(&dataset, W),
    )
    .expect("verify runs");
    assert!(
        has_violation(&report, "verify.reviewer-gate"),
        "a literal-valued reviewedBy is not an auditable node → gate must fire"
    );
}

// ── Typed formalization governance: conditional-carrier verify queries ───────

const NON_ENT_CARRIER_Q: &str = include_str!(
    "../../../slices/grounding/logic/queries/verify/non-entailment-carrier-required.rq"
);
const PROMOTION_CASES_Q: &str =
    include_str!("../../../slices/grounding/logic/queries/verify/promotion-cases-required.rq");

#[test]
fn non_entailment_carrier_required_green_then_red() {
    // A FormalizationCandidate with CategoryNonEntailmentObligation that HAS
    // a candidateNonEntailment link → zero rows (obligation is wired).
    let cand = "http://ex/cand-ne";
    let obl = "http://ex/obl-ne";
    let (candidate, cat, ne_cat, candidate_ne) = (
        lg("FormalizationCandidate"),
        lg("candidateCategory"),
        lg("CategoryNonEntailmentObligation"),
        lg("candidateNonEntailment"),
    );
    let q = (
        "queries/verify/non-entailment-carrier-required.rq".to_owned(),
        NON_ENT_CARRIER_Q.to_owned(),
    );
    // Green: candidate with CategoryNonEntailmentObligation AND a candidateNonEntailment.
    let green = dataset(&[
        (cand, RDF_TYPE, &candidate),
        (cand, &cat, &ne_cat),
        (cand, &candidate_ne, obl),
    ]);
    let report = verify(
        green.as_ref(),
        std::slice::from_ref(&q),
        &selected_world(&green, W),
    )
    .expect("verify runs");
    assert!(
        !has_violation(&report, "verify.non-entailment-carrier-required"),
        "a CategoryNonEntailmentObligation candidate with candidateNonEntailment is coherent"
    );
    // Red: CategoryNonEntailmentObligation candidate MISSING candidateNonEntailment → violation.
    let red = dataset(&[(cand, RDF_TYPE, &candidate), (cand, &cat, &ne_cat)]);
    let report = verify(
        red.as_ref(),
        std::slice::from_ref(&q),
        &selected_world(&red, W),
    )
    .expect("verify runs");
    assert!(
        has_violation(&report, "verify.non-entailment-carrier-required"),
        "a CategoryNonEntailmentObligation candidate with no candidateNonEntailment → violation"
    );
}

#[test]
fn promotion_cases_required_green_then_red() {
    // An ACCEPTED candidate in an entailment-asserting category that HAS both
    // positive and negative cases → zero rows.
    let (cand, pos, neg, reviewer) = (
        "http://ex/cand-pc",
        "http://ex/pos-case",
        "http://ex/neg-case",
        "http://ex/reviewer",
    );
    let (candidate, lifecycle, accepted, reviewed_by, cat, int_cat, pos_case, neg_case) = (
        lg("FormalizationCandidate"),
        lg("candidateLifecycle"),
        lg("CandidateAccepted"),
        lg("reviewedBy"),
        lg("candidateCategory"),
        lg("CategoryIntegrityConstraint"),
        lg("candidatePositiveCase"),
        lg("candidateNegativeCase"),
    );
    let q = (
        "queries/verify/promotion-cases-required.rq".to_owned(),
        PROMOTION_CASES_Q.to_owned(),
    );
    // Green: accepted + entailment category + both cases present.
    let green = dataset(&[
        (cand, RDF_TYPE, &candidate),
        (cand, &lifecycle, &accepted),
        (cand, &reviewed_by, reviewer),
        (cand, &cat, &int_cat),
        (cand, &pos_case, pos),
        (cand, &neg_case, neg),
    ]);
    let report = verify(
        green.as_ref(),
        std::slice::from_ref(&q),
        &selected_world(&green, W),
    )
    .expect("verify runs");
    assert!(
        !has_violation(&report, "verify.promotion-cases-required"),
        "an accepted entailment-category candidate with both cases is promotion-ready"
    );
    // Red: accepted + entailment category MISSING the negative case → violation.
    let red = dataset(&[
        (cand, RDF_TYPE, &candidate),
        (cand, &lifecycle, &accepted),
        (cand, &reviewed_by, reviewer),
        (cand, &cat, &int_cat),
        (cand, &pos_case, pos),
        // neg_case intentionally absent
    ]);
    let report = verify(
        red.as_ref(),
        std::slice::from_ref(&q),
        &selected_world(&red, W),
    )
    .expect("verify runs");
    assert!(
        has_violation(&report, "verify.promotion-cases-required"),
        "an accepted entailment-category candidate missing the negative case → violation"
    );
}
