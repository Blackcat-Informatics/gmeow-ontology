// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The adapter recovers the correct 1-based source line for a subject's bare IRI.
#[test]
fn adapter_yields_spans_for_turtle_subject() {
    let turtle = concat!(
        "@prefix ex: <https://example.test/> .\n",
        "ex:alice a ex:Person .\n",
        "ex:bob a ex:Person .\n",
    );
    let ingested = PurrdfAdapter
        .ingest("slices/x/module.ttl", "text/turtle", turtle.as_bytes())
        .expect("ingest");
    let index = ingested.spans.into_index();
    let alice = index
        .lookup("https://example.test/alice")
        .expect("alice tracked");
    assert_eq!(alice.line, 2, "ex:alice is on line 2");
    let bob = index
        .lookup("https://example.test/bob")
        .expect("bob tracked");
    assert_eq!(bob.line, 3, "ex:bob is on line 3");
    assert_eq!(alice.path.as_ref(), "slices/x/module.ttl");
}

/// Source attribution retains the parser's base origin and resolved subjects
/// together, so later receipt construction cannot require another parse.
#[test]
fn adapter_retains_document_base_with_source_positions() {
    let ingested = PurrdfAdapter
        .ingest(
            "example.ttl",
            "text/turtle",
            b"@base <https://example.org/base/> .\n<s> <p> <o> .\n",
        )
        .expect("ingest");
    let base = ingested.document_base.as_ref().expect("declared base");
    assert_eq!(base.iri().as_str(), "https://example.org/base/");
    assert_eq!(
        base.origin(),
        purrdf::iri::BaseOrigin::Directive { line: 1, column: 7 }
    );
    assert!(
        ingested
            .dataset
            .term_id_by_iri("https://example.org/base/s")
            .is_some()
    );
    let mut public = SpanIndex::new();
    public.extend_from_index(ingested.spans.index());
    let original = ingested
        .spans
        .index()
        .lookup("https://example.org/base/s")
        .expect("source span");
    let projected = public
        .lookup("https://example.org/base/s")
        .expect("projected span");
    assert_eq!(projected.line, 2);
    assert!(Arc::ptr_eq(&original.path, &projected.path));
}

/// Minimal-allocation shape: many subjects from ONE file share ONE path `Arc`
/// (interned once), both in the live index and after a serde round-trip.
#[test]
fn span_index_interns_path_once_per_file() {
    let turtle = concat!(
        "@prefix ex: <https://example.test/> .\n",
        "ex:a a ex:T .\n",
        "ex:b a ex:T .\n",
        "ex:c a ex:T .\n",
    );
    let index = PurrdfAdapter
        .ingest("slices/x/module.ttl", "text/turtle", turtle.as_bytes())
        .expect("ingest")
        .spans
        .into_index();
    let a = index.lookup("https://example.test/a").expect("a");
    let b = index.lookup("https://example.test/b").expect("b");
    let c = index.lookup("https://example.test/c").expect("c");
    assert!(
        Arc::ptr_eq(&a.path, &b.path) && Arc::ptr_eq(&b.path, &c.path),
        "all subjects of one file must share one interned path Arc"
    );

    // The interning survives a serde round-trip (one Arc per distinct path).
    let json = serde_json::to_vec(&index).expect("serialize");
    let round: SpanIndex = serde_json::from_slice(&json).expect("deserialize");
    let ra = round.lookup("https://example.test/a").expect("a");
    let rb = round.lookup("https://example.test/b").expect("b");
    let rc = round.lookup("https://example.test/c").expect("c");
    assert!(
        Arc::ptr_eq(&ra.path, &rb.path) && Arc::ptr_eq(&rb.path, &rc.path),
        "deserialize must re-intern one Arc per distinct path"
    );
    assert_eq!(round, index, "round-trip is value-preserving");
}

/// A span maps cleanly onto a diagnostics `Location` (path + 1-based line/column).
#[test]
fn source_span_maps_onto_location() {
    let span = SourceSpan::new(Arc::from("slices/x/module.ttl"), 9, 4, 128);
    let location = span.to_location();
    assert_eq!(location.path.as_deref(), Some("slices/x/module.ttl"));
    assert_eq!(location.line, Some(9));
    assert_eq!(location.column, Some(4));
}

/// Enrichment fills a logical-only SHACL focus location's physical coordinates
/// while preserving the bare-IRI `logical` join key.
#[test]
fn enrich_fills_focus_location_from_span() {
    let mut index = SpanIndex::new();
    index.insert(
        "https://example.test/thing",
        SourceSpan::new(Arc::from("slices/x/module.ttl"), 7, 3, 42),
    );
    let mut report = gmeow_errors::Report::new("shacl");
    let mut finding = gmeow_errors::Finding::new(
        gmeow_errors::Severity::Warning,
        "shacl.MinCount",
        "missing value",
    );
    finding.add_location(gmeow_errors::model::Location {
        logical: Some("https://example.test/thing".to_owned()),
        ..gmeow_errors::model::Location::default()
    });
    report.add_finding(finding);

    enrich_findings_with_spans(&mut report, &index);

    let loc = report.findings[0].primary_location().expect("a location");
    assert_eq!(loc.path.as_deref(), Some("slices/x/module.ttl"));
    assert_eq!(loc.line, Some(7));
    assert_eq!(loc.column, Some(3));
    assert_eq!(
        loc.logical.as_deref(),
        Some("https://example.test/thing"),
        "the bare-IRI join key is preserved"
    );
}
