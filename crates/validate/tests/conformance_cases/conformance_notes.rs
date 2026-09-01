// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_notes.py
//!
//! Each batched case builds an inline Turtle graph containing the triples that
//! the Python test assembled via `g.add(...)`, converts to N-Triples, and
//! validates against the whole shapes corpus.
//!
//! The cross-slice TBox membership twins, the exact motivation-count twin, and
//! the four projection `.rq` parse+eval guards (formerly retained in Python)
//! now live below as native conformance tests.

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;
use regex::Regex;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_ANNOTATION_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AnnotationProperty";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// A minimal graph carrying just the `gmeow:` prefix, used as the source for the
/// projection `.rq` parse+eval guards below.
fn empty_gmeow_graph() -> GraphStore {
    GraphStore::parse_ttl("@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n")
}

// ── Cross-slice structural guards (subjects not in notes/module.ttl) ──────────

/// Twin of `test_evidence_span_is_information_object`: `gmeow:EvidenceSpan`
/// (evidencespan slice) is `rdfs:subClassOf gmeow:InformationObject`.
#[gmeow_test_batch_macros::batch_test]
fn evidence_span_is_information_object() {
    let g = GraphStore::ontology();
    assert!(
        g.has(
            Some(&gm("EvidenceSpan")),
            Some(RDFS_SUBCLASS_OF),
            Some(&gm("InformationObject"))
        ),
        "gmeow:EvidenceSpan must be rdfs:subClassOf gmeow:InformationObject"
    );
}

/// Twin of `test_selector_sub_class_of_evidence_span`: `gmeow:Selector`
/// (evidencespan slice) is `rdfs:subClassOf gmeow:EvidenceSpan`.
#[gmeow_test_batch_macros::batch_test]
fn selector_sub_class_of_evidence_span() {
    let g = GraphStore::ontology();
    assert!(
        g.has(
            Some(&gm("Selector")),
            Some(RDFS_SUBCLASS_OF),
            Some(&gm("EvidenceSpan"))
        ),
        "gmeow:Selector must be rdfs:subClassOf gmeow:EvidenceSpan"
    );
}

/// Twin of `test_notes_are_standpoint_indexed`: the standpoint machinery
/// (`gmeow:accordingTo`, standpoint slice) is an `owl:AnnotationProperty`
/// available to notes via the statement/provenance layer.
#[gmeow_test_batch_macros::batch_test]
fn notes_are_standpoint_indexed() {
    let g = GraphStore::ontology();
    assert!(
        g.has(
            Some(&gm("accordingTo")),
            Some(RDF_TYPE),
            Some(OWL_ANNOTATION_PROPERTY)
        ),
        "gmeow:accordingTo must be an owl:AnnotationProperty"
    );
}

// ── Open value vocabulary (dynamic exact count) ───────────────────────────────

/// Twin of `test_motivation_values_are_individuals`: exactly ten seed
/// `gmeow:AnnotationMotivation` individuals exist (`len(…) == 10`).
#[gmeow_test_batch_macros::batch_test]
fn motivation_values_are_individuals() {
    let g = GraphStore::ontology();
    assert_eq!(
        g.subjects_of_type(&gm("AnnotationMotivation")).len(),
        10,
        "expected exactly 10 gmeow:AnnotationMotivation seed individuals"
    );
}

// ── Generated query header hygiene ───────────────────────────────────────────

/// Generated query headers are published metadata and must not carry local
/// tracker tokens consisting of a hash followed by one to five decimal digits.
#[gmeow_test_batch_macros::batch_test]
fn generated_query_headers_exclude_tracker_tokens() {
    let tracker_token = Regex::new(r"#[0-9]{1,5}\b").expect("tracker-token regex must compile");

    for digits in ["1", "12", "123", "1234", "12345"] {
        let token = format!("#{digits}");
        assert!(
            tracker_token.is_match(&token),
            "policy regex must reject tracker token {token}"
        );
    }
    for suffix in ["123456", "TextPositionSelector"] {
        let identifier = format!("#{suffix}");
        assert!(
            !tracker_token.is_match(&identifier),
            "policy regex must allow technical identifier {identifier}"
        );
    }
    for identifier in ["RDF 1.2", "SHA-256", "schema:ClaimReview"] {
        assert!(
            !tracker_token.is_match(identifier),
            "policy regex must allow technical identifier {identifier}"
        );
    }

    let queries = generated_queries();
    assert!(
        !queries.is_empty(),
        "authenticated generated-query archive must not be empty"
    );

    let mut violations = Vec::new();
    for (name, bytes) in queries {
        let query = std::str::from_utf8(bytes)
            .unwrap_or_else(|error| panic!("authenticated query {name} is not UTF-8: {error}"));
        let mut header_lines = query
            .lines()
            .take_while(|line| line.starts_with('#'))
            .peekable();
        assert!(
            header_lines.peek().is_some(),
            "authenticated generated query {name} has no leading comment header"
        );
        for (line_index, line) in header_lines.enumerate() {
            if let Some(token) = tracker_token.find(line) {
                violations.push(format!(
                    "{name}:{} contains {}",
                    line_index + 1,
                    token.as_str()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "generated query headers contain tracker tokens:\n{}",
        violations.join("\n")
    );
}

// ── Projection round-trip / parse+eval guards ─────────────────────────────────
//
// Strictly stronger than the Python `prepareQuery` syntax-only check: the query
// must parse AND evaluate as a CONSTRUCT (the non-panicking call IS the
// assertion). `construct` hard-fails on any parse or eval error.

/// Twin of `test_notes_oa_projection_executable`: `web-annotation.rq`.
#[gmeow_test_batch_macros::batch_test]
fn notes_oa_projection_executable() {
    let g = empty_gmeow_graph();
    let out = g.construct(&[], &read_query("generated/queries/web-annotation.rq"));
    assert!(out.triple_count() < usize::MAX);
}

/// Twin of `test_notes_schema_projection_executable`: `schema-org.rq`.
#[gmeow_test_batch_macros::batch_test]
fn notes_schema_projection_executable() {
    let g = empty_gmeow_graph();
    let out = g.construct(&[], &read_query("generated/queries/schema-org.rq"));
    assert!(out.triple_count() < usize::MAX);
}

/// Twin of `test_notes_as_projection_executable`: `activitystreams.rq`.
#[gmeow_test_batch_macros::batch_test]
fn notes_as_projection_executable() {
    let g = empty_gmeow_graph();
    let out = g.construct(&[], &read_query("generated/queries/activitystreams.rq"));
    assert!(out.triple_count() < usize::MAX);
}

/// Twin of `test_notes_markdown_projection_executable`: `markdown.rq`.
#[gmeow_test_batch_macros::batch_test]
fn notes_markdown_projection_executable() {
    let g = empty_gmeow_graph();
    let out = g.construct(&[], &read_query("generated/queries/markdown.rq"));
    assert!(out.triple_count() < usize::MAX);
}

// ── Turtle prefix block shared by all notes tests ─────────────────────────────

const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

// ── Tests migrated from tests/test_notes.py ───────────────────────────────────

#[batch_cases]
#[case::note_with_content_passes_shacl(Case::inline(format!(
    "{PREFIXES}\
ex:note a gmeow:Note .
ex:note gmeow:noteContent \"A test note.\" .
"
)))]
#[case::note_with_label_passes_shacl(Case::inline(format!(
    "{PREFIXES}\
ex:note a gmeow:Note .
ex:note rdfs:label \"Test Note\" .
"
)))]
#[case::annotation_with_target_passes_shacl(Case::inline(format!(
    "{PREFIXES}\
ex:ann a gmeow:Annotation .
ex:ann gmeow:annotationTarget ex:doc .
ex:ann gmeow:annotationMotivation gmeow:motivationCommenting .
ex:doc a gmeow:Entity .
"
)))]
#[case::highlight_with_selector_passes_shacl(Case::inline(format!(
    "{PREFIXES}\
ex:hl a gmeow:Highlight .
ex:hl gmeow:annotationTarget ex:doc .
ex:hl gmeow:annotationTargetSpan ex:span .
ex:hl gmeow:annotationMotivation gmeow:motivationHighlighting .
ex:doc a gmeow:Entity .
ex:span a gmeow:EvidenceSpan .
ex:span gmeow:selectorTextQuote \"highlighted text\" .
"
)))]
#[case::retracted_note_displayable_false(Case::inline(format!(
    "{PREFIXES}\
ex:note a gmeow:Note .
ex:note gmeow:noteContent \"A retracted note.\" .
ex:note gmeow:displayable \"false\"^^xsd:boolean .
"
)))]
// A bare Note (no content, no label) must fail SHACL with a message mentioning
// note content or rdfs:label (case-insensitive disjunction).
#[case::note_without_content_or_label_fails_shacl(Case::inline(format!(
    "{PREFIXES}\
ex:note a gmeow:Note .
"
)).fails().any_violation_ci(&["note content", "rdfs:label"]))]
// An Annotation without annotationTarget must fail SHACL.
#[case::annotation_without_target_fails_shacl(Case::inline(format!(
    "{PREFIXES}\
ex:ann a gmeow:Annotation .
ex:ann gmeow:annotationMotivation gmeow:motivationCommenting .
"
)).shape_union().fails().fails_on_path("https://blackcatinformatics.ca/gmeow/annotationTarget", "MinCountConstraintComponent"))]
// A Highlight without annotationTargetSpan must fail SHACL mentioning selector.
#[case::highlight_without_selector_fails_shacl(Case::inline(format!(
    "{PREFIXES}\
ex:hl a gmeow:Highlight .
ex:hl gmeow:annotationTarget ex:doc .
ex:hl gmeow:annotationMotivation gmeow:motivationHighlighting .
ex:doc a gmeow:Entity .
"
)).shape_union().fails().fails_on_path("https://blackcatinformatics.ca/gmeow/annotationTargetSpan", "MinCountConstraintComponent"))]
fn notes(#[case] case: Case) {
    case.run();
}
