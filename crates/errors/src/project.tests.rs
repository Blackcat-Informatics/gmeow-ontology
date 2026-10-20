// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use crate::code::register_code;
use crate::diag::{Diag, StageId};
use crate::grade::{FindingCategory, Grade, Severity, Standpoint};
use crate::ledger::{DiagLedger, fingerprint_iri};
use crate::model::Location;

#[test]
fn named_graph_identity_survives_merge_replay_and_every_projection() {
    use crate::render;
    let mut expected = None;
    for graphs in [
        ["urn:world:b", "urn:world:a"],
        ["urn:world:a", "urn:world:b"],
    ] {
        let mut ledger = DiagLedger::new();
        for graph in graphs {
            let diag = diag_at(
                "test.project.graph",
                "same.ttl",
                Grade::new(
                    Severity::Error,
                    FindingCategory::DataShapeViolation,
                    Standpoint::Binding,
                ),
            )
            .with_focus("urn:focus")
            .with_graph(purrdf_core::BaseIri::parse(graph).unwrap());
            ledger.attach(diag, StageId::new("test"));
        }
        assert_eq!(ledger.len(), 2);
        let report = ledger.project_report("test").normalized();
        assert_ne!(
            report.findings[0].finding_iri,
            report.findings[1].finding_iri
        );
        assert_ne!(report.findings[0].anchor_iri, report.findings[1].anchor_iri);
        let json = render::to_json(&report).unwrap();
        if let Some(expected) = &expected {
            assert_eq!(&json, expected);
        } else {
            expected = Some(json.clone());
        }
        let sarif: serde_json::Value =
            serde_json::from_str(&render::to_sarif(&report).unwrap()).unwrap();
        let results = sarif["runs"][0]["results"].as_array().unwrap();
        for result in results {
            assert_eq!(
                result["partialFingerprints"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["gmeowFindingHash/v3"]
            );
        }
        assert_ne!(
            results[0]["partialFingerprints"],
            results[1]["partialFingerprints"]
        );
        for graph in graphs {
            assert!(json.contains(graph));
            assert!(
                results
                    .iter()
                    .any(|result| result["properties"]["gmeow.sourceGraph"] == graph)
            );
            assert!(render::to_text(&report).contains(&format!("source graph: {graph}")));
            assert!(render::to_html(&report).contains(graph));
            assert!(render::to_gmeow_rdf(&report).contains(&format!(
                "<https://blackcatinformatics.ca/gmeow/findingSourceGraph> <{graph}>"
            )));
        }
        let native = render::to_gmeow_dataset(&report).unwrap();
        for graph in graphs {
            assert!(native.quads().any(|quad| {
                native.resolve(quad.p)
                    == purrdf_core::TermRef::Iri(
                        "https://blackcatinformatics.ca/gmeow/findingSourceGraph",
                    )
                    && native.resolve(quad.o) == purrdf_core::TermRef::Iri(graph)
                    && quad.g.is_some_and(|id| {
                        native.resolve(id)
                            == purrdf_core::TermRef::Iri(
                                "https://blackcatinformatics.ca/gmeow/graph/diagnostics",
                            )
                    })
            }));
        }
        let bytes = serde_json::to_vec(&ledger.emit_sorted()).unwrap();
        let nodes: Vec<crate::DiagNode> = serde_json::from_slice(&bytes).unwrap();
        let mut replay = DiagLedger::new();
        replay.replay(nodes);
        assert_eq!(
            render::to_json(&replay.project_report("test")).unwrap(),
            json
        );
        let mut reworded = report.clone();
        reworded.findings[0].message = "different prose".into();
        let changed: serde_json::Value =
            serde_json::from_str(&render::to_sarif(&reworded).unwrap()).unwrap();
        let iri = &report.findings[0].finding_iri;
        let original = results
            .iter()
            .find(|row| {
                row["partialFingerprints"]["gmeowFindingHash/v3"] == iri.as_deref().unwrap()
            })
            .unwrap();
        let reworded_result = changed["runs"][0]["results"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| {
                row["properties"]["gmeow.sourceGraph"]
                    == report.findings[0].source_graph.as_ref().unwrap().as_str()
            })
            .unwrap();
        assert_eq!(
            reworded_result["partialFingerprints"],
            original["partialFingerprints"]
        );
    }
}

#[test]
fn graph_replay_refuses_relative_or_malformed_identity_without_normalizing() {
    let graph = "https://example.org/a/../b/é";
    let ctx = crate::SourceContext {
        graph: Some(purrdf_core::BaseIri::parse(graph).unwrap()),
        ..crate::SourceContext::default()
    };
    assert!(ctx.is_non_trivial());
    let mut value = serde_json::to_value(&ctx).unwrap();
    assert_eq!(value["graph"], graph);
    assert_eq!(
        serde_json::from_value::<crate::SourceContext>(value.clone()).unwrap(),
        ctx
    );
    for bad in ["relative", "http://example.org/<bad>", ""] {
        value["graph"] = serde_json::json!(bad);
        assert!(serde_json::from_value::<crate::SourceContext>(value.clone()).is_err());
        let mut finding =
            serde_json::to_value(crate::Finding::new(Severity::Error, "code", "message")).unwrap();
        finding["source_graph"] = serde_json::json!(bad);
        assert!(serde_json::from_value::<crate::Finding>(finding).is_err());
    }
    let unscoped = serde_json::to_value(crate::SourceContext::default()).unwrap();
    assert!(unscoped.get("graph").is_none());
    assert!(!crate::SourceContext::default().is_non_trivial());
}

#[test]
#[should_panic(expected = "contradicts its pinned digest")]
fn graph_identity_cannot_be_replaced_in_a_pinned_node() {
    let mut ledger = DiagLedger::new();
    ledger.attach(
        diag_at(
            "test.graph.pin",
            "same.ttl",
            Grade::new(
                Severity::Error,
                FindingCategory::DataShapeViolation,
                Standpoint::Binding,
            ),
        )
        .with_graph(purrdf_core::BaseIri::parse("urn:world:a").unwrap()),
        StageId::new("test"),
    );
    let mut node = ledger.emit_sorted()[0].clone();
    node.source_ctx.graph = Some(purrdf_core::BaseIri::parse("urn:world:b").unwrap());
    DiagLedger::new().replay([node]);
}

#[test]
fn projection_normalization_preserves_the_primary_source_location() {
    let mut ledger = DiagLedger::new();
    ledger.attach(
        diag_at(
            "test.project.primary",
            "z-source.ttl",
            Grade::new(
                Severity::Error,
                FindingCategory::DataShapeViolation,
                Standpoint::Binding,
            ),
        )
        .with_location(Location {
            path: Some("z-source.ttl".into()),
            logical: Some("urn:source".into()),
            ..Location::default()
        })
        .with_focus("a-focus"),
        StageId::new("test"),
    );
    let report = ledger.project_report("test").normalized();
    let finding = &report.findings[0];
    assert_eq!(
        finding.primary_location().unwrap().path.as_deref(),
        Some("z-source.ttl")
    );
    assert_eq!(
        finding.primary_location().unwrap().logical.as_deref(),
        Some("urn:source")
    );
    assert!(
        finding
            .locations
            .iter()
            .any(|location| location.logical.as_deref() == Some("a-focus"))
    );
    assert_eq!(
        report.normalized().findings[0].primary_location(),
        finding.primary_location()
    );
}

#[test]
fn merged_observations_have_one_canonical_order_on_every_projection_and_replay() {
    let mut expected = None;
    for reversed in [false, true] {
        let mut ledger = DiagLedger::new();
        let mut observations = vec![("zeta", "9"), ("alpha", "2"), ("alpha", "1")];
        if reversed {
            observations.reverse();
        }
        for (message, value) in observations {
            let mut diag = diag_at(
                "test.project.messages",
                "source.ttl",
                Grade::new(
                    Severity::Error,
                    FindingCategory::DataShapeViolation,
                    Standpoint::Binding,
                ),
            );
            diag.inner_mut().message = message.into();
            ledger.attach(
                diag.with_observed(crate::Slot::new(value)),
                StageId::new("test"),
            );
        }
        assert_eq!(
            ledger.messages(Severity::Error).collect::<Vec<_>>(),
            ["alpha", "alpha", "zeta"]
        );
        let nodes: Vec<_> = ledger.emit_sorted().into_iter().cloned().collect();
        assert_eq!(nodes[0].observations.len(), 3);
        assert_eq!(
            nodes[0].observations[0].observed.as_ref().unwrap().lexical,
            "1"
        );
        let report = serde_json::to_vec(&ledger.project_report("test")).unwrap();
        if let Some(expected) = &expected {
            assert_eq!(&report, expected);
        } else {
            expected = Some(report.clone());
        }
        let mut nodes = nodes;
        nodes[0].observations.reverse();
        let mut replay = DiagLedger::new();
        replay.replay(nodes);
        assert_eq!(
            serde_json::to_vec(&replay.project_report("test")).unwrap(),
            report
        );
    }
}

fn diag_at(code: &'static str, path: &str, grade: Grade) -> Diag {
    let c = register_code(code);
    Diag::new(c, grade, "msg").with_location(Location {
        path: Some(path.to_owned()),
        ..Location::default()
    })
}

#[test]
fn projected_finding_carries_the_grade_standpoint() {
    // U1: the projected finding carries the gating standpoint truth-axis, so the
    // RDF `gmeow:findingStandpoint` twin (and its SHACL up-set shape) is not
    // vacuous. Without this the gate morphism's binding-standpoint conjunct
    // would have nothing to read on a real projected finding.
    let mut ledger = DiagLedger::new();
    ledger.attach(
        diag_at(
            "test.project.standpoint",
            "s.ttl",
            Grade::new(
                Severity::Error,
                FindingCategory::DataShapeViolation,
                Standpoint::Binding,
            ),
        ),
        StageId::new("s"),
    );
    let finding = &ledger.findings("validate")[0];
    assert_eq!(finding.standpoint, Some(Standpoint::Binding));
    assert_eq!(finding.category, Some(FindingCategory::DataShapeViolation));
}

#[test]
fn projection_retains_focus_and_a_distinct_logical_source_anchor() {
    for logical in [None, Some("urn:source:expression")] {
        let mut diag = diag_at(
            "test.project.focus",
            "slices/example/module.ttl",
            Grade::new(
                Severity::Error,
                FindingCategory::ModelingDisciplineViolation,
                Standpoint::Binding,
            ),
        )
        .with_focus("urn:source:term");
        diag.inner_mut().source_ctx.location.logical = logical.map(str::to_owned);
        let mut ledger = DiagLedger::new();
        ledger.attach(diag, StageId::new("test.project.focus"));
        let report = ledger.project_report("validate");
        let finding = &report.findings[0];
        assert_eq!(
            finding.primary_location().unwrap().path.as_deref(),
            Some("slices/example/module.ttl")
        );
        assert!(
            finding
                .locations
                .iter()
                .any(|location| location.logical.as_deref() == Some("urn:source:term"))
        );
        if let Some(logical) = logical {
            assert_eq!(
                finding.primary_location().unwrap().logical.as_deref(),
                Some(logical)
            );
        }
        let wire = crate::render::to_sarif(&report).unwrap();
        assert!(wire.contains("urn:source:term"));
        if let Some(logical) = logical {
            assert!(wire.contains(logical));
        }
    }
}

#[test]
fn projected_finding_carries_labels_as_related_locations() {
    // A multi-anchor witness (e.g. a SHACL result with a result-path / value,
    // or a "defined here / used there" lint) carries its secondary Label spans
    // through the projection as related locations — no secondary anchor is lost
    // to the single primary source context.
    use crate::diag::Label;
    use crate::model::Location as ModelLocation;
    let mut ledger = DiagLedger::new();
    let diag = diag_at(
        "test.project.labels",
        "s.ttl",
        Grade::new(
            Severity::Error,
            FindingCategory::DataShapeViolation,
            Standpoint::Binding,
        ),
    )
    .with_label(Label {
        location: ModelLocation {
            logical: Some("path https://ex/p".to_owned()),
            ..ModelLocation::default()
        },
        text: "path".to_owned(),
    });
    ledger.attach(diag, StageId::new("s"));
    let finding = &ledger.findings("validate")[0];
    assert!(
        finding
            .related_locations
            .iter()
            .any(|l| l.logical.as_deref() == Some("path https://ex/p")),
        "a Label span must project to a related location"
    );
    // The label TEXT survives losslessly in related_labels, beside its location —
    // the message the LSP renders as DiagnosticRelatedInformation (the bare
    // related_locations twin above carries no message).
    let related_label = finding
        .related_labels
        .iter()
        .find(|l| l.location.logical.as_deref() == Some("path https://ex/p"))
        .expect("a text-bearing Label must project to a related label");
    assert_eq!(related_label.message, "path");
}

#[test]
fn projected_finding_carries_antecedents_as_related_locations() {
    // U2: a witness with an antecedent edge projects that edge as a related
    // location keyed on the cause's stable finding IRI — the provenance chain
    // Phase-5 remediation and `gmeow explain` consume.
    let mut ledger = DiagLedger::new();
    let cause = diag_at(
        "test.project.cause",
        "cause.ttl",
        Grade::new(
            Severity::Note,
            FindingCategory::ProjectionLoss,
            Standpoint::Perspectival,
        ),
    );
    let cause_ref = ledger.attach(cause, StageId::new("s"));
    let effect = diag_at(
        "test.project.effect",
        "effect.ttl",
        Grade::new(
            Severity::Error,
            FindingCategory::DataShapeViolation,
            Standpoint::Binding,
        ),
    )
    .with_antecedents([cause_ref]);
    ledger.attach(effect, StageId::new("s"));

    let cause_iri = {
        // The cause node's stable finding IRI, recomputed from its identity.
        let node = ledger
            .emit_sorted()
            .into_iter()
            .find(|n| n.code == "test.project.cause")
            .expect("cause node present");
        fingerprint_iri(&node.fingerprint)
    };
    let effect_finding = ledger
        .findings("validate")
        .into_iter()
        .find(|f| f.code == "test.project.effect")
        .expect("effect finding present");
    assert!(
        effect_finding
            .related_locations
            .iter()
            .any(|l| l.logical.as_deref() == Some(cause_iri.as_str())),
        "the effect finding must relate to its antecedent cause by finding IRI"
    );
}

#[test]
fn advice_standpoint_and_help_uri_survive_the_projection() {
    // D2a: the lossy advice projection used to keep only `.text`; the faithful
    // projection keeps each advice's standpoint AND outward help URI on the
    // structured `finding.advice`, while the flat `suggestions` text twin still
    // renders for the existing surfaces.
    use crate::diag::Advice;
    use crate::grade::Standpoint;
    let mut ledger = DiagLedger::new();
    let diag = diag_at(
        "test.project.advice",
        "a.ttl",
        Grade::new(
            Severity::Warning,
            FindingCategory::PolicyWarning,
            Standpoint::Advisory,
        ),
    )
    .with_advice(Advice {
        standpoint: Standpoint::Perspectival,
        text: "prefer a rigid sortal".to_owned(),
        help_uri: Some("https://ex/help#sortal".to_owned()),
    });
    ledger.attach(diag, StageId::new("s"));
    let finding = &ledger.findings("validate")[0];
    // Flat text twin preserved (existing renderers).
    assert_eq!(
        finding.suggestions,
        vec!["prefer a rigid sortal".to_owned()]
    );
    // Structured advice keeps the standpoint + help URI the old projection dropped.
    assert_eq!(finding.advice.len(), 1);
    assert_eq!(finding.advice[0].standpoint, Standpoint::Perspectival);
    assert_eq!(
        finding.advice[0].help_uri.as_deref(),
        Some("https://ex/help#sortal")
    );
}

#[test]
fn remediation_projects_onto_the_finding() {
    // D2a: an authored Remediation on the witness rides onto the projected
    // finding's `remediation` (the SARIF-fixes / CLI "how to fix" payload).
    use crate::diag::{ArtifactChange, Region, Remediation};
    use crate::grade::Standpoint;
    let mut ledger = DiagLedger::new();
    let diag = diag_at(
        "test.project.remediation",
        "a.ttl",
        Grade::new(
            Severity::Error,
            FindingCategory::DataShapeViolation,
            Standpoint::Binding,
        ),
    )
    .with_remediation(
        Remediation::new(
            "attach both relata through gmeow:mediates",
            Standpoint::Binding,
        )
        .with_artifact_change(ArtifactChange {
            artifact_uri: "core/x.ttl".to_owned(),
            region: Region {
                start_line: Some(12),
                ..Region::default()
            },
            replacement: "gmeow:mediates ex:r .".to_owned(),
        }),
    );
    ledger.attach(diag, StageId::new("s"));
    let finding = &ledger.findings("validate")[0];
    assert_eq!(finding.remediation.len(), 1);
    assert_eq!(
        finding.remediation[0].text,
        "attach both relata through gmeow:mediates"
    );
    assert!(finding.remediation[0].artifact_change.is_some());
}

#[test]
fn projected_subject_and_antecedent_object_iris_close() {
    // D3 join-closure: build two findings where one is the other's antecedent,
    // project, and assert the child's antecedent-edge object IRI textually
    // equals the parent finding's OWN subject IRI (`finding_iri`). This is the
    // equality the declared root-cause / cluster meta-rules match on: subject and
    // antecedent-object must be the SAME blake3 fingerprint IRI.
    let mut ledger = DiagLedger::new();
    let cause = diag_at(
        "test.project.close.cause",
        "cause.ttl",
        Grade::new(
            Severity::Note,
            FindingCategory::ProjectionLoss,
            Standpoint::Perspectival,
        ),
    );
    let cause_ref = ledger.attach(cause, StageId::new("s"));
    let effect = diag_at(
        "test.project.close.effect",
        "effect.ttl",
        Grade::new(
            Severity::Error,
            FindingCategory::DataShapeViolation,
            Standpoint::Binding,
        ),
    )
    .with_antecedents([cause_ref]);
    ledger.attach(effect, StageId::new("s"));

    let findings = ledger.findings("validate");
    let cause_finding = findings
        .iter()
        .find(|f| f.code == "test.project.close.cause")
        .expect("cause finding");
    let effect_finding = findings
        .iter()
        .find(|f| f.code == "test.project.close.effect")
        .expect("effect finding");
    // The parent's own canonical subject IRI.
    let parent_subject = cause_finding
        .finding_iri
        .as_deref()
        .expect("ledger witness carries a fingerprint IRI");
    // The child's structured antecedent edge object.
    assert_eq!(
        effect_finding.antecedents,
        vec![parent_subject.to_owned()],
        "the antecedent-edge object IRI must equal the parent finding's subject IRI"
    );
    // And it closes textually with the ledger's fingerprint_iri for the cause.
    let node = ledger
        .emit_sorted()
        .into_iter()
        .find(|n| n.code == "test.project.close.cause")
        .expect("cause node");
    assert_eq!(parent_subject, fingerprint_iri(&node.fingerprint));
}
