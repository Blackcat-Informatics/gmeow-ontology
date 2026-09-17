// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use crate::stages::source_load::BASE_GRAPH_PATH;

const SYNTHETIC_SOURCE: &str = "<https://example.org/fixture> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/Fixture> .";

fn add_catalog(upstream: &mut BTreeMap<String, StageProduct>, source: &str) {
    upstream.insert(
        crate::stages::parse_sources::STAGE_ID.into(),
        crate::stages::parse_sources::synthetic_product(source),
    );
}

use super::*;

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).unwrap();
    std::fs::write(path, content).unwrap();
}

fn mock_repo(shapes: &str) -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    write(&repo.path().join("shapes/gmeow-shapes.ttl"), shapes);
    write(
        &repo.path().join("generated/shapes/frame-shapes.ttl"),
        "# generated\n",
    );
    std::fs::create_dir_all(repo.path().join("slices")).unwrap();
    repo
}

/// The fresh product-byte map covering the mock repo's one generated union
/// member — `validate_source_graph` fails closed on any on-disk generated
/// member without a fresh entry (the stale-disk-fold class).
fn mock_fresh() -> std::collections::BTreeMap<String, Vec<u8>> {
    std::collections::BTreeMap::from([(
        "generated/shapes/frame-shapes.ttl".to_string(),
        b"# generated\n".to_vec(),
    )])
}

use crate::stages::diag_render::{record_digest, verify_record_digest};

/// A recorded verdict with a violation DELETED by hand is refused, even though every
/// validated input is byte-identical and the input digest therefore still matches.
///
/// This is the exact hand-edit the input-only guard could not see: `shacl.json` is a
/// product, nothing else on disk moves when it is edited, and the gate reads its
/// findings. The record's own content digest is what makes the edit visible.
#[test]
fn a_verdict_with_a_deleted_violation_is_refused() {
    let mut report = Report::new("shacl");
    report.add_finding(Finding::new(
        Severity::Error,
        "shacl.violation",
        "ex:Thing violates ex:Shape",
    ));
    report.add_finding(Finding::new(
        Severity::Error,
        "shacl.violation",
        "ex:Other violates ex:Shape",
    ));
    report
        .metadata
        .insert(SHACL_INPUT_DIGEST_KEY.to_owned(), json!("blake3:inputs"));
    let digest = record_digest(&report, SHACL_RECORD_DIGEST_KEY).expect("digest");
    report
        .metadata
        .insert(SHACL_RECORD_DIGEST_KEY.to_owned(), json!(digest));

    // Control: the sealed record is admitted.
    verify_record_digest(&report, SHACL_RECORD_DIGEST_KEY, "<in-memory>")
        .expect("a sealed verdict is admitted");

    // The hand-edit: drop one violation, leave the declared digest (and every input,
    // hence the input digest) untouched — the whole point is that nothing else moves.
    let mut tampered = report.clone();
    tampered.findings.remove(0);
    assert_eq!(
        tampered.metadata.get(SHACL_INPUT_DIGEST_KEY),
        report.metadata.get(SHACL_INPUT_DIGEST_KEY),
        "the input digest is untouched by the edit — that is why it cannot catch it"
    );
    let err = verify_record_digest(&tampered, SHACL_RECORD_DIGEST_KEY, "<in-memory>")
        .expect_err("a verdict with a violation deleted must be REFUSED");
    assert!(
        err.to_string().contains(SHACL_RECORD_DIGEST_KEY),
        "the refusal names the violated witness: {err}"
    );

    // Rewriting a finding's MESSAGE (same count) is caught too — the fold is over
    // content, not cardinality.
    let mut reworded = report.clone();
    reworded.findings[0].message = "ex:Thing is fine, actually".to_owned();
    verify_record_digest(&reworded, SHACL_RECORD_DIGEST_KEY, "<in-memory>")
        .expect_err("a reworded finding must be REFUSED");

    // An absent witness is unattestable content, not a pass.
    let mut unwitnessed = report.clone();
    unwitnessed.metadata.remove(SHACL_RECORD_DIGEST_KEY);
    let err = verify_record_digest(&unwitnessed, SHACL_RECORD_DIGEST_KEY, "<in-memory>")
        .expect_err("a verdict carrying no record digest must be REFUSED");
    assert!(
        err.to_string().contains(SHACL_RECORD_DIGEST_KEY),
        "the refusal names the missing witness: {err}"
    );
}

/// PRODUCER → CONSUMER, in one process, through the REAL renderer: the bytes
/// [`render_artifacts`] writes to `shacl.json` are parsed back exactly as
/// `gmeow-dev validate` parses the committed file, and must verify.
///
/// This is the round trip the class of bug lives in, and it is deliberately driven
/// with a report the producer would build but the renderer would NOT write verbatim:
/// the findings are appended out of `sort_key` order and one rule is pushed twice.
/// `to_json` writes `Report::normalized()`, so the written record has its findings
/// REORDERED and its duplicate rule DROPPED. A digest folded over the pre-render
/// report therefore attests a value nobody can recompute — the exact disagreement
/// this test would have caught, and now cannot recur.
#[test]
fn the_rendered_record_verifies_against_the_digest_the_renderer_stamped() {
    let mut report = Report::new("shacl");
    // Out of sort_key order (Error sorts before Warning; within a severity, by code).
    report.add_finding(Finding::new(Severity::Warning, "shacl.zzz", "a warning"));
    report.add_finding(Finding::new(
        Severity::Error,
        "shacl.aaa",
        "ex:Thing violates ex:Shape",
    ));
    report.add_finding(Finding::new(Severity::Warning, "shacl.aaa", "another"));
    // The advisory wing pushes one rule PER FIRING, so a rule genuinely repeats;
    // `normalize` deduplicates by id, shortening the rendered rule list.
    report.add_rule(gmeow_errors::Rule::new("shacl.aaa", Severity::Error));
    report.add_rule(gmeow_errors::Rule::new("shacl.aaa", Severity::Error));
    report.add_rule(gmeow_errors::Rule::new("shacl.zzz", Severity::Warning));
    report
        .metadata
        .insert(SHACL_INPUT_DIGEST_KEY.to_owned(), json!("blake3:inputs"));
    report
        .metadata
        .insert("shaclResultCount".to_owned(), json!(3));

    let artifacts = render_artifacts(report.clone(), None, None)
        .expect("render")
        .artifacts;
    let json = artifacts.get(SHACL_JSON_PATH).expect("rendered shacl.json");
    let recorded: Report = serde_json::from_slice(json).expect("parse the committed record");

    // The renderer really did rewrite the content the naive fold would have digested.
    assert_ne!(
        recorded.findings.len(),
        0,
        "the rendered record carries the findings"
    );
    assert_eq!(
        recorded.rules.len(),
        2,
        "the renderer deduplicated the repeated rule — a pre-render fold would have \
             digested three"
    );
    assert_eq!(
        recorded.findings[0].code, "shacl.aaa",
        "the renderer reordered the findings — a pre-render fold would have digested \
             the append order"
    );

    // The consumer's check: the SAME call `gmeow-dev validate` makes over the parsed
    // committed record.
    verify_record_digest(&recorded, SHACL_RECORD_DIGEST_KEY, "<round-trip>").expect(
        "the record the renderer WROTE must verify against the digest the renderer \
             STAMPED — writer and reader are one fold over the rendered form",
    );

    // And the seal is over the rendered content, so an edit to it is still refused.
    let mut tampered = recorded.clone();
    tampered.findings.remove(0);
    verify_record_digest(&tampered, SHACL_RECORD_DIGEST_KEY, "<round-trip>")
        .expect_err("an edit to the rendered record is still refused");
}

/// A producer that passes no `seal` writes NO record digest — the seal is opt-in per
/// producer, and `stage-compile-logic`'s record (which nobody reads back) stays
/// byte-unchanged.
#[test]
fn an_unsealed_render_carries_no_record_digest() {
    let mut report = Report::new("logic-compile");
    report.add_finding(Finding::new(Severity::Note, "logic.loss", "a lossy drop"));
    let artifacts = crate::stages::diag_render::render_diagnostics_artifacts(
        "stage-compile-logic",
        report.clone(),
        &crate::stages::diag_render::DiagnosticsPaths {
            json: SHACL_JSON_PATH,
            sarif: SHACL_SARIF_PATH,
            html: SHACL_HTML_PATH,
            rdf: SHACL_RDF_PATH,
        },
        None,
        None,
        None,
    )
    .expect("render")
    .artifacts;
    let recorded: Report =
        serde_json::from_slice(artifacts.get(SHACL_JSON_PATH).expect("json")).expect("parse");
    assert!(
        !recorded.metadata.contains_key(SHACL_RECORD_DIGEST_KEY),
        "an unsealed render stamps nothing"
    );
}

#[test]
fn validate_stage_emits_sarif_for_shacl_violation() {
    let repo = mock_repo(
        r#"
@prefix ex: <https://example.test/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .

ex:RequiredShape a sh:NodeShape ;
    sh:targetNode ex:thing ;
    sh:property [
        sh:path ex:required ;
        sh:minCount 1 ;
        sh:message "required value is missing" ;
    ] .
"#,
    );
    let (report, _adv) = validate_source_graph(repo.path(), b"", &mock_fresh()).expect("validate");
    assert_eq!(report.error_count(), 1);
    assert_eq!(
        report.metadata["shaclGatePassed"],
        serde_json::Value::Bool(false)
    );

    let artifacts = render_artifacts(report.clone(), None, None)
        .expect("render")
        .artifacts;
    let sarif: serde_json::Value =
        serde_json::from_slice(&artifacts[SHACL_SARIF_PATH]).expect("SARIF artifact is JSON");
    assert_eq!(sarif["version"], "2.1.0");
    assert_eq!(
        sarif["runs"][0]["automationDetails"]["id"],
        serde_json::Value::String("shacl".to_string())
    );
    assert_eq!(
        sarif["runs"][0]["results"][0]["ruleId"],
        serde_json::Value::String("shacl.MinCountConstraintComponent".to_string())
    );
}

#[test]
fn diagnostics_report_finding_carries_ledger_identity_and_nontrivial_anchor() {
    // The G1c production path: `validate_source_graph` → `diagnostics_report` routes
    // the SHACL result through a `DiagLedger`, so the projected finding carries the
    // blake3 `finding_iri` + code-blind `anchor_iri` (with `anchor_non_trivial`) the
    // cross-node-glut meta-rule joins on — NOT the identity-less hand-built finding.
    let repo = mock_repo(
        r#"
@prefix ex: <https://example.test/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .

ex:RequiredShape a sh:NodeShape ;
    sh:targetNode ex:thing ;
    sh:property [
        sh:path ex:required ;
        sh:minCount 1 ;
        sh:message "required value is missing" ;
    ] .
"#,
    );
    let (report, _adv) = validate_source_graph(repo.path(), b"", &mock_fresh()).expect("validate");
    assert_eq!(report.findings.len(), 1);
    let finding = &report.findings[0];
    assert!(
        finding.finding_iri.as_deref().is_some_and(
            |iri| iri.starts_with("https://blackcatinformatics.ca/gmeow/diagnostics/finding/")
        ),
        "a routed SHACL finding must carry a blake3 finding IRI, not the FNV fallback"
    );
    assert!(
        finding.anchor_iri.as_deref().is_some_and(
            |iri| iri.starts_with("https://blackcatinformatics.ca/gmeow/diagnostics/anchor/")
        ),
        "a routed SHACL finding must carry a code-blind anchor IRI"
    );
    assert!(
        finding.anchor_non_trivial,
        "the focus node is a NonTrivial anchor the glut join can fire on"
    );
}

/// The FULL cross-surface parity and drift guard
/// `ValidateStage::run` (not just `validate_source_graph`, which returns
/// BEFORE the enrichment call) routes its report through the SAME
/// `gmeow_validate::enrich::enrich_findings` the CLI/consumer
/// `data_validate::run` path calls
/// (`crates/validate/tests/proof_carrying_findings.rs`'s
/// `cross_surface_parity_cli_path_is_enriched`), so the two consumer surfaces
/// cannot silently drift apart — the original bug this whole feature fixes.
/// Falsifiable: removing the `enrich_findings` call at the bottom of
/// `ValidateStage::run` (this file) makes both assertions below fail.
#[test]
fn stage_validate_run_is_enriched_matching_the_cli_path() {
    use purrdf::RdfDatasetBuilder;

    let repo = mock_repo(
        r#"
@prefix ex: <https://example.test/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .

ex:RequiredShape a sh:NodeShape ;
    sh:targetNode ex:thing ;
    sh:property [
        sh:path ex:required ;
        sh:minCount 1 ;
        sh:message "required value is missing" ;
    ] .
"#,
    );

    // A minimal `stage-source-load` product: an empty base graph (mirrors the
    // existing `validate_source_graph(repo.path(), b"")` fixtures) plus the
    // digest-pinned `REP_SPAN_TABLE` blob every downstream consumer of the
    // span table requires present (`StageProduct::span_index`).
    let dataset = RdfDatasetBuilder::new().freeze().expect("empty dataset");
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    artifacts.insert(BASE_GRAPH_PATH.to_string(), Vec::new());
    let span_index = crate::ingest::SpanIndex::new();
    let span_blob = serde_json::to_vec(&span_index).expect("encode span index");
    let bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
        dataset,
        artifacts,
        DatasetProvenance::new(),
        crate::stages::carrier::REP_SPAN_TABLE,
        "application/json",
        span_blob,
    );
    let product = StageProduct::from_bundle("stage-source-load", Arc::new(bundle));
    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    upstream.insert("stage-source-load".to_string(), product);
    add_catalog(&mut upstream, SYNTHETIC_SOURCE);
    // The stage consumes the four shape producers fail-closed (the
    // stale-disk-fold class): every generated union member must arrive as a
    // fresh product byte, so the fixture supplies header-only members.
    for (producer, rels) in [
        (
            "stage-compile-logic",
            &[
                crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH,
                crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH,
            ][..],
        ),
        (
            "stage-export-constraint-shapes",
            &[crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH][..],
        ),
        (
            "stage-export-frame-shapes",
            &[crate::stages::frame_shapes::FRAME_SHAPES_PATH][..],
        ),
        (
            "stage-export-result-shapes",
            &[crate::stages::result_shapes::RESULT_SHAPES_PATH][..],
        ),
    ] {
        let artifacts: BTreeMap<String, Vec<u8>> = rels
            .iter()
            .map(|rel| ((*rel).to_string(), b"# generated\n".to_vec()))
            .collect();
        upstream.insert(
            producer.to_string(),
            StageProduct::from_artifacts(producer, artifacts),
        );
    }
    // The D5 abductive tier consumes stage-reason's reasoned closure; an empty-EDB
    // fixture yields an empty closure (the reasoned union is the authored graph alone).
    upstream.insert(
        "stage-reason".to_string(),
        crate::stages::reason::reason_product(b"").expect("stage-reason fixture product"),
    );
    let input = StageInput {
        root: repo.path(),
        upstream: &upstream,
    };

    let output = ValidateStage::new()
        .validate_input(input, BTreeMap::new())
        .expect("validate stage run");
    let json_bytes = output
        .product
        .artifact(SHACL_JSON_PATH)
        .expect("shacl.json artifact on the stage product");
    let report: Report = serde_json::from_slice(json_bytes).expect("shacl.json parses as a Report");

    assert!(
        !report.rules.is_empty(),
        "ValidateStage::run must populate report.rules (rule_catalog::populate_rules), \
             matching the CLI data_validate::run path"
    );
    let finding = report
        .findings
        .iter()
        .find(|f| f.code == "shacl.MinCountConstraintComponent")
        .expect("the SHACL minCount finding");
    assert!(
        !finding.remediation.is_empty(),
        "the pipeline validate-stage report must carry a remediation, matching the CLI \
             path: {finding:?}"
    );
}

/// Build the full `ValidateStage::run` harness — a `stage-source-load` product
/// with an empty base graph + `REP_SPAN_TABLE` blob, plus header-only members for
/// the four shape producers — parameterized on the authored `shapes/gmeow-shapes.ttl`
/// body, and run the stage. All enrichment controls reuse this exact harness
/// shape rather than constructing a divergent twin.
/// The base-graph fixture (N-Quads, default graph): an individual whose data MATCHES
/// the advisory constraint in `ADVICE_SHAPE` (`ex:badThing a gmeow:Foo`). The
/// data-matching guard fires exactly one Info result, which the bridge lifts into a
/// Note advisory + one ComplianceAssessment through the full stage.
const ADVICE_BASE_NQ: &str = "<https://ex.test/badThing> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/gmeow/Foo> .\n";

/// An advisory `logic:Constraint` in its projected SHACL form: a `sh:SPARQLConstraint`
/// at `sh:severity sh:Info` (the advisory tier) carrying `logic:formalizes` (its
/// provenance), whose guard returns every `gmeow:Foo` instance. It fires against
/// `ADVICE_BASE_NQ`'s individual, and the bridge re-projects that Info match as a
/// Note + deonticRecommendation advisory.
const ADVICE_SHAPE: &str = r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
<https://ex.test/FooAdviceShape> a sh:NodeShape ;
    logic:formalizes gmeow:Foo ;
    sh:targetClass gmeow:Foo ;
    sh:sparql [
        a sh:SPARQLConstraint ;
        sh:severity sh:Info ;
        sh:message "prefer a more specific sortal than bare gmeow:Foo" ;
        sh:select "SELECT $this WHERE { $this a <https://blackcatinformatics.ca/gmeow/Foo> }" ;
    ] .
"#;

fn run_full_stage(base_nq: &str, shapes: &str) -> StageOutput {
    run_full_stage_with_reason(base_nq, shapes, b"")
}

fn run_full_stage_with_reason(base_nq: &str, shapes: &str, reason_nq: &[u8]) -> StageOutput {
    use purrdf::RdfDatasetBuilder;

    let repo = mock_repo(shapes);

    let dataset = RdfDatasetBuilder::new().freeze().expect("empty dataset");
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    artifacts.insert(BASE_GRAPH_PATH.to_string(), base_nq.as_bytes().to_vec());
    let span_index = crate::ingest::SpanIndex::new();
    let span_blob = serde_json::to_vec(&span_index).expect("encode span index");
    let bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
        dataset,
        artifacts,
        DatasetProvenance::new(),
        crate::stages::carrier::REP_SPAN_TABLE,
        "application/json",
        span_blob,
    );
    let product = StageProduct::from_bundle("stage-source-load", Arc::new(bundle));
    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    upstream.insert("stage-source-load".to_string(), product);
    add_catalog(&mut upstream, base_nq);
    for (producer, rels) in [
        (
            "stage-compile-logic",
            &[
                crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH,
                crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH,
            ][..],
        ),
        (
            "stage-export-constraint-shapes",
            &[crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH][..],
        ),
        (
            "stage-export-frame-shapes",
            &[crate::stages::frame_shapes::FRAME_SHAPES_PATH][..],
        ),
        (
            "stage-export-result-shapes",
            &[crate::stages::result_shapes::RESULT_SHAPES_PATH][..],
        ),
    ] {
        let artifacts: BTreeMap<String, Vec<u8>> = rels
            .iter()
            .map(|rel| ((*rel).to_string(), b"# generated\n".to_vec()))
            .collect();
        upstream.insert(
            producer.to_string(),
            StageProduct::from_artifacts(producer, artifacts),
        );
    }
    // Tiny explicit synthetic inputs exercise the consumer without a corpus build.
    upstream.insert(
        "stage-reason".to_string(),
        crate::stages::reason::reason_product(reason_nq).expect("stage-reason fixture product"),
    );
    let input = StageInput {
        root: repo.path(),
        upstream: &upstream,
    };
    ValidateStage::new()
        .validate_input(input, BTreeMap::new())
        .expect("validate stage run")
}

#[test]
fn validation_consumes_contextual_result_literals_and_quoted_evidence() {
    let source = purrdf::parse_dataset(
        br#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex: <urn:validation-context:> .
ex:context a logic:AttributedContext ; logic:contextWorld ex:world ;
  logic:contextStandpoint ex:observer ; logic:evidenceClosure logic:ClosedWorldClosure .
ex:request a logic:ContextualEvaluationRequest ;
  logic:queryFormula ex:formula ; logic:queryContext ex:context .
ex:formula a logic:Formula ; logic:relation ex:predicate ;
  logic:argument [ logic:termIndex 0 ; logic:termIri ex:item ],
                 [ logic:termIndex 1 ; logic:termIri ex:value ] .
ex:world {
  ex:claim rdf:reifies <<( ex:item ex:predicate ex:value )>> ;
    gmeow:accordingTo ex:observer ;
    gmeow:standpointSupportStatus gmeow:supportSupported .
}
"#,
        "application/trig",
        None,
    )
    .unwrap();
    let nq = purrdf::canonical_flat_nquads(&source).unwrap();
    let output = run_full_stage_with_reason("", "", nq.as_bytes());
    let report: Report = serde_json::from_slice(
        output
            .product
            .artifact(SHACL_JSON_PATH)
            .expect("validation report"),
    )
    .unwrap();
    assert!(report.ok(), "{:?}", report.findings);
}

/// The GMEOW namespace prefix. `crates/validate/src/advisory.rs`'s `GMEOW`
/// constant is crate-private, so this trivial namespace string is redeclared
/// here — the same per-module local-const idiom used across the workspace
/// (`crates/docs`, `crates/conformance`, …) rather than a shared export.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Assert the `graph/norm-claims` graph carries the D4 `gmeow:ComplianceAssessment`
/// claim shape: exactly one subject typed `gmeow:ComplianceAssessment`, with exactly
/// one `gmeow:complianceVerdict` and a `gmeow:vantage` of `gmeowBestPractice`, whose
/// `gmeow:assessedNorm` object carries `gmeow:deonticModality` = `deonticRecommendation`
/// AND a `gmeow:normIssuer`. Falsifiable: an empty or malformed norm-claims graph
/// fails every assertion below (not a vacuous existence check).
fn assert_compliance_assessment_present(ds: &purrdf::RdfDataset) {
    use purrdf::RdfTerm;

    let quads: Vec<_> = ds.owned_quads().collect();

    let assessment_class = format!("{GMEOW}ComplianceAssessment");
    let assessment_subjects: Vec<RdfTerm> = quads
        .iter()
        .filter(|q| {
            q.predicate.as_str() == RDF_TYPE_IRI
                && matches!(&q.object, RdfTerm::Iri(o) if o == &assessment_class)
        })
        .map(|q| q.subject.clone())
        .collect();
    assert_eq!(
        assessment_subjects.len(),
        1,
        "expected exactly one gmeow:ComplianceAssessment subject in graph/norm-claims, \
             got {assessment_subjects:?}"
    );
    let assessment = &assessment_subjects[0];

    let verdict_pred = format!("{GMEOW}complianceVerdict");
    let verdicts: Vec<_> = quads
        .iter()
        .filter(|q| &q.subject == assessment && q.predicate.as_str() == verdict_pred)
        .collect();
    assert_eq!(
        verdicts.len(),
        1,
        "expected exactly one gmeow:complianceVerdict on the assessment, got {verdicts:?}"
    );

    let vantage_pred = format!("{GMEOW}vantage");
    let best_practice_standpoint =
        RdfTerm::Iri(gmeow_validate::advisory::BEST_PRACTICE_STANDPOINT_IRI.to_owned());
    let vantages: Vec<_> = quads
        .iter()
        .filter(|q| {
            &q.subject == assessment
                && q.predicate.as_str() == vantage_pred
                && q.object == best_practice_standpoint
        })
        .collect();
    assert_eq!(
        vantages.len(),
        1,
        "expected exactly one gmeow:vantage = gmeowBestPractice on the assessment, \
             got {vantages:?}"
    );

    let assessed_norm_pred = format!("{GMEOW}assessedNorm");
    let norms: Vec<RdfTerm> = quads
        .iter()
        .filter(|q| &q.subject == assessment && q.predicate.as_str() == assessed_norm_pred)
        .map(|q| q.object.clone())
        .collect();
    assert_eq!(
        norms.len(),
        1,
        "expected exactly one gmeow:assessedNorm on the assessment, got {norms:?}"
    );
    let norm = &norms[0];

    let modality_pred = format!("{GMEOW}deonticModality");
    let deontic_recommendation =
        RdfTerm::Iri(gmeow_validate::advisory::DEONTIC_RECOMMENDATION_IRI.to_owned());
    let modalities: Vec<_> = quads
        .iter()
        .filter(|q| {
            &q.subject == norm
                && q.predicate.as_str() == modality_pred
                && q.object == deontic_recommendation
        })
        .collect();
    assert_eq!(
        modalities.len(),
        1,
        "expected the assessedNorm to carry gmeow:deonticModality = deonticRecommendation, \
             got {modalities:?}"
    );

    let issuer_pred = format!("{GMEOW}normIssuer");
    let issuers: Vec<_> = quads
        .iter()
        .filter(|q| &q.subject == norm && q.predicate.as_str() == issuer_pred)
        .collect();
    assert!(
        !issuers.is_empty(),
        "expected the assessedNorm to carry a gmeow:normIssuer, found none"
    );
}

/// BOTH advisory wings must ride a CONFORMING run over a base graph
/// carrying one accepted recommendation candidate. Reuses the full
/// `ValidateStage::run` harness with a shape module that cannot fire against the
/// base graph (no `sh:targetNode`/property shape), so the run is genuinely
/// conforming (`shacl.clean`), and asserts:
///  - the report carries a HARVESTED flat advisory finding (`advice.*`, tagged
///    `advisory-harvested`) at the Advisory standpoint (routed into
///    `graph/diagnostics`), NOT the raw `shacl.*` Info finding (suppressed);
///  - the stage product's `graph/norm-claims` carries the materialised
///    `gmeow:ComplianceAssessment` claim, in full documented shape.
///
/// Falsifiable: this asserts the actual emitted content, not mere presence.
#[test]
fn stage_validate_emits_both_advice_projections() {
    let output = run_full_stage(ADVICE_BASE_NQ, ADVICE_SHAPE);

    let json_bytes = output
        .product
        .artifact(SHACL_JSON_PATH)
        .expect("shacl.json artifact on the stage product");
    let report: Report = serde_json::from_slice(json_bytes).expect("shacl.json parses as a Report");
    assert_eq!(
        report.error_count(),
        0,
        "the advisory Info match must NOT gate — a conforming run: {report:?}"
    );

    let advisory_finding = report
        .findings
        .iter()
        .find(|f| f.code.starts_with("advice.") && f.tags.iter().any(|t| t == "advisory-harvested"))
        .expect("a harvested advice.* finding must be present when the guard matched");
    assert_eq!(
        advisory_finding.severity,
        gmeow_errors::Severity::Note,
        "the harvested advisory is a Note: {advisory_finding:?}"
    );
    assert_eq!(
        advisory_finding.standpoint,
        Some(gmeow_errors::Standpoint::Advisory),
        "the advisory finding must carry the Advisory standpoint: {advisory_finding:?}"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.severity == gmeow_errors::Severity::Info
                && f.code.starts_with("shacl.")
                && f.code != "shacl.clean"),
        "the raw shacl.* Info constraint finding must be SUPPRESSED (re-projected as the \
             Note; only the informational shacl.clean record may remain): {report:?}"
    );

    let norm_claims = output
        .product
        .dataset()
        .project_named_graph(crate::stages::carrier::GRAPH_NORM_CLAIMS);
    assert_compliance_assessment_present(&norm_claims);
}

/// The `gmeow:ComplianceAssessment` claim must be
/// emitted UNCONDITIONALLY — even on a NON-conforming run — because it rides the
/// same unconditional completion path as the flat advisory Note (never gated behind
/// `report.conforms`). Reuses the SHACL-violation shape from
/// `validate_stage_emits_sarif_for_shacl_violation` inside the full `run` harness so
/// the report genuinely carries a SHACL error. Falsifiable: guarding the emit behind
/// `if report.conforms` (or any early return before the emit) makes this test fail.
#[test]
fn stage_validate_emits_advice_claim_even_when_nonconforming() {
    // Both the advisory Info shape (which the base graph's gmeow:Foo individual matches)
    // AND a hard minCount violation shape, so the run is genuinely non-conforming yet the
    // advisory claim still rides the unconditional completion path.
    let shapes = format!(
        "{ADVICE_SHAPE}\n\
@prefix ex: <https://example.test/> .\n\
ex:RequiredShape a sh:NodeShape ;\n\
    sh:targetNode ex:thing ;\n\
    sh:property [\n\
        sh:path ex:required ;\n\
        sh:minCount 1 ;\n\
        sh:message \"required value is missing\" ;\n\
    ] .\n"
    );
    let output = run_full_stage(ADVICE_BASE_NQ, &shapes);

    let json_bytes = output
        .product
        .artifact(SHACL_JSON_PATH)
        .expect("shacl.json artifact on the stage product");
    let report: Report = serde_json::from_slice(json_bytes).expect("shacl.json parses as a Report");
    assert!(
        report.error_count() >= 1,
        "the minCount-violation corpus must be genuinely non-conforming: {report:?}"
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code == "shacl.MinCountConstraintComponent"),
        "expected the SHACL minCount violation finding: {report:?}"
    );

    let norm_claims = output
        .product
        .dataset()
        .project_named_graph(crate::stages::carrier::GRAPH_NORM_CLAIMS);
    assert_compliance_assessment_present(&norm_claims);
}

/// The D5 abductive tier reads the REASONED graph, so `ValidateStage::run` HARD-FAILS
/// when its `stage-reason` upstream is absent — it never silently falls back to the
/// authored-only source graph (the silent-capability-degradation violation this fix
/// forbids). Falsifiable: restoring an authored-graph fallback in place of the
/// stage-reason `ok_or_else` makes this expect-err assertion fail.
#[test]
fn stage_validate_hard_fails_without_the_reasoned_upstream() {
    use purrdf::RdfDatasetBuilder;

    let repo = mock_repo("# no shapes\n");
    let dataset = RdfDatasetBuilder::new().freeze().expect("empty dataset");
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    artifacts.insert(BASE_GRAPH_PATH.to_string(), Vec::new());
    let span_blob =
        serde_json::to_vec(&crate::ingest::SpanIndex::new()).expect("encode span index");
    let bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
        dataset,
        artifacts,
        DatasetProvenance::new(),
        crate::stages::carrier::REP_SPAN_TABLE,
        "application/json",
        span_blob,
    );
    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    upstream.insert(
        "stage-source-load".to_string(),
        StageProduct::from_bundle("stage-source-load", Arc::new(bundle)),
    );
    add_catalog(&mut upstream, SYNTHETIC_SOURCE);
    // Every generated-shape producer is present, so the stage reaches the abductive
    // tier — but stage-reason is deliberately OMITTED.
    for (producer, rels) in [
        (
            "stage-compile-logic",
            &[
                crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH,
                crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH,
            ][..],
        ),
        (
            "stage-export-constraint-shapes",
            &[crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH][..],
        ),
        (
            "stage-export-frame-shapes",
            &[crate::stages::frame_shapes::FRAME_SHAPES_PATH][..],
        ),
        (
            "stage-export-result-shapes",
            &[crate::stages::result_shapes::RESULT_SHAPES_PATH][..],
        ),
    ] {
        let artifacts: BTreeMap<String, Vec<u8>> = rels
            .iter()
            .map(|rel| ((*rel).to_string(), b"# generated\n".to_vec()))
            .collect();
        upstream.insert(
            producer.to_string(),
            StageProduct::from_artifacts(producer, artifacts),
        );
    }
    let err = match ValidateStage::new().validate_input(
        StageInput {
            root: repo.path(),
            upstream: &upstream,
        },
        BTreeMap::new(),
    ) {
        Ok(_) => panic!("validate must hard-fail without stage-reason, never authored-only"),
        Err(e) => e,
    };
    assert!(
        format!("{err:?}").contains("stage-reason"),
        "the hard-fail must name the missing stage-reason upstream: {err:?}"
    );
}
