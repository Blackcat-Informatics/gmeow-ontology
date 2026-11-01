// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic::reason::refute::{FragmentBoundary, FragmentFamily};
use purrdf::{BlankScope, TermValue};
use purrdf_lex::json::record::{from_slice, to_vec};

const WIRE: &str = r#"{"input_blake3":[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31],"admission":{"contract":"retained raw contract","source_worlds":{"raw":{"graph":null,"assertions":18446744073709551615}},"selected_worlds":{"raw":{"graph":{"Blank":{"label":"same","scope":17}},"definitions":[{"subject":{"Triple":{"s":{"Blank":{"label":"same","scope":17}},"p":{"Iri":"urn:p"},"o":{"Triple":{"s":{"Blank":{"label":"same","scope":19}},"p":{"Iri":"urn:q"},"o":{"Literal":{"lexical_form":"01","datatype":"urn:raw","language":"MiXeD","direction":"Rtl"}}}}}},"predicate":"urn:definition","object":{"Iri":"urn:operand"},"graph":null}],"refusal":{"Combined":[{"SourceAdmission":{"world":"raw","premises":[],"issue":"MalformedNil"}},{"Uncertified":{"family":"DatatypeValueSpace","obstructions":["a","z"]}},{"Uncertified":{"family":"Counting","obstructions":["count"]}},{"Uncertified":{"family":"CaseSplit","obstructions":["split"]}}]}}}}}"#;

#[test]
fn native_class_source_wire_retains_all_boundary_families_and_raw_quoted_identity() {
    let observation: SourceAdmissionObservation = from_slice(WIRE.as_bytes()).unwrap();
    assert_eq!(to_vec(&observation), WIRE.as_bytes());
    let expected_digest: [u8; 32] = std::array::from_fn(|index| u8::try_from(index).unwrap());
    assert_eq!(observation.input_blake3, expected_digest);
    assert_eq!(
        observation.admission.source_worlds["raw"].assertions,
        u64::MAX
    );
    assert!(observation.admission.source_worlds["raw"].graph.is_none());
    let selected = &observation.admission.selected_worlds["raw"];
    assert_eq!(
        selected.graph,
        Some(TermValue::Blank {
            label: "same".into(),
            scope: BlankScope(17)
        })
    );
    let TermValue::Triple { s, o, .. } = &selected.definitions[0].subject else {
        panic!("retained statement")
    };
    assert_eq!(
        s.as_ref(),
        &TermValue::Blank {
            label: "same".into(),
            scope: BlankScope(17)
        }
    );
    let TermValue::Triple { s, o, .. } = o.as_ref() else {
        panic!("retained nested statement")
    };
    assert_eq!(
        s.as_ref(),
        &TermValue::Blank {
            label: "same".into(),
            scope: BlankScope(19)
        }
    );
    let TermValue::Literal {
        lexical_form,
        datatype,
        language,
        direction,
    } = o.as_ref()
    else {
        panic!("retained literal")
    };
    assert_eq!(lexical_form, "01");
    assert_eq!(datatype, "urn:raw");
    assert_eq!(language.as_deref(), Some("MiXeD"));
    assert_eq!(*direction, Some(purrdf::RdfTextDirection::Rtl));
    let FragmentBoundary::Combined(boundaries) = selected.refusal.as_ref().unwrap() else {
        panic!("complete boundary set")
    };
    assert_eq!(boundaries.len(), 4);
    let families = boundaries
        .iter()
        .filter_map(|boundary| match boundary {
            FragmentBoundary::Uncertified { family, .. } => Some(*family),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        families,
        [
            FragmentFamily::DatatypeValueSpace,
            FragmentFamily::Counting,
            FragmentFamily::CaseSplit
        ]
    );
    // Hydration preserves evidence; the existing consumer separately validates scope.
    assert!(observation.admission.validate().is_err());
    let omitted_refusal = WIRE.replace(",\"refusal\":{\"Combined\":[{\"SourceAdmission\":{\"world\":\"raw\",\"premises\":[],\"issue\":\"MalformedNil\"}},{\"Uncertified\":{\"family\":\"DatatypeValueSpace\",\"obstructions\":[\"a\",\"z\"]}},{\"Uncertified\":{\"family\":\"Counting\",\"obstructions\":[\"count\"]}},{\"Uncertified\":{\"family\":\"CaseSplit\",\"obstructions\":[\"split\"]}}]}", "");
    let absent: SourceAdmissionObservation = from_slice(omitted_refusal.as_bytes()).unwrap();
    assert!(absent.admission.selected_worlds["raw"].refusal.is_none());
    let explicit_null =
        omitted_refusal.replace("\"definitions\":", "\"refusal\":null,\"definitions\":");
    assert_eq!(
        from_slice::<SourceAdmissionObservation>(explicit_null.as_bytes()).unwrap(),
        absent
    );
}

#[test]
fn native_class_source_admission_refuses_digest_record_and_shadowed_map_drift() {
    let unknown = WIRE
        .replace("\"input_blake3\":", "\"future\":null,\"input_blake3\":")
        .replace("\"contract\":", "\"future\":null,\"contract\":")
        .replace("\"graph\":", "\"future\":null,\"graph\":")
        .replace("\"world\":", "\"future\":null,\"world\":")
        .replace("\"family\":", "\"future\":null,\"family\":");
    assert_eq!(
        to_vec(&from_slice::<SourceAdmissionObservation>(unknown.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
    let shadow = WIRE.replace("\"source_worlds\":{", "\"source_worlds\":{\"raw\":{\"graph\":null,\"assertions\":0},")
        .replace("[\"a\",\"z\"]", "[\"z\",\"a\",\"z\"]")
        .replace("\"Combined\":[", "\"Combined\":[{\"Uncertified\":{\"family\":\"CaseSplit\",\"obstructions\":[\"split\"]}},");
    assert_eq!(
        to_vec(&from_slice::<SourceAdmissionObservation>(shadow.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
    for invalid in [
        WIRE.replace("\"input_blake3\":", "\"input_blake3\":[],\"input_blake3\":"),
        WIRE.replace("30,31]", "30]"),
        WIRE.replace("30,31]", "30,31,32]"),
        WIRE.replace("[0,1,2,3", "[256,1,2,3"),
        WIRE.replace("[0,1,2,3", "[-1,1,2,3"),
        WIRE.replace("[0,1,2,3", "[0e0,1,2,3"),
        WIRE.replace(
            "\"assertions\":18446744073709551615",
            "\"assertions\":18446744073709551616",
        ),
        WIRE.replace(
            "\"source_worlds\":{",
            "\"source_worlds\":{\"raw\":{\"graph\":null,\"assertions\":false},",
        ),
        WIRE.replace(
            "\"selected_worlds\":{",
            "\"selected_worlds\":{\"raw\":{\"graph\":null,\"definitions\":false},",
        ),
        WIRE.replace("\"graph\":null,", ""),
        WIRE.replace(
            "\"graph\":{\"Blank\":{\"label\":\"same\",\"scope\":17}},",
            "",
        ),
        WIRE.replace(",\"graph\":null", ""),
        WIRE.replace("\"contract\":", "\"contract\":\"earlier\",\"contract\":"),
        WIRE.replace("\"predicate\":\"urn:definition\",", ""),
        WIRE.replace("\"refusal\":", "\"refusal\":null,\"refusal\":"),
        WIRE.replace(
            "\"SourceAdmission\":",
            "\"SourceAdmission\":null,\"Uncertified\":",
        ),
        WIRE.replace("\"DatatypeValueSpace\"", "\"Unknown\""),
        WIRE.replace("\"premises\":[],", ""),
        r#"[[],{"contract":"raw","source_worlds":{},"selected_worlds":{}}]"#.to_owned(),
    ] {
        assert!(
            from_slice::<SourceAdmissionObservation>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}
