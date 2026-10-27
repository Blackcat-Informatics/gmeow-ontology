// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored evidence transport; no source loading or execution.
use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const WIRE: &str = r#"{"iri":"","relation":"Equiv","morphism_class":"LossyLens","morphism_kind":"CommitmentShiftingBridge","mnemomorphic":false,"determinacy":null,"get_leg":"","put_leg":null,"law_claims":[{"law":"SectionLaw","verdict":"ObligationUnknown","condition":null},{"law":"GetPut","verdict":"ObligationViolated","condition":"DischargeBoundedCorpus"},{"law":"SectionLaw","verdict":"ObligationUnknown","condition":null}],"caveats":[{"iri":"","comments":[["note","http://www.w3.org/1999/02/22-rdf-syntax-ns#langString","EN",null],["note","http://www.w3.org/1999/02/22-rdf-syntax-ns#langString","en",null]]}],"loss_evidence":[],"confidence":["0.90","http://www.w3.org/2001/XMLSchema#decimal",null,null],"evidence_strength":null,"axis_evidence":{"sources":["z","a","z"],"scale":"","probability_model":null},"weight":["-2.5","http://www.w3.org/2001/XMLSchema#decimal",null,null],"probability":null,"according_to":"","preservation":null,"source_endpoint":null,"target_endpoint":null,"grounding":false,"recovery_cases":[{"iri":"","transform":{"And":[]}}]}"#;

#[test]
fn native_correspondence_preserves_complete_wire_and_raw_evidence() {
    let native: Correspondence = from_slice(WIRE.as_bytes()).unwrap();
    let retained: Correspondence = serde_json::from_str(WIRE).unwrap();
    assert_eq!(native, retained);
    assert_eq!(to_vec(&native), serde_json::to_vec(&retained).unwrap());
    assert_eq!(native.axis_evidence.sources, ["z", "a", "z"]);
    assert_eq!(native.law_claims.len(), 3);
    assert_eq!(native.law_claims[0], native.law_claims[2]);
    assert_eq!(native.caveats[0].comments.len(), 1);
    assert_eq!(
        native.caveats[0].comments[0].language.as_deref(),
        Some("en")
    );
    assert!(native.loss_evidence.is_empty());
    assert_eq!(native.confidence.unwrap().literal().lexical_form, "0.90");

    let legs = br#"{"Seq":[{"Step":""},{"Inverse":{"Alt":[{"Step":"opaque"},{"Seq":[]}]}}]}"#;
    let native: LegPath = from_slice(legs).unwrap();
    let retained: LegPath = serde_json::from_slice(legs).unwrap();
    assert_eq!(native, retained);
    assert_eq!(to_vec(&native), legs);
}

fn check_unit_tags<T: FromJson + ToJson + serde::de::DeserializeOwned + serde::Serialize>(
    tags: &[&str],
) {
    for tag in tags {
        let wire = format!("\"{tag}\"");
        let native: T = from_slice(wire.as_bytes()).unwrap();
        let retained: T = serde_json::from_str(&wire).unwrap();
        assert_eq!(to_vec(&native), serde_json::to_vec(&retained).unwrap());
        let object = format!("{{\"{tag}\":null}}");
        assert_eq!(
            to_vec(&from_slice::<T>(object.as_bytes()).unwrap()),
            wire.as_bytes()
        );
    }
}

#[test]
fn native_correspondence_admits_every_unit_tag_and_refuses_ambiguous_evidence() {
    check_unit_tags::<CorrespondenceRelation>(&[
        "Equiv",
        "Subsumes",
        "SubsumedBy",
        "Overlaps",
        "RelatedMatch",
        "Disjoint",
    ]);
    check_unit_tags::<MorphismClass>(&[
        "Isomorphism",
        "SectionRetraction",
        "WellBehavedLens",
        "LossyLens",
        "Prism",
        "AffineCorrespondence",
        "BridgeView",
    ]);
    check_unit_tags::<MorphismKind>(&["InstitutionMorphism", "CommitmentShiftingBridge"]);
    check_unit_tags::<Determinacy>(&["Crisp", "Vague"]);
    check_unit_tags::<CorrespondenceLaw>(&["GetPut", "PutGet", "PutPut", "SectionLaw"]);
    check_unit_tags::<DischargeVerdict>(&[
        "ObligationDischarged",
        "ObligationUnknown",
        "ObligationViolated",
    ]);
    check_unit_tags::<DischargeCondition>(&[
        "DischargeCertifiedFragment",
        "DischargeFiniteClosure",
        "DischargeSyntacticReachability",
        "DischargeConservativeExtension",
        "DischargeBoundedCorpus",
    ]);
    let future = WIRE.replacen("{", r#"{"future":[],"#, 1);
    assert!(from_slice::<Correspondence>(future.as_bytes()).is_ok());
    let omitted_options = WIRE
        .replace(r#""put_leg":null,"#, "")
        .replace(r#""determinacy":null,"#, "")
        .replace(r#","condition":null"#, "");
    let decoded: Correspondence = from_slice(omitted_options.as_bytes()).unwrap();
    assert!(decoded.put_leg.is_none());
    assert!(decoded.determinacy.is_none());
    assert!(decoded.law_claims[0].condition.is_none());
    for bad in [
        WIRE.replacen(r#""iri":"""#, r#""iri":"","iri":"other""#, 1),
        WIRE.replace(r#""mnemomorphic":false"#, r#""mnemomorphic":null"#),
        WIRE.replace(r#""loss_evidence":[]"#, r#""loss_evidence":null"#),
        WIRE.replace(
            r#""axis_evidence":{"sources""#,
            r#""axis_evidence":{"sources":[],"sources""#,
        ),
        WIRE.replace(
            r#""condition":null"#,
            r#""condition":{"DischargeFiniteClosure":false}"#,
        ),
        WIRE.replace(r#""relation":"Equiv""#, r#""relation":"Unknown""#),
        WIRE.replace(r#""grounding":false,"#, ""),
    ] {
        assert!(
            from_slice::<Correspondence>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
    assert!(from_slice::<CorrespondenceCaveat>(br#"{"iri":"raw","comments":[]}"#).is_err());
    for bad in [
        br#"{"Seq":[{"Step":false}]}"#.as_slice(),
        br#"{"Step":"p","Seq":[]}"#,
        br#"["Step","p"]"#,
    ] {
        assert!(from_slice::<LegPath>(bad).is_err());
    }
    let mut caveat: CorrespondenceCaveat = serde_json::from_str(r#"{"iri":"raw","comments":[["note","http://www.w3.org/1999/02/22-rdf-syntax-ns#langString","en",null]]}"#).unwrap();
    caveat.comments[0].datatype = Some("http://www.w3.org/2001/XMLSchema#integer".to_owned());
    let bytes = to_vec(&caveat);
    assert_eq!(bytes, serde_json::to_vec(&caveat).unwrap());
    assert!(
        String::from_utf8(bytes.clone())
            .unwrap()
            .contains("http://www.w3.org/2001/XMLSchema#integer")
    );
    assert!(from_slice::<CorrespondenceCaveat>(&bytes).is_err());
}
