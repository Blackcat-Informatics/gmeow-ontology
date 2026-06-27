// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_notes.py (#867)
//!
//! Each test builds an inline Turtle graph containing the triples that the
//! Python test assembled via `g.add(...)`, converts to N-Triples, and validates
//! against the whole shapes corpus.
//!
//! Retained in Python (not migrated):
//!   - `test_evidence_span_is_information_object`: cross-slice TBox check
//!     (EvidenceSpan subject lives in evidencespan slice, not notes).
//!   - `test_selector_sub_class_of_evidence_span`: cross-slice TBox check.
//!   - `test_motivation_values_are_individuals`: dynamic `len(…)==10` count
//!     check — not expressible as a static instance fixture.
//!   - `test_notes_are_standpoint_indexed`: cross-slice TBox check
//!     (accordingTo lives in the standpoint slice).
//!   - `test_notes_oa_projection_executable`: SPARQL parse test (no SHACL).
//!   - `test_notes_schema_projection_executable`: SPARQL parse test.
//!   - `test_notes_as_projection_executable`: SPARQL parse test.
//!   - `test_notes_markdown_projection_executable`: SPARQL parse test.

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

// ── Turtle prefix block shared by all notes tests ─────────────────────────────

const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

// ── Tests migrated from tests/test_notes.py ───────────────────────────────────

#[rstest]
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
)).fails().violations_ci(&["annotationtarget"]))]
// A Highlight without annotationTargetSpan must fail SHACL mentioning selector.
#[case::highlight_without_selector_fails_shacl(Case::inline(format!(
    "{PREFIXES}\
ex:hl a gmeow:Highlight .
ex:hl gmeow:annotationTarget ex:doc .
ex:hl gmeow:annotationMotivation gmeow:motivationHighlighting .
ex:doc a gmeow:Entity .
"
)).fails().violations_ci(&["selector"]))]
fn notes(#[case] case: Case) {
    case.run();
}
