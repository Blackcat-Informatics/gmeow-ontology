// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::dsl::{CompetencyQuestion, ExpectedCell, ExpectedRow};
use gmeow_logic_compile::result_shape::{ColumnKind, ResultColumn, ResultShape, RowCardinality};

#[test]
fn source_shape_pins_preserve_laws_without_namespace_or_shared_owner_guessing() {
    // gmeow-test-input: synthetic-only
    let surface = ConformanceShapes::new(
        parse_shapes(
            r#"
            @prefix sh: <http://www.w3.org/ns/shacl#> .
            @prefix ex: <https://ex/> .
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
            ex:Law a sh:NodeShape ; sh:targetNode ex:item ;
                sh:property [ sh:path ex:required ; sh:minCount 1 ;
                    gmeow:enforcesFailureClass ex:RequiredFailure ] .
            ex:Other a sh:NodeShape ; sh:targetNode ex:item ;
                sh:property [ sh:path ex:other ; sh:minCount 1 ] .
            ex:First a sh:NodeShape ; sh:targetNode ex:item ; sh:property ex:shared .
            ex:Second a sh:NodeShape ; sh:targetNode ex:item ; sh:property ex:shared .
            ex:shared sh:path ex:sharedPath ; sh:minCount 1 .
        "#,
            None,
        )
        .expect("synthetic shape surface"),
    );
    let data = store_from_turtle("");
    let report = validate_dataset(&data, &surface.shapes).expect("synthetic violations");
    let mut checked = BTreeSet::new();
    for result in &report.results {
        let actual = result.source_shape.to_string();
        let actual = strip_angle(&actual);
        let path = result
            .result_path
            .as_ref()
            .expect("property path")
            .to_string();
        let path = strip_angle(&path);
        checked.insert(path.to_owned());
        match path {
            "https://ex/required" => {
                assert!(surface.matches_source(actual, "https://ex/Law"));
                assert!(!surface.matches_source(actual, "https://foreign/Law"));
                assert!(!surface.matches_source(actual, "https://ex/Other"));
                assert_eq!(
                    surface.failure_classes.for_result(result),
                    Some("https://ex/RequiredFailure")
                );
            }
            "https://ex/other" => {
                assert!(surface.matches_source(actual, "https://ex/Other"));
                assert!(!surface.matches_source(actual, "https://ex/Law"));
            }
            "https://ex/sharedPath" => {
                assert!(surface.matches_source(actual, "https://ex/shared"));
                assert!(!surface.matches_source(actual, "https://ex/First"));
                assert!(!surface.matches_source(actual, "https://ex/Second"));
            }
            other => panic!("unexpected property {other}"),
        }
    }
    assert_eq!(checked.len(), 3, "every product contract was exercised");
    assert!(surface.matches_source("https://ex/Law", "https://ex/Law"));
    assert!(!surface.matches_source("https://foreign/Law", "https://ex/Law"));
}

/// Materialize inline Turtle into a native dataset via the canonical codec.
fn store_from_turtle(ttl: &str) -> Arc<RdfDataset> {
    native_query::dataset_from_turtle(ttl).expect("valid turtle")
}

/// A minimal SELECT competency question over an inline query.
fn cq_with(query: &str) -> CompetencyQuestion {
    CompetencyQuestion {
        iri: "https://example.org/cqShape".to_owned(),
        query_inline: Some(query.to_owned()),
        query_file: None,
        project_query_file: None,
        expect_ask: None,
        expect_row_count: None,
        exact_rows: false,
        expected_rows: Vec::new(),
        reasoning: ReasoningProfile::None,
        data_file: None,
        result_shape: None,
        input_shape: None,
        consumes: None,
        rationales: Vec::new(),
    }
}

const Q_X: &str = "PREFIX ex: <https://example.org/> \
        SELECT ?x WHERE { ?x a ex:Thing }";

fn one_thing_store() -> Arc<RdfDataset> {
    store_from_turtle("@prefix ex: <https://example.org/> .\nex:a a ex:Thing .\n")
}

#[test]
fn generated_shape_union_is_scoped_back_to_the_slice_authority() {
    let module = store_from_turtle(
        r#"
            @prefix ex: <https://example.org/> .
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

            ex:Slice a owl:Ontology ; rdfs:isDefinedBy ex:Slice .
            ex:Owned a owl:Class ; rdfs:isDefinedBy ex:Slice .
            ex:OwnedConstraint rdfs:isDefinedBy ex:Slice ;
                logic:formalizes ex:OwnedShapeAuthority .
            "#,
    );
    let shapes_ttl = r#"
            @prefix ex: <https://example.org/> .
            @prefix logic: <https://blackcatinformatics.ca/logic/> .
            @prefix sh: <http://www.w3.org/ns/shacl#> .

            ex:Owned-shape a sh:NodeShape ; sh:targetClass ex:Owned .
            ex:OwnedProceduralShape a sh:NodeShape ;
                logic:formalizes ex:OwnedShapeAuthority ;
                sh:targetClass ex:Owned .
            ex:Foreign-shape a sh:NodeShape ; sh:targetClass ex:Foreign .
        "#;
    let parsed = parse_shapes(shapes_ttl, None).expect("shape union parses");
    let scoped = scope_shapes_to_slice(parsed, &module, None)
        .expect("shape union scopes to module authority");
    let ids: BTreeSet<String> = scoped
        .node_shapes
        .iter()
        .map(|shape| shape.id.to_string())
        .collect();

    assert_eq!(
        ids,
        BTreeSet::from([
            "<https://example.org/Owned-shape>".to_owned(),
            "<https://example.org/OwnedProceduralShape>".to_owned(),
        ])
    );
}

#[test]
fn lang_gmn_nonlexical_guard_rejects_word_form_subclasses() {
    let spec_path = paths::repo_root().join("slices/grounding/lang/tests/structural.ttl");
    let spec = dsl::load_spec(&spec_path).expect("lang structural assertions parse");
    let pattern = spec
        .structural
        .iter()
        .find(|assertion| assertion.iri.ends_with("saGmnSignsAreNonLexicalForms"))
        .and_then(|assertion| assertion.pattern.as_deref())
        .expect("the GMN non-lexical structural ASK is present");
    let store = store_from_turtle(
        r#"
            @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
            @prefix lang: <https://blackcatinformatics.ca/lang/> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix ex: <https://example.org/> .

            lang:SyntacticWord rdfs:subClassOf lang:WordForm .
            ex:form a lang:Form, lang:SyntacticWord .
            ex:denotation a lang:Denotation ;
                gmeow:gmnDenotationGrapheme ex:glyph ;
                lang:denotedForm ex:form .
            "#,
    );

    assert!(
        !run_ask(&store, pattern).expect("the GMN non-lexical ASK executes"),
        "a directly situated GMN form must still be rejected when its specific type is a WordForm subclass"
    );
}

/// A `gmeow:saFailWitness` must actually TRIP the ban: over module ∪ fixture the
/// assertion's pattern is required to be violated. A fixture that supplies the banned
/// triple passes the teeth check; a fixture that does NOT supply it hard-fails — proving
/// the teeth check is not itself vacuous (a `scopeModule` ban whose ASK is a typo or is
/// dead would otherwise pass forever, since the real module never carries the banned
/// pattern). This is the teeth of the teeth check.
#[test]
fn structural_fail_witness_requires_the_ban_to_trip() {
    let tmp = tempfile::tempdir().expect("temp slice dir");
    let dir = tmp.path();
    // The real module never carries the banned triple.
    std::fs::write(
        dir.join("module.ttl"),
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
    )
    .expect("write module");
    // A witness that DOES supply the banned pattern.
    std::fs::write(
        dir.join("witness-trips.ttl"),
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:preferredRank a owl:ObjectProperty .\n",
    )
    .expect("write tripping witness");
    // A witness that does NOT supply it (an unrelated triple).
    std::fs::write(
        dir.join("witness-inert.ttl"),
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             gmeow:somethingElse a owl:ObjectProperty .\n",
    )
    .expect("write inert witness");

    let pattern = "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/> \
                       PREFIX owl: <http://www.w3.org/2002/07/owl#> \
                       ASK { gmeow:preferredRank a owl:ObjectProperty }";
    let tripping = StructuralAssertion {
        iri: "https://example.org/saBanned".to_owned(),
        polarity: Polarity::MustNot,
        pattern: Some(pattern.to_owned()),
        shape: None,
        scope: Scope::Module,
        fail_witness: Some("witness-trips.ttl".to_owned()),
        rationales: Vec::new(),
    };
    // A tripping witness: normal check passes (module clean) AND the teeth check passes.
    let (mut mo, mut mae) = (None, None);
    run_structural_cell(&tripping, dir, &mut mo, &mut mae)
        .expect("a witness that supplies the banned pattern trips the mustNot ban");

    // An inert witness: the teeth check must hard-fail.
    let inert = StructuralAssertion {
        fail_witness: Some("witness-inert.ttl".to_owned()),
        ..tripping.clone()
    };
    let (mut mo2, mut mae2) = (None, None);
    let err = run_structural_cell(&inert, dir, &mut mo2, &mut mae2)
        .expect_err("a witness that fails to supply the banned pattern must hard-fail");
    assert!(
        err.message().contains("did NOT trip") && err.message().contains("vacuous"),
        "unexpected error: {err}"
    );
}

#[test]
fn result_shape_conforming_bindings_pass() {
    let store = one_thing_store();
    let mut cq = cq_with(Q_X);
    // ?x is an IRI, required, exactly one row — matches the data.
    cq.result_shape = Some(ResultShape::new(
        vec![ResultColumn::required("x", ColumnKind::Iri)],
        RowCardinality::Count(1),
    ));
    cq.expect_row_count = Some(1); // satisfy the row-comparison tier too
    execute_competency_query(&store, &cq, Q_X).expect("conforming shape passes");
}

#[test]
fn result_shape_term_kind_mismatch_hard_fails() {
    let store = one_thing_store();
    let mut cq = cq_with(Q_X);
    // ?x declared a literal, but the data binds an IRI.
    cq.result_shape = Some(ResultShape::new(
        vec![ResultColumn::required(
            "x",
            ColumnKind::Literal { datatype: None },
        )],
        RowCardinality::Contains,
    ));
    let err =
        execute_competency_query(&store, &cq, Q_X).expect_err("term-kind mismatch must hard-fail");
    assert!(
        err.message().contains("result-shape contract") && err.message().contains("term-kind"),
        "unexpected error: {err}"
    );
}

/// The composition pre-check (`is_satisfiable_by`) surfaces a mismatch when
/// the producer LACKS a column the consumer requires.  The input shape
/// declares two required columns {x:IRI, y:IRI}; the producer only provides
/// {x:IRI} — so `input.is_satisfiable_by(&producer)` must be `Err` with a
/// `MissingColumn` variant naming "y".
#[test]
fn is_satisfiable_by_surfaces_missing_required_column() {
    use gmeow_logic_compile::result_shape::Mismatch;

    let input = ResultShape::new(
        vec![
            ResultColumn::required("x", ColumnKind::Iri),
            ResultColumn::required("y", ColumnKind::Iri),
        ],
        RowCardinality::Contains,
    );
    let producer = ResultShape::new(
        vec![ResultColumn::required("x", ColumnKind::Iri)],
        RowCardinality::Contains,
    );
    let err = input
        .is_satisfiable_by(&producer)
        .expect_err("producer missing required column must be Err");
    assert!(
        matches!(err, Mismatch::MissingColumn { ref var } if var == "y"),
        "expected MissingColumn {{ var: y }}, got: {err:?}"
    );
}

/// A `gmeow:cqDataFile` overlay must (a) make the fixture's instances visible to
/// the query, (b) never leak into the shared base dataset (the frozen IR is
/// immutable — the overlay is a UNION into a fresh dataset, so the base is
/// untouched by construction), and (c) be rejected outright in the RDFS lane.
#[test]
fn cq_data_file_overlay_applies_and_is_removed() {
    let tmp = tempfile::tempdir().expect("temp slice dir");
    let dir = tmp.path();
    let fixture = "@prefix ex: <https://example.org/test/> .\n\
                       @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
                       ex:event1 a gmeow:Event .\n";
    std::fs::write(dir.join("data.ttl"), fixture).expect("write fixture");

    // Empty shared base: the only way the SELECT matches is via the overlay.
    let store = store_from_turtle("@prefix ex: <https://example.org/> .\n");
    let cq = CompetencyQuestion {
        iri: "https://example.org/test/cqOverlay".to_owned(),
        query_inline: Some(
            "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/> \
                 SELECT ?e WHERE { ?e a gmeow:Event }"
                .to_owned(),
        ),
        query_file: None,
        project_query_file: None,
        expect_ask: None,
        expect_row_count: None,
        exact_rows: false,
        expected_rows: vec![ExpectedRow {
            cells: vec![ExpectedCell {
                var: "e".to_owned(),
                value: TermValue::Iri("https://example.org/test/event1".to_owned()),
            }],
        }],
        reasoning: ReasoningProfile::None,
        data_file: Some("data.ttl".to_owned()),
        result_shape: None,
        input_shape: None,
        consumes: None,
        rationales: Vec::new(),
    };

    run_competency_cell(&store, &cq, dir).expect("overlay cell must pass");
    assert_eq!(
        store.quad_count(),
        0,
        "the overlay must not leak into the shared base dataset (the union builds a fresh one)"
    );

    // Same cell in the RDFS lane: hard-fail, never silently under-answer.
    let mut rdfs_cq = cq.clone();
    rdfs_cq.reasoning = ReasoningProfile::Rdfs;
    let err = run_competency_cell(&store, &rdfs_cq, dir)
        .expect_err("cqDataFile + reasoningRdfs must be rejected");
    assert!(
        err.message().contains("reasoningNone"),
        "unexpected error: {err}"
    );
}

/// The actual conformance handler distinguishes a named sole law from a shared
/// component code. Tiny synthetic laws exercise this contract without loading,
/// projecting or constructing any repository corpus.
#[test]
fn sole_finding_contract_has_both_pinned_and_unpinned_teeth() {
    let directory = tempfile::tempdir().expect("synthetic slice");
    let module = store_from_turtle("");
    let baseline = NativeChannelBaseline::measure(&module).expect("empty synthetic baseline");
    // gmeow-test-input: synthetic-only
    let shapes = ConformanceShapes::new(parse_shapes(r#"
        @prefix sh: <http://www.w3.org/ns/shacl#> .
        @prefix ex: <https://example.org/> .
        ex:FirstLaw a sh:NodeShape ; sh:targetNode ex:item ;
            sh:sparql [ sh:select "SELECT $this WHERE { FILTER NOT EXISTS { $this <https://example.org/first> ?value } }" ] .
        ex:SecondLaw a sh:NodeShape ; sh:targetNode ex:item ;
            sh:sparql [ sh:select "SELECT $this WHERE { FILTER NOT EXISTS { $this <https://example.org/second> ?value } }" ] .
    "#, None).expect("two synthetic laws"));
    let cell = |sole, pin: Option<&str>| ExampleConformance {
        iri: "https://example.org/soleProbe".to_owned(),
        file: "case.ttl".to_owned(),
        outcome: Outcome::Violates,
        violation_code: Some("shacl.SPARQLConstraintComponent".to_owned()),
        expected_source_shape: pin.map(ToOwned::to_owned),
        expected_sole_finding: sole,
        expected_failure_class: None,
        rationales: Vec::new(),
    };
    std::fs::write(
        directory.path().join("case.ttl"),
        "<https://example.org/item> a <https://example.org/Thing> .\n",
    )
    .expect("write two-law counter-example");
    let first = Some("https://example.org/FirstLaw");
    run_conformance_cell(
        &cell(None, first),
        directory.path(),
        &module,
        &shapes,
        &baseline,
    )
    .expect("the expected finding exists when soleness is not claimed");
    let error = run_conformance_cell(
        &cell(Some(true), first),
        directory.path(),
        &module,
        &shapes,
        &baseline,
    )
    .expect_err("a second named law cannot hide behind the same component code");
    assert!(
        error.message().contains("expectedSoleFinding")
            && error.message().contains("also raised")
            && error.message().contains("SecondLaw"),
        "{error}"
    );
    let error = run_conformance_cell(
        &cell(Some(true), None),
        directory.path(),
        &module,
        &shapes,
        &baseline,
    )
    .expect_err("an unpinned soleness claim has no identified law");
    assert!(
        error.message().contains("expectedSoleFinding")
            && error
                .message()
                .contains("without gmeow:expectedSourceShape"),
        "{error}"
    );
    std::fs::write(
        directory.path().join("case.ttl"),
        "<https://example.org/item> <https://example.org/second> \"present\" .\n",
    )
    .expect("write a sole-law counter-example");
    run_conformance_cell(
        &cell(Some(true), first),
        directory.path(),
        &module,
        &shapes,
        &baseline,
    )
    .expect("the correctly pinned sole finding passes");
}

#[test]
fn parsed_fixture_cache_observes_changed_invalid_and_missing_source() {
    let directory = tempfile::tempdir().expect("synthetic source");
    let path = directory.path().join("fixture.ttl");
    let one = "<urn:s> <urn:p> <urn:o> .\n";
    std::fs::write(&path, one).unwrap();
    let first = native_query::dataset_from_file(&path).expect("first content");
    let unchanged = native_query::dataset_from_file(&path).expect("unchanged content");
    assert!(Arc::ptr_eq(&first, &unchanged));
    std::fs::write(&path, format!("{one}<urn:s> <urn:q> <urn:new> .\n")).unwrap();
    let changed = native_query::dataset_from_file(&path).expect("changed content");
    assert_eq!(first.quad_count(), 1);
    assert_eq!(changed.quad_count(), 2);
    assert!(!Arc::ptr_eq(&first, &changed));
    std::fs::write(&path, "not a Turtle document").unwrap();
    let error = native_query::dataset_from_file(&path).unwrap_err();
    assert!(error.message().contains("parse"), "{error}");
    std::fs::remove_file(&path).unwrap();
    let error = native_query::dataset_from_file(&path).unwrap_err();
    assert!(error.message().contains("read"), "{error}");
}

/// The DECLARATIVE half of the same requirement has teeth.
///
/// `shapes/test-dsl-shapes.ttl` states the pin requirement as SHACL so a cell is
/// rejected at DSL-lint time, not only when the harness reaches it. That file is on
/// the `EXCLUDED` list of every shape union in the repository (it lints the test DSL,
/// never the data graph), and `dev_validate` does not yet populate
/// `test_dsl_shapes_ttl`, so nothing else executes it — which is exactly the shape a
/// rule takes when it is decorative. This runs the SHIPPED file against a synthetic
/// `gmeow:ExampleConformance` cell and requires it to red, so the rule cannot rot into
/// prose. The green half is the same cell with the pin bound.
#[test]
fn the_test_dsl_shapes_reject_an_unpinned_sole_finding_declaration() {
    let shapes_ttl = std::fs::read_to_string(paths::repo_root().join("shapes/test-dsl-shapes.ttl"))
        .expect("the test-DSL shape file is readable");
    let shapes = parse_shapes(&shapes_ttl, None).expect("the test-DSL shape file parses");

    let cell = |pin: &str| {
        format!(
            r#"
                @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
                @prefix ex: <https://example.org/> .
                ex:ec a gmeow:ExampleConformance ;
                    gmeow:exampleFile "tests/counter-examples/x.ttl" ;
                    gmeow:expectedOutcome gmeow:violates ;
                    gmeow:expectedViolationCode "shacl.MinCountConstraintComponent" ;
                    {pin}
                    gmeow:expectedSoleFinding true .
                "#
        )
    };
    let report = validate_dataset(&store_from_turtle(&cell("")), &shapes)
        .expect("validating the DSL cell succeeds");
    let unpinned: Vec<String> = report
        .results
        .iter()
        .filter(|r| matches!(r.severity, purrdf::shapes::report::Severity::Violation))
        .filter_map(gmeow_validate::findings::shacl_message)
        .collect();
    assert!(
        unpinned
            .iter()
            .any(|m| m.contains("must also bind gmeow:expectedSourceShape")),
        "the shape rule must reject a soleness declaration with no pinned law; got: \
             {unpinned:?}"
    );

    let pinned = validate_dataset(
        &store_from_turtle(&cell("gmeow:expectedSourceShape ex:SomeConstraintShape ;")),
        &shapes,
    )
    .expect("validating the pinned DSL cell succeeds");
    let remaining: Vec<String> = pinned
        .results
        .iter()
        .filter(|r| matches!(r.severity, purrdf::shapes::report::Severity::Violation))
        .filter_map(gmeow_validate::findings::shacl_message)
        .collect();
    assert!(
        remaining.is_empty(),
        "a pinned soleness declaration is well-formed and must raise nothing; got: \
             {remaining:?}"
    );
}

const MUSIC_PROJECTION_FIXTURE: &str = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex: <https://example.org/music-projection/> .
ex:pitch a gmeow:MusicalParameter .
ex:duration a gmeow:MusicalParameter .
ex:loss a gmeow:ProjectionLoss ; gmeow:accountsForParameter ex:duration .
ex:system a gmeow:NotationSystem .
ex:profile a gmeow:NotationProjectionProfile ;
    gmeow:notationSystemOf ex:system ;
    gmeow:representableParameter ex:pitch ;
    gmeow:declaredLoss ex:loss .
"#;

#[test]
fn music_projection_loss_guards_accept_complete_profiles_and_reject_each_witness() {
    // gmeow-test-input: synthetic-only
    // Read the authored query specification, never the repository music corpus.
    let slice = paths::repo_root().join("slices/extensions/music");
    let spec = dsl::load_spec(&slice.join("tests/structural.ttl")).unwrap();
    let complete = store_from_turtle(MUSIC_PROJECTION_FIXTURE);
    let inventory = spec
        .structural
        .iter()
        .find(|cell| cell.iri.ends_with("saNotationProjectionInventoriesExist"))
        .expect("music inventories are guarded against vacuous success");
    let inventory_pattern = inventory.pattern.as_deref().unwrap();
    assert!(run_ask(&complete, inventory_pattern).unwrap());
    assert!(!run_ask(&store_from_turtle(""), inventory_pattern).unwrap());

    for name in [
        "saNotationProfilesAccountForEveryParameter",
        "saProjectionLossesAccountForParameters",
        "saNotationSystemsHaveProjectionProfiles",
        "saNotationSystemsHaveOnlyOneProjectionProfile",
    ] {
        let cell = spec
            .structural
            .iter()
            .find(|cell| cell.iri.ends_with(name))
            .unwrap_or_else(|| panic!("the music specification must register {name}"));
        assert_eq!(cell.polarity, Polarity::MustNot);
        assert_eq!(cell.scope, Scope::Module);
        let pattern = cell.pattern.as_deref().unwrap();
        assert!(
            !run_ask(&complete, pattern).unwrap(),
            "{name} rejects complete accounting"
        );
        let witness = std::fs::read_to_string(
            slice.join(
                cell.fail_witness
                    .as_ref()
                    .expect("each omission ban needs a registered failing witness"),
            ),
        )
        .unwrap();
        let broken = store_from_turtle(&format!("{MUSIC_PROJECTION_FIXTURE}\n{witness}"));
        assert!(
            run_ask(&broken, pattern).unwrap(),
            "{name} must detect its registered witness"
        );
    }
}
