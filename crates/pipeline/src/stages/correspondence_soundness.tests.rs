// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn target_axiom_object_transfer_preserves_rdf12_identity() {
    let source = parse_dataset(
            "<http://example.org/s> <http://example.org/p> <<( _:claim <http://example.org/p> \"claim\"@ar--rtl )>> .".as_bytes(),
            NativeRdfFormat::Turtle.media_type(), None,
        ).expect("RDF 1.2 source axiom");
    let object = source.quads().next().expect("one axiom").o;
    let expected = source.to_owned_term(object);
    let mut builder = RdfDatasetBuilder::new();
    let subject = builder.intern_iri("http://example.org/s").unwrap();
    let predicate = builder.intern_iri("http://example.org/p").unwrap();
    let transferred = intern_object(&mut builder, &source, object).unwrap();
    builder
        .push_quad(subject, predicate, transferred, None)
        .unwrap();
    let target = builder.freeze().expect("transferred axiom");
    assert_eq!(
        target.to_owned_term(target.quads().next().expect("transferred axiom").o),
        expected
    );
}

#[test]
fn canonical_property_typing_loads_the_target_axiom_prefix() {
    let ontology = parse_dataset(
        b"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
              @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
              gmeow:geometry a logic:ObjectProperty .\n",
        NativeRdfFormat::Turtle.media_type(),
        None,
    )
    .expect("parse canonical property fixture");
    let view = DslView::new(&ontology);
    let mappings = [Mapping {
        subject_id: "gmeow:geometry".to_owned(),
        predicate_id: "skos:closeMatch".to_owned(),
        object_id: "geo:hasGeometry".to_owned(),
        confidence: "1.0".to_owned(),
        mapping_justification: "semapv:ManualMappingCuration".to_owned(),
    }];

    assert_eq!(
        referenced_prefixes(&mappings, &view),
        BTreeSet::from(["geo".to_owned()])
    );
}

#[test]
fn fno_audit_requires_the_generated_catalog_and_never_substitutes_source() {
    let temp = tempfile::tempdir().unwrap();
    let projections = temp.path().join("generated/projections");
    let old_source = temp.path().join("dsl/mappings/transforms.fno.ttl");
    std::fs::create_dir_all(old_source.parent().unwrap()).unwrap();
    std::fs::write(&old_source, "<urn:old> <urn:p> <urn:o> .").unwrap();
    assert!(
        fno_catalog(&projections).is_err(),
        "authored source cannot stand in for missing generated output"
    );
    std::fs::create_dir_all(&projections).unwrap();
    std::fs::write(
        projections.join(FNO_FUNCTIONS_FILE),
        "<urn:generated> <urn:p> <urn:o> .",
    )
    .unwrap();
    let actual = fno_catalog(&projections).expect("selected generated catalog");
    assert!(actual.term_id_by_iri("urn:generated").is_some());
    assert!(actual.term_id_by_iri("urn:old").is_none());
}
