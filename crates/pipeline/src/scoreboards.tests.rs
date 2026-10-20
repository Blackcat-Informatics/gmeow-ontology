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

fn worked_claim_audit() -> &'static ClaimAuditReport {
    static REPORT: std::sync::OnceLock<ClaimAuditReport> = std::sync::OnceLock::new();
    REPORT.get_or_init(|| {
        let bytes = gmeow_bundle_import::load_authenticated_corpus_artifact(
            &root(),
            corpus_observation::ARTIFACT,
        )
        .expect("producer-selected claim audit is required; tests cannot rebuild it");
        corpus_observation::restore(&bytes).expect("restore authenticated native audit witnesses")
    })
}

#[test]
fn claim_audit_flags_the_worked_fixture_without_deleting_claims() {
    let report = worked_claim_audit();
    let ex = "https://blackcatinformatics.ca/gmeow/examples/hallucination-kg/";

    assert_eq!(
        report.query_rows["claims-without-evidence"][0]["claim"],
        Some(format!("{ex}claim-hallucinated"))
    );
    assert_eq!(
        report.query_rows["stale-source-claims"][0]["claim"],
        Some(format!("{ex}claim-stale"))
    );
    assert_eq!(report.shacl_count(Severity::Error), 0);
    assert!(report.shacl_count(Severity::Warning) > 0);
    assert!(
        report
            .claims
            .iter()
            .any(|claim| claim.claim == format!("{ex}claim-hallucinated"))
    );
}

#[test]
fn flat_claim_json_preserves_flags_evidence_and_contradictions() {
    let report = worked_claim_audit();
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

    let rendered = render_claim_audit_json(report).expect("json");
    assert!(rendered.contains("\"claims\""));
    assert!(render_claim_audit_text(report).contains("claims audited: 5"));
}

pub(super) fn synthetic_claim_audit(severity: &str) -> ClaimAuditReport {
    let store = parse_dataset(b"<urn:claim> a <urn:Claim> .", "text/turtle", None).unwrap();
    let text = format!(
        r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix gm: <https://blackcatinformatics.ca/gmeow/> .
gm:ClaimNeedsEvidenceConstraintProceduralConstraintShape a sh:NodeShape ;
    sh:targetNode <urn:claim> ; sh:property [
        sh:path <urn:evidence> ; sh:minCount 1 ; sh:severity sh:Warning ;
        gm:enforcesFailureClass <urn:MissingEvidence> ] .
gm:GroundingSpanShape a sh:NodeShape ;
    sh:targetNode <urn:claim> ; sh:property [
        sh:path <urn:span> ; sh:minCount 1 ; sh:severity {severity} ;
        gm:enforcesFailureClass <urn:MissingSpan> ] .
gm:StaleSourceConstraintProceduralConstraintShape a sh:NodeShape ;
    sh:targetNode <urn:claim> ; sh:property [
        sh:path <urn:source> ; sh:minCount 1 ; sh:severity sh:Info ;
        gm:enforcesFailureClass <urn:MissingSource> ] .
"#
    );
    let (dataset, shapes) = gmeow_validate::store::parse_validation_shapes(&text).unwrap();
    let classes = gmeow_validate::findings::FailureClassIndex::from_shapes_dataset(&dataset);
    let shapes = retain_claim_audit_shapes(shapes).unwrap();
    let mut report = ClaimAuditReport::default();
    report
        .ledger
        .union(&validate_claim_shapes(&store, &shapes, &classes).unwrap());
    for name in AUDIT_HEADLINE {
        report
            .record_query(
                &PathBuf::from("queries/audit").join(format!("{name}.rq")),
                (*name).to_owned(),
                native_query::Solutions {
                    variables: vec!["claim".to_owned(), "source".to_owned()],
                    rows: vec![vec![
                        Some(TermValue::iri("urn:claim")),
                        Some(TermValue::iri("urn:source")),
                    ]],
                },
            )
            .unwrap();
    }
    report
}

#[test]
fn claim_audit_diagnostics_maps_headlines_and_shacl() {
    let report = synthetic_claim_audit("sh:Violation");
    let diag = claim_audit_diagnostics(&report);
    assert!(!report.ok());
    assert_eq!(diag.error_count(), 1);
    assert_eq!(report.shacl_count(Severity::Error), 1);
    assert_eq!(report.shacl_count(Severity::Warning), 1);
    assert_eq!(report.shacl_count(Severity::Info), 1);
    assert_eq!(report.flagged(), 3);
    for code in [
        "audit.ungrounded-claim",
        "audit.contradicted-claim",
        "audit.stale-source",
    ] {
        assert!(
            diag.findings
                .iter()
                .any(|finding| finding.code == code && finding.severity == Severity::Warning)
        );
    }
    let shapes: Vec<_> = diag
        .findings
        .iter()
        .filter(|finding| finding.code == "shacl.MinCountConstraintComponent")
        .collect();
    assert_eq!(shapes.len(), 3);
    assert!(
        shapes
            .iter()
            .all(|finding| finding.failure_class.is_some() && finding.finding_iri.is_some())
    );
    assert!(
        shapes.iter().all(
            |finding| finding.related_locations.iter().any(|location| location
                .logical
                .as_deref()
                .is_some_and(|logical| logical.starts_with("path ")))
        )
    );
    assert!(render_claim_audit_text(&report).contains("SHACL information: 1"));
    let mut projection = claim_audit_diagnostics(&report);
    projection.findings.clear();
    assert!(
        !report.ok(),
        "clearing a projection must not erase a fatal witness"
    );
}

#[test]
fn claim_audit_keeps_nonfatal_native_levels_and_rejects_unknown_severity() {
    let mut warning_only = synthetic_claim_audit("sh:Warning");
    assert!(warning_only.ok());
    assert_eq!(warning_only.shacl_count(Severity::Warning), 2);
    assert_eq!(warning_only.shacl_count(Severity::Info), 1);
    warning_only
        .record_query(
            Path::new("queries/audit/evidence-coverage.rq"),
            "evidence-coverage".to_owned(),
            native_query::Solutions {
                variables: vec!["claim".to_owned()],
                rows: vec![vec![Some(TermValue::iri("urn:other"))]],
            },
        )
        .unwrap();
    assert_eq!(
        warning_only.flagged(),
        3,
        "coverage rows are not strict-mode claim flags"
    );
    let unknown = synthetic_claim_audit("<urn:custom-severity>");
    assert!(!unknown.ok(), "an unknown native severity must fail closed");
    assert_eq!(unknown.shacl_count(Severity::Error), 1);
}

#[test]
fn claim_audit_query_identity_retains_native_binding_distinctions() {
    let store = parse_dataset(br#"<urn:claim> <urn:value> <urn:item>, "urn:item", "", "text"@en, "text"@fr, "text"@en--ltr, "text"@en--rtl,
        <<( <urn:first> <urn:predicate> <urn:object> )>>, <<( <urn:second> <urn:predicate> <urn:object> )>> ."#, "text/turtle", None).unwrap();
    let mut solutions = native_query::select(
        &store,
        "SELECT ?claim ?value WHERE { ?claim <urn:value> ?value }",
    )
    .unwrap();
    assert_eq!(solutions.rows.len(), 9);
    solutions
        .rows
        .push(vec![Some(TermValue::iri("urn:claim")), None]);
    for scope in [1, 2] {
        solutions.rows.push(vec![
            Some(TermValue::iri("urn:claim")),
            Some(TermValue::Blank {
                label: "shared".to_owned(),
                scope: purrdf::BlankScope(scope),
            }),
        ]);
    }
    let mut report = ClaimAuditReport::default();
    report
        .record_query(
            Path::new("queries/audit/stale-source-claims.rq"),
            "stale-source-claims".to_owned(),
            solutions,
        )
        .unwrap();
    assert_eq!(
        report.ledger().len(),
        12,
        "display-equivalent RDF terms must retain distinct witnesses"
    );
    assert_eq!(report.query_rows["stale-source-claims"].len(), 12);
    assert!(
        report.query_rows["stale-source-claims"]
            .iter()
            .any(|row| row.get("value").is_some_and(Option::is_none))
    );
    assert!(report.query_rows["stale-source-claims"].iter().any(|row| {
        row.get("value")
            .is_some_and(|value| value.as_deref() == Some(""))
    }));
    assert!(report.ok(), "headline audit warnings remain non-gating");
    let findings = claim_audit_diagnostics(&report).findings;
    assert!(findings.iter().all(
        |finding| finding.primary_location().unwrap().path.as_deref()
            == Some("queries/audit/stale-source-claims.rq")
    ));
    assert!(findings.iter().any(|finding| {
        finding
            .related_labels
            .iter()
            .any(|label| label.message == "?value")
    }));
}

#[test]
fn claim_audit_binding_roles_survive_column_reordering_and_reject_missing_claims() {
    let store = parse_dataset(b"<urn:claim> <urn:value> \"value\" .", "text/turtle", None).unwrap();
    let path = Path::new("queries/audit/stale-source-claims.rq");
    let mut left = ClaimAuditReport::default();
    let mut right = ClaimAuditReport::default();
    for (report, columns) in [(&mut left, "?claim ?value"), (&mut right, "?value ?claim")] {
        let query = format!("SELECT {columns} WHERE {{ ?claim <urn:value> ?value }}");
        let solutions = native_query::select(&store, &query).unwrap();
        report
            .record_query(path, "stale-source-claims".to_owned(), solutions)
            .unwrap();
    }
    assert_eq!(left.query_rows, right.query_rows);
    assert_eq!(
        left.ledger().emit_sorted()[0].fingerprint,
        right.ledger().emit_sorted()[0].fingerprint
    );
    assert_eq!(
        right.ledger().emit_sorted()[0]
            .source_ctx
            .location
            .logical
            .as_deref(),
        Some("urn:claim")
    );
    let missing =
        native_query::select(&store, "SELECT ?value WHERE { ?claim <urn:value> ?value }").unwrap();
    let mut refused = ClaimAuditReport::default();
    let failure = refused
        .record_query(path, "stale-source-claims".to_owned(), missing)
        .unwrap_err();
    assert_eq!(
        failure.inner().source_ctx.location.path.as_deref(),
        path.to_str()
    );
    assert!(
        refused.ledger().is_empty(),
        "an invalid result must not leave partial diagnostics"
    );
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
fn aggregate_recall_gate_rejects_unmeasured_corpora_even_with_a_zero_floor() {
    // Exercise the real per-file round-trip gate: OWL linkage is deliberately
    // excluded from addressable recall, even when the output preserves it.
    let source = dataset_from_nt(
        "<urn:recall:class> <http://www.w3.org/2002/07/owl#equivalentClass> <urn:recall:peer> .\n",
    )
    .unwrap();
    // Use the same language-map loader as the real acceptance context rather
    // than introducing a second table constructor for this regression.
    let tag_map = gmeow_validate::language_tags::load_tag_map(b"", "nt").unwrap();
    let round_trip = gate_round_trip(&source, &source, &tag_map).unwrap();
    assert_eq!(round_trip.metrics["addressable"], 0.0);
    let linkage_only = vec![FileAcceptance {
        source: "linkage-only.ttl".to_owned(),
        source_triples: 1,
        output_triples: 1,
        gates: vec![round_trip],
    }];
    for results in [Vec::new(), linkage_only] {
        assert_eq!(corpus_recall_pct(&results), 0.0);
        for floor in [0.0, ACCEPTANCE_MIN_RECALL_PCT] {
            let gate = aggregate_recall_gate(&results, floor);
            assert!(gate.hard);
            assert!(!gate.passed);
            assert!(gate.summary.contains("no addressable source triples"));
            assert!(!corpus_passed(&results, floor));
        }
    }
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
