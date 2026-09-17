// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Admission contracts for GMEOW correspondence fields and selected leg programs.

use super::*;
use crate::frontend::{OwnerDisposition, OwnerFamily, PreparedLogicSource};
use std::collections::BTreeSet;

const CORR: &str = "https://example.org/corr";
const BASE: &str = "ex:corr a logic:Correspondence ; logic:correspondenceRelation logic:Subsumes ; logic:morphismClass logic:LossyLens ; logic:morphismKind logic:InstitutionMorphism .";

fn source(extra: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    purrdf::parse_dataset(format!(r#"
        @prefix ex: <https://example.org/> .
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix gm: <https://blackcatinformatics.ca/gmeow/> .
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
        {BASE}
        logic:correspondence-program logic:hasPreservation logic:ExactPreservation ; logic:hasCorrespondence ex:corr .
        {extra}
    "#).as_bytes(), "text/turtle", None).unwrap()
}

#[test]
fn authored_caveats_survive_compilation_and_each_common_logic_projection() {
    let dataset = source(
        r#"
        ex:corr logic:hasCaveat ex:zCaveat, ex:aCaveat .
        ex:zCaveat rdfs:comment "Limited to the declared standpoint." .
        ex:aCaveat rdfs:comment "Overlap does not assert entity equivalence."@en,
            "Le chevauchement ne signifie pas une équivalence."@fr,
            "تحذير"@ar--rtl, "0001"^^ex:noteKind .
    "#,
    );
    let prepared = PreparedLogicSource::new(&dataset).unwrap();
    let compiled = prepared.compile_with_sources(None).unwrap();
    let expected = parse_correspondence(&dataset).unwrap().correspondences;
    assert_eq!(compiled.program().correspondences, expected);
    assert_eq!(expected[0].caveats.len(), 2);
    assert!(expected[0].caveats[0].iri.ends_with("aCaveat"));
    let comments = &expected[0].caveats[0].comments;
    assert_eq!(comments.len(), 4);
    assert!(comments.iter().any(|comment| comment.lexical_form == "0001"
        && comment.datatype_iri() == "https://example.org/noteKind"));
    assert!(
        comments
            .iter()
            .any(|comment| comment.language.as_deref() == Some("fr"))
    );
    assert!(
        comments
            .iter()
            .any(|comment| comment.language.as_deref() == Some("ar")
                && comment.direction == Some(purrdf::RdfTextDirection::Rtl))
    );

    // Use only the typed correspondence: generic source axioms must not act as a
    // second copy that conceals a missing caveat in the shared IR.
    let program = crate::ir::LogicProgram::new(vec![], vec![], vec![], None)
        .with_correspondences(expected.clone())
        .unwrap();
    let cached: crate::ir::LogicProgram =
        serde_json::from_slice(&serde_json::to_vec(&program).unwrap()).unwrap();
    assert_eq!(cached, program);
    let clif = crate::clif::project_clif(&program).unwrap();
    let cgif = crate::cgif::project_cgif(&program).unwrap();
    let xcl = crate::xcl::project_xcl(&program).unwrap();
    for (name, (restored, _)) in [
        (
            "CLIF",
            crate::clif::parse_clif_str(&clif.content, None).unwrap(),
        ),
        (
            "CGIF",
            crate::cgif::parse_cgif_str(&cgif.content, None).unwrap(),
        ),
        (
            "XCL",
            crate::xcl::parse_xcl_str(&xcl.content, None).unwrap(),
        ),
    ] {
        assert_eq!(restored.correspondences, expected, "{name}");
    }
}

#[test]
fn malformed_authored_caveats_reject_the_owner_without_poisoning_other_owners() {
    for extra in [
        "ex:corr logic:hasCaveat 7 .",
        "ex:corr logic:hasCaveat ex:caveat .",
        "ex:corr logic:hasCaveat ex:caveat . ex:caveat rdfs:comment ex:notText .",
    ] {
        let dataset = source(&format!(
            "{extra}\n{}",
            BASE.replace("ex:corr", "ex:healthy")
        ));
        let prepared = PreparedLogicSource::new(&dataset).unwrap();
        let compiled = prepared.compile_with_sources(None).unwrap();
        assert_eq!(compiled.program().correspondences.len(), 1, "{extra}");
        assert!(
            compiled.program().correspondences[0]
                .iri
                .ends_with("healthy")
        );
        assert!(
            compiled.owner_lowerings().iter().any(|owner| {
                owner.family == OwnerFamily::Correspondence
                    && owner.disposition == OwnerDisposition::Rejected
                    && !owner.diagnostics.is_empty()
            }),
            "{extra}"
        );
        let (correspondences, errors) = extract_correspondences(&dataset);
        assert_eq!(correspondences, compiled.program().correspondences);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, CORR);
    }
}

#[test]
fn caveat_identity_is_owned_and_canonical() {
    let mut baseline = parse_correspondence(&source(""))
        .unwrap()
        .correspondences
        .remove(0);
    let first = CorrespondenceCaveat {
        iri: format!("{CORR}/a"),
        comments: vec![purrdf::RdfLiteral::simple("A limitation")],
    };
    let second = CorrespondenceCaveat {
        iri: format!("{CORR}/b"),
        comments: vec![purrdf::RdfLiteral::simple("Another limitation")],
    };
    baseline = baseline
        .with_caveats(vec![first.clone(), second.clone()])
        .unwrap();
    let reordered = baseline
        .clone()
        .with_caveats(vec![second, first.clone()])
        .unwrap();
    assert_eq!(baseline.content_key(), reordered.content_key());
    assert!(
        baseline
            .clone()
            .with_caveats(vec![first.clone(), first])
            .is_err()
    );
    let original = crate::ir::LogicProgram::new(vec![], vec![], vec![], None)
        .with_correspondences(vec![baseline.clone()])
        .unwrap();
    baseline.caveats[0].comments[0]
        .lexical_form
        .push_str(" under a narrower context");
    let changed = crate::ir::LogicProgram::new(vec![], vec![], vec![], None)
        .with_correspondences(vec![baseline])
        .unwrap();
    assert_ne!(original.canonical_key(), changed.canonical_key());
}

#[test]
fn every_caveat_literal_component_participates_in_ir_and_cache_identity() {
    let mut keys = BTreeSet::new();
    for suffix in [
        "",
        "@en",
        "@fr",
        "@ar--ltr",
        "@ar--rtl",
        "^^ex:firstType",
        "^^ex:secondType",
    ] {
        let dataset = source(&format!(
            "ex:corr logic:hasCaveat ex:caveat . ex:caveat rdfs:comment \"same lexical value\"{suffix} ."
        ));
        let program = parse_correspondence(&dataset).unwrap();
        assert!(keys.insert(program.content_key()), "{suffix}");
        let restored = parse_correspondence(
            &purrdf::parse_dataset(
                project_correspondence(&program).as_bytes(),
                "application/n-triples",
                None,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(restored, program, "{suffix}");
        let restored: CorrespondenceProgram =
            serde_json::from_slice(&serde_json::to_vec(&program).unwrap()).unwrap();
        assert_eq!(restored, program, "{suffix}");
    }
}

#[test]
fn invalid_caveat_literal_components_fail_typed_and_cache_admission() {
    let dataset =
        source("ex:corr logic:hasCaveat ex:caveat . ex:caveat rdfs:comment \"warning\"@ar--rtl .");
    let program = parse_correspondence(&dataset).unwrap();
    let valid = serde_json::to_value(&program).unwrap();
    for (slot, value) in [
        (3, serde_json::json!("unknown")),
        (2, serde_json::Value::Null),
        (1, serde_json::json!(XSD_BOOLEAN)),
    ] {
        let mut invalid = valid.clone();
        invalid["correspondences"][0]["caveats"][0]["comments"][0][slot] = value;
        assert!(serde_json::from_value::<CorrespondenceProgram>(invalid).is_err());
    }
    let mut caveat = program.correspondences[0].caveats[0].clone();
    caveat.comments[0].language = None;
    assert!(
        program.correspondences[0]
            .clone()
            .with_caveats(vec![caveat])
            .is_err()
    );
    let mut empty = valid;
    empty["correspondences"][0]["caveats"][0]["comments"] = serde_json::json!([]);
    assert!(serde_json::from_value::<CorrespondenceProgram>(empty).is_err());
}

#[test]
fn malformed_optional_axes_reject_the_owner_instead_of_becoming_absent() {
    for (predicate, value) in [
        ("logic:confidence", "ex:notNumeric"),
        ("logic:confidence", "\"not a number\""),
        ("logic:evidenceStrength", "\"NaN\""),
        ("logic:weight", "\"INF\""),
        ("logic:probability", "\"not a probability\""),
        ("logic:mnemomorphic", "\"maybe\""),
        ("logic:mnemomorphic", "2"),
        ("logic:hasDeterminacy", "logic:UnrecognizedDeterminacy"),
        ("logic:preservationKind", "logic:UnrecognizedPreservation"),
        (
            "logic:preservationKind",
            "<https://example.org/ExactPreservation>",
        ),
        ("logic:getLeg", "\"not a program IRI\""),
        ("gm:accordingTo", "\"not a standpoint IRI\""),
    ] {
        let dataset = source(&format!("ex:corr {predicate} {value} ."));
        let prepared = PreparedLogicSource::new(&dataset).unwrap();
        let compiled = prepared.compile_with_sources(None).unwrap();
        assert!(
            compiled.program().correspondences.is_empty(),
            "{predicate} {value}"
        );
        let owner = compiled
            .owner_lowerings()
            .iter()
            .find(|owner| owner.family == OwnerFamily::Correspondence)
            .unwrap();
        assert_eq!(
            owner.disposition,
            OwnerDisposition::Rejected,
            "{predicate} {value}"
        );
        assert!(owner.diagnostics.iter().any(|&index| {
            compiled.diagnostics()[index]
                .message
                .contains(predicate.split_once(':').unwrap().1)
        }));
        assert!(
            parse_correspondence(&dataset).is_err(),
            "cache rederivation must reject {predicate} {value}"
        );
    }
}

#[test]
fn conflicting_scalar_values_cannot_select_a_winning_axis() {
    for extra in [
        "ex:corr logic:confidence 0.25, 0.75 .",
        "ex:corr gm:accordingTo ex:first, ex:second .",
        "ex:corr logic:mnemomorphic true, false .",
        "ex:corr logic:getLeg ex:first, ex:second .",
        "ex:corr logic:correspondenceRelation logic:RelatedMatch .",
        "logic:correspondence-program logic:hasPreservation logic:ValidationOnly .",
    ] {
        let error = parse_correspondence(&source(extra)).unwrap_err();
        assert!(
            error.message().contains("at most one distinct value"),
            "{extra}: {error}"
        );
    }
}

#[test]
fn every_law_and_registry_member_is_required() {
    for extra in [
        "ex:corr logic:hasLawClaim \"not an IRI\" .",
        "ex:corr logic:recoveryCase \"not an IRI\" .",
        "ex:corr logic:hasCaveat \"not an IRI\" .",
        "logic:correspondence-program logic:hasCorrespondence \"not an IRI\" .",
    ] {
        let error = parse_correspondence(&source(extra)).unwrap_err();
        assert!(
            error.message().contains("non-IRI member"),
            "{extra}: {error}"
        );
    }
    for extra in [
        "ex:corr logic:hasLawClaim ex:claim . ex:claim logic:lawClaimed logic:GetPut ; logic:lawDischargeVerdict logic:ObligationUnknown ; logic:lawDischargeCondition logic:UnknownCondition .",
        "ex:corr logic:hasLawClaim ex:claim . ex:claim logic:lawClaimed ex:GetPut ; logic:lawDischargeVerdict logic:ObligationUnknown .",
        "ex:corr logic:hasLawClaim ex:claim . ex:claim logic:lawClaimed logic:GetPut ; logic:lawDischargeVerdict ex:ObligationDischarged .",
        "ex:corr logic:hasCaveat ex:caveat . ex:caveat rdfs:comment ex:missingText .",
    ] {
        assert!(parse_correspondence(&source(extra)).is_err(), "{extra}");
    }
}

#[test]
fn absent_axes_and_valid_boolean_lexicals_preserve_declared_semantics() {
    let absent = parse_correspondence(&source("")).unwrap();
    assert_eq!(absent.correspondences[0].confidence, None);
    assert_eq!(absent.correspondences[0].preservation, None);
    assert!(!absent.correspondences[0].mnemomorphic);
    for (lexical, expected) in [("true", true), ("1", true), ("false", false), ("0", false)] {
        let program = parse_correspondence(&source(&format!(
            "ex:corr logic:mnemomorphic \"{lexical}\"^^xsd:boolean ."
        )))
        .unwrap();
        assert_eq!(program.correspondences[0].mnemomorphic, expected);
    }
    let program = parse_correspondence(&source("ex:corr logic:confidence 0.25 ; logic:evidenceStrength 0.5 ; logic:weight -2.0 ; logic:probability 0.75 .")).unwrap();
    let corr = &program.correspondences[0];
    assert_eq!(
        corr.confidence.as_ref().unwrap().literal().lexical_form,
        "0.25"
    );
    assert_eq!(
        corr.evidence_strength
            .as_ref()
            .unwrap()
            .literal()
            .lexical_form,
        "0.5"
    );
    assert_eq!(corr.weight.as_ref().unwrap().literal().lexical_form, "-2.0");
    assert_eq!(
        corr.probability.as_ref().unwrap().literal().lexical_form,
        "0.75"
    );
}

#[test]
fn native_duplicate_rows_are_one_field_value_without_a_lexical_index() {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    let corr = builder.intern_iri(CORR);
    let confidence = builder.intern_iri(&p_confidence());
    let value = builder.intern_literal(purrdf::RdfLiteral::typed(
        "0.25".to_owned(),
        XSD_DECIMAL.to_owned(),
    ));
    builder.push_quad(corr, confidence, value, None);
    builder.push_annotation(corr, confidence, value);
    let dataset = builder.freeze().unwrap();
    let view = CorrespondenceView::from_dataset(&dataset);
    assert_eq!(
        view.numeric_obj::<true>(CORR, &p_confidence())
            .unwrap()
            .unwrap()
            .literal()
            .lexical_form,
        "0.25"
    );
}

#[test]
fn malformed_path_members_cannot_shorten_a_selected_leg() {
    for body in [
        "ex:root a gm:SeqPath ; gm:pathSteps ex:cell . ex:cell gm:pathItem ex:p ; gm:pathNext \"bad tail\" .",
        "ex:root a gm:SeqPath ; gm:pathSteps ex:cell . ex:cell gm:pathItem ex:p ; gm:pathNext ex:cell .",
        "ex:root a gm:InversePath ; gm:pathStep ex:root .",
        "ex:root a gm:InversePath, gm:SeqPath ; gm:pathStep ex:p .",
        "ex:root a gm:InversePath ; gm:pathStep ex:p, ex:q .",
        "ex:root a gm:SeqPath ; gm:pathSteps ex:cell . ex:cell gm:pathItem \"bad item\" .",
        "ex:root a gm:SeqPath .",
        "ex:root a gm:UnimplementedPath .",
        "ex:root gm:pathSteps ex:cell . ex:cell gm:pathItem ex:p .",
    ] {
        let dataset = source(&format!(
            "ex:corr logic:getLeg ex:leg . ex:leg gm:path ex:root . {body}"
        ));
        let prepared = PreparedLogicSource::new(&dataset).unwrap();
        let compiled = prepared.compile_with_sources(None).unwrap();
        assert!(compiled.program().transaction_programs.is_empty(), "{body}");
        assert!(
            parse_correspondence(&dataset).is_err(),
            "the correspondence cache/parser must reject the same malformed selected path: {body}"
        );
        let rejected = compiled
            .owner_lowerings()
            .iter()
            .find(|owner| owner.family == OwnerFamily::TransactionProgram)
            .unwrap();
        assert_eq!(rejected.disposition, OwnerDisposition::Rejected, "{body}");
        assert!(!rejected.diagnostics.is_empty());
        assert!(
            extract_leg_programs(&dataset, &compiled.program().correspondences).is_err(),
            "{body}"
        );
    }
}

#[test]
fn nested_leg_order_and_all_selected_programs_survive_strict_reads() {
    let dataset = source(
        "ex:corr logic:getLeg ex:leg ; logic:putLeg ex:leg .
        ex:leg gm:path ex:root . ex:root a gm:SeqPath ; gm:pathSteps ex:first .
        ex:first gm:pathItem ex:p ; gm:pathNext ex:last . ex:last gm:pathItem ex:inverse .
        ex:inverse a gm:InversePath ; gm:pathStep ex:q .",
    );
    let (correspondences, errors) = extract_correspondences(&dataset);
    assert!(errors.is_empty());
    let legs = extract_leg_programs(&dataset, &correspondences).unwrap();
    assert_eq!(legs.len(), 1);
    assert_eq!(
        legs[0].body,
        LegPath::Seq(vec![
            LegPath::Step("https://example.org/p".into()),
            LegPath::Inverse(Box::new(LegPath::Step("https://example.org/q".into())))
        ])
    );
    let missing = source("ex:corr logic:getLeg ex:missing .");
    let (correspondences, _) = extract_correspondences(&missing);
    assert!(extract_leg_programs(&missing, &correspondences).is_err());
}

#[test]
fn correspondence_projection_round_trips_present_leg_bodies_and_symbolic_legs() {
    let dataset = source(
        "ex:corr logic:getLeg ex:get ; logic:putLeg ex:symbolicPut .
        ex:get gm:path ex:root . ex:root a gm:SeqPath ; gm:pathSteps ex:first .
        ex:first gm:pathItem ex:p ; gm:pathNext ex:last .
        ex:last gm:pathItem ex:inverse .
        ex:inverse a gm:InversePath ; gm:pathStep ex:q .",
    );
    let program = parse_correspondence(&dataset).unwrap();
    assert_eq!(program.leg_programs.len(), 1);
    assert_eq!(program.leg_programs[0].iri, "https://example.org/get");
    assert_eq!(
        program.leg_programs[0].body,
        LegPath::Seq(vec![
            LegPath::Step("https://example.org/p".into()),
            LegPath::Inverse(Box::new(LegPath::Step("https://example.org/q".into())))
        ])
    );
    assert_eq!(
        program.correspondences[0].put_leg.as_deref(),
        Some("https://example.org/symbolicPut")
    );

    let projected = project_correspondence_dataset(&program).unwrap();
    assert_eq!(parse_correspondence(&projected).unwrap(), program);
}

#[test]
fn correspondence_weight_retains_declared_double_domain() {
    for lexical in ["1E-100", "1E100", "5E-324"] {
        let value = crate::ir::FiniteNumericLiteral::new(purrdf::RdfLiteral::typed(
            lexical.to_owned(),
            "http://www.w3.org/2001/XMLSchema#double".to_owned(),
        ))
        .unwrap();
        let mut program = parse_correspondence(&source("")).unwrap();
        program.correspondences[0].weight = Some(value.clone());
        let rendered = project_correspondence(&program);
        let dataset =
            purrdf::parse_dataset(rendered.as_bytes(), "application/n-triples", None).unwrap();
        let parsed = parse_correspondence(&dataset).unwrap();
        assert_eq!(parsed.correspondences[0].weight, Some(value));
    }
}

/// Cache ingress must enforce the same coordinate contract as authored RDF ingress.
#[test]
fn quantitative_cache_admission_cannot_bypass_datatype_or_axis_range() {
    let dataset = source(
        "ex:corr logic:confidence 0.5 ; logic:evidenceStrength 0.5 ; logic:probability 0.5 ; logic:weight -2.0 .",
    );
    let correspondence = parse_correspondence(&dataset)
        .unwrap()
        .correspondences
        .remove(0);
    let original = serde_json::to_value(&correspondence).unwrap();
    for field in ["confidence", "evidence_strength", "probability", "weight"] {
        for (lexical, datatype) in [
            ("0.5", "http://www.w3.org/2001/XMLSchema#string"),
            ("INF", "http://www.w3.org/2001/XMLSchema#double"),
            ("0.5", "https://example.org/unknownNumericType"),
        ] {
            let mut wire = original.clone();
            wire[field] = serde_json::json!([lexical, datatype, null, null]);
            assert!(
                serde_json::from_value::<Correspondence>(wire).is_err(),
                "{field} {datatype}"
            );
        }
    }
    for field in ["confidence", "evidence_strength", "probability"] {
        let mut wire = original.clone();
        // Rounding this decimal through binary64 would incorrectly admit it as 1.
        wire[field] = serde_json::json!(["1.000000000000000001", XSD_DECIMAL, null, null]);
        assert!(
            serde_json::from_value::<Correspondence>(wire).is_err(),
            "{field}"
        );
    }
}

#[test]
fn correspondence_source_coordinates_require_numeric_datatypes_and_exact_range() {
    for value in [
        "\"0.5\"",
        "\"0.5\"@en",
        "\"0.5\"^^ex:numeric",
        "1.000000000000000001",
        "\"0.5\"^^xsd:boolean",
    ] {
        let dataset = source(&format!("ex:corr logic:confidence {value} ."));
        assert!(parse_correspondence(&dataset).is_err(), "{value}");
        let prepared = PreparedLogicSource::new(&dataset).unwrap();
        let compiled = prepared.compile_with_sources(None).unwrap();
        assert!(compiled.program().correspondences.is_empty(), "{value}");
        assert!(
            compiled
                .owner_lowerings()
                .iter()
                .any(|owner| owner.disposition == OwnerDisposition::Rejected),
            "{value}"
        );
    }
}
