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
        rationale: None,
    }
}

const Q_X: &str = "PREFIX ex: <https://example.org/> \
        SELECT ?x WHERE { ?x a ex:Thing }";

fn one_thing_store() -> Arc<RdfDataset> {
    store_from_turtle("@prefix ex: <https://example.org/> .\nex:a a ex:Thing .\n")
}

fn authenticated_shape_union() -> &'static str {
    static SHAPES: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SHAPES.get_or_init(|| {
        String::from_utf8(
            gmeow_bundle_import::load_authenticated_corpus_artifact(
                &paths::repo_root(),
                "validate-conformance-shapes.ttl",
            )
            .expect("load producer-selected conformance shapes read-only"),
        )
        .expect("authenticated conformance shapes are UTF-8")
    })
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
        rationale: None,
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
        rationale: None,
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

/// `gmeow:expectedSoleFinding` can FAIL, on the production conformance surface.
///
/// The red half drives the shipped `compensation-typed-as-its-forward-receipt.ttl`
/// counter-example — the one fixture in the enactment corpus whose single authored
/// defect is MEASURED to cascade into three laws — through the real
/// `run_conformance_cell`, against the real generated shape surface, with the
/// sole-finding flag set. Without the flag the cell passes, because every check
/// before it is an EXISTENCE check: some finding carries the code, and some finding
/// carrying it came from the pinned shape. That is precisely the gap that made
/// "and NO other finding" unfalsifiable everywhere it is written, so the green half
/// is the SAME cell with the flag unbound, and the difference between them is the
/// whole of what the new field buys.
fn the_sole_finding_flag_rejects_a_fixture_that_trips_a_second_law() {
    let slice_dir = paths::repo_root().join("slices/grounding/logic");
    let shapes_ttl = authenticated_shape_union();
    let shapes = parse_shapes(shapes_ttl, None).expect("authenticated shape surface parses");
    let owned_module = native_query::with_owl_rdfs_projection(
        &native_query::dataset_from_file(&paths::module_file(&slice_dir))
            .expect("the logic module parses"),
    );
    let module = native_query::with_owl_rdfs_projection(
        &native_query::dataset_from_files(&paths::conformance_module_files(&slice_dir))
            .expect("the grounding kernel modules parse"),
    );
    let local_shapes = slice_dir.join("shapes.ttl");
    let shapes = scope_shapes_to_slice(
        shapes,
        &owned_module,
        local_shapes.is_file().then_some(local_shapes.as_path()),
    )
    .expect("shape ownership scopes to the logic slice");
    let shapes = ConformanceShapes::new(shapes);

    let cell = |sole: Option<bool>| ExampleConformance {
        iri: "https://example.org/ecSoleFindingProbe".to_owned(),
        file: "tests/counter-examples/compensation-typed-as-its-forward-receipt.ttl".to_owned(),
        outcome: Outcome::Violates,
        violation_code: Some("shacl.SPARQLConstraintComponent".to_owned()),
        expected_source_shape: Some(
            "https://blackcatinformatics.ca/logic/\
                 CompensationNotInverseConstraintProceduralConstraintShape"
                .to_owned(),
        ),
        expected_sole_finding: sole,
        expected_failure_class: None,
        rationale: None,
    };

    let baseline =
        NativeChannelBaseline::measure(&module).expect("the logic module measures cleanly");

    run_conformance_cell(&cell(None), &slice_dir, &module, &shapes, &baseline).expect(
        "without the flag the cell passes on the existence checks alone — which is the \
             behaviour every unmigrated cell keeps",
    );

    let err = run_conformance_cell(&cell(Some(true)), &slice_dir, &module, &shapes, &baseline)
        .expect_err("the cascade must be caught once the cell claims sole-ness");
    let message = err.message();
    assert!(
        message.contains("expectedSoleFinding") && message.contains("also raised"),
        "the failure must name the flag and enumerate the intruding findings, so an \
             author can see WHICH other law fired; got: {message}"
    );
    assert!(
        message.contains("ReceiptRequiresAttemptConstraint")
            || message.contains("CompensationBindsExactForwardEffectConstraint"),
        "the intruder list must name the cascading laws by shape; got: {message}"
    );
}

/// An UNPINNED `gmeow:expectedSoleFinding` is a hard failure, and the fixture that
/// proves why is one the old fallback could not fail on.
///
/// `translation-unanalyzed-overclaim.ttl` trips TWO distinct lang laws —
/// `lang:UnmarkedSourceOverclaimConstraintProceduralConstraintShape` and
/// `lang:UnmarkedTargetOverclaimConstraintProceduralConstraintShape` — and BOTH raise
/// `shacl.SPARQLConstraintComponent`. The first cut's unpinned reading asked only
/// whether some OTHER shape raised a finding carrying the expected code, so on this
/// fixture every violating shape answered "yes, I am one of them" and the intruder set
/// came back empty: the cell claimed soleness, tripped two laws, and went green. That
/// is the vacuity, and it is why the pin is now REQUIRED rather than a fallback.
///
/// The two halves are the same cell differing only in the pin: unpinned is rejected as
/// a cell-configuration failure that names the missing property, and pinned is rejected
/// for the real reason, naming the SECOND law by shape.
fn an_unpinned_sole_finding_claim_is_a_hard_failure() {
    let slice_dir = paths::repo_root().join("slices/grounding/lang");
    let shapes_ttl = authenticated_shape_union();
    let shapes = parse_shapes(shapes_ttl, None).expect("authenticated shape surface parses");
    let owned_module = native_query::with_owl_rdfs_projection(
        &native_query::dataset_from_file(&paths::module_file(&slice_dir))
            .expect("the lang module parses"),
    );
    let module = native_query::with_owl_rdfs_projection(
        &native_query::dataset_from_files(&paths::conformance_module_files(&slice_dir))
            .expect("the grounding kernel modules parse"),
    );
    let local_shapes = slice_dir.join("shapes.ttl");
    let shapes = scope_shapes_to_slice(
        shapes,
        &owned_module,
        local_shapes.is_file().then_some(local_shapes.as_path()),
    )
    .expect("shape ownership scopes to the lang slice");
    let shapes = ConformanceShapes::new(shapes);

    let cell = |pin: Option<&str>| ExampleConformance {
        iri: "https://example.org/ecUnpinnedSoleProbe".to_owned(),
        file: "tests/counter-examples/translation-unanalyzed-overclaim.ttl".to_owned(),
        outcome: Outcome::Violates,
        violation_code: Some("shacl.SPARQLConstraintComponent".to_owned()),
        expected_source_shape: pin.map(ToOwned::to_owned),
        expected_sole_finding: Some(true),
        expected_failure_class: None,
        rationale: None,
    };

    let baseline =
        NativeChannelBaseline::measure(&module).expect("the lang module measures cleanly");

    let err = run_conformance_cell(&cell(None), &slice_dir, &module, &shapes, &baseline)
        .expect_err(
            "a soleness claim with no named law must be rejected outright — under the old \
             fallback this very cell passed while the fixture tripped two laws",
        );
    let message = err.message();
    assert!(
        message.contains("expectedSoleFinding")
            && message.contains("without gmeow:expectedSourceShape"),
        "the failure must name both properties so an author knows what to bind; got: {message}"
    );

    let err = run_conformance_cell(
        &cell(Some(
            "https://blackcatinformatics.ca/lang/\
                 UnmarkedSourceOverclaimConstraintProceduralConstraintShape",
        )),
        &slice_dir,
        &module,
        &shapes,
        &baseline,
    )
    .expect_err("once the law is named, the SECOND law raising the same code is an intruder");
    let message = err.message();
    assert!(
        message.contains("also raised")
            && message.contains("UnmarkedTargetOverclaimConstraintProceduralConstraintShape"),
        "the intruder list must name the second law by shape, not merely by component code \
             (both laws raise shacl.SPARQLConstraintComponent); got: {message}"
    );
}

#[test]
fn sole_finding_contract_has_both_pinned_and_unpinned_teeth() {
    // One authenticated whole-shape parse serves both halves of this single
    // contract. Under nextest, separate #[test] cases are separate processes and
    // would each restore and parse the same corpus product.
    the_sole_finding_flag_rejects_a_fixture_that_trips_a_second_law();
    an_unpinned_sole_finding_claim_is_a_hard_failure();
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
        .filter_map(|r| r.message.clone())
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
        .filter_map(|r| r.message.clone())
        .collect();
    assert!(
        remaining.is_empty(),
        "a pinned soleness declaration is well-formed and must raise nothing; got: \
             {remaining:?}"
    );
}
