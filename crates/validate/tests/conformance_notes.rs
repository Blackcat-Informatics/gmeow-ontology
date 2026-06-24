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

// ── Turtle prefix block shared by all notes tests ─────────────────────────────

const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

// ── Tests migrated from tests/test_notes.py ───────────────────────────────────

/// `test_note_with_content_passes_shacl` — a Note with gmeow:noteContent passes
/// SHACL (NoteContentShape requires noteContent or rdfs:label).
#[test]
fn note_with_content_passes_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:note a gmeow:Note .
ex:note gmeow:noteContent \"A test note.\" .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "Note with noteContent must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_note_with_label_passes_shacl` — a Note with rdfs:label passes SHACL
/// (NoteContentShape requires noteContent OR rdfs:label).
#[test]
fn note_with_label_passes_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:note a gmeow:Note .
ex:note rdfs:label \"Test Note\" .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "Note with rdfs:label must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_note_without_content_or_label_fails_shacl` — a bare Note (no content,
/// no label) must fail SHACL with a message mentioning note content or rdfs:label.
#[test]
fn note_without_content_or_label_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:note a gmeow:Note .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "Note without content or label must fail SHACL"
    );
    let msgs: Vec<String> = violations(&report)
        .into_iter()
        .map(|m| m.to_lowercase())
        .collect();
    assert!(
        msgs.iter()
            .any(|m| m.contains("note content") || m.contains("rdfs:label")),
        "violation message must mention note content or rdfs:label; got: {:?}",
        msgs
    );
}

/// `test_annotation_without_target_fails_shacl` — an Annotation without
/// annotationTarget must fail SHACL (WebAnnotationShape requires exactly one
/// annotationTarget).
#[test]
fn annotation_without_target_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:ann a gmeow:Annotation .
ex:ann gmeow:annotationMotivation gmeow:motivationCommenting .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "Annotation without annotationTarget must fail SHACL"
    );
    let msgs: Vec<String> = violations(&report)
        .into_iter()
        .map(|m| m.to_lowercase())
        .collect();
    assert!(
        msgs.iter().any(|m| m.contains("annotationtarget")),
        "violation message must mention annotationTarget; got: {:?}",
        msgs
    );
}

/// `test_annotation_with_target_passes_shacl` — a fully-populated Annotation
/// (with target and motivation) passes SHACL.
#[test]
fn annotation_with_target_passes_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:ann a gmeow:Annotation .
ex:ann gmeow:annotationTarget ex:doc .
ex:ann gmeow:annotationMotivation gmeow:motivationCommenting .
ex:doc a gmeow:Entity .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "Annotation with target and motivation must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_highlight_without_selector_fails_shacl` — a Highlight without
/// annotationTargetSpan must fail SHACL with a message mentioning selector.
#[test]
fn highlight_without_selector_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:hl a gmeow:Highlight .
ex:hl gmeow:annotationTarget ex:doc .
ex:hl gmeow:annotationMotivation gmeow:motivationHighlighting .
ex:doc a gmeow:Entity .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "Highlight without annotationTargetSpan must fail SHACL"
    );
    let msgs: Vec<String> = violations(&report)
        .into_iter()
        .map(|m| m.to_lowercase())
        .collect();
    assert!(
        msgs.iter().any(|m| m.contains("selector")),
        "violation message must mention selector; got: {:?}",
        msgs
    );
}

/// `test_highlight_with_selector_passes_shacl` — a Highlight with
/// annotationTargetSpan pointing to an EvidenceSpan passes SHACL.
#[test]
fn highlight_with_selector_passes_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:hl a gmeow:Highlight .
ex:hl gmeow:annotationTarget ex:doc .
ex:hl gmeow:annotationTargetSpan ex:span .
ex:hl gmeow:annotationMotivation gmeow:motivationHighlighting .
ex:doc a gmeow:Entity .
ex:span a gmeow:EvidenceSpan .
ex:span gmeow:selectorTextQuote \"highlighted text\" .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "Highlight with EvidenceSpan selector must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_retracted_note_displayable_false` — a Note with displayable=false
/// (retracted, Principle 10) passes SHACL.
#[test]
fn retracted_note_displayable_false() {
    let ttl = format!(
        "{PREFIXES}\
ex:note a gmeow:Note .
ex:note gmeow:noteContent \"A retracted note.\" .
ex:note gmeow:displayable \"false\"^^xsd:boolean .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    // displayable false is valid; the shape only warns when a reply's
    // parent is suppressed.
    assert!(
        ok(&report),
        "Note with displayable=false must pass SHACL; violations: {:?}",
        violations(&report)
    );
}
