// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn validation_flat_view_keeps_statement_metadata_and_original_worlds() {
    use purrdf::{RdfAnnotation, RdfDatasetBuilder, RdfLiteral, RdfReifier, RdfTerm, RdfTriple};

    let reifier = RdfTerm::iri("urn:claim");
    let mut builder = RdfDatasetBuilder::new();
    for (world, object) in [
        ("urn:world:a", "urn:value:a"),
        ("urn:world:b", "urn:value:b"),
    ] {
        builder.push_owned_reifier(
            &RdfReifier::new(
                reifier.clone(),
                RdfTriple::new(
                    RdfTerm::iri("urn:subject"),
                    "urn:predicate",
                    RdfTerm::iri(object),
                ),
            )
            .in_graph(Some(RdfTerm::iri(world))),
        );
        builder.push_owned_annotation(
            &RdfAnnotation::new(
                reifier.clone(),
                "urn:source",
                RdfTerm::Literal(RdfLiteral::simple(world)),
            )
            .in_graph(Some(RdfTerm::iri(world))),
        );
    }
    let original = builder.freeze().expect("two explicitly scoped claims");
    let flat = flatten_to_default_graph(&original).expect("validation projection");
    let statements: std::collections::HashSet<_> = flat
        .owned_reifiers()
        .map(|row| {
            assert_eq!(row.reifier, reifier);
            assert_eq!(row.graph, None);
            row.statement.object
        })
        .collect();
    assert_eq!(
        statements,
        std::collections::HashSet::from([RdfTerm::iri("urn:value:a"), RdfTerm::iri("urn:value:b")])
    );
    let sources: std::collections::HashSet<_> = flat
        .owned_annotations()
        .map(|row| {
            assert_eq!(row.reifier, reifier);
            assert_eq!(row.graph, None);
            assert_eq!(row.predicate, "urn:source");
            row.object
        })
        .collect();
    assert_eq!(
        sources,
        std::collections::HashSet::from([
            RdfTerm::Literal(RdfLiteral::typed(
                "urn:world:a",
                "http://www.w3.org/2001/XMLSchema#string"
            )),
            RdfTerm::Literal(RdfLiteral::typed(
                "urn:world:b",
                "http://www.w3.org/2001/XMLSchema#string"
            )),
        ])
    );
    assert_eq!(original.named_graphs().count(), 2);
    assert!(original.owned_reifiers().all(|row| row.graph.is_some()));
    assert!(original.owned_annotations().all(|row| row.graph.is_some()));
    assert!(Arc::ptr_eq(
        &flat,
        &flatten_to_default_graph(&flat).unwrap()
    ));
}

#[test]
fn fixture_shape_selections_preserve_member_order_and_exclusions() {
    let members = vec![
        ("z.ttl".to_owned(), b"# z".to_vec()),
        ("validation-shapes.ttl".to_owned(), b"# validation".to_vec()),
        ("result-shapes.ttl".to_owned(), b"# result".to_vec()),
        ("a.ttl".to_owned(), b"# a".to_vec()),
    ];
    // gmeow-test-input: synthetic-only; four tiny archive members.
    let first = shape_corpus_variants_from_members(&members).unwrap();
    let reversed: Vec<_> = members.into_iter().rev().collect();
    // gmeow-test-input: synthetic-only
    let second = shape_corpus_variants_from_members(&reversed).unwrap();
    assert_eq!(first.production, second.production);
    assert_eq!(first.conformance, second.conformance);
    assert_eq!(first.domain_conformance, second.domain_conformance);
    assert_eq!(first.production, "# a\n# result\n# validation\n# z\n");
    assert_eq!(first.conformance, "# a\n# result\n# z\n");
    assert_eq!(first.domain_conformance, "# a\n# validation\n# z\n");
}

#[test]
fn is_json_ld_matches_ids_and_media_type() {
    assert!(is_json_ld("json-ld"));
    assert!(is_json_ld("jsonld"));
    assert!(is_json_ld("application/ld+json"));
    assert!(is_json_ld("  JSON-LD  "));
    assert!(!is_json_ld("turtle"));
    assert!(!is_json_ld("application/json"));
}

#[test]
fn deep_pass_failure_folds_advisory_note_and_preserves_tier1() {
    // Graceful degradation (AC2): when the Tier-2 pass cannot run — here the
    // bundle bytes are unreadable, so import_gts_events fails — the pre-existing
    // Tier-1 findings survive unchanged and exactly one validate.deep.unavailable
    // advisory Note is folded. No panic, no error propagation.
    let mut report = Report::new("validate");
    report.add_finding(
        Finding::new(
            Severity::Error,
            "tier1.fixture",
            "a pre-existing Tier-1 finding",
        )
        .with_tool("validate"),
    );

    let outcome = purrdf::import_gts_events(b"not a gts bundle")
        .map(|_| ())
        .map_err(|error| DeepPassError::Unavailable(format!("GTS read error: {error}")));
    fold_deep_outcome(outcome, report.findings.len(), "fixture.ttl", &mut report);

    let unavailable: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.code == "validate.deep.unavailable")
        .collect();
    assert_eq!(
        unavailable.len(),
        1,
        "exactly one advisory note on a failed deep pass: {:?}",
        report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );
    assert_eq!(unavailable[0].severity, Severity::Note);
    assert_eq!(
        unavailable[0]
            .locations
            .first()
            .and_then(|l| l.path.as_deref()),
        Some("fixture.ttl"),
        "validate.deep.unavailable must carry the origin path as its location"
    );
    assert!(
        report.findings.iter().any(|f| f.code == "tier1.fixture"),
        "the pre-existing Tier-1 finding must be preserved"
    );
    // No inconsistency error was fabricated from the failed pass.
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.code == "validate.deep.inconsistent")
    );
}

/// Build canonical GTS bytes from an arbitrary Turtle string for use in
/// deep-pass tests. Mirrors the same helper in `validate_all` tests.
fn gts_bytes_from_turtle(ttl: &str) -> Vec<u8> {
    let dataset =
        purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse test turtle");
    // gmeow-test-input: synthetic-only
    purrdf::gts_write::to_gts(
        &dataset,
        &purrdf::RdfLookaside::default(),
        "gmeow-validate-data-deep-test",
    )
    .expect("encode GTS bytes")
}

#[test]
fn deep_pass_rejects_missing_native_laws_as_a_required_contract() {
    let bytes = gts_bytes_from_turtle("<urn:test:s> <urn:test:p> <urn:test:o> .");
    let mut report = Report::new("validate");
    let bundle = purrdf::import_gts_events(&bytes).expect("tiny native bundle");
    let user = data_dataset(b"<urn:user:s> <urn:user:p> <urn:user:o> .", "turtle")
        .expect("tiny user graph");
    run_deep_pass(None, &bundle.dataset, &user, "synthetic.ttl", &mut report);
    assert!(report.findings.iter().any(|finding| finding.code
        == crate::codes::VALIDATE_DEEP_CONTRACT_INVALID
        && finding.severity == Severity::Error
        && finding.message.contains("native verification laws")));
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.code == crate::codes::VALIDATE_DEEP_UNAVAILABLE),
        "a missing selected law capability must not become a successful advisory-only deep pass"
    );
}

#[test]
fn missing_deep_archive_preserves_tier1_and_matches_prepared_validation() {
    let shape_text = br#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
<urn:shape:required> a sh:NodeShape ; sh:targetNode <urn:user> ;
    sh:property [ sh:path <urn:required> ; sh:minCount 1 ;
                  sh:message "Required product value is absent" ] .
"#;
    let dataset = data_dataset(b"<urn:ontology> <urn:describes> <urn:user> .", "turtle")
        .expect("tiny ontology");
    let mut builder = purrdf::gts_compose::SnapshotBuilder::new();
    builder.add_dataset(&dataset).expect("tiny ontology rows");
    let archive =
        purrdf::ustar::write_archive_borrowed([("validation-shapes.ttl", shape_text.as_slice())])
            .expect("tiny selected shapes");
    // gmeow-test-input: synthetic-only; one shape and one ontology triple.
    let bytes = {
        let emission = gmeow_gts_profile::emit_gmeow_gts(
            builder,
            vec![purrdf::gts_compose::BlobRow {
                data: archive,
                media_type: "application/x-tar".to_owned(),
                rep: REP_SHAPES.to_owned(),
            }],
            Vec::new(),
            None,
            &gmeow_gts_profile::baseline_medium_plan(),
        )
        .expect("tiny product bundle without deep laws");
        assert!(
            emission.ingestion.declarations_omitted.is_empty(),
            "unexpected GMEOW fixture graph omissions: {:?}",
            emission.ingestion.declarations_omitted
        );
        emission.bytes
    };
    let user = b"<urn:user> <urn:other> <urn:value> .";
    let report = run(user, "turtle", &bytes, "urn:", "user.ttl", true)
        .expect("missing deep archive still returns a failing product report");
    let imported = import_validation_bundle(&bytes, false).expect("required shallow input");
    let shapes = Tier1Shapes::from_imported(&imported).expect("selected shapes");
    let prepared = run_with(
        BundleParts {
            native_gates: Some(Err(import_validation_bundle(&bytes, true).unwrap_err())),
            shapes: &shapes,
            dataset: &imported.bundle.dataset,
        },
        user,
        "turtle",
        "urn:",
        "user.ttl",
        true,
    )
    .expect("prepared product report");
    assert_eq!(
        serde_json::to_value(&report).unwrap(),
        serde_json::to_value(&prepared).unwrap()
    );
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.severity == Severity::Error
                && finding.message.contains("Required product value is absent"))
    );
    assert_eq!(
        report
            .findings
            .iter()
            .filter(
                |finding| finding.code == crate::codes::VALIDATE_DEEP_CONTRACT_INVALID
                    && finding.severity == Severity::Error
            )
            .count(),
        1
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|finding| finding.code == crate::codes::VALIDATE_DEEP_UNAVAILABLE)
    );
}

/// Regression guard for the hard-fail discipline: a bundle whose declared
/// `logic:ReasoningContract` carries a GARBLED `logic:admissibleValuation`
/// (here `logic:Nonsense`, an unrecognised local name) must produce a
/// `Severity::Error` finding with code `validate.deep.contract-invalid`, NOT
/// a `validate.deep.unavailable` advisory Note. The gate must FAIL.
///
/// This test catches the defect where `run_deep_pass` was collapsing both
/// failure modes (invalid input and infrastructure unavailability) into a
/// single non-failing advisory, silently passing a bundle with invalid data.
#[test]
fn deep_pass_garbled_contract_produces_error_not_advisory() {
    let garbled_bundle = gts_bytes_from_turtle(
        "\
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
logic:c rdf:type logic:ReasoningContract ;
    logic:admissibleValuation logic:Nonsense .
",
    );

    let mut report = Report::new("validate");
    let bundle = purrdf::import_gts_events(&garbled_bundle).expect("tiny garbled-contract bundle");
    let user = data_dataset(
            b"<http://example.org/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/T> .\n",
            "n-triples",
        ).expect("tiny user graph");
    run_deep_pass(None, &bundle.dataset, &user, "fixture.nt", &mut report);

    // Must NOT fold an advisory note — that is the defect this test guards against.
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.code == "validate.deep.unavailable"),
        "a garbled contract must NOT produce an advisory note (validate.deep.unavailable); \
             it is invalid INPUT, not an availability failure: {:?}",
        report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );

    // Must fold a hard-fail Error finding.
    let contract_error: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.code == "validate.deep.contract-invalid")
        .collect();
    assert_eq!(
        contract_error.len(),
        1,
        "exactly one validate.deep.contract-invalid error must be emitted: {:?}",
        report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );
    assert_eq!(
        contract_error[0].severity,
        Severity::Error,
        "a garbled contract policy must be a hard-fail Error finding"
    );
    assert_eq!(
        contract_error[0]
            .locations
            .first()
            .and_then(|l| l.path.as_deref()),
        Some("fixture.nt"),
        "the contract-invalid finding must carry the origin path"
    );
    assert!(
        !report.ok(),
        "a garbled contract policy must fail the gate (report.ok() must be false)"
    );
}

#[test]
fn report_json_round_trips() {
    // The wasm/CLI boundary (`validate_json`) serializes a Report to JSON; this
    // guards that the canonical Report model round-trips through serde_json so a
    // client can parse the findings back losslessly.
    let mut report = Report::new("validate");
    report.add_finding(
        Finding::new(Severity::Error, "tier1.fixture", "a fixture finding").with_tool("validate"),
    );
    let json = serde_json::to_string(&report).expect("Report must serialize to JSON");
    let back: Report = serde_json::from_str(&json).expect("Report JSON must deserialize back");
    assert_eq!(
        report, back,
        "Report must round-trip through JSON unchanged"
    );
}

#[test]
fn validate_json_surfaces_missing_shapes_as_err_string() {
    // A plain GTS bundle carries no `shapes-archive` blob, so the wasm/CLI entry
    // must return an Err STRING (not panic) that names the missing surface — the
    // no-optionality hard-fail surfaced as a boundary-friendly error.
    let bundle = gts_bytes_from_turtle("@prefix ex: <http://example.org/> .\nex:a ex:b ex:c .\n");
    let err = validate_json(
            b"<http://example.org/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/T> .\n",
            "n-triples",
            &bundle,
            "https://blackcatinformatics.ca/gmeow/",
            "fixture.nt",
        )
        .expect_err("a bundle without a shapes-archive must be an Err");
    assert!(err.is::<crate::error::Dataset>());
    assert!(
        err.message().contains("shapes-archive"),
        "the error must name the missing bundle surface: {}",
        err.message()
    );
}

/// A tiny explicit SHACL/ontology fixture exercises the same resident
/// constructor as the production consumer, without a corpus or bundle.
fn tier1_from_ttl(shapes_ttl: &str, ontology_ttl: &str) -> Tier1Shapes {
    let ontology =
        purrdf::parse_dataset(ontology_ttl.as_bytes(), "text/turtle", None).expect("ontology ds");
    Tier1Shapes::from_shapes_and_ontology(shapes_ttl, ontology).expect("prepare Tier-1 shapes")
}

#[test]
fn property_failure_identity_and_retained_shapes_reach_the_consumer() {
    // gmeow-test-input: synthetic-only
    let tier1 = tier1_from_ttl(
        r#"@prefix sh: <http://www.w3.org/ns/shacl#> .
               @prefix ex: <https://ex/> .
               @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
               ex:Law a sh:NodeShape ; sh:targetNode ex:item ;
                   sh:property [ sh:path ex:required ; sh:minCount 1 ;
                       gmeow:enforcesFailureClass ex:RequiredValueFailure ] ."#,
        "",
    );
    let retained = tier1.parsed_shapes().dataset();
    let retained_index = crate::findings::FailureClassIndex::from_shapes_dataset(retained);
    assert_eq!(tier1.failure_classes, retained_index);
    let enforces = retained
        .term_id_by_value(&TermValue::iri(
            crate::findings::GMEOW_ENFORCES_FAILURE_CLASS,
        ))
        .expect("the property shape declares its failure class");
    let property = retained
        .quads_for_pattern(None, Some(enforces), None, GraphMatch::Any)
        .next()
        .expect("the failure annotation lives in the retained shape dataset");
    let property_identity =
        purrdf::shapes::term::term_id_to_native(retained, property.s).to_string();
    assert!(property_identity.starts_with("_:"));
    let report = tier1
        .validate(b"", "turtle", "https://ex/", "synthetic.ttl")
        .expect("validate synthetic product input");
    assert_eq!(report.findings.len(), 1);
    let finding = &report.findings[0];
    assert_eq!(finding.code, "shacl.MinCountConstraintComponent");
    assert_eq!(
        finding.failure_class.as_deref(),
        Some("https://ex/RequiredValueFailure")
    );
    assert_eq!(
        finding.detail.as_deref(),
        Some(format!("source shape: {property_identity}").as_str())
    );
}

/// The consumer-path wiring proof (F1): `Tier1Shapes::validate` — the shared core
/// `gmeow validate <file>` and the MCP `validate_local` tool both reach — applies the
/// advisory split. A bare `gmeow:Entity` individual (the anti-pattern the Info-severity
/// advisory guard matches) must surface as a `Severity::Note`, `advice.*` finding
/// carrying the formalized term's howToUse suggestion and a "Use when:" useWhen entry —
/// NOT a raw `shacl.* Info` finding for that shape.
#[test]
fn validate_wires_the_advisory_split_for_a_bare_entity() {
    const SHAPES: &str = r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
<https://ex.test/EntityAdviceShape> a sh:NodeShape ;
    logic:formalizes gmeow:Entity ;
    sh:targetClass gmeow:Entity ;
    sh:sparql [
        a sh:SPARQLConstraint ;
        sh:severity sh:Info ;
        sh:message "prefer a more specific sortal than bare gmeow:Entity" ;
        sh:select "SELECT $this WHERE { $this a <https://blackcatinformatics.ca/gmeow/Entity> }" ;
    ] .
"#;
    const ONTOLOGY: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:Entity gmeow:howToUse \"Type each instance with its most specific sortal.\"@x-gmeow-english ;
    gmeow:useWhen \"Use for a genuinely category-neutral resource.\"@x-gmeow-english .
";
    let tier1 = tier1_from_ttl(SHAPES, ONTOLOGY);
    let data = "<https://ex.test/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                    <https://blackcatinformatics.ca/gmeow/Entity> .\n";
    let report = tier1
        .validate(
            data.as_bytes(),
            "n-triples",
            "https://blackcatinformatics.ca/gmeow/",
            "user-data.ttl",
        )
        .expect("Tier-1 validate must succeed");

    // The advisory is a Note, code in the advice.* family.
    let advice: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.code.starts_with(crate::codes::ADVICE_FAMILY))
        .collect();
    assert_eq!(
        advice.len(),
        1,
        "exactly one advice.* Note finding must be wired in by validate: {:?}",
        report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );
    let advice = advice[0];
    assert_eq!(advice.severity, Severity::Note);

    // The raw shacl.* Info finding for the advisory shape must have been SUPPRESSED.
    assert!(
            !report
                .findings
                .iter()
                .any(|f| f.code.starts_with(crate::codes::SHACL_FAMILY)
                    && f.severity == Severity::Info),
            "the raw shacl.* Info finding must be suppressed once split into advice: {:?}",
            report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
        );

    // howToUse populates the suggestions verbatim; useWhen surfaces as guidance.
    assert!(
        advice
            .suggestions
            .iter()
            .any(|s| s == "Type each instance with its most specific sortal."),
        "the advice must carry the term's gmeow:howToUse as a suggestion: {:?}",
        advice.suggestions
    );
    assert!(
        advice
            .suggestions
            .iter()
            .any(|s| s == "Use when: Use for a genuinely category-neutral resource."),
        "the advice must carry the term's gmeow:useWhen as a \"Use when:\" entry: {:?}",
        advice.suggestions
    );
}

/// The shared subclass-hierarchy fixture for the [`inject_subclass_shortcuts`] proofs
/// below: `ex:A ⊑ ex:B ⊑ ex:C` (a two-hop chain, so a one-hop-only fix would fail the
/// transitivity proof), plus an unrelated `ex:D` with no subsumption edge to `ex:A`.
const SUBCLASS_ONTOLOGY: &str = "\
@prefix ex: <https://ex.test/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
ex:A rdfs:subClassOf ex:B .
ex:B rdfs:subClassOf ex:C .
";

/// A `sh:targetClass` shape requiring `ex:name` on instances of `class_local`
/// (e.g. `C`, `D`) — the pattern that is dead when every real instance is
/// typed with a proper subclass rather than the targeted class itself.
fn subclass_probe_shapes(class_local: &str) -> String {
    format!(
        "\
@prefix ex: <https://ex.test/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
<https://ex.test/{class_local}Shape> a sh:NodeShape ;
    sh:targetClass ex:{class_local} ;
    sh:property [ sh:path ex:name ; sh:minCount 1 ] .
"
    )
}

/// Bundle-hierarchy regression: a focus node typed ONLY as a proper subclass (`ex:x a
/// ex:A`, never `ex:x a ex:C` directly) IS selected by a shape whose `sh:targetClass`
/// names an ANCESTOR (`ex:C`) the isolated data graph never restates — the exact defect
/// the shipped `gmeow validate <file>` CLI hit on `math:ArgumentSlotContiguityConstraint`
/// (`sh:targetClass math:MathematicalExpression` never selecting an
/// `math:ApplicationExpression`-typed root). Without [`inject_subclass_shortcuts`], this
/// finding is silently absent because the isolated data graph carries no
/// `rdfs:subClassOf` triple at all.
#[test]
fn subclass_typed_focus_node_is_selected_across_the_bundle_hierarchy() {
    let tier1 = tier1_from_ttl(&subclass_probe_shapes("C"), SUBCLASS_ONTOLOGY);
    let data = "<https://ex.test/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                    <https://ex.test/A> .\n";
    let report = tier1
        .validate(
            data.as_bytes(),
            "n-triples",
            "https://ex.test/",
            "user-data.ttl",
        )
        .expect("Tier-1 validate must succeed");

    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code.starts_with(crate::codes::SHACL_FAMILY)),
        "a node typed only as a proper subclass of the shape's sh:targetClass must still \
             be selected as a focus node (missing ex:name must be flagged): {:?}",
        report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );
}

/// Bundle-hierarchy regression, the negative twin: a shape targeting an UNRELATED class
/// (`ex:D`, no subsumption edge to/from `ex:A` in [`SUBCLASS_ONTOLOGY`]) must NOT select
/// an `ex:A`-typed focus node — the shortcut injection must not over-approximate and
/// select every instance for every shape regardless of its real class.
#[test]
fn unrelated_class_shape_is_not_selected() {
    let tier1 = tier1_from_ttl(&subclass_probe_shapes("D"), SUBCLASS_ONTOLOGY);
    let data = "<https://ex.test/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                    <https://ex.test/A> .\n";
    let report = tier1
        .validate(
            data.as_bytes(),
            "n-triples",
            "https://ex.test/",
            "user-data.ttl",
        )
        .expect("Tier-1 validate must succeed");

    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.code.starts_with(crate::codes::SHACL_FAMILY)),
        "a shape targeting an unrelated, unconnected class must select NO focus node: {:?}",
        report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );
}

/// Bundle-hierarchy regression, the transitivity proof: the shape targets `ex:C`, the data
/// is typed only `ex:A`, and [`SUBCLASS_ONTOLOGY`] connects them ONLY via the two-hop
/// chain `ex:A ⊑ ex:B ⊑ ex:C` — `ex:A` carries no DIRECT `rdfs:subClassOf ex:C` edge, so
/// this fails if the shortcut injection only walked one hop instead of the full
/// transitive ancestor set ([`gufo::proper_ancestors`], the same BFS the OntoUML
/// disciplines already trust).
#[test]
fn subclass_shortcut_injection_is_transitive_across_two_hops() {
    // Sanity: the fixture really is two hops, not a direct edge (guards against a
    // fixture typo silently turning this into the single-hop test above).
    let ontology = purrdf::parse_dataset(SUBCLASS_ONTOLOGY.as_bytes(), "text/turtle", None)
        .expect("ontology parses");
    assert!(
        !gufo::proper_ancestors(&ontology, "https://ex.test/A").is_empty(),
        "fixture sanity: ex:A must have at least one ancestor"
    );
    assert!(
        SUBCLASS_ONTOLOGY
            .lines()
            .filter(|l| l.contains("ex:A") && l.contains("ex:C"))
            .count()
            == 0,
        "fixture sanity: ex:A must NOT carry a direct edge to ex:C"
    );

    let tier1 = tier1_from_ttl(&subclass_probe_shapes("C"), SUBCLASS_ONTOLOGY);
    let data = "<https://ex.test/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
                    <https://ex.test/A> .\n";
    let report = tier1
        .validate(
            data.as_bytes(),
            "n-triples",
            "https://ex.test/",
            "user-data.ttl",
        )
        .expect("Tier-1 validate must succeed");

    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code.starts_with(crate::codes::SHACL_FAMILY)),
        "a two-hop transitive ancestor (ex:A ⊑ ex:B ⊑ ex:C) must still be reached by the \
             shortcut injection: {:?}",
        report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );
}
