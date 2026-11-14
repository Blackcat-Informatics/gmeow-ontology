// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Evidence coordinates (provenance, confidence) select no context: a unit whose
//! scope has only those is admitted with them left out of the problem and recorded
//! on the unit. A unit with any context coordinate stays blocked and names it.

use std::collections::BTreeSet;

use purrdf_lex::json::record::{FromJson, ToJson};

use super::*;
use crate::frontend::{
    AdmittedSemanticUnit, EdgeScope, SemanticUnitAdmission, SemanticUnitDisposition,
    SemanticUnitKind, SourceEdgeRole, StructuralSourceGraph,
};
use crate::ir::{ContextualScope, EvidenceCoordinate, LogicModality};
use crate::tptp::CanonAdmissionPublication;

const PREFIXES: &str = "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
    @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
    @prefix ex: <https://example.org/> .\n";

/// Each evidence coordinate: its authored annotation, its name and its coordinate.
const EVIDENCE: [(&str, &str, EvidenceCoordinate); 2] = [
    (
        "logic:provenance ex:report",
        "logic:provenance",
        EvidenceCoordinate::Provenance,
    ),
    (
        "logic:confidence 0.7",
        "logic:confidence",
        EvidenceCoordinate::Confidence,
    ),
];

/// Each context coordinate: its authored annotation and its name.
const CONTEXT: [(&str, &str); 4] = [
    ("logic:standpoint ex:observer", "logic:standpoint"),
    ("logic:time \"2026\"", "logic:time"),
    ("logic:modality logic:Epistemic", "logic:modality"),
    ("logic:inModule ex:module", "logic:inModule"),
];

const FORMULAS: &str = "ex:left a logic:Formula ; logic:relation ex:p ;\n\
        logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ],\n\
                       [ logic:termIndex 1 ; logic:termIri ex:b ] .\n\
    ex:right a logic:Formula ; logic:relation ex:q ;\n\
        logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ],\n\
                       [ logic:termIndex 1 ; logic:termIri ex:b ] .\n";

/// One source for every position a scope can sit at, each carrying `annotation`.
fn positions(annotation: &str) -> Vec<(&'static str, String)> {
    let rule = |head_scope: &str, rule_scope: &str, body: &str| {
        format!(
            "ex:rule a logic:Rule {rule_scope} ;\n\
                 logic:head [ rdf:reifies <<( ex:Bird ex:p ex:v )>> {head_scope} ] {body} ."
        )
    };
    vec![
        (
            "claim",
            format!("ex:claim rdf:reifies <<( ex:a ex:p ex:b )>> ; {annotation} ."),
        ),
        ("rule", rule("", &format!("; {annotation}"), "")),
        ("head atom", rule(&format!("; {annotation}"), "", "")),
        (
            "body atom",
            rule(
                "",
                "",
                &format!("; logic:body [ rdf:reifies <<( ex:Bird ex:q ex:v )>> ; {annotation} ]"),
            ),
        ),
        ("formula", format!("{FORMULAS}ex:left {annotation} .")),
        (
            "formula descendant",
            format!(
                "{FORMULAS}ex:root a logic:Formula ; logic:and ex:left, ex:right .\n\
                 ex:left {annotation} ."
            ),
        ),
        (
            "class definition",
            format!(
                "ex:Whole logic:subClassOf ex:Part .\n\
                 ex:R a logic:Restriction ; logic:onProperty ex:p ; \
                 logic:someValuesFrom ex:D ; {annotation} ."
            ),
        ),
        (
            "contract declaration",
            format!(
                "ex:s ex:q ex:o .\n\
                 ex:contract a logic:ReasoningContract ; \
                 logic:modelSemantics logic:ClassicalSemantics ; {annotation} ."
            ),
        ),
    ]
}

fn run(source: &str) -> (SourceAdmission, crate::tptp::FofProjection) {
    project(&format!("{PREFIXES}{source}"))
}

fn recorded(admission: &SourceAdmission) -> Vec<&SemanticUnitAdmission> {
    admission
        .units
        .iter()
        .filter(|unit| unit.evidence_left_out.is_some())
        .collect()
}

/// INVARIANT: a unit with no evidence left out keeps exactly the identifier and
/// the serde and hand-written JSON encodings it has always had. The literals were
/// captured from the code before the evidence record existed.
#[test]
fn a_plain_admitted_unit_keeps_its_identifier_and_encodings() {
    const ID: &str = "1a6556e6ff8078587e1eadcb0729ea7cf499ee3756efebe2f080b0ac4d26632d";
    const JSON: &str = r#"{"id":"1a6556e6ff8078587e1eadcb0729ea7cf499ee3756efebe2f080b0ac4d26632d","kind":"Assertion","anchor":"quad|9:I|5:urn:s|9:I|5:urn:p|9:I|5:urn:o|0:","documents":[],"disposition":{"Admitted":{"Assertion":{"subject":{"Iri":"urn:s"},"predicate":"urn:p","object":{"Iri":"urn:o"}}}}}"#;
    let (admission, _) = project("<urn:s> <urn:p> <urn:o> .");
    let [unit] = admission.units.as_slice() else {
        panic!("one plain assertion, one unit: {:?}", admission.units);
    };
    assert_eq!(unit.id, ID);
    assert!(unit.evidence_left_out.is_none());
    assert_eq!(
        purrdf_lex::json::write_compact(&unit.to_json()),
        JSON,
        "hand-written JSON"
    );
    assert_eq!(serde_json::to_string(unit).unwrap(), JSON, "serde");
    let decoded = SemanticUnitAdmission::from_json(&purrdf_lex::json::read(JSON).unwrap())
        .expect("the older form decodes with the record absent");
    assert_eq!(&decoded, unit);
    let decoded: SemanticUnitAdmission = serde_json::from_str(JSON).unwrap();
    assert_eq!(&decoded, unit);
}

/// A reified claim whose scope has only evidence is admitted, decided, and the
/// unit says which coordinates were left out and which authored node it came from.
#[test]
fn provenance_and_confidence_only_claims_are_admitted_and_record_what_was_left_out() {
    let (admission, projection) = run("ex:claim rdf:reifies <<( ex:a ex:p ex:b )>> ; \
         logic:provenance ex:report ; logic:confidence 0.7 .");
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{:?}",
        projection.blockers
    );
    let units = recorded(&admission);
    let [unit] = units.as_slice() else {
        panic!("one claim, one recorded unit: {units:?}");
    };
    assert!(matches!(
        unit.disposition,
        SemanticUnitDisposition::Admitted(AdmittedSemanticUnit::Assertion(_))
    ));
    let left_out = unit.evidence_left_out.as_ref().unwrap();
    assert_eq!(
        left_out.coordinates,
        BTreeSet::from([
            EvidenceCoordinate::Provenance,
            EvidenceCoordinate::Confidence
        ])
    );
    assert_eq!(left_out.nodes.len(), 1);
    assert!(
        left_out
            .nodes
            .iter()
            .all(|node| node.contains("https://example.org/claim")),
        "{left_out:?}"
    );
    assert!(
        projection
            .sentences
            .iter()
            .any(|sentence| sentence.source_units.contains(&unit.id)
                && sentence.body.contains("I|https://example.org/p")),
        "the claim is in the problem, not only its record"
    );
    assert!(
        !projection.problem.unwrap().contains("report"),
        "the left-out evidence is not part of the problem"
    );
}

/// INVARIANT: claims lowered from one node with several sources are one formula
/// held by several units, none of which shares an identifier with another.
#[test]
fn two_sources_of_one_claim_are_one_formula_held_by_two_units() {
    let (admission, projection) = run("ex:claim rdf:reifies <<( ex:a ex:p ex:b )>> ; \
         logic:provenance ex:one, ex:two .");
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{:?}",
        projection.blockers
    );
    let units = recorded(&admission);
    assert_eq!(units.len(), 2, "{units:?}");
    assert_ne!(units[0].id, units[1].id);
    assert_ne!(units[0].anchor, units[1].anchor);
    assert_eq!(
        units[0].evidence_left_out, units[1].evidence_left_out,
        "both name the one authored node"
    );
    let holding: Vec<_> = projection
        .sentences
        .iter()
        .filter(|sentence| sentence.body.contains("I|https://example.org/p"))
        .collect();
    let [sentence] = holding.as_slice() else {
        panic!("one formula: {holding:?}");
    };
    assert_eq!(
        sentence.source_units,
        units.iter().map(|unit| unit.id.clone()).collect(),
        "the one sentence names both units"
    );
    let ids: BTreeSet<_> = admission.units.iter().map(|unit| &unit.id).collect();
    assert_eq!(ids.len(), admission.units.len(), "{:?}", admission.units);
}

/// Rules lowered from one node with two sources stay two admitted units with
/// different program indices, each with its own record.
#[test]
fn two_sources_of_one_rule_are_two_units_each_with_its_record() {
    let (admission, projection) = run("ex:rule a logic:Rule ; logic:provenance ex:one, ex:two ;\n\
         logic:head [ rdf:reifies <<( ex:Bird ex:p ex:v )>> ] .");
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{:?}",
        projection.blockers
    );
    let rules: Vec<_> = admission
        .units
        .iter()
        .filter(|unit| unit.kind == SemanticUnitKind::Rule)
        .collect();
    assert_eq!(rules.len(), 2, "{rules:?}");
    let indices: BTreeSet<_> = rules
        .iter()
        .map(|unit| match &unit.disposition {
            SemanticUnitDisposition::Admitted(AdmittedSemanticUnit::Rule { index }) => *index,
            other => panic!("an admitted rule, not {other:?}"),
        })
        .collect();
    assert_eq!(indices.len(), 2);
    for unit in &rules {
        let left_out = unit.evidence_left_out.as_ref().expect("each unit records");
        assert_eq!(
            left_out.coordinates,
            BTreeSet::from([EvidenceCoordinate::Provenance])
        );
        assert_eq!(left_out.nodes.len(), 1);
        assert!(
            left_out
                .nodes
                .iter()
                .all(|n| n.contains("example.org/rule"))
        );
    }
    let ids: BTreeSet<_> = admission.units.iter().map(|unit| &unit.id).collect();
    assert_eq!(ids.len(), admission.units.len(), "{:?}", admission.units);
    assert!(
        projection
            .sentences
            .iter()
            .any(|sentence| sentence.source_units.len() == 2),
        "the two lowered rules are one sentence held by two units"
    );
}

/// Two provenance-only claims that contradict each other are both in the problem:
/// a contested fact needs a standpoint, and the problem is the contradiction.
#[test]
fn contradicting_provenance_only_claims_both_reach_the_problem() {
    let (admission, projection) = run("ex:A logic:disjointWith ex:B .\n\
         ex:one rdf:reifies <<( ex:a logic:instanceOf ex:A )>> ; logic:provenance ex:s1 .\n\
         ex:two rdf:reifies <<( ex:a logic:instanceOf ex:B )>> ; logic:provenance ex:s2 .");
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{:?}",
        projection.blockers
    );
    let units = recorded(&admission);
    assert_eq!(units.len(), 2, "{units:?}");
    let claims: Vec<_> = projection
        .sentences
        .iter()
        .filter(|sentence| units.iter().any(|u| sentence.source_units.contains(&u.id)))
        .collect();
    assert_eq!(claims.len(), 2, "two distinct claims: {claims:?}");
    assert_ne!(claims[0].body, claims[1].body);
}

/// A unit with provenance AND a context coordinate stays blocked and the message
/// names the context coordinate, not the evidence.
#[test]
fn evidence_beside_a_context_coordinate_stays_blocked_naming_the_context() {
    for (annotation, name) in CONTEXT {
        for (position, source) in positions(&format!("logic:provenance ex:report ; {annotation}")) {
            let (admission, projection) = run(&source);
            assert_eq!(
                projection.status,
                FofProjectionStatus::Blocked,
                "{position} {name}"
            );
            assert!(projection.problem.is_none());
            let blocker = projection
                .blockers
                .iter()
                .find(|blocker| {
                    matches!(
                        blocker.code.as_str(),
                        "CONTEXT_ERASURE_FORBIDDEN"
                            | "CONTEXTUAL_REIFICATION_REQUIRES_TYPED_TRANSLATION"
                    )
                })
                .unwrap_or_else(|| panic!("{position} {name}: {:?}", projection.blockers));
            assert!(
                blocker.reason.contains(name),
                "{position}: the blocker must name {name}: {}",
                blocker.reason
            );
            assert!(
                !blocker.reason.contains("logic:provenance"),
                "{position}: evidence is not what blocks: {}",
                blocker.reason
            );
            assert!(
                recorded(&admission).iter().all(|unit| !matches!(
                    unit.disposition,
                    SemanticUnitDisposition::Blocked { .. }
                )),
                "{position}: a blocked unit records nothing as left out"
            );
        }
    }
}

/// INVARIANT: every context coordinate blocks at every position, naming itself.
#[test]
fn every_context_coordinate_blocks_at_every_position() {
    for (annotation, name) in CONTEXT {
        for (position, source) in positions(annotation) {
            let (_, projection) = run(&source);
            assert_eq!(
                projection.status,
                FofProjectionStatus::Blocked,
                "{position} {name}: {:?}",
                projection.blockers
            );
            assert!(projection.problem.is_none(), "{position} {name}");
            assert!(
                projection
                    .blockers
                    .iter()
                    .any(|blocker| blocker.reason.contains(name)),
                "{position} {name}: {:?}",
                projection.blockers
            );
        }
    }
}

/// INVARIANT: every evidence coordinate is admitted at every position, left out
/// of the problem and recorded.
#[test]
fn every_evidence_coordinate_is_admitted_and_recorded_at_every_position() {
    for (annotation, name, coordinate) in EVIDENCE {
        for (position, source) in positions(annotation) {
            let (admission, projection) = run(&source);
            assert_eq!(
                projection.status,
                FofProjectionStatus::Complete,
                "{position} {name}: {:?}",
                projection.blockers
            );
            assert!(
                recorded(&admission).iter().any(|unit| {
                    unit.evidence_left_out
                        .as_ref()
                        .is_some_and(|left_out| left_out.coordinates.contains(&coordinate))
                }),
                "{position} {name}: nothing silent: {:?}",
                admission.units
            );
            for unit in recorded(&admission) {
                let left_out = unit.evidence_left_out.as_ref().unwrap();
                assert!(!left_out.nodes.is_empty(), "{position}: names its node");
                assert!(
                    matches!(
                        unit.disposition,
                        SemanticUnitDisposition::Admitted(_)
                            | SemanticUnitDisposition::ConfigurationDeclaration { .. }
                    ),
                    "{position}: only read units record"
                );
            }
        }
    }
}

/// A unit with the record round-trips through every codec and its identifier
/// covers the record.
#[test]
fn a_unit_with_evidence_left_out_round_trips_through_every_codec() {
    let theory = compile(&format!(
        "{PREFIXES}ex:claim rdf:reifies <<( ex:a ex:p ex:b )>> ; logic:provenance ex:report ."
    ));
    let publication = CanonAdmissionPublication::from_compiled_source(&theory);
    let unit = recorded(publication.admission())
        .into_iter()
        .next()
        .expect("one recorded unit")
        .clone();

    let json = purrdf_lex::json::write_compact(&unit.to_json());
    assert!(json.contains(r#""evidence_left_out":{"coordinates":["provenance"],"nodes":["#));
    let from_json = SemanticUnitAdmission::from_json(&purrdf_lex::json::read(&json).unwrap())
        .expect("hand-written JSON round-trips");
    assert_eq!(from_json, unit);
    assert_eq!(serde_json::to_string(&unit).unwrap(), json, "serde agrees");
    let from_serde: SemanticUnitAdmission = serde_json::from_str(&json).unwrap();
    assert_eq!(from_serde, unit);

    let bytes = publication.to_bytes().unwrap();
    let loaded = CanonAdmissionPublication::from_bytes(&bytes).expect("canon admission JSON");
    assert_eq!(loaded.to_bytes().unwrap(), bytes);
    assert_eq!(
        recorded(loaded.admission()),
        recorded(publication.admission())
    );

    // The record is part of the identifier: the same unit without it differs.
    let mut stripped = unit.clone();
    stripped.evidence_left_out = None;
    assert_ne!(stripped.id, unit.id);
    let other = compile(&format!(
        "{PREFIXES}ex:claim rdf:reifies <<( ex:a ex:p ex:b )>> ; logic:confidence 0.7 ."
    ));
    let other = CanonAdmissionPublication::from_compiled_source(&other);
    assert_ne!(
        recorded(other.admission())[0].id,
        unit.id,
        "different evidence, different identifier"
    );
}

/// INVARIANT: the scope rule is stated once for scopes and once for edges, and the
/// two agree on every one of the six coordinates.
#[test]
fn the_scope_rule_and_the_edge_rule_agree_on_every_coordinate() {
    let logic = "https://blackcatinformatics.ca/logic/";
    let scope = |apply: fn(&mut ContextualScope)| {
        let mut scope = ContextualScope::default();
        apply(&mut scope);
        scope
    };
    let cases: [(&str, ContextualScope, bool); 6] = [
        (
            "standpoint",
            scope(|s| s.standpoint = Some("urn:s".into())),
            false,
        ),
        ("time", scope(|s| s.time = Some("2026".into())), false),
        (
            "modality",
            scope(|s| s.modality = LogicModality::Epistemic),
            false,
        ),
        (
            "inModule",
            scope(|s| s.module = Some("urn:m".into())),
            false,
        ),
        (
            "provenance",
            scope(|s| s.provenance = Some("urn:p".into())),
            true,
        ),
        ("confidence", scope(|s| s.confidence = Some(0.5)), true),
    ];
    for (local, scope, evidence) in cases {
        let iri = format!("{logic}{local}");
        let role = StructuralSourceGraph::predicate_role(&iri).expect("a scope predicate");
        let edge = StructuralSourceGraph::edge_scope(role, &iri);
        assert_eq!(scope.selects_no_context(), evidence, "{local}");
        match edge {
            EdgeScope::Evidence(coordinate) => {
                assert!(evidence, "{local}");
                assert_eq!(scope.evidence_coordinates(), BTreeSet::from([coordinate]));
                assert!(scope.context_coordinates().is_empty());
            }
            EdgeScope::Context(name) => {
                assert!(!evidence, "{local}");
                assert_eq!(scope.context_coordinates(), vec![name.as_str()]);
            }
            EdgeScope::None => panic!("{local} is a scope coordinate"),
        }
    }
    assert_eq!(
        StructuralSourceGraph::edge_scope(SourceEdgeRole::FormulaComponent, "urn:x"),
        EdgeScope::None
    );
}
