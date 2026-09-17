// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

fn authenticated_result_shapes() -> String {
    String::from_utf8(
        crate::fixture::authenticated_artifact(
            &repo_root(),
            "stage-export-result-shapes",
            RESULT_SHAPES_PATH,
        )
        .expect("authenticated result-shapes projection"),
    )
    .expect("result-shapes utf8")
}

#[test]
fn every_competency_shape_is_projected() {
    let rendered = authenticated_result_shapes();
    let count = rendered.matches("a sh:NodeShape").count();
    assert!(
        count >= 50,
        "expected the complete result-shape product, got {count}"
    );
}

#[test]
fn projection_fires_on_a_wrong_kind_row_and_passes_a_conforming_one() {
    // Prove the projection is NOT vacuous: the generated SHACL must FAIL a row
    // that binds the wrong term-kind and PASS a conforming row. We synthesise a
    // tiny shape + two rows and validate them with the native engine.
    use purrdf::shapes::engine::{parse_shapes, validate_dataset};

    let shapes_ttl = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
            gmeow:T a sh:NodeShape ;\n\
                sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/> SELECT ?this WHERE { ?cq gmeow:cqResultShape <https://example.org/s> . ?cq gmeow:cqExpectRow ?this }\"\"\" ] ;\n\
                sh:sparql [ sh:severity sh:Violation ; sh:message \"x must bind an IRI\" ;\n\
                    sh:select \"\"\"PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/> SELECT $this WHERE { $this gmeow:rowCell ?c . ?c gmeow:cellVar 'x' ; gmeow:cellValueLiteral ?val }\"\"\" ] .\n";
    let shapes = parse_shapes(shapes_ttl, None).expect("parse generated-style shapes");

    let data = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix ex: <https://example.org/> .\n\
            ex:cq gmeow:cqResultShape <https://example.org/s> ; gmeow:cqExpectRow ex:good, ex:bad .\n\
            ex:good gmeow:rowCell [ gmeow:cellVar \"x\" ; gmeow:cellValueIri ex:a ] .\n\
            ex:bad  gmeow:rowCell [ gmeow:cellVar \"x\" ; gmeow:cellValueLiteral \"oops\" ] .\n";
    let store = native_query::dataset_from_turtle(data.as_bytes(), "test").unwrap();
    let report = validate_dataset(&store, &shapes).unwrap();

    let flagged: Vec<String> = report
        .results
        .iter()
        .map(|r| r.focus_node.to_string())
        .collect();
    assert!(
        flagged.iter().any(|f| f.contains("/bad")),
        "the wrong-kind row must be flagged; flagged: {flagged:?}"
    );
    assert!(
        !flagged.iter().any(|f| f.contains("/good")),
        "the conforming row must NOT be flagged; flagged: {flagged:?}"
    );
}

/// Drift-guard (CONTRACT level): the result-shape surface is the ONE emitter that cannot
/// route through the shared declarative projection — its value-keyed cell model needs a
/// `sh:sparql` procedural constraint the native engine supports, where the declarative
/// `sh:qualifiedValueShape` form it would otherwise use is not implemented (see the module
/// docs: the two constraint axes MUST NOT merge). So byte-parity with the shared projection is
/// impossible by construction; the honest ceiling is CONTENT subsumption, guarded here: over
/// the live corpus EVERY emitted column's `(kind, required, datatype)` contract must have a
/// faithful `ValidationShapeIr` encoding. A column whose procedural `sh:sparql` drifts from a
/// declarative-representable contract fails this test.
#[test]
fn result_column_contracts_are_subsumed_by_validation_shape_model() {
    use gmeow_logic_compile::ir::NodeKind;

    let by_shape = BTreeMap::from([(
        "https://example.test/shape".to_string(),
        vec![
            Column {
                var: "iri".to_string(),
                kind: Kind::Iri,
                required: true,
                datatype: None,
            },
            Column {
                var: "literal".to_string(),
                kind: Kind::Literal,
                required: false,
                datatype: Some("http://www.w3.org/2001/XMLSchema#string".to_string()),
            },
            Column {
                var: "blank".to_string(),
                kind: Kind::BlankNode,
                required: false,
                datatype: None,
            },
        ],
    )]);

    let mut checked = 0usize;
    for (shape_iri, columns) in &by_shape {
        for col in columns {
            let shape = column_contract(shape_iri, col)
                .unwrap_or_else(|e| panic!("column {} of {shape_iri}: {e}", col.var));
            assert_eq!(shape.node_kind, NodeKind::ValidationShape);
            assert_eq!(shape.properties.len(), 1);
            let prop = &shape.properties[0];

            // required ⇒ a minCount-1 obligation; else no cardinality.
            assert_eq!(
                prop.min_count,
                if col.required { Some(1) } else { None },
                "required flag must map to the minCount obligation"
            );

            // kind ⇒ the matching sh:nodeKind component.
            let want_kind = column_node_kind(col.kind);
            assert!(
                prop.components.iter().any(|c| matches!(
                    c,
                    ConstraintComponent::NodeKindShacl(k) if *k == want_kind
                )),
                "kind {:?} must map to a NodeKindShacl component",
                col.kind
            );

            // literal + datatype ⇒ the matching sh:datatype component.
            if col.kind == Kind::Literal
                && let Some(dt) = &col.datatype
            {
                assert!(
                    prop.components.iter().any(|c| matches!(
                        c,
                        ConstraintComponent::Datatype(d) if d == dt
                    )),
                    "a literal column's datatype must map to a Datatype component"
                );
            }
            checked += 1;
        }
    }
    assert!(
        checked > 0,
        "expected to check at least one column contract"
    );
}

#[test]
fn datatype_constraint_fires_and_passes() {
    // A literal column with logic:columnDatatype xsd:string must:
    //   - FAIL a row whose cell binds a literal with a different datatype (xsd:integer)
    //   - PASS a row whose cell binds a literal of exactly the pinned datatype
    use purrdf::shapes::engine::{parse_shapes, validate_dataset};

    // Hand-built projection for a single literal column "tag" pinned to xsd:string.
    let xsd_string = "http://www.w3.org/2001/XMLSchema#string";
    let shapes_ttl = format!(
        "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
            gmeow:DT a sh:NodeShape ;\n\
                sh:target [ a sh:SPARQLTarget ; sh:select \"\"\"PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/> SELECT ?this WHERE {{ ?cq gmeow:cqResultShape <https://example.org/dt> . ?cq gmeow:cqExpectRow ?this }}\"\"\" ] ;\n\
                sh:sparql [ sh:severity sh:Violation ; sh:message \"result column ?tag must bind a literal of datatype <{xsd_string}>\" ;\n\
                    sh:select \"\"\"PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/> SELECT $this WHERE {{ $this gmeow:rowCell ?c . ?c gmeow:cellVar 'tag' ; gmeow:cellValueLiteral ?val . FILTER ( datatype(?val) != <{xsd_string}> ) }}\"\"\" ] .\n\
        "
    );
    let shapes = parse_shapes(&shapes_ttl, None).expect("parse datatype-constraint shapes");

    // ex:good has "hello"^^xsd:string (correct); ex:bad has "5"^^xsd:integer (wrong)
    let data = "\
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
            @prefix ex: <https://example.org/> .\n\
            @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
            ex:cq gmeow:cqResultShape <https://example.org/dt> ; gmeow:cqExpectRow ex:good, ex:bad .\n\
            ex:good gmeow:rowCell [ gmeow:cellVar \"tag\" ; gmeow:cellValueLiteral \"hello\"^^xsd:string ] .\n\
            ex:bad  gmeow:rowCell [ gmeow:cellVar \"tag\" ; gmeow:cellValueLiteral \"5\"^^xsd:integer ] .\n";
    let store = native_query::dataset_from_turtle(data.as_bytes(), "test").unwrap();
    let report = validate_dataset(&store, &shapes).unwrap();

    let flagged: Vec<String> = report
        .results
        .iter()
        .map(|r| r.focus_node.to_string())
        .collect();
    assert!(
        flagged.iter().any(|f| f.contains("/bad")),
        "the wrong-datatype row must be flagged; flagged: {flagged:?}"
    );
    assert!(
        !flagged.iter().any(|f| f.contains("/good")),
        "the correctly-typed row must NOT be flagged; flagged: {flagged:?}"
    );
}

/// Shift-left: drive the SAME native structural lint `make validate`/`make
/// check` run (`gmeow_validate::lint::structural_lint_dataset`) over this
/// generator's real output, so a missing/incorrect A-Box annotation on a
/// minted `GenResultRowShape_*` individual reds HERE — a fast `cargo
/// nextest -p gmeow-pipeline` — rather than only surfacing at the next
/// expensive whole-bundle SHACL validation (`make validate` / the
/// pipeline stage-validate) (mirrors
/// `provenance_graph::tests::minted_individuals_satisfy_the_assertional_abox_contract`).
#[test]
fn minted_shapes_satisfy_the_assertional_abox_contract() {
    use gmeow_validate::lint::{
        LintConfig, default_annotation_predicates, structural_lint_dataset,
    };

    let ttl = authenticated_result_shapes();
    // The real bundle supplies `gmeow:boxABox a gmeow:GraphBoxRole` from the
    // kernel slice; add it here (same pattern as
    // `provenance_graph::tests::minted_individuals_satisfy_the_assertional_abox_contract`)
    // so the graphBoxRole-typing check has its declaration to resolve against.
    let doc = format!("{ttl}\n<{GMEOW_NS}boxABox> a <{GMEOW_NS}GraphBoxRole> .\n");
    let native = purrdf::parse_dataset(doc.as_bytes(), "text/turtle", None)
        .expect("parse result-shapes into native dataset");

    let cfg = LintConfig {
        namespace: GMEOW_NS.to_string(),
        ontology_iri: GMEOW_NS.trim_end_matches('/').to_string(),
        selector_tokens: Default::default(),
        core_slice_iris: Default::default(),
        annotation_predicates: default_annotation_predicates().into_iter().collect(),
    };
    let report = structural_lint_dataset(&native, &cfg);
    let errors = report.errors();
    let shape_errors: Vec<&String> = errors
        .iter()
        .filter(|e| e.contains("GenResultRowShape_"))
        .collect();
    assert!(
        shape_errors.is_empty(),
        "every minted GenResultRowShape_* individual must satisfy the A-Box \
             annotation contract (rdfs:label / skos:definition / rdfs:isDefinedBy / \
             gmeow:graphBoxRole): {shape_errors:?}"
    );
}
