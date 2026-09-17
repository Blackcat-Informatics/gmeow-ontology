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
    origin
        .record_tool_call(
            "urn:gmeow:tool:store_claim",
            ToolCallOptions {
                arguments: Some(r#"{"text":"widgets are blue"}"#),
                result: Some(r#"{"ok":true}"#),
                invocation: Some("urn:gmeow:invocation:0"),
                generated: &[blue.id.as_str()],
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
                arguments: Some("{}"),
                result: Some(r#"{"ok":true}"#),
                invocation: None,
                generated: &[],
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
    assert!(typed_as(GMEOW_TOOL_CALL), "the call is a gmeow:ToolCall");
    assert!(
        typed_as(GMEOW_SOFTWARE_AGENT),
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
    }];
    let diag = claim_segment(&[], &calls).expect_err("a non-IRI tool must be refused");
    let message = format!("{diag:?}");
    assert!(
        message.contains("gmeow:usedTool"),
        "the refusal must name the field: {message}"
    );
}
