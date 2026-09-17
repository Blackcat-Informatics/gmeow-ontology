// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tiny complete-report controls shared by native admission and actual cache tests.

use super::*;
use crate::stages::diag_render::{DiagnosticsPaths, render_diagnostics_artifacts};
use gmeow_errors::{Finding, FindingCategory, Location, Severity, Standpoint};

pub(crate) const TERM: &str = "https://example.org/documented-property";

/// Populate report-only evidence, including optional fields that cannot safely
/// ride a positional codec and values deliberately absent from the RDF graph.
pub(crate) fn rich_report(owner: DiagnosticReportOwner) -> Report {
    use gmeow_errors::diag::{
        Advice, ArtifactChange, Guidance, GuidanceModality, GuidanceSource, Region, Remediation,
    };
    use gmeow_errors::model::{DiagnosticAttribution, RelatedLabel, Rule};
    let location = Location::new(
        Some("source.ttl".into()),
        Some(17),
        Some(9),
        Some(TERM.into()),
    )
    .with_gts_term(u64::MAX)
    .with_gts_quad(7)
    .with_gts_reifier(8)
    .with_gts_frame(9)
    .with_gts_segment(10);
    let mut finding = Finding::new(
        Severity::Warning,
        "owned.diagnostic",
        format!("{} quoted \"text\"\nsecond line", owner.tool()),
    )
    .with_tool(owner.tool())
    .with_category(FindingCategory::DataShapeViolation)
    .with_standpoint(Standpoint::Binding);
    finding.locations.push(location.clone());
    finding.related_locations.push(Location::new(
        None,
        None,
        None,
        Some("https://example.org/antecedent".into()),
    ));
    finding.related_labels.push(RelatedLabel {
        location: location.clone(),
        message: "source label".into(),
    });
    finding
        .suggestions
        .push("retain the authored alternative".into());
    finding.advice.push(Advice {
        standpoint: Standpoint::Advisory,
        text: "contextual advice".into(),
        help_uri: Some("https://example.org/advice".into()),
    });
    finding.remediation.push(
        Remediation::new("repair the source", Standpoint::Binding)
            .with_help_uri("https://example.org/repair")
            .with_artifact_change(ArtifactChange {
                artifact_uri: "source.ttl".into(),
                region: Region {
                    start_line: Some(17),
                    start_column: Some(9),
                    end_line: Some(17),
                    end_column: Some(12),
                },
                replacement: "replacement".into(),
            }),
    );
    finding.finding_iri = Some("https://example.org/finding".into());
    finding.anchor_iri = Some("https://example.org/anchor".into());
    finding.anchor_non_trivial = true;
    finding
        .antecedents
        .push("https://example.org/antecedent".into());
    finding.root_cause = Some("https://example.org/root".into());
    finding.cluster = Some("https://example.org/cluster".into());
    finding
        .cross_node_glut_with
        .push("https://example.org/opposed".into());
    finding.tags.push("owned-tag".into());
    finding.detail = Some("complete evidence detail".into());
    finding.failure_class = Some("https://example.org/Failure".into());
    for (slice, role) in [("a", "shape-owner"), ("b", "focus-origin")] {
        finding.attributions.push(DiagnosticAttribution {
            slice_iri: format!("https://example.org/slice/{slice}"),
            role: role.into(),
            evidence: Some(format!("{slice} source proof")),
        });
    }
    finding
        .documented_terms
        .extend([TERM.to_owned(), TERM.to_owned()]);
    finding.guidance.push(Guidance {
        modality: GuidanceModality::AvoidWhen,
        source: GuidanceSource::DocumentedTerm,
        term_iri: TERM.into(),
        text: "the source excludes this context".into(),
        standpoint: Standpoint::Perspectival,
        help_uri: Some("https://example.org/guidance".into()),
    });
    finding
        .derived_from_quads
        .push("https://example.org/quad".into());
    finding.cited_iris.push("https://example.org/cited".into());
    let mut report = Report::new(owner.tool());
    report.add_finding(finding);
    report.add_finding(Finding::new(
        Severity::Error,
        "earlier",
        "normalized before warning",
    ));
    let mut rule = Rule::new("owned.diagnostic", Severity::Warning)
        .with_remediation("registry repair")
        .with_how_to_use("registry use");
    rule.title = Some("rule label".into());
    rule.description = Some("rule definition".into());
    rule.help_uri = Some("https://example.org/rule".into());
    report.rules.extend([rule.clone(), rule]);
    report.metadata.insert(
        "nested-evidence".into(),
        serde_json::json!({"array":[null,true,17,-3,1.25],"source":"original","max":u64::MAX}),
    );
    report.metadata.insert(
        crate::stages::validate::SHACL_INPUT_DIGEST_KEY.into(),
        serde_json::json!("blake3:explicit-tiny-input"),
    );
    report
}

/// Exercise the real final renderer and native producer pairing over a supplied
/// tiny report, including its terminal artifacts. No corpus or engine is loaded.
pub(crate) fn product(owner: DiagnosticReportOwner, report: Report) -> StageProduct {
    let (stage, paths, seal) = match owner {
        DiagnosticReportOwner::CompileLogic => (
            "stage-compile-logic",
            DiagnosticsPaths {
                json: crate::stages::compile_logic::DIAG_JSON_PATH,
                sarif: crate::stages::compile_logic::DIAG_SARIF_PATH,
                html: crate::stages::compile_logic::DIAG_HTML_PATH,
                rdf: crate::stages::compile_logic::DIAG_RDF_PATH,
            },
            None,
        ),
        DiagnosticReportOwner::Validate => (
            "stage-validate",
            DiagnosticsPaths {
                json: crate::stages::validate::SHACL_JSON_PATH,
                sarif: crate::stages::validate::SHACL_SARIF_PATH,
                html: crate::stages::validate::SHACL_HTML_PATH,
                rdf: crate::stages::validate::SHACL_RDF_PATH,
            },
            Some(crate::stages::validate::SHACL_RECORD_DIGEST_KEY),
        ),
    };
    let rendered = render_diagnostics_artifacts(stage, report, &paths, None, None, seal).unwrap();
    let mut bundle = crate::bundle::bundle_from_artifacts_over(
        rendered.dataset,
        rendered.artifacts,
        purrdf::provenance::DatasetProvenance::new(),
    );
    pin_diagnostics(
        &mut bundle,
        stage,
        Arc::new(DiagnosticsPublication::producer(owner, rendered.report).unwrap()),
    )
    .unwrap();
    StageProduct::from_bundle(stage, Arc::new(bundle))
}

/// Assemble only the diagnostic contract of a tiny retained snapshot, reusing
/// the same source publications and complete-owner admission as production.
pub(crate) fn snapshot_product(upstream: &BTreeMap<String, StageProduct>) -> StageProduct {
    let publication = snapshot_diagnostics(upstream).unwrap();
    let graph = Arc::new(purrdf::RdfDataset::union(
        &upstream
            .values()
            .map(|product| product.dataset())
            .collect::<Vec<_>>(),
    ));
    let mut bundle = crate::bundle::bundle_from_artifacts_over(
        graph,
        BTreeMap::new(),
        purrdf::provenance::DatasetProvenance::new(),
    );
    pin_diagnostics(&mut bundle, "stage-snapshot", publication).unwrap();
    StageProduct::from_bundle("stage-snapshot", Arc::new(bundle))
}

/// The complete native Report equals the final rendered value, including every
/// source field, normalization and the selected record seal.
#[test]
fn native_diagnostics_report_matches_terminal_value_and_seal() {
    // gmeow-test-input: synthetic-only
    for owner in [
        DiagnosticReportOwner::CompileLogic,
        DiagnosticReportOwner::Validate,
    ] {
        let original = rich_report(owner);
        let product = product(owner, original.clone());
        let publication = diagnostics_from_product(&product, &product.stage_id).unwrap();
        let actual = publication.report(owner).unwrap();
        let path = match owner {
            DiagnosticReportOwner::CompileLogic => crate::stages::compile_logic::DIAG_JSON_PATH,
            DiagnosticReportOwner::Validate => crate::stages::validate::SHACL_JSON_PATH,
        };
        let recorded: Report = serde_json::from_slice(product.artifact(path).unwrap()).unwrap();
        assert_eq!(actual.as_ref(), &recorded);
        let mut expected = original.normalized();
        if owner == DiagnosticReportOwner::Validate {
            let key = crate::stages::validate::SHACL_RECORD_DIGEST_KEY;
            let digest = crate::stages::diag_render::record_digest(&expected, key).unwrap();
            expected
                .metadata
                .insert(key.into(), serde_json::Value::String(digest));
            crate::stages::diag_render::verify_record_digest(actual, key, "tiny native report")
                .unwrap();
        }
        assert_eq!(actual.as_ref(), &expected);
        assert_eq!(actual.rules.len(), 1);
        assert_eq!(actual.findings[0].code, "earlier");
        let rich = &actual.findings[1];
        assert_eq!(rich.documented_terms, vec![TERM.to_owned()]);
        assert_eq!(rich.attributions.len(), 2);
        assert_eq!(rich.related_labels[0].message, "source label");
    }
}

/// Native publication observes actual meta-rule enrichment before the final seal,
/// rather than retaining the producer's earlier report value.
#[test]
fn native_diagnostics_publish_meta_enrichment_before_sealing() {
    // gmeow-test-input: synthetic-only
    let source = purrdf::dataset_from_bytes(
        br#"
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        @prefix ex: <https://example.org/> .
        ex:trace a logic:Rule, gmeow:DiagnosticMetaRule ;
            logic:provenance ex:source ;
            logic:head [ rdf:subject ex:finding ;
                rdf:predicate gmeow:findingRootCause ; rdf:object ex:derivedRoot ] .
    "#,
        purrdf::NativeRdfFormat::Turtle,
    )
    .unwrap();
    let meta = crate::stages::meta_findings::MetaProgram::from_source_dataset(&source)
        .unwrap()
        .unwrap();
    let mut original = rich_report(DiagnosticReportOwner::Validate);
    original.findings[0].root_cause = None;
    let rendered = render_diagnostics_artifacts(
        "stage-validate",
        original.clone(),
        &DiagnosticsPaths {
            json: "report.json",
            sarif: "report.sarif",
            html: "report.html",
            rdf: "report.nq",
        },
        None,
        Some(&meta),
        Some(crate::stages::validate::SHACL_RECORD_DIGEST_KEY),
    )
    .unwrap();
    let published = &rendered.report;
    let finding = published
        .findings
        .iter()
        .find(|finding| finding.code == "owned.diagnostic")
        .unwrap();
    assert_eq!(
        finding.root_cause.as_deref(),
        Some("https://example.org/derivedRoot")
    );
    assert!(original.findings[0].root_cause.is_none());
    let final_json: Report = serde_json::from_slice(&rendered.artifacts["report.json"]).unwrap();
    assert_eq!(published.as_ref(), &final_json);
    crate::stages::diag_render::verify_record_digest(
        published,
        crate::stages::validate::SHACL_RECORD_DIGEST_KEY,
        "meta-enriched native report",
    )
    .unwrap();
    assert!(rendered.dataset.owned_quads().any(|quad| {
        quad.subject == purrdf::RdfTerm::iri("https://example.org/finding")
            && quad.predicate == "https://blackcatinformatics.ca/gmeow/findingRootCause"
            && quad.object == purrdf::RdfTerm::iri("https://example.org/derivedRoot")
    }));
}

/// An empty selected report still has its mandatory owner and declared graph.
#[test]
fn native_diagnostics_empty_report_is_a_real_publication() {
    // gmeow-test-input: synthetic-only
    let product = product(
        DiagnosticReportOwner::CompileLogic,
        Report::new("logic-compile"),
    );
    let reports = diagnostics_from_product(&product, "stage-compile-logic").unwrap();
    assert!(
        reports
            .report(DiagnosticReportOwner::CompileLogic)
            .unwrap()
            .findings
            .is_empty()
    );
    assert!(crate::handle_identity::contains_graph(
        product.bundle(),
        GRAPH_DIAGNOSTICS
    ));
}

/// Original producers can release after snapshot assembly; only the two report
/// Arcs remain shared, and the same borrowed docs fold yields the same joins.
#[test]
fn native_diagnostics_snapshot_shares_reports_after_producer_release() {
    // gmeow-test-input: synthetic-only
    let mut upstream = BTreeMap::new();
    for owner in [
        DiagnosticReportOwner::CompileLogic,
        DiagnosticReportOwner::Validate,
    ] {
        let product = product(owner, rich_report(owner));
        upstream.insert(product.stage_id.clone(), product);
    }
    let selected = snapshot_diagnostics(&upstream).unwrap();
    for (stage, owner) in [
        ("stage-compile-logic", DiagnosticReportOwner::CompileLogic),
        ("stage-validate", DiagnosticReportOwner::Validate),
    ] {
        assert!(Arc::ptr_eq(
            selected.report(owner).unwrap(),
            diagnostics_from_product(&upstream[stage], stage)
                .unwrap()
                .report(owner)
                .unwrap()
        ));
    }
    let known = std::collections::BTreeSet::from([TERM.to_owned()]);
    let before =
        crate::stages::docs_render::diagnostics_digest_from_upstream(&upstream, &known, &[])
            .unwrap();
    assert_eq!(before.total, 4);
    assert_eq!(
        before.by_term[TERM].len(),
        2,
        "primary and secondary attribution must not duplicate one finding"
    );
    assert_eq!(before.by_slice.len(), 2);
    assert!(before.by_slice.values().all(|findings| findings.len() == 2));
    assert!(before.by_term[TERM][0].message.starts_with("shacl "));
    assert!(
        before.by_term[TERM][1]
            .message
            .starts_with("logic-compile ")
    );
    let compile = upstream.remove("stage-compile-logic").unwrap();
    assert!(
        snapshot_diagnostics(&upstream).is_err(),
        "a lone validation report cannot form the snapshot"
    );
    upstream.insert(compile.stage_id.clone(), compile);
    let snapshot = snapshot_product(&upstream);
    for owner in [
        DiagnosticReportOwner::CompileLogic,
        DiagnosticReportOwner::Validate,
    ] {
        assert!(Arc::ptr_eq(
            selected.report(owner).unwrap(),
            diagnostics_from_product(&snapshot, "stage-snapshot")
                .unwrap()
                .report(owner)
                .unwrap()
        ));
    }
    let released: BTreeMap<_, _> = upstream
        .into_iter()
        .map(|(stage, product)| (stage, product.into_carrier_released().unwrap()))
        .collect();
    assert!(snapshot_diagnostics(&released).is_err());
    let reports = diagnostics_from_product(&snapshot, "stage-snapshot").unwrap();
    let after = crate::stages::docs_render::diagnostics_digest_from_reports(
        reports.report(DiagnosticReportOwner::Validate).unwrap(),
        reports.report(DiagnosticReportOwner::CompileLogic).unwrap(),
        &known,
        &[],
    );
    assert_eq!(before, after);
    assert!(diagnostics_from_product(&snapshot, "stage-validate").is_err());
    assert!(snapshot_diagnostics(&BTreeMap::new()).is_err());
}

/// A valid artifact cannot replace missing native identity, wrong ownership or a
/// payload modified after its immutable publication commitment was captured.
#[test]
fn native_diagnostics_refuse_missing_wrong_and_substituted_publications() {
    // gmeow-test-input: synthetic-only
    let owner = DiagnosticReportOwner::Validate;
    let mut absent = product(owner, rich_report(owner));
    let _ = Arc::make_mut(&mut absent.bundle).detach_handle(GRAPH_DIAGNOSTICS);
    assert!(
        absent
            .artifact(crate::stages::validate::SHACL_JSON_PATH)
            .is_some()
    );
    assert!(diagnostics_from_product(&absent, "stage-validate").is_err());
    let mut changed = product(owner, rich_report(owner));
    let mut report = diagnostics_from_product(&changed, "stage-validate")
        .unwrap()
        .report(owner)
        .unwrap()
        .as_ref()
        .clone();
    report.findings[1].locations[0].logical = Some("https://example.org/substitution".into());
    let publication = Arc::new(DiagnosticsPublication::producer(owner, Arc::new(report)).unwrap());
    pin_diagnostics(
        Arc::make_mut(&mut changed.bundle),
        "stage-validate",
        publication,
    )
    .unwrap();
    assert!(
        diagnostics_from_product(&changed, "stage-validate")
            .unwrap_err()
            .message()
            .contains("identity changed")
    );
    let mut wrong = product(owner, rich_report(owner));
    let bundle = Arc::make_mut(&mut wrong.bundle);
    let pin = bundle.graph_digest(GRAPH_DIAGNOSTICS);
    bundle
        .pin_handle(
            GRAPH_DIAGNOSTICS,
            PipelineHandle::Logic(Arc::new(gmeow_logic_compile::ir::LogicProgram::new(
                vec![],
                vec![],
                vec![],
                None,
            ))),
            pin,
        )
        .unwrap();
    assert!(
        diagnostics_from_product(&wrong, "stage-validate")
            .unwrap_err()
            .message()
            .contains("Diagnostics handle arm")
    );
    let wrong_owner = DiagnosticsPublication::producer(
        DiagnosticReportOwner::CompileLogic,
        Arc::new(Report::new("logic-compile")),
    )
    .unwrap();
    assert!(
        wrong_owner
            .validate_binding("stage-validate", GRAPH_DIAGNOSTICS)
            .is_err()
    );
    assert!(
        DiagnosticsPublication::producer(owner, Arc::new(Report::new("logic-compile"))).is_err()
    );
}
