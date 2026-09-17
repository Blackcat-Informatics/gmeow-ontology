// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn iri_minting_matches_historical() {
    assert_eq!(
        local("https://blackcatinformatics.ca/gmeow/fullName"),
        "fullName"
    );
    assert_eq!(
        param_iri("https://blackcatinformatics.ca/gmeow/addressLocality"),
        "https://blackcatinformatics.ca/gmeow/paramAddressLocality"
    );
    assert_eq!(
        output_iri("https://blackcatinformatics.ca/gmeow/fnBirthEventToDate"),
        "https://blackcatinformatics.ca/gmeow/outBirthEventToDate"
    );
    assert_eq!(
        impl_iri("schema-org"),
        "https://blackcatinformatics.ca/gmeow/implSchemaOrg"
    );
    assert_eq!(camel("schema-org"), "SchemaOrg");
}

#[test]
fn lower_fno_extracts_nested_collection_atoms() {
    use purrdf::{NativeRdfFormat, RdfDatasetBuilder, parse_dataset};
    let dsl_ttl = r#"
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            gmeow:fnX a gmeow:ProjectionFunction ;
                rdfs:label "x" ;
                gmeow:fnInput gmeow:eventTime ;
                gmeow:fnOutput gmeow:outVal ; gmeow:fnOutputType rdfs:Literal .
            gmeow:cellX a gmeow:ProjectionMapping ;
                gmeow:hasMappingPattern [
                    gmeow:anchor "person" ; gmeow:value "bdate" ;
                    gmeow:atom (
                        [ gmeow:subjectVar "birth" ; gmeow:predicate gmeow:eventType ; gmeow:objectValue gmeow:eventTypeBirth ]
                        [ gmeow:subjectVar "birth" ; gmeow:predicate gmeow:eventTime ; gmeow:objectVar "bdate" ]
                    ) ] ;
                gmeow:hasBinding [ gmeow:profile "schema-org" ; gmeow:transform gmeow:fnX ] .
        "#;
    let onto_ttl = r#"
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            gmeow:eventTime rdfs:range rdfs:Literal .
        "#;
    // Merge via push_dataset exactly as the caller does (fresh blank scope).
    let mut db = RdfDatasetBuilder::new();
    db.push_dataset(
        &parse_dataset(
            dsl_ttl.as_bytes(),
            NativeRdfFormat::Turtle.media_type(),
            None,
        )
        .expect("parse dsl"),
    );
    let dsl = db.freeze().expect("freeze dsl");

    let onto = parse_dataset(
        onto_ttl.as_bytes(),
        NativeRdfFormat::Turtle.media_type(),
        None,
    )
    .expect("parse onto");
    let (_, analysis) = super::super::correspondence_frontend::transpile_correspondences_indexed(
        &DslView::new(&dsl),
    )
    .expect("admit mappings");
    let nt = lower_fno(&DslView::new(&dsl), &DslView::new(&onto), &analysis)
        .expect("lower")
        .catalog;
    // The param-mapping for (fnX, schema-org, paramEventTime ↦ ?bdate) must appear.
    assert!(
        nt.contains("functionParameter> <https://blackcatinformatics.ca/gmeow/paramEventTime>"),
        "expected paramEventTime mapping; got:\n{nt}"
    );
    assert!(
        nt.contains("implementationProperty> \"bdate\""),
        "expected ?bdate impl property; got:\n{nt}"
    );
}

/// Shift-left: parse the COMPLETED FnO catalog dataset back and confirm every
/// A-Box FnO individual (`fno:Parameter`/`fno:Function`/`fno:Output`/
/// `fno:Implementation`) carries all four structural annotations — the exact
/// contract `gmeow_validate::lint::structural_lint_dataset` enforces at
/// the whole-bundle SHACL validation (`make validate` / the pipeline
/// stage-validate), caught here in a fast `cargo nextest -p
/// gmeow-logic-compile` instead.
#[test]
fn fno_abox_individuals_carry_full_structural_annotations() {
    use purrdf::{NativeRdfFormat, RdfDatasetBuilder, parse_dataset};
    let dsl_ttl = r#"
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            gmeow:fnY a gmeow:ProjectionFunction ;
                rdfs:label "y" ;
                gmeow:fnInput gmeow:eventTime ;
                gmeow:fnOutput gmeow:outVal ; gmeow:fnOutputType rdfs:Literal .
            gmeow:cellY a gmeow:ProjectionMapping ;
                gmeow:hasMappingPattern [
                    gmeow:anchor "person" ; gmeow:value "bdate" ;
                    gmeow:atom (
                        [ gmeow:subjectVar "birth" ; gmeow:predicate gmeow:eventTime ; gmeow:objectVar "bdate" ]
                    ) ] ;
                gmeow:hasBinding [ gmeow:profile "schema-org" ; gmeow:transform gmeow:fnY ] .
        "#;
    // `eventTime` deliberately carries an authored label/definition (exercises
    // the onto-derived branch of `predicate_label_and_description`); the
    // function itself authors no `skos:definition`, so its `fno:Function`
    // individual exercises the completion pass's defensive definition fallback.
    // The `lang:carrierTag` graft mirrors the real ontology's grounding: with
    // it declared, `build_tag_map` remaps EVERY English literal in this public
    // FnO document — both `to_quads`-native ones and the ones this completion
    // pass derives — from the internal `x-gmeow-english` carrier tag to the
    // public `en` BCP-47 tag, so the two sources end up carrying the SAME
    // final tag (proving the derived literals interoperate with the existing
    // carrier-tag retag pipeline rather than diverging from it).
    let onto_ttl = r#"
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix skos: <http://www.w3.org/2004/02/skos/core#> .
            @prefix lang: <https://blackcatinformatics.ca/lang/> .
            gmeow:eventTime rdfs:range rdfs:Literal ;
                rdfs:label "event time" ;
                skos:definition "The time an event occurred." .
            gmeow:englishGmeowCarrier lang:carrierTag "x-gmeow-english" ;
                lang:varietyOf gmeow:englishSign .
            gmeow:englishSign skos:notation "en" .
        "#;
    let mut db = RdfDatasetBuilder::new();
    db.push_dataset(
        &parse_dataset(
            dsl_ttl.as_bytes(),
            NativeRdfFormat::Turtle.media_type(),
            None,
        )
        .expect("parse dsl"),
    );
    let dsl = db.freeze().expect("freeze dsl");
    let onto = parse_dataset(
        onto_ttl.as_bytes(),
        NativeRdfFormat::Turtle.media_type(),
        None,
    )
    .expect("parse onto");

    let (_, analysis) = super::super::correspondence_frontend::transpile_correspondences_indexed(
        &DslView::new(&dsl),
    )
    .expect("admit mappings");
    let catalog = lower_fno(&DslView::new(&dsl), &DslView::new(&onto), &analysis)
        .expect("lower")
        .catalog;

    let catalog_ds = parse_dataset(
        catalog.as_bytes(),
        NativeRdfFormat::NTriples.media_type(),
        None,
    )
    .expect("parse completed fno catalog");
    let view = DslView::new(&catalog_ds);

    let document_iri = format!("{ONTOLOGY_IRI}/projections/functions");
    let expected_counts: [(&str, usize); 4] = [
        (FNO_PARAMETER, 1),
        (FNO_FUNCTION, 1),
        (FNO_OUTPUT, 1),
        (FNO_IMPLEMENTATION, 1),
    ];
    let mut checked = 0usize;
    for (fno_type, expected_count) in expected_counts {
        let subjects = view.subjects_of_type(fno_type);
        assert_eq!(
            subjects.len(),
            expected_count,
            "expected {expected_count} individual(s) typed {fno_type}, got {subjects:?}"
        );
        for subject in subjects {
            checked += 1;
            // The carrier-tag graft above resolves `x-gmeow-english` → `en`
            // for this public FnO document, so BOTH `to_quads`-native labels
            // (e.g. `fnY`'s DSL-authored label) and this completion pass's
            // OWN derived literals (e.g. `implSchemaOrg`'s, which never came
            // from `to_quads` at all — `FnImpl` has no label/description
            // field) must land on the identical public tag: proof the
            // completion pass's carrier-tagged literals ride the exact same
            // retag pipeline as everything else in the catalog.
            let label = view
                .object_literal(&subject, RDFS_LABEL)
                .unwrap_or_else(|| panic!("{subject} ({fno_type}) missing rdfs:label"));
            assert!(
                catalog.contains(&format!("\"{}\"@en", label.replace('"', "\\\""))),
                "{subject}'s rdfs:label {label:?} must carry the carrier-tag-resolved \
                     public 'en' tag; catalog:\n{catalog}"
            );
            let definition = view
                .object_literal(&subject, SKOS_DEFINITION)
                .unwrap_or_else(|| panic!("{subject} ({fno_type}) missing skos:definition"));
            assert!(
                catalog.contains(&format!("\"{}\"@en", definition.replace('"', "\\\""))),
                "{subject}'s skos:definition {definition:?} must carry the \
                     carrier-tag-resolved public 'en' tag; catalog:\n{catalog}"
            );
            assert_eq!(
                view.object_iri(&subject, gmeow_errors::abox::RDFS_IS_DEFINED_BY),
                Some(document_iri.clone()),
                "{subject} ({fno_type}) must be rdfs:isDefinedBy the FnO catalog graph"
            );
            assert_eq!(
                view.object_iri(&subject, gmeow_errors::abox::GRAPH_BOX_ROLE),
                Some(gmeow_errors::abox::BOX_ABOX.to_owned()),
                "{subject} ({fno_type}) must carry gmeow:graphBoxRole gmeow:boxABox"
            );
        }
    }
    assert_eq!(checked, 4, "fixture must exercise all four FnO A-Box types");

    // The catalog's own `owl:Ontology` document header is NOT exempt from the
    // structural lint (only `<ns>self`-defined terms are) — it must ALSO carry
    // all four annotations, but with `gmeow:graphBoxRole gmeow:boxTBox` (a
    // T-Box document), never `gmeow:boxABox`.
    let header_label = view
        .object_literal(&document_iri, RDFS_LABEL)
        .expect("fno catalog header missing rdfs:label");
    assert!(
        catalog.contains(&format!("\"{}\"@en", header_label.replace('"', "\\\""))),
        "fno catalog header's rdfs:label {header_label:?} must carry the \
             carrier-tag-resolved public 'en' tag; catalog:\n{catalog}"
    );
    let header_definition = view
        .object_literal(&document_iri, SKOS_DEFINITION)
        .expect("fno catalog header missing skos:definition");
    assert!(
        catalog.contains(&format!(
            "\"{}\"@en",
            header_definition.replace('"', "\\\"")
        )),
        "fno catalog header's skos:definition {header_definition:?} must carry the \
             carrier-tag-resolved public 'en' tag; catalog:\n{catalog}"
    );
    assert_eq!(
        view.object_iri(&document_iri, gmeow_errors::abox::RDFS_IS_DEFINED_BY),
        Some(document_iri.clone()),
        "fno catalog header must be rdfs:isDefinedBy the FnO catalog graph"
    );
    assert_eq!(
        view.object_iri(&document_iri, gmeow_errors::abox::GRAPH_BOX_ROLE),
        Some(gmeow_errors::abox::BOX_TBOX.to_owned()),
        "fno catalog header (a T-Box document) must carry gmeow:graphBoxRole \
             gmeow:boxTBox, never gmeow:boxABox"
    );
    assert_ne!(
        view.object_iri(&document_iri, gmeow_errors::abox::GRAPH_BOX_ROLE),
        Some(gmeow_errors::abox::BOX_ABOX.to_owned()),
        "fno catalog header must never carry gmeow:boxABox"
    );
}

#[test]
fn order_binds_is_topological_with_alpha_tiebreak() {
    // c depends on a+b; a,b independent → a, b, then c.
    let binds = vec![
        Bind {
            var: "c".to_owned(),
            refs: BTreeSet::from(["a".to_owned(), "b".to_owned()]),
        },
        Bind {
            var: "b".to_owned(),
            refs: BTreeSet::new(),
        },
        Bind {
            var: "a".to_owned(),
            refs: BTreeSet::new(),
        },
    ];
    assert_eq!(order_binds(binds).unwrap(), vec!["a", "b", "c"]);
}

#[test]
fn order_binds_rejects_cycle() {
    // a depends on b and b depends on a → unresolvable cycle, hard-fail.
    let binds = vec![
        Bind {
            var: "a".to_owned(),
            refs: BTreeSet::from(["b".to_owned()]),
        },
        Bind {
            var: "b".to_owned(),
            refs: BTreeSet::from(["a".to_owned()]),
        },
    ];
    let err = order_binds(binds).expect_err("a BIND cycle must be rejected");
    assert!(
        err.message().contains("cyclic BIND/mint dependency"),
        "{err}"
    );
}
