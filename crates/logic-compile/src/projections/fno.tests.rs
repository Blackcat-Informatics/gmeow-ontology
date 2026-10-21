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
    )
    .unwrap();
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
    )
    .unwrap();
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

const CALLABLE_FIXTURE: &str = r#"
@prefix ex: <https://example.org/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix dcterms: <http://purl.org/dc/terms/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
ex:call a logic:CallableSignature ;
    logic:inputParameters ( ex:z ex:a ) ; logic:outputParameters ( ex:result ) ;
    rdfs:label "call"@en, "appel"@fr, "اسم"@ar--rtl ;
    skos:definition "interface"@en ; dcterms:description "distinct description"@en ;
    rdfs:isDefinedBy ex:owner ; rdfs:seeAlso ex:law .
ex:z a logic:CallParameter ; logic:parameterPredicate ex:source ;
    logic:parameterValueType xsd:integer ; logic:parameterRequired true ;
    rdfs:label "first input"@en ; skos:definition "integer slot"@en ; rdfs:isDefinedBy ex:owner .
ex:a a logic:CallParameter ; logic:parameterPredicate ex:source ;
    logic:parameterValueType xsd:string ; logic:parameterRequired false ;
    rdfs:label "second input"@en ; skos:definition "string slot"@en ; rdfs:isDefinedBy ex:owner .
ex:result a logic:CallResult ; logic:parameterPredicate ex:value ;
    logic:parameterValueType xsd:decimal ; rdfs:label "result"@en ;
    skos:definition "decimal result"@en ; rdfs:isDefinedBy ex:owner .
"#;

fn lower_callable_fixture(source: &str) -> gmeow_errors::Result<FnoLowering> {
    let dsl =
        purrdf::parse_dataset(source.as_bytes(), "text/turtle", None).expect("synthetic source");
    let onto = purrdf::parse_dataset(b"", "text/turtle", None).expect("empty ontology");
    let view = DslView::new(&dsl);
    let (_, analysis) =
        super::super::correspondence_frontend::transpile_correspondences_indexed(&view)?;
    lower_fno(&view, &DslView::new(&onto), &analysis)
}

#[test]
fn canonical_callables_preserve_order_identity_types_and_rdf12_metadata() {
    let lowering = lower_callable_fixture(CALLABLE_FIXTURE).expect("lower canonical interfaces");
    let ds = purrdf::parse_dataset(lowering.catalog.as_bytes(), "text/turtle", None)
        .expect("generated catalog");
    let view = DslView::new(&ds);
    let call = "https://example.org/call";
    let fno = "https://w3id.org/function/ontology#";
    let inputs = view.rdf_list(view.first_object(call, &format!("{fno}expects")).as_ref());
    assert_eq!(
        inputs,
        vec![
            purrdf::TermValue::Iri("https://example.org/z".into()),
            purrdf::TermValue::Iri("https://example.org/a".into())
        ]
    );
    let outputs = view.rdf_list(view.first_object(call, &format!("{fno}returns")).as_ref());
    assert_eq!(
        outputs,
        vec![purrdf::TermValue::Iri("https://example.org/result".into())]
    );
    for (slot, datatype, required) in [
        ("z", "integer", Some("true")),
        ("a", "string", Some("false")),
        ("result", "decimal", None),
    ] {
        let iri = format!("https://example.org/{slot}");
        assert_eq!(
            view.object_iri(&iri, &format!("{fno}type")),
            Some(format!("http://www.w3.org/2001/XMLSchema#{datatype}"))
        );
        assert_eq!(
            view.object_literal(&iri, &format!("{fno}required"))
                .as_deref(),
            required
        );
    }
    // Both input slots deliberately use the same predicate with distinct identity and type.
    assert_eq!(
        view.object_iri("https://example.org/z", &format!("{fno}predicate")),
        view.object_iri("https://example.org/a", &format!("{fno}predicate"))
    );
    let source = purrdf::parse_dataset(CALLABLE_FIXTURE.as_bytes(), "text/turtle", None).unwrap();
    let source_view = DslView::new(&source);
    for predicate in [
        RDFS_LABEL,
        gmeow_errors::abox::RDFS_IS_DEFINED_BY,
        SKOS_DEFINITION,
        "http://purl.org/dc/terms/description",
        "http://www.w3.org/2000/01/rdf-schema#seeAlso",
    ] {
        let actual = view.objects_of(call, predicate);
        let expected = source_view.objects_of(call, predicate);
        assert_eq!(
            actual.len(),
            expected.len(),
            "metadata multiplicity for {predicate}"
        );
        for value in expected {
            assert!(actual.contains(&value), "lost RDF value {value:?}");
        }
    }
    assert_eq!(
        lowering.ledger.len(),
        1,
        "canonical callables participate in the preservation ledger"
    );
    assert_eq!(
        lowering.catalog,
        lower_callable_fixture(CALLABLE_FIXTURE).unwrap().catalog
    );
}

#[test]
fn canonical_callables_fail_closed_on_invalid_or_unrepresentable_declarations() {
    let cases = [
        (
            "missing requiredness",
            CALLABLE_FIXTURE.replace("logic:parameterRequired true ;", ""),
            "parameterRequired",
        ),
        (
            "boolean kind",
            CALLABLE_FIXTURE.replace(
                "logic:parameterRequired true",
                "logic:parameterRequired \"true\"",
            ),
            "xsd:boolean",
        ),
        (
            "duplicate field",
            CALLABLE_FIXTURE.replace(
                "logic:parameterPredicate ex:source",
                "logic:parameterPredicate ex:source, ex:other",
            ),
            "exactly one",
        ),
        (
            "wrong type kind",
            CALLABLE_FIXTURE.replace(
                "logic:parameterValueType xsd:integer",
                "logic:parameterValueType \"integer\"",
            ),
            "requires an IRI",
        ),
        (
            "undeclared input",
            CALLABLE_FIXTURE.replace("( ex:z ex:a )", "( ex:missing ex:a )"),
            "not a declared CallParameter",
        ),
        (
            "missing result",
            CALLABLE_FIXTURE.replace("( ex:result )", "()"),
            "exactly one output",
        ),
        (
            "multiple results",
            CALLABLE_FIXTURE.replace("( ex:result )", "( ex:result ex:result )"),
            "multiple outputs are unsupported",
        ),
        (
            "extra capability",
            CALLABLE_FIXTURE.replace(
                "ex:call a logic:CallableSignature ;",
                "ex:call a logic:CallableSignature ; logic:unloweredFeature true ;",
            ),
            "no selected FnO lowering",
        ),
        (
            "additional source type",
            CALLABLE_FIXTURE.replace(
                "ex:call a logic:CallableSignature ;",
                "ex:call a logic:CallableSignature, ex:Other ;",
            ),
            "additional source type",
        ),
        (
            "missing owner",
            CALLABLE_FIXTURE.replace("rdfs:isDefinedBy ex:owner ;", ""),
            "source owner",
        ),
        (
            "orphan input",
            CALLABLE_FIXTURE.replace("( ex:z ex:a )", "( ex:z )"),
            "orphan",
        ),
    ];
    for (name, source, expected) in cases {
        let error = match lower_callable_fixture(&source) {
            Ok(_) => panic!("{name} must be refused"),
            Err(error) => error,
        };
        assert!(error.to_string().contains(expected), "{name}: {error}");
    }
    for (name, list, expected) in [
        (
            "cycle",
            "ex:head <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> ex:z ; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> ex:head .",
            "cyclic",
        ),
        (
            "unterminated",
            "ex:head <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> ex:z .",
            "not nil-terminated",
        ),
        (
            "branch",
            "ex:head <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> ex:z, ex:a ; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .",
            "branches",
        ),
    ] {
        let source = format!(
            "{} {list}",
            CALLABLE_FIXTURE.replace("( ex:z ex:a )", "ex:head")
        );
        let error = match lower_callable_fixture(&source) {
            Ok(_) => panic!("{name} must be refused"),
            Err(error) => error,
        };
        assert!(error.to_string().contains(expected), "{name}: {error}");
    }
}

#[test]
fn canonical_callable_identity_cannot_collide_with_a_derived_projection_node() {
    let source = format!(
        "{} gmeow:fnX a gmeow:ProjectionFunction ; gmeow:fnOutput ex:value ; gmeow:fnOutputType xsd:decimal .",
        CALLABLE_FIXTURE.replace("ex:z", "gmeow:outX")
    );
    let error = match lower_callable_fixture(&source) {
        Ok(_) => panic!("identity collision must be refused"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("collides with a derived projection identity"),
        "{error}"
    );
}

fn lower_callable_views(
    dsl_source: &str,
    ontology_source: &str,
) -> gmeow_errors::Result<FnoLowering> {
    let dsl = purrdf::parse_dataset(dsl_source.as_bytes(), "text/turtle", None)
        .expect("synthetic mapping view");
    let ontology = purrdf::parse_dataset(ontology_source.as_bytes(), "text/turtle", None)
        .expect("synthetic ontology view");
    let view = DslView::new(&dsl);
    let (_, analysis) =
        super::super::correspondence_frontend::transpile_correspondences_indexed(&view)?;
    lower_fno(&view, &DslView::new(&ontology), &analysis)
}

/// Independent reference-only method contract: FnO preserves its links, while
/// mathematical AST validation and execution remain outside this projection.
fn mathematical_callable_fixture() -> String {
    let source = CALLABLE_FIXTURE
        .replace("ex:call a logic:CallableSignature ;",
            "ex:call a logic:CallableSignature, gmeow:ObservationMethod ; gmeow:calculationExpression ex:recipe ;")
        .replace("ex:z a logic:CallParameter ;",
            "ex:z a logic:CallParameter ; <https://blackcatinformatics.ca/math/hasMathematicalSymbol> ex:symbol ;");
    format!(
        "{source}\nex:symbol a <https://blackcatinformatics.ca/math/MathematicalSymbol> .\nex:recipe a <https://blackcatinformatics.ca/math/ApplicationExpression> .\nex:call skos:scopeNote \"portée\"@fr, \"typed note\"^^xsd:string, \"اتجاه\"@ar--rtl .\n"
    )
}

/// Preserve source references and role types without inventing FnO recipe or
/// symbol predicates. Metadata keeps language, datatype and direction identities.
#[test]
fn canonical_callables_preserve_selected_mathematical_extensions_from_both_views() {
    let source = mathematical_callable_fixture();
    let original = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None).unwrap();
    let original = DslView::new(&original);
    for (dsl, ontology) in [(&source[..], ""), ("", &source[..])] {
        let lowering = lower_callable_views(dsl, ontology).expect("selected source extensions");
        let dataset =
            purrdf::parse_dataset(lowering.catalog.as_bytes(), "text/turtle", None).unwrap();
        let view = DslView::new(&dataset);
        for (subject, predicate) in [
            (
                "https://example.org/z",
                "https://blackcatinformatics.ca/math/hasMathematicalSymbol",
            ),
            (
                "https://example.org/call",
                "https://blackcatinformatics.ca/gmeow/calculationExpression",
            ),
            (
                "https://example.org/call",
                "http://www.w3.org/2004/02/skos/core#scopeNote",
            ),
        ] {
            let actual = view.objects_of(subject, predicate);
            let expected = original.objects_of(subject, predicate);
            assert_eq!(actual.len(), expected.len(), "{subject}: {predicate}");
            for value in expected {
                assert!(actual.contains(&value), "lost {value:?}");
            }
        }
        for (subject, kind) in [
            (
                "https://example.org/z",
                "https://blackcatinformatics.ca/logic/CallParameter",
            ),
            (
                "https://example.org/call",
                "https://blackcatinformatics.ca/gmeow/ObservationMethod",
            ),
            (
                "https://example.org/call",
                "https://blackcatinformatics.ca/logic/CallableSignature",
            ),
        ] {
            assert!(
                view.object_iris(subject, gmeow_errors::abox::RDF_TYPE)
                    .contains(&kind.to_owned())
            );
        }
        let inputs = view.rdf_list(
            view.first_object(
                "https://example.org/call",
                "https://w3id.org/function/ontology#expects",
            )
            .as_ref(),
        );
        assert_eq!(
            inputs,
            vec![
                purrdf::TermValue::Iri("https://example.org/z".into()),
                purrdf::TermValue::Iri("https://example.org/a".into())
            ]
        );
        assert_eq!(
            view.object_iri(
                "https://example.org/z",
                "https://w3id.org/function/ontology#predicate"
            ),
            Some("https://example.org/source".into())
        );
        assert_eq!(
            view.object_iri(
                "https://example.org/z",
                "https://w3id.org/function/ontology#type"
            ),
            Some("http://www.w3.org/2001/XMLSchema#integer".into())
        );
        assert_eq!(
            view.object_literal(
                "https://example.org/z",
                "https://w3id.org/function/ontology#required"
            )
            .as_deref(),
            Some("true")
        );
        assert_eq!(lowering.ledger.len(), 1);
    }
    let other = source.replace("https://example.org/", "https://second.example.org/");
    let merged = lower_callable_views(&source, &other).expect("distinct owners from both views");
    let dataset = purrdf::parse_dataset(merged.catalog.as_bytes(), "text/turtle", None).unwrap();
    let view = DslView::new(&dataset);
    for base in ["https://example.org/", "https://second.example.org/"] {
        assert_eq!(
            view.object_iri(
                &format!("{base}call"),
                "https://blackcatinformatics.ca/gmeow/calculationExpression"
            ),
            Some(format!("{base}recipe"))
        );
        assert_eq!(
            view.object_iri(
                &format!("{base}z"),
                "https://blackcatinformatics.ca/math/hasMathematicalSymbol"
            ),
            Some(format!("{base}symbol"))
        );
    }
    assert_eq!(merged.ledger.len(), 2);
    assert!(
        lower_callable_views(&source, &source).is_err(),
        "extensions must not bypass duplicate owner refusal"
    );
}

/// Recognized extensions are not a blanket allowance: object kind, scalar
/// cardinality and declared target type remain mandatory in either source view.
#[test]
fn canonical_callables_refuse_malformed_mathematical_extension_targets() {
    let source = mathematical_callable_fixture();
    let symbol = "<https://blackcatinformatics.ca/math/hasMathematicalSymbol> ex:symbol";
    let recipe = "gmeow:calculationExpression ex:recipe";
    let mut cases = Vec::new();
    for (field, target) in [(symbol, "ex:symbol"), (recipe, "ex:recipe")] {
        for replacement in [
            "\"literal\"".to_owned(),
            "[]".to_owned(),
            "<<( ex:s ex:p ex:o )>>".to_owned(),
            format!("{target}, ex:other"),
        ] {
            cases.push((
                source.replace(field, &field.replace(target, &replacement)),
                "exactly one named IRI",
            ));
        }
    }
    for (declaration, replacement) in [
        (
            "ex:symbol a <https://blackcatinformatics.ca/math/MathematicalSymbol> .",
            "",
        ),
        (
            "ex:symbol a <https://blackcatinformatics.ca/math/MathematicalSymbol> .",
            "ex:symbol a <https://blackcatinformatics.ca/math/ApplicationExpression> .",
        ),
        (
            "ex:recipe a <https://blackcatinformatics.ca/math/ApplicationExpression> .",
            "",
        ),
        (
            "ex:recipe a <https://blackcatinformatics.ca/math/ApplicationExpression> .",
            "ex:recipe a <https://blackcatinformatics.ca/math/MathematicalSymbol> .",
        ),
    ] {
        cases.push((source.replace(declaration, replacement), "must be declared"));
    }
    for (source, expected) in cases {
        for (dsl, ontology) in [(&source[..], ""), ("", &source[..])] {
            let error = match lower_callable_views(dsl, ontology) {
                Ok(_) => panic!("malformed target must fail: {source}"),
                Err(error) => error,
            };
            assert!(error.to_string().contains(expected), "{error}");
        }
    }
    let declaration = "ex:symbol a <https://blackcatinformatics.ca/math/MathematicalSymbol> .";
    let untyped = source.replace(declaration, "");
    let foreign_target = format!("@prefix ex: <https://example.org/> . {declaration}");
    assert!(
        lower_callable_views(&untyped, &foreign_target).is_err(),
        "a separate source view cannot supply implicit target ownership"
    );
}

/// Observation-method type and recipe select one paired interface extension;
/// omission stays legitimate for ordinary callables and unsupported roles fail.
#[test]
fn canonical_callables_require_paired_method_recipe_and_keep_role_boundaries() {
    let source = mathematical_callable_fixture();
    for source in [
        source.replace(", gmeow:ObservationMethod", ""),
        source.replace("gmeow:calculationExpression ex:recipe ;", ""),
    ] {
        let error = match lower_callable_fixture(&source) {
            Ok(_) => panic!("unpaired method must fail"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("must be selected together"),
            "{error}"
        );
    }
    for (subject, field) in [
        ("ex:z", "gmeow:calculationExpression ex:recipe"),
        ("ex:result", "gmeow:calculationExpression ex:recipe"),
        (
            "ex:call",
            "<https://blackcatinformatics.ca/math/hasMathematicalSymbol> ex:symbol",
        ),
        (
            "ex:result",
            "<https://blackcatinformatics.ca/math/hasMathematicalSymbol> ex:symbol",
        ),
    ] {
        let source = format!("{source}\n{subject} {field} .");
        let error = match lower_callable_fixture(&source) {
            Ok(_) => panic!("misplaced extension must fail"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("no selected FnO lowering"),
            "{error}"
        );
    }
    for subject in ["ex:z", "ex:result"] {
        let source = format!("{source}\n{subject} a gmeow:ObservationMethod .");
        let error = match lower_callable_fixture(&source) {
            Ok(_) => panic!("misplaced method type must fail"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("additional source type"),
            "{error}"
        );
    }
    assert!(lower_callable_fixture(CALLABLE_FIXTURE).is_ok());
    let parameter_only = source
        .replace(", gmeow:ObservationMethod", "")
        .replace("gmeow:calculationExpression ex:recipe ;", "");
    assert!(
        lower_callable_fixture(&parameter_only).is_ok(),
        "a mathematical parameter does not force an observation method"
    );
    for field in [
        "gmeow:calculationExpression ex:recipe",
        "<https://blackcatinformatics.ca/math/hasMathematicalSymbol> ex:symbol",
        "a gmeow:ObservationMethod",
    ] {
        let source = format!("{CALLABLE_DECLARATION}\nex:operation {field} .");
        let error = match lower_callable_fixture(&source) {
            Ok(_) => panic!("a declaration must not invent an interface"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("no selected FnO lowering")
                || error.to_string().contains("additional source type"),
            "{error}"
        );
    }
}

/// Scope notes use the existing exact RDF metadata path, never a loophole for
/// executable or anonymous fields.
#[test]
fn canonical_callables_refuse_invalid_scope_note_metadata() {
    for value in ["\"\"", "ex:note", "[]"] {
        let source = format!("{CALLABLE_FIXTURE}\nex:call skos:scopeNote {value} .");
        let error = match lower_callable_fixture(&source) {
            Ok(_) => panic!("invalid metadata must fail"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("invalid value kind"), "{error}");
    }
}

const CALLABLE_DECLARATION: &str = r#"
    @prefix ex: <https://example.org/> .
    @prefix logic: <https://blackcatinformatics.ca/logic/> .
    @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
    @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
    @prefix skos: <http://www.w3.org/2004/02/skos/core#> .
    ex:operation a logic:Callable ; rdfs:label "operation"@en ;
        skos:definition "A named operation, without an asserted interface."@en ;
        skos:example "FILTER(ex:operation(?x))"@en ;
        rdfs:isDefinedBy ex:owner ; gmeow:graphBoxRole gmeow:boxRBox ;
        gmeow:howToUse "Resolve its separately declared interface before invocation."@en .
"#;

#[test]
fn callable_declarations_keep_identity_and_metadata_without_inventing_a_signature() {
    for (dsl, onto) in [(CALLABLE_DECLARATION, ""), ("", CALLABLE_DECLARATION)] {
        let result = lower_callable_views(dsl, onto).expect("project operation declaration");
        let dataset =
            purrdf::parse_dataset(result.catalog.as_bytes(), "text/turtle", None).unwrap();
        let view = DslView::new(&dataset);
        let iri = "https://example.org/operation";
        assert!(
            view.subjects_of_type("https://w3id.org/function/ontology#Function")
                .contains(&iri.to_owned())
        );
        for field in ["expects", "returns"] {
            assert!(
                view.objects_of(iri, &format!("https://w3id.org/function/ontology#{field}"))
                    .is_empty(),
                "the declaration does not supply {field}"
            );
        }
        assert_eq!(
            view.object_iris(iri, gmeow_errors::abox::RDFS_IS_DEFINED_BY),
            vec!["https://example.org/owner"]
        );
        assert_eq!(
            view.object_iris(iri, gmeow_errors::abox::GRAPH_BOX_ROLE),
            vec!["https://blackcatinformatics.ca/gmeow/boxRBox"]
        );
        assert_eq!(
            view.object_literal(iri, "http://www.w3.org/2004/02/skos/core#example")
                .as_deref(),
            Some("FILTER(ex:operation(?x))")
        );
        assert_eq!(result.ledger.len(), 1);
    }
    let source = CALLABLE_DECLARATION.replace(
        "ex:operation a logic:Callable ;",
        "ex:operation a logic:Callable ; logic:inputParameters () ;",
    );
    let error = match lower_callable_views(&source, "") {
        Ok(_) => panic!("interface data must not be silently omitted"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("no selected FnO lowering"),
        "{error}"
    );
}

#[test]
fn canonical_interfaces_are_admitted_from_ontology_sources_without_duplicate_ownership() {
    let source = CALLABLE_FIXTURE.replace(
        "a logic:CallableSignature ;",
        "a logic:CallableSignature, logic:Callable ;",
    );
    let actual = lower_callable_views("", &source).expect("ontology-owned interface");
    let ds = purrdf::parse_dataset(actual.catalog.as_bytes(), "text/turtle", None).unwrap();
    let view = DslView::new(&ds);
    assert!(
        view.subjects_of_type("https://w3id.org/function/ontology#Function")
            .contains(&"https://example.org/call".to_owned())
    );
    assert_eq!(actual.ledger.len(), 1);
    let error = match lower_callable_views(&source, &source) {
        Ok(_) => panic!("two interface owners must be refused"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("multiple source owners"),
        "{error}"
    );
}
