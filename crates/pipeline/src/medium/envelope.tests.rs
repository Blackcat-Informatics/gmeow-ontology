// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::medium::registry::fixture;

const PAYLOAD: &[u8] = b"the frame payload bytes";
const STRATUM: &[u8] = b"the payload minus the medium-envelope subgraph";

fn registry() -> MediumRegistry {
    MediumRegistry::from_dataset(&fixture::dataset("")).expect("fixture registry")
}

fn facts<'a>(rep: &'a str, dictionary_id: Option<&'a str>) -> FrameFacts<'a> {
    FrameFacts {
        frame: "https://e/frame7",
        rep,
        payload: PAYLOAD,
        stratum_bytes: STRATUM,
        stratum: DigestStratum::PayloadExcludingMediumEnvelope,
        dictionary_id,
    }
}

fn capabilities(items: &[&str]) -> ReaderCapabilities {
    items.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn a_primed_frame_seals_and_reopens() {
    let registry = registry();
    let envelope = seal(
        &registry,
        &MediumSelection::Authored,
        &facts("cells-archive", Some("gmeow-core-v1")),
    )
    .expect("a primed frame seals");
    assert_eq!(
        envelope.dictionary.as_deref(),
        Some(crate::medium::registry::gm("dictCore").as_str())
    );
    assert_eq!(envelope.strata_digest, blake3_digest(STRATUM));
    assert_eq!(envelope.content_digest, blake3_digest(PAYLOAD));
    assert_ne!(
        envelope.strata_digest, envelope.content_digest,
        "the stratum digest is an ADDITION to the witness, not a rename of the content digest"
    );

    let dict = open(
        &envelope,
        &registry,
        &capabilities(&["zstd-dictionary", "zstd-rsyncable"]),
        PAYLOAD,
        STRATUM,
    )
    .expect("the envelope opens")
    .expect("a primed frame resolves a dictionary");
    assert_eq!(dict.id, "gmeow-core-v1");
}

#[test]
fn prehashed_sealing_is_identical_to_byte_sealing() {
    let registry = registry();
    let bytes = facts("cells-archive", Some("gmeow-core-v1"));
    let expected =
        seal(&registry, &MediumSelection::Authored, &bytes).expect("byte-backed frame seals");
    let content_digest = blake3_digest(PAYLOAD);
    let strata_digest = blake3_digest(STRATUM);
    let actual = seal_digests(
        &registry,
        &MediumSelection::Authored,
        &FrameDigestFacts {
            frame: bytes.frame,
            rep: bytes.rep,
            content_digest: &content_digest,
            strata_digest: &strata_digest,
            stratum: bytes.stratum,
            dictionary_id: bytes.dictionary_id,
        },
    )
    .expect("prehashed frame seals");

    assert_eq!(actual, expected);
}

#[test]
fn prehashed_sealing_refuses_noncanonical_digests() {
    let canonical = blake3_digest(PAYLOAD);
    let diag = seal_digests(
        &registry(),
        &MediumSelection::Authored,
        &FrameDigestFacts {
            frame: "https://e/frame7",
            rep: crate::medium::SNAPSHOT_WIRE_REP,
            content_digest: "not-a-digest",
            strata_digest: &canonical,
            stratum: DigestStratum::PayloadExcludingMediumEnvelope,
            dictionary_id: None,
        },
    )
    .expect_err("precomputed content identity stays fail-closed");
    assert_eq!(
        diag.code(),
        crate::error::MediumDigestMismatch::register(),
        "{diag}"
    );

    let diag = seal_digests(
        &registry(),
        &MediumSelection::Authored,
        &FrameDigestFacts {
            frame: "https://e/frame7",
            rep: crate::medium::SNAPSHOT_WIRE_REP,
            content_digest: &canonical,
            strata_digest: "not-a-digest",
            stratum: DigestStratum::PayloadExcludingMediumEnvelope,
            dictionary_id: None,
        },
    )
    .expect_err("precomputed stratum identity stays fail-closed");
    assert_eq!(
        diag.code(),
        crate::error::MediumDigestMismatch::register(),
        "{diag}"
    );
}

/// The declared no-dictionary medium round-trips as a SELECTION: no dictionary
/// is named, and none is expected.
#[test]
fn the_declared_baseline_medium_seals_without_a_dictionary() {
    let registry = registry();
    let envelope = seal(
        &registry,
        &MediumSelection::Authored,
        &facts(crate::medium::SNAPSHOT_WIRE_REP, None),
    )
    .expect("the baseline rep seals");
    assert_eq!(envelope.dictionary, None);
    assert_eq!(
        open(
            &envelope,
            &registry,
            &capabilities(&["zstd-rsyncable"]),
            PAYLOAD,
            STRATUM
        )
        .expect("the baseline envelope opens"),
        None
    );
}

/// A primed frame whose rep is assigned a dictionary but which declares none in
/// band is `MediumUndeclaredDictionary` — the payload is permanently undecodable
/// even though its bytes are intact.
#[test]
fn a_frame_declaring_no_dictionary_under_a_primed_rep_is_undeclared() {
    let diag = seal(
        &registry(),
        &MediumSelection::Authored,
        &facts("cells-archive", None),
    )
    .expect_err("a primed rep with no in-band dictionary must fail");
    assert_eq!(
        diag.code(),
        crate::error::MediumUndeclaredDictionary::register(),
        "{diag}"
    );
}

/// A frame primed with a dictionary the registry does not know never falls back
/// to an unprimed decode.
#[test]
fn an_unresolvable_in_band_dictionary_is_unknown() {
    let diag = seal(
        &registry(),
        &MediumSelection::Authored,
        &facts("cells-archive", Some("never-trained-v1")),
    )
    .expect_err("an unknown dictionary must fail");
    assert_eq!(
        diag.code(),
        crate::error::MediumUnknownDictionary::register(),
        "{diag}"
    );
    assert!(diag.to_string().contains("NO fallback"), "{diag}");
}

/// A frame primed with a REGISTERED dictionary that is not the one its rep is
/// assigned is refused: the wrong dictionary decodes to garbage that passes as
/// content.
#[test]
fn an_in_band_dictionary_disagreeing_with_the_assignment_is_refused() {
    let diag = seal(
        &registry(),
        &MediumSelection::Authored,
        &facts("cells-archive", Some("gmeow-terms-v1")),
    )
    .expect_err("a disagreeing dictionary must fail");
    assert_eq!(
        diag.code(),
        crate::error::MediumUnknownDictionary::register(),
        "{diag}"
    );
    assert!(diag.to_string().contains("garbage"), "{diag}");
}

/// A reader missing a declared capability gets a surfaced, describable gap —
/// never a silent dictionary-less decode.
#[test]
fn a_reader_missing_a_declared_capability_raises_opaque_frame() {
    let registry = registry();
    let envelope = seal(
        &registry,
        &MediumSelection::Authored,
        &facts("cells-archive", Some("gmeow-core-v1")),
    )
    .expect("seal");
    let diag = open(
        &envelope,
        &registry,
        &capabilities(&["zstd-rsyncable"]),
        PAYLOAD,
        STRATUM,
    )
    .expect_err("a reader without zstd-dictionary must not decode");
    assert_eq!(
        diag.code(),
        crate::error::MediumOpaqueFrame::register(),
        "{diag}"
    );
    assert!(diag.to_string().contains("zstd-dictionary"), "{diag}");
}

/// Digests are RECOMPUTED, not trusted — and a malformed digest literal is a
/// mismatch by construction rather than a value to compare.
#[test]
fn a_digest_that_disagrees_with_the_bytes_refuses_before_any_decode() {
    let registry = registry();
    let sealed = seal(
        &registry,
        &MediumSelection::Authored,
        &facts("cells-archive", Some("gmeow-core-v1")),
    )
    .expect("seal");
    let caps = capabilities(&["zstd-dictionary", "zstd-rsyncable"]);

    let diag = open(&sealed, &registry, &caps, b"different bytes", STRATUM)
        .expect_err("a content-digest mismatch must refuse");
    assert_eq!(
        diag.code(),
        crate::error::MediumDigestMismatch::register(),
        "{diag}"
    );

    let diag = open(&sealed, &registry, &caps, PAYLOAD, b"different stratum")
        .expect_err("a stratum-digest mismatch must refuse");
    assert_eq!(diag.code(), crate::error::MediumDigestMismatch::register());

    let mut malformed = sealed.clone();
    malformed.strata_digest = "blake3:CAFE".to_string();
    let diag = open(&malformed, &registry, &caps, PAYLOAD, STRATUM)
        .expect_err("a malformed digest literal must refuse");
    assert_eq!(diag.code(), crate::error::MediumDigestMismatch::register());
    assert!(diag.to_string().contains("64 lowercase hex"), "{diag}");
}

/// The stratum names the region the digest commits to — the whole reason the
/// self-referential snapshot envelope converges.
#[test]
fn the_digest_stratum_individuals_are_the_ontology_ones() {
    assert_eq!(
        DigestStratum::PayloadExcludingMediumEnvelope.iri(),
        "https://blackcatinformatics.ca/gmeow/stratumPayloadExcludingMediumEnvelope"
    );
    assert_eq!(
        DigestStratum::WholePayload.iri(),
        "https://blackcatinformatics.ca/gmeow/stratumWholePayload"
    );
}
