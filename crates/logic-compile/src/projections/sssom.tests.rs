// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

#[test]
fn render_one_emits_canonical_tsv() {
    let table = ns_to_prefix();
    let make = |subj: &str, pred: &str, obj: &str, c: Option<f64>| {
        checked_mapping(
            sssom_id(subj, table),
            None,
            sssom_id(pred, table),
            sssom_id(obj, table),
            None,
            sssom_id(DEFAULT_JUSTIFICATION, table),
            c,
            None,
        )
        .expect("well-formed row")
    };
    // Two rows, deliberately out of (subject, predicate, object) order.
    let rows = vec![
        make(
            &format!("{GMEOW}Zeta"),
            "http://www.w3.org/2004/02/skos/core#closeMatch",
            &format!("{GMEOW}Bar"),
            Some(0.8),
        ),
        make(
            &format!("{GMEOW}Alpha"),
            "http://www.w3.org/2004/02/skos/core#exactMatch",
            &format!("{GMEOW}Foo"),
            Some(1.0),
        ),
    ];
    let meta = MappingSet {
        set_id: "https://blackcatinformatics.ca/gmeow/mappings/demo".to_owned(),
        license: "https://creativecommons.org/licenses/by/4.0/".to_owned(),
        comment: "Demo  set\nwith   wrap".to_owned(),
        trailer: "# REFUSED nothing here".to_owned(),
    };
    let text = render_one(&rows, Some(&meta), "0.1.0", "2026-06-03");
    let expected = "\
# mapping_set_id: https://blackcatinformatics.ca/gmeow/mappings/demo
# mapping_set_version: 0.1.0
# license: https://creativecommons.org/licenses/by/4.0/
# mapping_tool: gmeow-dev sync --mode update --outputs generated (mappings)
# mapping_tool_version: 0.1.0
# mapping_date: 2026-06-03
# comment: \"Demo set with wrap\"
# curie_map:
#   gmeow: https://blackcatinformatics.ca/gmeow/
#   semapv: https://w3id.org/semapv/vocab/
#   skos: http://www.w3.org/2004/02/skos/core#
# # REFUSED nothing here
subject_id\tpredicate_id\tobject_id\tmapping_justification\tconfidence\tcomment
gmeow:Alpha\tskos:exactMatch\tgmeow:Foo\tsemapv:ManualMappingCuration\t1.0\t
gmeow:Zeta\tskos:closeMatch\tgmeow:Bar\tsemapv:ManualMappingCuration\t0.8\t
";
    assert_eq!(text, expected);
}

#[test]
fn label_column_appears_only_when_populated() {
    let table = ns_to_prefix();
    let row = checked_mapping(
        sssom_id(&format!("{GMEOW}Foo"), table),
        Some("Foo label".to_owned()),
        sssom_id("http://www.w3.org/2004/02/skos/core#exactMatch", table),
        sssom_id(&format!("{GMEOW}Bar"), table),
        None,
        sssom_id(DEFAULT_JUSTIFICATION, table),
        None,
        None,
    )
    .expect("well-formed row");
    let text = render_one(&[row], None, "0.1.0", "2026-06-03");
    let header_row = text
        .lines()
        .find(|l| l.starts_with("subject_id"))
        .expect("column header");
    assert_eq!(
        header_row,
        "subject_id\tsubject_label\tpredicate_id\tobject_id\tmapping_justification\tconfidence\tcomment"
    );
    assert!(!text.contains("mapping_set_id"));
}

#[test]
fn checked_mapping_rejects_tab_in_cell() {
    let table = ns_to_prefix();
    let err = checked_mapping(
        sssom_id(&format!("{GMEOW}Foo"), table),
        Some("has\ttab".to_owned()),
        sssom_id("http://www.w3.org/2004/02/skos/core#exactMatch", table),
        sssom_id(&format!("{GMEOW}Bar"), table),
        None,
        sssom_id(DEFAULT_JUSTIFICATION, table),
        None,
        None,
    )
    .expect_err("a cell with a tab must be rejected");
    assert!(err.message().contains("subject_label"), "{err}");
}

#[test]
fn lower_sssom_extracts_over_dslview() {
    // One native RDF-1.2 alignment cell + its MappingSet header, exercising the
    // per-file header rendering and `alignment_terms` off the sole native reader.
    let ttl = br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .

gmeow:set1 a gmeow:MappingSet ;
    gmeow:sssomFile "demo.sssom.tsv" ;
    gmeow:setId "https://blackcatinformatics.ca/gmeow/mappings/demo" ;
    gmeow:license "https://creativecommons.org/licenses/by/4.0/" .

gmeow:Foo skos:exactMatch gmeow:Bar {|
    gmeow:sssomFile  "demo.sssom.tsv" ;
    gmeow:confidence 1.0
|} .
"#;
    let ds = purrdf::parse_dataset(ttl, "text/turtle", None).expect("parse native ttl");
    let view = DslView::new(&ds);

    // Build the materialized correspondence lookup from the same view, exactly as the
    // pipeline stage does, so the ledger gate consumes the materialized typed relation.
    let (_program, lookup) =
        crate::projections::correspondence_frontend::transpile_correspondences_indexed(&view)
            .expect("transpile lookup");
    let out = lower_sssom(&view, "0.1.0", "2026-06-03", &lookup).expect("lower sssom");
    let tsv = out.sets.get("demo.sssom.tsv").expect("one set emitted");
    assert!(tsv.contains("# mapping_set_id: https://blackcatinformatics.ca/gmeow/mappings/demo"));
    assert!(
        tsv.ends_with(
            "gmeow:Foo\tskos:exactMatch\tgmeow:Bar\tsemapv:ManualMappingCuration\t1.0\t\n"
        )
    );
    assert_eq!(
        alignment_terms(&lookup).unwrap(),
        BTreeSet::from([format!("{GMEOW}Foo"), format!("{GMEOW}Bar")])
    );
}

#[test]
fn lower_sssom_emits_projection_binding_rows() {
    let ttl = br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
@prefix schema: <https://schema.org/> .
@prefix odrl:  <http://www.w3.org/ns/odrl/2/> .

gmeow:set1 a gmeow:MappingSet ;
    gmeow:sssomFile "demo.sssom.tsv" ;
    gmeow:setId "https://blackcatinformatics.ca/gmeow/mappings/demo" ;
    gmeow:license "https://creativecommons.org/licenses/by/4.0/" .

gmeow:mapName a gmeow:ProjectionMapping ;
    gmeow:subjectLabel "GMEOW name" ;
    gmeow:objectLabel "schema.org name" ;
    gmeow:justification <https://w3id.org/semapv/vocab/ManualMappingCuration> ;
    gmeow:comment "Curated exact property correspondence." ;
    gmeow:hasMappingPattern [
        gmeow:anchor "s" ; gmeow:value "name" ;
        gmeow:atom ( [ gmeow:subjectVar "s" ; gmeow:predicate gmeow:name ; gmeow:objectVar "name" ] ) ;
        gmeow:edoalSource gmeow:name
    ] ;
    gmeow:hasBinding [
        gmeow:profile "schema-org" ; gmeow:toPredicate schema:name ;
        gmeow:relation "=" ; gmeow:confidence 0.9 ;
        gmeow:emitSssom true ; gmeow:sssomPredicate skos:exactMatch ;
        gmeow:sssomFile "demo.sssom.tsv"
    ] .

gmeow:mapActionReproduce a gmeow:ProjectionMapping ;
    gmeow:hasMappingPattern [
        gmeow:anchor "rule" ;
        gmeow:atom ( [ gmeow:subjectVar "rule" ; gmeow:predicate gmeow:ruleAction ; gmeow:objectValue gmeow:actionReproduce ] )
    ] ;
    gmeow:hasBinding [
        gmeow:profile "odrl" ; gmeow:relation "=" ; gmeow:confidence 0.85 ;
        gmeow:emitSssom true ; gmeow:sssomPredicate skos:exactMatch ;
        gmeow:sssomFile "demo.sssom.tsv" ;
        gmeow:templateAtoms ( [ gmeow:tSubj "rule" ; gmeow:tPred odrl:action ; gmeow:tObjValue odrl:reproduce ] )
    ] .
"#;
    let ds = purrdf::parse_dataset(ttl, "text/turtle", None).expect("parse projection ttl");
    let view = DslView::new(&ds);
    let (_program, lookup) =
        crate::projections::correspondence_frontend::transpile_correspondences_indexed(&view)
            .expect("transpile lookup");

    let out = lower_sssom(&view, "0.1.0", "2026-06-03", &lookup).expect("lower sssom");
    let tsv = out.sets.get("demo.sssom.tsv").expect("one set emitted");
    assert!(tsv.contains(
            "subject_id\tsubject_label\tpredicate_id\tobject_id\tobject_label\tmapping_justification\tconfidence\tcomment"
        ));
    assert!(tsv.contains(
            "gmeow:name\tGMEOW name\tskos:exactMatch\tschema:name\tschema.org name\tsemapv:ManualMappingCuration\t0.9\tCurated exact property correspondence."
        ));
    assert!(tsv.contains(
            "gmeow:actionReproduce\t\tskos:exactMatch\todrl:reproduce\t\tsemapv:ManualMappingCuration\t0.85\t"
        ));
    assert_eq!(out.ledger.len(), 2);
    assert_eq!(
        alignment_terms(&lookup).unwrap(),
        BTreeSet::from([
            format!("{GMEOW}actionReproduce"),
            format!("{GMEOW}name"),
            "http://www.w3.org/ns/odrl/2/reproduce".to_owned(),
            "https://schema.org/name".to_owned(),
        ])
    );
}

/// Transpile + lower a native-form corpus, returning the typed program and the SSSOM
/// sets so a test can assert BOTH artifacts materialize from one shared derivation.
fn transpile_and_lower(
    ttl: &[u8],
) -> gmeow_errors::Result<(
    crate::projections::correspondence::CorrespondenceProgram,
    SssomLowering,
)> {
    let ds = purrdf::parse_dataset(ttl, "text/turtle", None).expect("parse native ttl");
    let view = DslView::new(&ds);
    let (program, lookup) =
        crate::projections::correspondence_frontend::transpile_correspondences_indexed(&view)?;
    let lowering = lower_sssom(&view, "0.1.0", "2026-06-03", &lookup)?;
    Ok((program, lowering))
}

/// The CANONICAL native alignment-cell form (R4/AC3). Each cell is one
/// RDF-1.2 asserting-annotation `s skos:*Match o {| … |}`; the reifier's annotation
/// block carries the SSSOM/correspondence fields. `gmeow:sssomFile` is the REQUIRED
/// discriminator. The migration tool must emit byte-compatible output of this shape.
const NATIVE_PROLOGUE: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
@prefix schema: <https://schema.org/> .
@prefix gufo:  <http://purl.org/nemo/gufo#> .
@prefix semapv: <https://w3id.org/semapv/vocab/> .
";

#[test]
fn native_owl_and_rdfs_alignment_predicates_are_read() {
    // Alignment cells carry OWL/RDFS alignment predicates
    // (owl:equivalentClass/equivalentProperty/sameAs, rdfs:subClassOf/subPropertyOf), not
    // only the five skos:*Match names — 88 such cells exist in the corpus. The native
    // reader MUST read them too, else the greenfield migration would orphan them.
    use crate::ir::{CorrespondenceRelation, MorphismClass};
    let cases: &[(&str, &str, CorrespondenceRelation, MorphismClass)] = &[
        (
            "owl",
            "equivalentClass",
            CorrespondenceRelation::Equiv,
            MorphismClass::WellBehavedLens,
        ),
        (
            "owl",
            "equivalentProperty",
            CorrespondenceRelation::Equiv,
            MorphismClass::WellBehavedLens,
        ),
        (
            "rdfs",
            "subClassOf",
            CorrespondenceRelation::Subsumes,
            MorphismClass::LossyLens,
        ),
        (
            "rdfs",
            "subPropertyOf",
            CorrespondenceRelation::Subsumes,
            MorphismClass::LossyLens,
        ),
    ];
    for (pfx, local, relation, mclass) in cases {
        let ttl = format!(
            "{NATIVE_PROLOGUE}@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
gmeow:OnlineAccount {pfx}:{local} schema:Thing {{|
    gmeow:sssomFile      \"gmeow-accounts.sssom.tsv\" ;
    gmeow:justification  semapv:ManualMappingCuration ;
    gmeow:confidence     0.9
|}} .
"
        );
        let (program, _lowering) =
            transpile_and_lower(ttl.as_bytes()).expect("native owl/rdfs cell lowers");
        assert_eq!(program.correspondences.len(), 1, "{pfx}:{local}");
        let corr = &program.correspondences[0];
        assert_eq!(corr.relation, *relation, "{pfx}:{local} relation");
        assert_eq!(corr.morphism_class, *mclass, "{pfx}:{local} class");
    }
}

#[test]
fn native_five_match_predicates_materialize_row_and_correspondence() {
    use crate::ir::{CorrespondenceRelation, MorphismClass};
    // predicate local-name → (expected relation, expected morphism class from the band).
    let cases: &[(&str, CorrespondenceRelation, MorphismClass)] = &[
        (
            "exactMatch",
            CorrespondenceRelation::Equiv,
            MorphismClass::WellBehavedLens,
        ),
        (
            "closeMatch",
            CorrespondenceRelation::Overlaps,
            MorphismClass::AffineCorrespondence,
        ),
        (
            "broadMatch",
            CorrespondenceRelation::Subsumes,
            MorphismClass::LossyLens,
        ),
        (
            "narrowMatch",
            CorrespondenceRelation::SubsumedBy,
            MorphismClass::LossyLens,
        ),
        (
            "relatedMatch",
            CorrespondenceRelation::RelatedMatch,
            MorphismClass::AffineCorrespondence,
        ),
    ];
    for (local, relation, mclass) in cases {
        let ttl = format!(
            "{NATIVE_PROLOGUE}
gmeow:VirtualLocation skos:{local} schema:VirtualLocation {{|
    gmeow:sssomFile      \"gmeow-places.sssom.tsv\" ;
    gmeow:justification  semapv:ManualMappingCuration ;
    gmeow:confidence     0.9
|}} .
"
        );
        let (program, lowering) = transpile_and_lower(ttl.as_bytes()).expect("native cell lowers");

        // One typed correspondence, carrying the band-derived relation + class.
        assert_eq!(program.correspondences.len(), 1, "{local}");
        let corr = &program.correspondences[0];
        assert_eq!(corr.relation, *relation, "{local} relation");
        assert_eq!(corr.morphism_class, *mclass, "{local} class");

        // One SSSOM row into the discriminator's file.
        let tsv = lowering
            .sets
            .get("gmeow-places.sssom.tsv")
            .unwrap_or_else(|| panic!("{local}: set emitted"));
        assert!(
                tsv.contains(&format!(
                    "gmeow:VirtualLocation\tskos:{local}\tschema:VirtualLocation\tsemapv:ManualMappingCuration\t0.9\t"
                )),
                "{local} row:\n{tsv}"
            );
    }
}

#[test]
fn native_grounding_cell_preserves_all_fields_and_passes_invariants() {
    let ttl = format!(
        "{NATIVE_PROLOGUE}
logic:Individual skos:closeMatch gufo:Individual {{|
    a                       logic:GroundingCorrespondence ;
    gmeow:sssomFile         \"gmeow-logic.sssom.tsv\" ;
    gmeow:justification     semapv:ManualMappingCuration ;
    logic:sourceEndpoint    logic:Individual ;
    logic:targetEndpoint    gufo:Individual ;
    logic:morphismClass     logic:AffineCorrespondence ;
    logic:morphismKind      logic:InstitutionMorphism ;
    logic:preservationKind  logic:SoundUnderApproximation
|}} .
"
    );
    let (program, lowering) =
        transpile_and_lower(ttl.as_bytes()).expect("grounding native cell lowers");

    // The cell reads back as a grounding correspondence with every field preserved.
    let cells = equivalence_cells(&DslView::new(
        &purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse"),
    ))
    .expect("well-formed cell reads");
    assert_eq!(cells.len(), 1);
    let cell = &cells[0];
    assert!(cell.grounding);
    assert_eq!(cell.sssom_file, "gmeow-logic.sssom.tsv");
    assert_eq!(
        cell.justification.as_deref(),
        Some("https://w3id.org/semapv/vocab/ManualMappingCuration")
    );
    assert_eq!(
        cell.source_endpoint.as_deref(),
        Some("https://blackcatinformatics.ca/logic/Individual")
    );
    assert_eq!(
        cell.target_endpoint.as_deref(),
        Some("http://purl.org/nemo/gufo#Individual")
    );
    assert_eq!(
        cell.morphism_class.as_deref(),
        Some("https://blackcatinformatics.ca/logic/AffineCorrespondence")
    );
    assert_eq!(
        cell.preservation.as_deref(),
        Some("https://blackcatinformatics.ca/logic/SoundUnderApproximation")
    );

    // The grounding correspondence and its SSSOM row both materialize.
    assert_eq!(program.correspondences.len(), 1);
    assert!(program.correspondences[0].grounding);
    assert!(
        lowering
            .sets
            .get("gmeow-logic.sssom.tsv")
            .expect("set emitted")
            .contains("logic:Individual\tskos:closeMatch\tgufo:Individual")
    );
}

#[test]
fn native_alignment_metadata_keeps_scoped_reifiers_and_graph_boundaries() {
    use purrdf::{BlankScope, RdfDatasetBuilder, RdfLiteral, TermRef};
    let mut builder = RdfDatasetBuilder::new();
    let predicate = builder.intern_iri("http://www.w3.org/2004/02/skos/core#closeMatch");
    let object = builder.intern_iri("https://schema.org/Thing");
    let file = builder.intern_iri(GM_SSSOM_FILE);
    let confidence = builder.intern_iri(GM_CONFIDENCE);
    let loss = builder.intern_iri(GM_LOSSY_DROP);
    let graph = builder.intern_iri("https://example.org/otherGraph");
    for (scope, name, score) in [(11, "Alpha", "0.25"), (12, "Bravo", "0.75")] {
        let subject = builder.intern_iri(&format!("{GMEOW}{name}"));
        let reifier = builder.intern_blank("cell", BlankScope(scope));
        let statement = builder.intern_triple(subject, predicate, object);
        builder.push_reifier(reifier, statement);
        let target = builder.intern_literal(RdfLiteral::simple(format!("{name}.sssom.tsv")));
        builder.push_annotation(reifier, file, target);
        builder.push_quad(reifier, file, target, None);
        let other = builder.intern_literal(RdfLiteral::simple("wrong.sssom.tsv"));
        builder.push_annotation_in_graph(reifier, file, other, Some(graph));
        let score = builder.intern_literal(RdfLiteral::typed(
            score,
            "http://www.w3.org/2001/XMLSchema#decimal",
        ));
        builder.push_annotation(reifier, confidence, score);
        let loss_value = builder.intern_literal(RdfLiteral::simple(format!("loss-of-{name}")));
        builder.push_annotation(reifier, loss, loss_value);
    }
    let dataset = builder.freeze().unwrap();
    let view = DslView::new(&dataset);
    let reifiers: Vec<_> = view
        .reified_statements()
        .map(|statement| statement.reifier())
        .collect();
    assert!(reifiers.iter().any(|r| matches!(
        r,
        TermRef::Blank {
            label: "cell",
            scope: BlankScope(11)
        }
    )));
    assert!(reifiers.iter().any(|r| matches!(
        r,
        TermRef::Blank {
            label: "cell",
            scope: BlankScope(12)
        }
    )));
    let cells = equivalence_cells(&view).unwrap();
    assert_eq!(cells.len(), 2);
    for (cell, name, score) in [(&cells[0], "Alpha", "0.25"), (&cells[1], "Bravo", "0.75")] {
        assert_eq!(cell.subject, format!("{GMEOW}{name}"));
        assert_eq!(
            cell.confidence.as_ref().unwrap().literal().lexical_form,
            score
        );
        assert_eq!(cell.sssom_file, format!("{name}.sssom.tsv"));
        assert_eq!(
            cell.lossy_drops,
            vec![RdfLiteral::typed(
                format!("loss-of-{name}"),
                "http://www.w3.org/2001/XMLSchema#string"
            )]
        );
    }
    // The full typed correspondence reader consumes these same admitted cells.
    let (program, _) =
        crate::projections::correspondence_frontend::transpile_correspondences_indexed(&view)
            .unwrap();
    assert_eq!(program.correspondences.len(), 2);
    assert_eq!(
        program.correspondences[0].loss_evidence,
        vec![RdfLiteral::typed(
            "loss-of-Alpha",
            "http://www.w3.org/2001/XMLSchema#string"
        )]
    );
    assert_eq!(
        program.correspondences[1].loss_evidence,
        vec![RdfLiteral::typed(
            "loss-of-Bravo",
            "http://www.w3.org/2001/XMLSchema#string"
        )]
    );
}

#[test]
fn malformed_native_alignment_fields_fail_instead_of_disappearing() {
    for fields in [
        "gmeow:sssomFile <https://example.org/wrong-kind>",
        "gmeow:sssomFile \"a.tsv\", \"b.tsv\"",
        "gmeow:sssomFile \"a.tsv\" ; gmeow:confidence \"not-a-number\"",
        "gmeow:sssomFile \"a.tsv\" ; gmeow:confidence \"NaN\"",
        "gmeow:sssomFile \"a.tsv\" ; gmeow:confidence 1.5",
        "gmeow:sssomFile \"a.tsv\" ; gmeow:justification \"not-an-iri\"",
        "gmeow:sssomFile \"a.tsv\" ; gmeow:lossyDrop <https://example.org/wrong-kind>",
    ] {
        let ttl =
            format!("{NATIVE_PROLOGUE} gmeow:Foo skos:closeMatch schema:Thing {{| {fields} |}} .");
        assert!(
            transpile_and_lower(ttl.as_bytes()).is_err(),
            "malformed cell admitted: {fields}"
        );
    }
}

#[test]
fn native_grounding_cell_missing_preservation_hard_fails() {
    // Same grounding cell as above but with logic:preservationKind DROPPED — the
    // grounding invariant must hard-fail naming the missing field.
    let ttl = format!(
        "{NATIVE_PROLOGUE}
logic:Individual skos:closeMatch gufo:Individual {{|
    a                       logic:GroundingCorrespondence ;
    gmeow:sssomFile         \"gmeow-logic.sssom.tsv\" ;
    gmeow:justification     semapv:ManualMappingCuration ;
    logic:sourceEndpoint    logic:Individual ;
    logic:targetEndpoint    gufo:Individual ;
    logic:morphismClass     logic:AffineCorrespondence ;
    logic:morphismKind      logic:InstitutionMorphism
|}} .
"
    );
    let err = match transpile_and_lower(ttl.as_bytes()) {
        Ok(_) => panic!("missing preservation must fail"),
        Err(err) => err,
    };
    assert!(
        err.message().contains("preservationKind"),
        "diagnostic should name the missing field: {err}"
    );
}

#[test]
fn bare_skos_exactmatch_without_sssomfile_is_ignored() {
    // A-Box coreference: a bare (un-annotated) skos:exactMatch with no reifier and no
    // gmeow:sssomFile discriminator MUST NOT be swept into the alignment corpus.
    let ttl = format!(
        "{NATIVE_PROLOGUE}
gmeow:Thing skos:exactMatch schema:Thing .

# A reified skos:*Match WITHOUT gmeow:sssomFile is also NOT an alignment cell.
gmeow:Other skos:exactMatch schema:Other {{|
    gmeow:confidence 0.5
|}} .
"
    );
    let (program, lowering) =
        transpile_and_lower(ttl.as_bytes()).expect("no cells still lowers cleanly");
    assert!(
        program.correspondences.is_empty(),
        "no alignment cell should be extracted"
    );
    assert!(lowering.sets.is_empty(), "no SSSOM set should be emitted");
    assert!(
        equivalence_cells(&DslView::new(
            &purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse")
        ))
        .expect("no cells reads clean")
        .is_empty()
    );
}

#[test]
fn native_cell_with_non_iri_object_hard_fails() {
    // A reifier that CARRIES the gmeow:sssomFile discriminator (so it IS an alignment
    // cell) but whose match object is a literal is MALFORMED — the fail-closed reader
    // must reject it, never silently drop it (the well-formedness gate moved SHACL→Rust).
    let ttl = format!(
        "{NATIVE_PROLOGUE}
gmeow:Foo skos:exactMatch \"not-an-iri\" {{|
    gmeow:sssomFile     \"gmeow-demo.sssom.tsv\" ;
    gmeow:justification semapv:ManualMappingCuration
|}} .
"
    );
    let err = match transpile_and_lower(ttl.as_bytes()) {
        Ok(_) => panic!("a gmeow:sssomFile-annotated cell with a non-IRI object must fail"),
        Err(err) => err,
    };
    assert!(
        err.message().contains("not") && err.message().contains("IRI"),
        "diagnostic should name the malformed non-IRI object: {err}"
    );
}

#[test]
fn projection_binding_exactmatch_overclaim_is_rejected() {
    let ttl = br#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
@prefix schema: <https://schema.org/> .

gmeow:mapLossyName a gmeow:ProjectionMapping ;
    gmeow:hasMappingPattern [
        gmeow:anchor "s" ; gmeow:value "name" ;
        gmeow:atom ( [ gmeow:subjectVar "s" ; gmeow:predicate gmeow:name ; gmeow:objectVar "name" ] ) ;
        gmeow:edoalSource gmeow:name
    ] ;
    gmeow:hasBinding [
        gmeow:profile "schema-org" ; gmeow:toPredicate schema:name ;
        gmeow:relation "<=" ; gmeow:confidence 0.9 ;
        gmeow:emitSssom true ; gmeow:sssomPredicate skos:exactMatch ;
        gmeow:sssomFile "demo.sssom.tsv"
    ] .
"#;
    let ds = purrdf::parse_dataset(ttl, "text/turtle", None).expect("parse projection ttl");
    let view = DslView::new(&ds);
    let (_program, lookup) =
        crate::projections::correspondence_frontend::transpile_correspondences_indexed(&view)
            .expect("transpile lookup");
    let err = match lower_sssom(&view, "0.1.0", "2026-06-03", &lookup) {
        Ok(_) => panic!("overclaim should be rejected"),
        Err(err) => err,
    };
    assert!(err.message().contains("Overclaim"), "{err}");
    assert!(err.message().contains("exactMatch"), "{err}");
}
