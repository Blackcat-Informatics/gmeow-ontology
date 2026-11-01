// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn census_retains_statement_assertions_without_inferred_or_empty_worlds() {
    let dataset = dataset_from_bytes(
        br#"
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
            <urn:default-subject> <urn:relation> <urn:default-object> .
            _:private { <urn:private-subject> <urn:relation> <urn:private-object> . }
            <urn:empty> {}
            <urn:observed> {
                <urn:subject> <urn:relation> <urn:object> .
                <urn:claim> rdf:reifies <<( <urn:subject> <urn:relation> <urn:object> )>> ;
                    logic:standpoint <urn:observer> .
            }
            "#,
        NativeRdfFormat::TriG,
    )
    .unwrap();
    let observed = observe(&dataset).unwrap();
    assert_eq!(
        observed.world_counts,
        BTreeMap::from([("urn:observed".to_owned(), 3)])
    );
}

/// Retain complete historical artifact bytes and their actual content identity.
fn assert_reference(name: &str, bytes: &[u8]) {
    let reference = include_str!("consistency/fixtures/native-json-v1.txt")
        .lines()
        .find(|line| line.starts_with(&format!("{name} ")))
        .expect("complete original consistency reference");
    let mut fields = reference.split_whitespace();
    assert_eq!(fields.next(), Some(name));
    let sha256 = fields.next().unwrap();
    assert_eq!(
        bytes,
        purrdf_hash::hex::decode(fields.next().unwrap()).unwrap()
    );
    assert_eq!(purrdf::ContentDigest::of(bytes).to_hex(), sha256);
}

/// Preserve full consistency evidence and strict source-world counts across transport.
#[test]
fn serialized_observation_preserves_world_witnesses_and_identified_gaps() {
    // Authored metadata pins both actual transport profiles without corpus production.
    {
        use gmeow_logic::reason::dl::{
            DlCoverage, DlGap, DlVerdict, InconsistencyWitness, UnsatClass,
        };
        let text = "authored é中😀\n\"\\";
        let authored = Observation {
            verdict: DlVerdict {
                consistent: false,
                unsatisfiable_classes: vec![UnsatClass {
                    class: "urn:class".into(),
                    world: "urn:world".into(),
                    premises: vec![("urn:subject".into(), "urn:predicate".into(), text.into())],
                }],
                inconsistencies: vec![InconsistencyWitness {
                    individual: "urn:individual".into(),
                    world: "urn:world".into(),
                    premises: vec![("urn:individual".into(), "urn:predicate".into(), text.into())],
                }],
                coverage: DlCoverage {
                    present: vec!["present".into()],
                    decided: vec!["decided".into()],
                    unsupported: vec!["unsupported".into()],
                },
                gaps: vec![DlGap::new("authored.gap", text)],
                boundary_findings: vec![gmeow_errors::Finding::new(
                    gmeow_errors::Severity::Warning,
                    "authored.boundary",
                    text,
                )],
            },
            world_counts: BTreeMap::from([
                ("urn:z-world".into(), u64::MAX),
                ("urn:a-world".into(), 0),
            ]),
        };
        let refusal: gmeow_errors::RecordedDiag = purrdf_lex::json::record::from_slice(br#"{"root":{"fingerprint":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"stage":"authored","grade":{"severity":"error","category":"data-shape-violation","standpoint":"binding"},"code":"authored.refusal","observations":[],"frames":[],"antecedents":[],"source_ctx":{"location":{},"focus":null,"term_role":null},"attributions":[],"advice":[],"labels":[],"tags":[],"knowledge":"supported","emitted_at":{"file":"authored.rs","line":1,"column":2}},"causes":[]}"#).unwrap();
        let outcomes: BTreeMap<String, Result<Observation, gmeow_errors::RecordedDiag>> =
            BTreeMap::from([
                ("case/z-error".into(), Err(refusal)),
                ("case/a-success".into(), Ok(authored.clone())),
            ]);
        let observation_bytes = purrdf_lex::json::record::to_vec(&authored);
        assert_reference("observation", &observation_bytes);
        let hydrated: Observation =
            purrdf_lex::json::record::from_slice(&observation_bytes).unwrap();
        assert_eq!(hydrated, authored);
        let value = gmeow_errors::json::recorded_map_to_json(&outcomes);
        let map_bytes = purrdf_lex::json::write_compact(&value).into_bytes();
        assert_reference("recorded-map", &map_bytes);
        let hydrated = recorded_observations_from_bytes(&map_bytes).unwrap();
        assert_eq!(gmeow_errors::json::recorded_map_to_json(&hydrated), value);

        let text = String::from_utf8(observation_bytes).unwrap();
        for token in ["-0", "-1", "1.5", "1e0", "18446744073709551616"] {
            let malformed = text.replace("\"urn:a-world\":0", &format!("\"urn:a-world\":{token}"));
            assert_ne!(malformed, text);
            let error = purrdf_lex::json::record::from_slice::<Observation>(malformed.as_bytes())
                .unwrap_err();
            let detail = error.to_string();
            assert!(
                detail.contains("world_counts")
                    && detail.contains("urn:a-world")
                    && detail.contains("u64"),
                "{detail}"
            );
        }
        let duplicate = text.replacen(
            "\"world_counts\":",
            "\"world_counts\":{},\"world_counts\":",
            1,
        );
        let error =
            purrdf_lex::json::record::from_slice::<Observation>(duplicate.as_bytes()).unwrap_err();
        assert!(error.to_string().contains("world_counts"), "{error}");
        let duplicate_world =
            text.replace("\"urn:a-world\":0", "\"urn:a-world\":0,\"urn:a-world\":7");
        let admitted: Observation =
            purrdf_lex::json::record::from_slice(duplicate_world.as_bytes()).unwrap();
        assert_eq!(admitted.world_counts["urn:a-world"], 7);
        let extended = text.replacen('{', "{\"extension\":true,", 1);
        let admitted: Observation =
            purrdf_lex::json::record::from_slice(extended.as_bytes()).unwrap();
        assert_eq!(admitted, authored);
    }
    let dataset = dataset_from_bytes(
        br#"
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
            <urn:bad> { <urn:x> a owl:Nothing . }
            <urn:good> { <urn:y> a <urn:C> . }
            <urn:gap> {
                <urn:a> a _:restriction .
                <urn:property> a owl:DatatypeProperty .
                _:restriction a owl:Restriction; owl:onProperty <urn:property>;
                    owl:someValuesFrom _:datatype .
                _:datatype a rdfs:Datatype; owl:onDatatype xsd:string;
                    owl:withRestrictions ( [ xsd:pattern "[a-z]+" ] ) .
            }
        "#,
        NativeRdfFormat::TriG,
    )
    .unwrap();
    let observed = observe(&dataset).unwrap();
    assert_eq!(observed.token(), "inconsistent");
    assert_eq!(observed.verdict.information_state(), InformationState::Both);
    assert!(!observed.verdict.gaps.is_empty());
    assert!(!observed.verdict.boundary_findings.is_empty());
    assert!(
        observed
            .verdict
            .inconsistencies
            .iter()
            .any(|witness| witness.world == "urn:bad")
    );
    assert_eq!(
        observed.world_counts,
        BTreeMap::from([
            ("urn:bad".to_owned(), 1),
            ("urn:good".to_owned(), 1),
            ("urn:gap".to_owned(), 11),
        ])
    );
    let bytes = purrdf_lex::json::record::to_vec(&observed);
    assert_eq!(
        purrdf_lex::json::record::from_slice::<Observation>(&bytes).unwrap(),
        observed
    );
}
