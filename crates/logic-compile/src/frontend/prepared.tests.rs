// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use purrdf::{DatasetView, GraphMatch, TermRef, TermValue, parse_dataset};

use super::*;

/// Composition failures retain their original kind, grade, focus and evidence.
#[test]
fn document_mapping_keeps_the_original_diagnostic_without_publishing_a_receipt() {
    let source = parse_dataset(b"<urn:s> <urn:p> <urn:o> .", "text/turtle", None).unwrap();
    let mut prepared = PreparedLogicSource::new(&source).unwrap();
    let original = Diag::of_kind(crate::error::FormulaAdmission {
        detail: "selected composition refused a source term".into(),
    })
    .with_focus("urn:source-term")
    .with_tag("mapping-control");
    let grade = original.grade();
    let code = original.code();
    let mut refusal = Some(original);
    let error = prepared
        .record_document(
            super::super::SourceDocument {
                path: "selected.ttl".into(),
                content_digest: "synthetic-digest".into(),
                role: "synthetic-source".into(),
                base: None,
            },
            &source,
            |_| {
                Err(refusal
                    .take()
                    .expect("the first failed mapping is terminal"))
            },
        )
        .unwrap_err();
    assert_eq!(error.code(), code);
    assert_eq!(error.grade(), grade);
    assert!(error.is::<crate::error::FormulaAdmission>());
    assert_eq!(
        error.inner().source_ctx.focus.as_ref().unwrap().0,
        "urn:source-term"
    );
    assert_eq!(error.inner().tags, ["mapping-control"]);
    assert!(
        error
            .inner()
            .context
            .iter()
            .any(|frame| frame.label.contains("selected.ttl"))
    );
    assert!(
        prepared.origins().is_empty(),
        "a refused mapping cannot publish provenance"
    );
}

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
        .as_ref()
        .term_id_by_value(&TermValue::iri(
            "https://blackcatinformatics.ca/logic/onProperty",
        ))
        .unwrap();
    let original = source
        .quads_for_pattern(None, Some(on_property), None, GraphMatch::Default)
        .next()
        .unwrap()
        .s;
    assert!(matches!(
        source.as_ref().resolve(original),
        TermRef::Blank { .. }
    ));
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
