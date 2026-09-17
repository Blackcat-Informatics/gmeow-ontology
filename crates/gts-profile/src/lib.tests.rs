// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn fixture_builder() -> SnapshotBuilder {
    let dataset = purrdf::parse_dataset(
        b"<https://e/s> <https://e/p> <https://e/o> .\n",
        purrdf::NativeRdfFormat::NTriples.media_type(),
        None,
    )
    .expect("parse fixture");
    let mut builder = SnapshotBuilder::new();
    builder.add_dataset(&dataset).expect("add fixture");
    builder
}

#[test]
fn production_profile_pins_transform_and_level() {
    assert_eq!(GMEOW_GTS_FRAME_TRANSFORM, "zstd-rsyncable");
    assert_eq!(GMEOW_GTS_ZSTD_LEVEL, 12);
    assert_eq!(purrdf::gts_compose::DIST_ZSTD_LEVEL, 12);

    let builder = fixture_builder();
    // gmeow-test-input: synthetic-only
    let bytes = emit_gmeow_gts(
        builder,
        vec![BlobRow {
            data: b"small payload must not fall back to plain zstd".to_vec(),
            media_type: "text/plain".to_string(),
            rep: "profile-test".to_string(),
        }],
        Vec::new(),
        None,
        &baseline_medium_plan(),
    )
    .expect("emit fixture");
    validate_mandated_frames(&bytes.bytes).expect("fixture uses mandated frame profile");
}

#[test]
fn owned_snapshot_emission_is_byte_identical() {
    let blob_rows = || {
        vec![BlobRow {
            data: b"the same blob bytes".to_vec(),
            media_type: "text/plain".to_string(),
            rep: "profile-test".to_string(),
        }]
    };
    // gmeow-test-input: synthetic-only
    let expected = purrdf::gts_compose::emit_gts(
        &fixture_builder(),
        "dist",
        Some(vec![GMEOW_GTS_FRAME_TRANSFORM.to_owned()]),
        blob_rows(),
        Vec::new(),
        None,
        None,
        None,
        purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
        &baseline_medium_plan(),
    )
    .expect("borrowed snapshot emission");
    // gmeow-test-input: synthetic-only
    let actual = emit_gmeow_gts(
        fixture_builder(),
        blob_rows(),
        Vec::new(),
        None,
        &baseline_medium_plan(),
    )
    .expect("owned snapshot emission");

    assert_eq!(
        actual.bytes, expected,
        "releasing the builder and skipping the redundant length probe cannot alter wire bytes"
    );
    validate_mandated_frames(&actual.bytes).expect("owned emission uses the mandated profile");
}

/// The leaf raises its OWN code namespace. A profile violation reported under
/// `pipeline.transform` would name a crate that does not own this check — and
/// depending on `gmeow-pipeline` to borrow that kind would reinstate the very
/// cycle this crate was extracted to break.
#[test]
fn profile_kind_keeps_its_registered_code() {
    assert_eq!(error::Profile::CODE, "gts-profile.frame");
}

#[test]
fn profile_validator_rejects_a_payload_without_a_transform_chain() {
    let builder = fixture_builder();
    // gmeow-test-input: synthetic-only
    let bytes = emit_gmeow_gts(
        builder,
        Vec::new(),
        Vec::new(),
        None,
        &baseline_medium_plan(),
    )
    .expect("emit fixture");
    let (mut items, torn) = iter_items(&bytes.bytes);
    assert!(torn.is_none());
    let payload = items
        .iter_mut()
        .skip(1)
        .find_map(|(_, item)| match item {
            Value::Map(entries) if map_get(entries, "d").is_some() => Some(entries),
            _ => None,
        })
        .expect("fixture has a payload frame");
    payload.retain(|(key, _)| !matches!(key, Value::Text(value) if value == "x"));

    let mut malformed = Vec::new();
    for (_, item) in items {
        ciborium::ser::into_writer(&item, &mut malformed).expect("serialize fixture item");
    }
    let error = validate_mandated_frames(&malformed).expect_err("missing transform must fail");
    assert!(
        error.to_string().contains("has no transform chain"),
        "{error}"
    );
}

#[test]
fn native_view_exit_carries_the_mandated_profile_and_reads_back() {
    let dataset = purrdf::parse_dataset(
        concat!(
            "<https://e/s> <https://e/p> <https://e/o> .\n",
            "<https://e/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ",
            "<<( <https://e/s> <https://e/p> <https://e/o> )>> .\n",
        )
        .as_bytes(),
        purrdf::NativeRdfFormat::NTriples.media_type(),
        None,
    )
    .expect("parse fixture");
    // gmeow-test-input: synthetic-only
    let bytes = view_to_gmeow_gts(&dataset).expect("serialize the carrier exit");
    validate_mandated_frames(&bytes.bytes).expect("carrier exit uses the mandated frame profile");
    let graph = purrdf::gts::reader::read(&bytes.bytes, false, None);
    assert!(!graph.quads.is_empty(), "the exit bytes read back as quads");
}

/// A bare `purrdf` `Writer` — the shape every non-`emit_gts` authorship path
/// used to take — emits payload frames with NO transform chain. Pin the
/// counter-example so the wrapper below is demonstrably load-bearing.
#[test]
fn a_bare_purrdf_writer_fails_the_mandated_profile() {
    // A bare writer violates the profile twice: its catalog declares no level,
    // and its frames carry no transform chain. Pin BOTH independently, so
    // neither rejection can mask a regression in the other.
    let mut writer = Writer::new("ai-package");
    writer.add_terms(&[iri_term("https://e/s")]);
    let error = validate_mandated_frames(&writer.into_bytes())
        .expect_err("a bare writer must fail the profile");
    assert!(error.to_string().contains("no level"), "{error}");

    // Now grant it the declared level and nothing else: the frame-level
    // violation must still stand on its own.
    let options = purrdf::gts::writer::WriterOptions {
        zstd_level: Some(GMEOW_GTS_ZSTD_LEVEL),
        ..Default::default()
    };
    let mut levelled =
        Writer::with_options("ai-package", options).expect("declared level is valid");
    levelled.add_terms(&[iri_term("https://e/s")]);
    let error = validate_mandated_frames(&levelled.into_bytes())
        .expect_err("a level-declaring bare writer still authors untransformed frames");
    assert!(
        error.to_string().contains("has no transform chain"),
        "{error}"
    );
}

/// An append-only file concatenates whole segments, each with its own header.
/// The audit must walk every segment (not stop at the first) and must not
/// mistake a later header for a malformed frame.
#[test]
fn multi_segment_append_is_audited_segment_by_segment() {
    let mut appended = mandated_segment("https://e/a");
    appended.extend_from_slice(&mandated_segment("https://e/b"));
    validate_mandated_frames(&appended).expect("every appended segment is audited");

    // A second segment authored WITHOUT the profile is caught, proving the walk
    // does not stop after the first header. A bare `Writer` violates the profile
    // twice over — its catalog declares no level and its frames carry no
    // transform chain — and either rejection is correct, so the assertion binds
    // to the invariant the test actually exists for: the failure is attributed
    // to the SECOND segment, i.e. the walk did not stop at the first header.
    let first = mandated_segment("https://e/a");
    let boundary = first.len();
    let mut mixed = first;
    let mut bare = Writer::new("ai-package");
    bare.add_terms(&[iri_term("https://e/b")]);
    mixed.extend_from_slice(&bare.into_bytes());
    let error =
        validate_mandated_frames(&mixed).expect_err("an unprofiled appended segment must fail");
    let offset: usize = error
        .to_string()
        .split("byte offset ")
        .nth(1)
        .and_then(|rest| {
            rest.split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|digits| digits.parse().ok())
        })
        .unwrap_or_else(|| panic!("the failure must name a byte offset: {error}"));
    assert!(
        offset >= boundary,
        "the failure must be attributed to the appended segment at or past byte {boundary}, \
             not the conforming first one: {error}"
    );
}

fn iri_term(iri: &str) -> Term {
    Term {
        kind: purrdf::gts::model::TermKind::Iri,
        value: Some(iri.to_string()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
        triple: None,
    }
}

fn mandated_segment(iri: &str) -> Vec<u8> {
    // gmeow-test-input: synthetic-only
    let mut writer = GmeowGtsWriter::new("ai-package");
    writer.add_terms(&[iri_term(iri)]).expect("terms frame");
    writer.into_bytes()
}

#[test]
fn segment_writer_stamps_the_mandated_profile_on_every_frame() {
    let terms = vec![
        iri_term("https://e/s"),
        iri_term("https://e/p"),
        iri_term("https://e/o"),
    ];
    // gmeow-test-input: synthetic-only
    let mut writer = GmeowGtsWriter::new("ai-package");
    writer.add_terms(&terms).expect("terms frame");
    writer.add_quads(&[(0, 1, 2, None)]).expect("quads frame");
    let bytes = writer.into_bytes();
    validate_mandated_frames(&bytes).expect("segment writer uses the mandated frame profile");

    // The segment still reads back as the quad it encodes — the transform is
    // decoded transparently, not a write-only stamp.
    let graph = purrdf::gts::reader::read(&bytes, false, None);
    assert_eq!(graph.quads.len(), 1, "one quad round-trips");
}
