// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use purrdf::{DatasetView, GraphMatch, TermRef, TermValue, parse_dataset};

use super::*;

#[test]
fn prepared_source_preserves_restriction_anchors_and_standard_compilation() {
    let source = parse_dataset(
        br#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
                @prefix ex: <https://example.org/> .
                ex:Thing logic:subClassOf [ a logic:Restriction ;
                    logic:onProperty ex:part ; logic:someValuesFrom ex:Part ] ."#,
        "text/turtle",
        None,
    )
    .unwrap();
    let prepared = PreparedLogicSource::new(&source).unwrap();
    let on_property = source
        .term_id_by_value(&TermValue::iri(
            "https://blackcatinformatics.ca/logic/onProperty",
        ))
        .unwrap();
    let original = source
        .quads_for_pattern(None, Some(on_property), None, GraphMatch::Default)
        .next()
        .unwrap()
        .s;
    assert!(matches!(source.resolve(original), TermRef::Blank { .. }));
    let mapped = prepared
        .source_term(original)
        .expect("the restriction remains addressable before lowering");
    let target_predicate = prepared
        .dataset()
        .term_id_by_value(&TermValue::iri(
            "https://blackcatinformatics.ca/logic/onProperty",
        ))
        .unwrap();
    assert_eq!(
        prepared
            .dataset()
            .quads_for_pattern(
                Some(mapped),
                Some(target_predicate),
                None,
                GraphMatch::Default
            )
            .count(),
        1
    );
    let (native, _) = prepared.compile(Some("selected.ttl".into())).unwrap();
    let (standard, _) =
        super::super::parse_logic_dataset(&source, Some("selected.ttl".into())).unwrap();
    assert_eq!(native.canonical_key(), standard.canonical_key());
    assert!(PreparedLogicSource::new(&purrdf::RdfDatasetBuilder::new().freeze().unwrap()).is_err());
}
