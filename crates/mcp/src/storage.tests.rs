// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The `xsd:dateTime` datatype IRI, so the clock is validated by a REAL XSD parser
/// rather than by a shape assertion written here.
const XSD_DATE_TIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";

/// Parse one logical stamp as an `xsd:dateTime`, hard-failing when it is not one.
fn as_datetime(stamp: &str) -> purrdf::xsd::XsdValue {
    purrdf::xsd::parse_by_iri(stamp, XSD_DATE_TIME)
        .unwrap_or_else(|e| panic!("{stamp:?} is not a valid xsd:dateTime: {e:?}"))
        .expect("xsd:dateTime is in the XSD value space")
}

/// The logical clock is monotone AND every value it emits is a real `xsd:dateTime` —
/// checked across both rollovers the old `1970-01-{seq/86400+1}` rendering broke on.
///
/// The second rollover is the one that mattered: at `seq >= 2_678_400` the old code
/// emitted `1970-01-32T00:00:00Z`, which no `xsd:dateTime` parser accepts, so every
/// consumer that reads the stamp — the trajectory auditor orders trajectories ON it —
/// would reject the record rather than order it. Monotonicity is asserted on the PARSED
/// values, not on the strings: a lexical comparison would pass for two stamps that no
/// parser accepts at all.
#[test]
fn the_logical_clock_is_monotone_and_every_stamp_is_a_valid_xsd_date_time() {
    // Around the day rollover, around the month rollover, and well past both.
    let seqs: Vec<u64> = [
        0u64,
        1,
        59,
        60,
        3_599,
        3_600,
        86_398,
        86_399,
        86_400,
        86_401,
        172_800,
        2_678_398,
        2_678_399,
        2_678_400,
        2_678_401,
        5_097_600,
        31_535_999,
        31_536_000,
        1_000_000_000,
    ]
    .to_vec();

    assert_eq!(
        logical_instant(0),
        "1970-01-01T00:00:00Z",
        "the clock is anchored AT the Unix epoch — that anchor is what makes the stamp \
             self-identifying as logical rather than a plausible fake wall time"
    );
    assert_eq!(
        logical_instant(86_400),
        "1970-01-02T00:00:00Z",
        "one day of records advances the day"
    );
    assert_eq!(
        logical_instant(2_678_400),
        "1970-02-01T00:00:00Z",
        "31 days of records advances the MONTH; the old rendering emitted 1970-01-32"
    );
    assert_eq!(
        logical_instant(31_536_000),
        "1971-01-01T00:00:00Z",
        "365 days of records advances the year"
    );
    // 1972 is a leap year: 1972-02-29 must exist, and 1973-01-01 must be 366 days
    // after 1972-01-01 — the leap rule is exercised, not assumed.
    assert_eq!(logical_instant(68_169_600), "1972-02-29T00:00:00Z");
    assert_eq!(logical_instant(68_256_000), "1972-03-01T00:00:00Z");
    assert_eq!(logical_instant(94_694_400), "1973-01-01T00:00:00Z");

    let mut previous: Option<(u64, purrdf::xsd::XsdValue)> = None;
    for seq in seqs {
        let stamp = logical_instant(seq);
        let value = as_datetime(&stamp);
        if let Some((prior_seq, prior)) = &previous {
            assert_eq!(
                purrdf::xsd::value_cmp(prior, &value),
                Some(std::cmp::Ordering::Less),
                "seq {prior_seq} -> {seq} must stamp a strictly later instant, got \
                     {} -> {stamp}",
                logical_instant(*prior_seq)
            );
        }
        previous = Some((seq, value));
    }

    // Dense sweep straight through the month rollover: every single second in the
    // window parses and every step is strictly increasing.
    let mut prior = as_datetime(&logical_instant(2_678_390));
    for seq in 2_678_391..=2_678_410u64 {
        let value = as_datetime(&logical_instant(seq));
        assert_eq!(
            purrdf::xsd::value_cmp(&prior, &value),
            Some(std::cmp::Ordering::Less),
            "the clock stalled or went backwards at seq {seq}"
        );
        prior = value;
    }
}

/// A store seeded from a transport segment re-serializes to the BYTE-IDENTICAL segment.
///
/// This is the whole contract [`claim_segment`] and [`seed_claim_store`] exist to hold:
/// the pair is an isomorphism on exactly the state the store's public write API can
/// express, so an exported session can be re-seeded into a different store — natively,
/// through `purrdf`'s own `Memory::store()` — and answer identically there.
#[test]
fn a_store_seeded_from_a_segment_reserializes_to_the_same_bytes() {
    let origin = InMemoryClaimStore::default();
    let blue = origin
        .store_claim(
            "widgets are blue",
            StoreOptions {
                source: Some("mcp:test"),
                confidence: Some(0.9),
                according_to: Some("urn:gmeow:party:lab"),
            },
        )
        .expect("stores");
    origin
        .store_claim(
            "gadgets are red",
            StoreOptions {
                source: None,
                confidence: None,
                according_to: None,
            },
        )
        .expect("stores");
    let retired = origin
        .store_claim(
            "sprockets are green",
            StoreOptions {
                source: None,
                confidence: Some(0.25),
                according_to: None,
            },
        )
        .expect("stores");
    origin
        .revise_claim(
            &retired.id,
            RevisionOptions {
                reason: Some("measured again"),
                superseded_by: None,
            },
        )
        .expect("revises");
    let arguments = serde_json::json!({"null": null, "empty": {}, "sequence": [false, false], "iri-looking": "urn:source:value", "number": 1.25});
    let result = serde_json::json!([]);
    let payload = StructuredPayload::from_json(Some(&arguments), Some(&result)).unwrap();
    origin
        .record_tool_call(
            "urn:gmeow:tool:store_claim",
            ToolCallOptions {
                arguments: Some(&purrdf::RdfLiteral::simple(
                    r#"{"text":"widgets are blue"}"#,
                )),
                result: Some(&purrdf::RdfLiteral::simple(r#"{"ok":true}"#)),
                invocation: Some("urn:gmeow:invocation:0"),
                generated: &[blue.id.as_str()],
                payload: Some(&payload),
                ..ToolCallOptions::default()
            },
        )
        .expect("records");

    let segment = origin.segment_nquads().expect("serializes");
    assert!(
        segment.contains("<urn:gmeow:session:claim:0000>"),
        "records are position-addressed: {segment}"
    );

    let seeded = InMemoryClaimStore::default();
    let (claims, calls) = seed_claim_store(&seeded, &segment).expect("seeds");
    assert_eq!((claims, calls), (3, 1), "every record is replayed");
    assert_eq!(
        seeded.segment_nquads().expect("re-serializes"),
        segment,
        "seed → re-serialize must land on the SAME bytes"
    );

    // The seeded store is a real store, not a transcript: the suppression took, and
    // the recorded call points at the id THIS store minted rather than at the address.
    let seeded_claims = seeded.claims().expect("reads");
    assert_eq!(seeded_claims.len(), 3);
    assert!(!seeded_claims[0].suppressed);
    assert!(
        seeded_claims[2].suppressed,
        "a suppressed claim seeds back suppressed, not dropped"
    );
    assert_eq!(seeded_claims[0].text, "widgets are blue");
    assert_eq!(seeded_claims[0].confidence, Some(0.9));
    assert_eq!(seeded_claims[0].source.as_deref(), Some("mcp:test"));
    assert_eq!(
        seeded_claims[0].according_to.as_deref(),
        Some("urn:gmeow:party:lab")
    );
    let seeded_calls = seeded.tool_calls().expect("reads");
    assert_eq!(seeded_calls[0].generated, vec![seeded_claims[0].id.clone()]);
}

/// An empty store serializes to an EMPTY segment — an answer, not a failure — and
/// seeding from it is a no-op rather than an error.
#[test]
fn an_empty_store_serializes_to_an_empty_segment() {
    let store = InMemoryClaimStore::default();
    let segment = store.segment_nquads().expect("serializes");
    assert_eq!(segment, "", "nothing stored, nothing serialized");
    let seeded = InMemoryClaimStore::default();
    assert_eq!(
        seed_claim_store(&seeded, &segment).expect("seeds"),
        (0, 0),
        "seeding an empty segment stores nothing and raises nothing"
    );
}

/// The emitted segment PARSES as N-Quads and carries the GMEOW vocabulary the
/// transport declares — asserted structurally over the parsed quads, never over a
/// substring of the text.
#[test]
fn the_segment_parses_and_carries_the_declared_vocabulary() {
    let store = InMemoryClaimStore::default();
    store
        .store_claim(
            "the segment parses",
            StoreOptions {
                source: None,
                confidence: None,
                according_to: None,
            },
        )
        .expect("stores");
    store
        .record_tool_call(
            "urn:gmeow:tool:recall",
            ToolCallOptions {
                arguments: Some(&purrdf::RdfLiteral::simple("{}")),
                result: Some(&purrdf::RdfLiteral::simple(r#"{"ok":true}"#)),
                invocation: None,
                generated: &[],
                ..ToolCallOptions::default()
            },
        )
        .expect("records");

    let segment = store.segment_nquads().expect("serializes");
    let dataset = purrdf::parse_dataset(segment.as_bytes(), "application/n-quads", None)
        .expect("the emitted segment parses as N-Quads");
    let quads = purrdf::flat_rdf_quads_from_dataset(&dataset);
    let typed_as = |class: &str| {
        quads.iter().any(|quad| {
            quad.predicate == RDF_TYPE
                && matches!(&quad.object, purrdf::RdfTerm::Iri(iri) if iri == class)
        })
    };
    assert!(
        typed_as(GMEOW_CLAIM_TOKEN),
        "the claim is a gmeow:ClaimToken"
    );
    assert!(
        typed_as("https://blackcatinformatics.ca/gmeow/ToolCall"),
        "the call is a gmeow:ToolCall"
    );
    assert!(
        typed_as("https://blackcatinformatics.ca/gmeow/SoftwareAgent"),
        "the called tool is typed a gmeow:SoftwareAgent, so the segment stands alone"
    );
    assert!(
        quads
            .iter()
            .any(|quad| quad.predicate == GMEOW_SESSION_STORE_SEGMENT),
        "each call carries the segment identifier that locates its record"
    );
}

/// A control character in a claim survives the round trip: it is escaped on the way
/// out, so the segment still PARSES, and comes back byte-identical.
///
/// N-Triples excludes the C0 controls from its quoted-literal production outright, so a
/// raw one would produce a segment no parser accepts — an export that silently could
/// not be read back.
#[test]
fn a_control_character_in_a_claim_survives_the_round_trip() {
    let store = InMemoryClaimStore::default();
    store
        .store_claim(
            "a bell \u{7} and a vertical tab \u{b} and a tab \t",
            StoreOptions {
                source: None,
                confidence: None,
                according_to: None,
            },
        )
        .expect("stores");
    let segment = store.segment_nquads().expect("serializes");
    purrdf::parse_dataset(segment.as_bytes(), "application/n-quads", None)
        .expect("a segment carrying a control character must still parse");

    let seeded = InMemoryClaimStore::default();
    seed_claim_store(&seeded, &segment).expect("seeds");
    assert_eq!(
        seeded.claims().expect("reads")[0].text,
        "a bell \u{7} and a vertical tab \u{b} and a tab \t"
    );
    assert_eq!(seeded.segment_nquads().expect("re-serializes"), segment);
}

/// A record the transport cannot carry is a HARD FAIL naming the field, never a
/// silently dropped edge — the difference between an export that refuses and an export
/// that ships an incomplete snapshot.
#[test]
fn a_record_the_transport_cannot_carry_fails_naming_the_field() {
    let calls = [ToolCallRecord {
        id: "urn:gmeow:mcp:call:0".to_owned(),
        tool: "not an iri".to_owned(),
        arguments: None,
        result: None,
        invocation: None,
        created: None,
        generated: Vec::new(),
        ..ToolCallRecord::default()
    }];
    let diag = claim_segment(&[], &calls).expect_err("a non-IRI tool must be refused");
    let message = format!("{diag:?}");
    assert!(
        message.contains("gmeow:usedTool"),
        "the refusal must name the field: {message}"
    );
}

/// The native MCP claim store preserves small payloads and the exact digest of
/// oversized tool arguments/results, including across a cold package reopen.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn tool_call_provenance_native_store_preserves_payloads_and_cold_readback() {
    // gmeow-test-input: synthetic-only
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic-claims.gts");
    let store = fs_claim_store(&path).unwrap();
    let claim_text = "the recorded claim stays intact";
    let tool = "urn:gmeow:tool:store_claim";
    let invocation = "urn:gmeow:invocation:synthetic";
    let claim = store
        .store_claim(
            claim_text,
            StoreOptions {
                source: None,
                confidence: None,
                according_to: None,
            },
        )
        .unwrap();
    let small = r#"{"text":"résumé ☃"}"#;
    let large = "x".repeat(5000);
    let digest = format!(
        "blake3:{}",
        purrdf_hash::hex::encode(purrdf_hash::blake3::hash(large.as_bytes()).as_bytes())
    );
    let mut written = Vec::new();
    for (arguments, result) in [(small, large.as_str()), (large.as_str(), small)] {
        written.push(
            store
                .record_tool_call(
                    tool,
                    ToolCallOptions {
                        arguments: Some(&purrdf::RdfLiteral::simple(arguments)),
                        result: Some(&purrdf::RdfLiteral::simple(result)),
                        invocation: Some(invocation),
                        generated: &[claim.id.as_str()],
                        opaque_profile: Some(OpaquePayloadProfile::InlineOrBlake3),
                        ..ToolCallOptions::default()
                    },
                )
                .unwrap(),
        );
    }
    assert_eq!(
        written[0]
            .arguments
            .as_ref()
            .map(|value| value.lexical_form.as_str()),
        Some(small)
    );
    assert_eq!(
        written[0]
            .result
            .as_ref()
            .map(|value| value.lexical_form.as_str()),
        Some(digest.as_str())
    );
    assert_eq!(
        written[1]
            .arguments
            .as_ref()
            .map(|value| value.lexical_form.as_str()),
        Some(digest.as_str())
    );
    assert_eq!(
        written[1]
            .result
            .as_ref()
            .map(|value| value.lexical_form.as_str()),
        Some(small)
    );

    // Both roles share one cyclic native graph. Equal blank labels from two
    // source datasets are distinct, while a blank reused inside a triple stays shared.
    let first = purrdf::parse_dataset(format!(
        r#"@prefix g: <https://blackcatinformatics.ca/gmeow/> .
        @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
        <urn:root> a g:StructuredValue, g:StructuredMap ; g:hasFacet _:cycle, _:left, _:right, _:quote, _:lang, _:identity, _:plain, _:typed .
        _:cycle a g:LabelledFacet ; g:facetLabel "same" ; g:facetStructuredValue <urn:root> .
        _:left a g:LabelledFacet ; g:facetLabel "same" ; g:facetStructuredValue <urn:left> .
        _:right a g:LabelledFacet ; g:facetLabel "right" ; g:facetStructuredValue <urn:right> .
        _:quote a g:LabelledFacet ; g:facetLabel "quote" ; g:facetStructuredValue _:quoted .
        _:lang a g:LabelledFacet ; g:facetLabel " language " ; g:facetStructuredValue _:language .
        _:identity a g:LabelledFacet ; g:facetLabel "identity" ; g:facetStructuredValue _:id .
        _:plain a g:LabelledFacet ; g:facetLabel "text" ; g:facetText "prose"@en .
        _:typed a g:LabelledFacet ; g:facetLabel "number" ; g:facetValue "0007"^^xsd:integer .
        <urn:left> a g:StructuredValue ; g:structuredTerm _:same .
        _:quoted a g:StructuredValue ; g:structuredTerm <<( <urn:s> <urn:p> _:same )>> .
        _:language a g:StructuredValue ; g:structuredTerm "bonjour"@fr--rtl .
        _:id a g:StructuredValue ; g:structuredTerm <{}> .
        <urn:source-description> <http://www.w3.org/2000/01/rdf-schema#label> "source description"@en ."#, claim.id
    ).as_bytes(), "text/turtle", None).unwrap();
    let second = purrdf::parse_dataset(
        b"@prefix g: <https://blackcatinformatics.ca/gmeow/> . GRAPH <urn:empty> {} GRAPH <urn:source> { <urn:extra> <http://www.w3.org/2000/01/rdf-schema#label> \"named source description\" . } <urn:right> a g:StructuredValue ; g:structuredTerm _:same .",
        "application/trig", None,
    ).unwrap();
    let graph = Arc::new(purrdf::RdfDataset::union(&[first.as_ref(), second.as_ref()]).unwrap());
    let root = purrdf::flat_rdf_quads_from_dataset(&graph)
        .into_iter()
        .find(|row| {
            row.predicate == RDF_TYPE
                && row.object
                    == purrdf::RdfTerm::iri("https://blackcatinformatics.ca/gmeow/StructuredMap")
        })
        .unwrap()
        .subject;
    let payload = StructuredPayload::new(graph, Some(root.clone()), Some(root)).unwrap();
    let before = store.store_bytes().unwrap();
    assert!(
        store
            .record_tool_call(
                tool,
                ToolCallOptions {
                    payload: Some(&payload),
                    audit: Some(ToolCallAudit {
                        schema: "not an IRI",
                        obtains: &[]
                    }),
                    ..ToolCallOptions::default()
                }
            )
            .is_err()
    );
    assert_eq!(
        store.store_bytes().unwrap(),
        before,
        "selected audit refusal must publish no call prefix"
    );
    for channel in [
        "g:facetText <urn:wrong>",
        "g:facetText true",
        "g:facetValue <urn:wrong>",
    ] {
        let graph = purrdf::parse_dataset(format!(
            "@prefix g: <https://blackcatinformatics.ca/gmeow/> . <urn:map> a g:StructuredValue, g:StructuredMap ; g:hasFacet <urn:facet> . <urn:facet> a g:LabelledFacet ; g:facetLabel \"key\" ; {channel} ."
        ).as_bytes(), "text/turtle", None).unwrap();
        assert!(
            StructuredPayload::new(graph, Some(purrdf::RdfTerm::iri("urn:map")), None).is_err(),
            "{channel}"
        );
    }
    let opaque_arguments = purrdf::RdfLiteral {
        lexical_form: "bonsoir".to_owned(),
        datatype: None,
        language: Some("fr".to_owned()),
        direction: Some(purrdf::RdfTextDirection::Rtl),
    };
    let opaque_result =
        purrdf::RdfLiteral::typed("0007", "http://www.w3.org/2001/XMLSchema#integer");
    let recorded = store
        .record_tool_call(
            tool,
            ToolCallOptions {
                arguments: Some(&opaque_arguments),
                result: Some(&opaque_result),
                opaque_profile: Some(OpaquePayloadProfile::Verbatim),
                payload: Some(&payload),
                invocation: Some(invocation),
                generated: &[claim.id.as_str()],
                ..ToolCallOptions::default()
            },
        )
        .unwrap();
    assert!(
        recorded
            .ingestion
            .as_ref()
            .unwrap()
            .declarations_omitted
            .is_empty()
    );
    assert!(recorded.ingestion.as_ref().unwrap().rows_consumed > 0);
    assert_ne!(recorded.created.as_deref(), Some("1970-01-01T00:00:00Z"));
    written.push(recorded);
    let repeated = store
        .record_tool_call(
            tool,
            ToolCallOptions {
                arguments: Some(&opaque_arguments),
                result: Some(&opaque_result),
                payload: Some(&payload),
                invocation: Some(invocation),
                generated: &[claim.id.as_str()],
                opaque_profile: Some(OpaquePayloadProfile::Verbatim),
                ..ToolCallOptions::default()
            },
        )
        .expect("repeated explicit occurrence IRI keeps a distinct call-owned graph");
    written.push(repeated);
    let browser = InMemoryClaimStore::default();
    for _ in 0..2 {
        browser
            .record_tool_call(
                tool,
                ToolCallOptions {
                    payload: Some(&payload),
                    ..ToolCallOptions::default()
                },
            )
            .expect("browser and native admit the same repeated IRI-rooted input");
    }
    let browser_export = browser.segment_nquads().unwrap();
    let browser_seeded = InMemoryClaimStore::default();
    seed_claim_store(&browser_seeded, &browser_export).unwrap();
    assert_eq!(browser_seeded.segment_nquads().unwrap(), browser_export);

    // Snapshot appends must not poison the delegated memory term/cache lineage.
    let later = store
        .store_claim(
            "after snapshot",
            StoreOptions {
                source: None,
                confidence: Some(0.5),
                according_to: None,
            },
        )
        .unwrap();
    store
        .revise_claim(
            &later.id,
            RevisionOptions {
                reason: Some("revised after snapshot"),
                superseded_by: None,
            },
        )
        .unwrap();

    let header_path = dir.path().join("header-only.gts");
    // Consume the header of this already-produced runtime store. Tests do not
    // construct a writer or invoke any snapshot/corpus producer.
    let (items, torn) = purrdf::gts::wire::iter_items(&before);
    assert!(torn.is_none());
    let header_end = items.get(1).expect("runtime store has payload frames").0;
    std::fs::write(&header_path, &before[..header_end]).unwrap();
    let header_store = fs_claim_store(&header_path).unwrap();
    header_store
        .record_tool_call(tool, ToolCallOptions::default())
        .expect("header-only tail admits its first payload");
    let header_bytes = header_store.store_bytes().unwrap();
    assert_eq!(
        purrdf::gts::reader::read_file_segments(&header_bytes)
            .segments
            .len(),
        1,
        "unprimed continuation must not add per-call headers"
    );
    let mut torn = header_bytes.clone();
    torn.pop();
    std::fs::write(&header_path, &torn).unwrap();
    assert!(
        header_store
            .record_tool_call(tool, ToolCallOptions::default())
            .is_err()
    );
    assert_eq!(
        std::fs::read(&header_path).unwrap(),
        torn,
        "malformed append input stays untouched"
    );
    drop(store);

    let reopened = fs_claim_store(&path).unwrap();
    let readback = reopened.tool_calls().unwrap();
    assert_eq!(readback.len(), written.len());
    for (actual, expected) in readback.iter().zip(&written) {
        assert_eq!(actual.id, expected.id);
        assert_eq!(actual.tool, tool);
        assert_eq!(actual.arguments, expected.arguments);
        assert_eq!(actual.result, expected.result);
        assert_eq!(actual.invocation.as_deref(), Some(invocation));
        assert_eq!(actual.generated, vec![claim.id.clone()]);
        assert_eq!(actual.created, expected.created);
        if let Some(expected_payload) = &expected.payload {
            let actual_payload = actual.payload.as_ref().unwrap();
            assert_eq!(
                actual_payload.arguments, actual_payload.result,
                "both roles retain one shared occurrence"
            );
            assert_eq!(
                payload::serialize(&actual_payload.graph).unwrap(),
                payload::serialize(&expected_payload.graph).unwrap()
            );
            assert!(
                actual_payload
                    .graph
                    .owned_named_graphs()
                    .any(|name| name == purrdf::RdfTerm::iri("urn:empty")),
                "empty named-graph identity survives the ownership envelope"
            );
            let rows = purrdf::flat_rdf_quads_from_dataset(&actual_payload.graph);
            let blanks: Vec<_> = rows
                .iter()
                .filter(|row| {
                    row.predicate.ends_with("/structuredTerm")
                        && matches!(row.object, purrdf::RdfTerm::BlankNode(_))
                })
                .map(|row| row.object.clone())
                .collect();
            assert_eq!(blanks.len(), 2);
            assert_ne!(
                blanks[0], blanks[1],
                "equal labels in different source scopes remain distinct"
            );
            assert!(
                rows.iter()
                    .any(|row| row.predicate.ends_with("/structuredTerm")
                        && row.object == purrdf::RdfTerm::iri(&claim.id)),
                "source scalar identifiers remain data"
            );
        }
    }
    let claims = reopened.claims().unwrap();
    assert_eq!(claims.len(), 2);
    assert!(claims[1].suppressed);
    assert_eq!(claims[0].id, claim.id);
    assert_eq!(claims[0].text, claim_text);
}
