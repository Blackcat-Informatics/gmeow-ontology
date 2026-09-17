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

#[test]
fn serialized_observation_preserves_world_witnesses_and_identified_gaps() {
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
    let bytes = serde_json::to_vec(&observed).unwrap();
    assert_eq!(
        serde_json::from_slice::<Observation>(&bytes).unwrap(),
        observed
    );
}
