// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn claim_audit_flags_the_worked_fixture_without_deleting_claims() {
    let root = root();
    let report = claim_audit(
        &root,
        &[root.join("tests/fixtures/coverage/hallucination-kg.ttl")],
    )
    .expect("audit report");
    let ex = "https://blackcatinformatics.ca/gmeow/examples/hallucination-kg/";

    assert_eq!(
        report.findings["claims-without-evidence"][0][0],
        format!("{ex}claim-hallucinated")
    );
    assert_eq!(
        report.findings["stale-source-claims"][0][0],
        format!("{ex}claim-stale")
    );
    assert!(report.shacl_errors.is_empty());
    assert!(!report.shacl_warnings.is_empty());
    assert!(
        report
            .claims
            .iter()
            .any(|claim| claim.claim == format!("{ex}claim-hallucinated"))
    );
}

#[test]
fn flat_claim_json_preserves_flags_evidence_and_contradictions() {
    let root = root();
    let report = claim_audit(
        &root,
        &[root.join("tests/fixtures/coverage/hallucination-kg.ttl")],
    )
    .expect("audit report");
    let ex = "https://blackcatinformatics.ca/gmeow/examples/hallucination-kg/";
    let by_iri: BTreeMap<_, _> = report
        .claims
        .iter()
        .map(|claim| (claim.claim.as_str(), claim))
        .collect();

    assert_eq!(by_iri.len(), 5);
    let grounded = by_iri[format!("{ex}claim-grounded").as_str()];
    assert_eq!(grounded.confidence.as_deref(), Some("0.95"));
    assert_eq!(grounded.evidence[0].start, Some(60));
    assert_eq!(grounded.evidence[0].end, Some(141));
    assert_eq!(
        grounded.evidence[0].polarity.as_deref(),
        Some("polaritySupports")
    );

    assert!(
        by_iri[format!("{ex}claim-hallucinated").as_str()]
            .flags
            .ungrounded
    );
    assert!(
        by_iri[format!("{ex}claim-hallucinated").as_str()]
            .evidence
            .is_empty()
    );
    assert!(by_iri[format!("{ex}claim-low").as_str()].flags.contradicted);
    assert_eq!(
        by_iri[format!("{ex}claim-low").as_str()].contradicts,
        vec![format!("{ex}claim-high")]
    );
    assert!(by_iri[format!("{ex}claim-stale").as_str()].flags.stale);

    let rendered = render_claim_audit_json(&report).expect("json");
    assert!(rendered.contains("\"claims\""));
    assert!(render_claim_audit_text(&report).contains("claims audited: 5"));
}

#[test]
fn claim_audit_diagnostics_maps_headlines_and_shacl() {
    let report = ClaimAuditReport {
        findings: BTreeMap::from([
            (
                "claims-without-evidence".to_owned(),
                vec![vec!["ex:claim-a".to_owned(), "evidence".to_owned()]],
            ),
            (
                "claims-contradicted-by-higher-confidence".to_owned(),
                vec![vec!["ex:claim-b".to_owned(), "x".to_owned()]],
            ),
            (
                "stale-source-claims".to_owned(),
                vec![vec!["ex:claim-c".to_owned(), "src".to_owned()]],
            ),
        ]),
        shacl_errors: vec!["focus ex:x violates sh:minCount".to_owned()],
        shacl_warnings: vec!["focus ex:y soft warning".to_owned()],
        claims: Vec::new(),
    };

    let diag = claim_audit_diagnostics(&report);
    let by_code: BTreeMap<_, _> = diag
        .findings
        .iter()
        .map(|item| (item.code.as_str(), item.severity))
        .collect();
    assert_eq!(by_code["audit.ungrounded-claim"], Severity::Warning);
    assert_eq!(by_code["audit.contradicted-claim"], Severity::Warning);
    assert_eq!(by_code["audit.stale-source"], Severity::Warning);
    assert_eq!(by_code["audit.shacl-error"], Severity::Error);
    assert_eq!(by_code["audit.shacl-warning"], Severity::Warning);
    assert_eq!(diag.error_count(), 1);
}

#[test]
fn aggregate_recall_gate_is_hard_and_bites_below_the_floor() {
    let results = synthetic_recall_results(65, 100);
    let aggregate = corpus_recall_pct(&results);
    // The production acceptance command owns the real external-corpus measurement.
    // This unit test pins only the pure aggregate-gate arithmetic.
    assert!(
        aggregate >= ACCEPTANCE_MIN_RECALL_PCT,
        "synthetic aggregate {aggregate:.2}% must clear the pinned floor"
    );

    // The gate is HARD, and at the native floor it passes.
    let pass = aggregate_recall_gate(&results, ACCEPTANCE_MIN_RECALL_PCT);
    assert!(pass.hard, "aggregate-recall gate must be a hard gate");
    assert!(pass.passed, "aggregate gate must pass at the pinned floor");
    assert_eq!(pass.name, "aggregate-recall-floor");

    // Forcing the floor above the measured recall makes the hard gate FAIL, and
    // the diagnostics fold (consumed by `make check`) surfaces it as an Error.
    let fail = aggregate_recall_gate(&results, aggregate + 1.0);
    assert!(!fail.passed, "gate must fail when the floor exceeds recall");
    assert!(fail.hard);
}

#[test]
fn honest_coverage_reports_total_gap_occurrences_not_distinct_terms() {
    // One uncovered term occurring 20 times, plus a second occurring once: the gate
    // must report the TRUE occurrence volume (21) as `gap_occurrences`, keep the
    // distinct-term count (2) as `gap_terms`, and NEVER collapse 21 down to 2.
    let empty = dataset_from_nt("").expect("empty dataset");
    let gap_terms = BTreeMap::from([
        ("foaf:knows".to_owned(), 20usize),
        ("foaf:homepage".to_owned(), 1usize),
    ]);
    let gate = gate_coverage(&empty, &empty, 0, &gap_terms).expect("coverage gate");

    assert_eq!(
        gate.metrics.get("gap_terms").copied(),
        Some(2.0),
        "distinct-term count must stay 2: {gate:#?}"
    );
    assert_eq!(
        gate.metrics.get("gap_occurrences").copied(),
        Some(21.0),
        "total occurrence volume must be 20 + 1 = 21, not the distinct-term count: {gate:#?}"
    );
    assert!(
        gate.summary.contains("2 distinct gap term(s)")
            && gate.summary.contains("21 gap occurrence(s)"),
        "summary must disclose BOTH distinct terms and total occurrences: {}",
        gate.summary
    );
}

#[test]
fn corpus_passed_is_false_when_the_aggregate_gate_fails() {
    let results = synthetic_recall_results(65, 100);
    let aggregate = corpus_recall_pct(&results);

    // At the pinned floor the corpus passes: every per-file hard gate passes AND the
    // aggregate floor clears.
    assert!(
        corpus_passed(&results, ACCEPTANCE_MIN_RECALL_PCT),
        "corpus must pass at the pinned floor (aggregate {aggregate:.2}%)"
    );

    // Forcing the floor above the measured recall makes the HARD aggregate gate fail;
    // the structured corpus verdict MUST turn false even though every per-file hard
    // gate still passes (the integrity bug: `all(per-file)` alone reported true).
    assert!(
        results.iter().all(FileAcceptance::passed),
        "per-file hard gates all pass, so only the aggregate gate can flip the verdict"
    );
    assert!(
        !corpus_passed(&results, aggregate + 1.0),
        "corpus verdict must be false when the aggregate floor exceeds measured recall"
    );
}

fn synthetic_recall_results(recovered: usize, addressable: usize) -> Vec<FileAcceptance> {
    let mut gate = GateResult::new(
        "round-trip-superset",
        true,
        true,
        "synthetic arithmetic fixture".to_owned(),
    );
    gate.metrics
        .insert("recovered".to_owned(), recovered as f64);
    gate.metrics
        .insert("addressable".to_owned(), addressable as f64);
    vec![FileAcceptance {
        source: "synthetic.ttl".to_owned(),
        source_triples: addressable,
        output_triples: recovered,
        gates: vec![gate],
    }]
}
