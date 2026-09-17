// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::medium::registry::fixture;

/// The materialized bundle is a pipeline-owned artifact, so its mandated-frame
/// audit stays with the pipeline crate rather than moving to the leaf that owns
/// the rule.
#[test]
fn committed_bundle_uses_the_mandated_frame_profile() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root");
    let bytes = gmeow_bundle_import::load_authenticated_source_bytes(root)
        .expect("authenticated GMEOW bundle; tests never produce it");
    gmeow_gts_profile::validate_mandated_frames(&bytes)
        .expect("the materialized bundle uses the mandated frame profile");
}

/// A `gmeow-` prefixed dictionary the store fixtures prime with, trained over a
/// corpus big enough for zstd to accept.
fn dict_bytes() -> Vec<u8> {
    let owned: Vec<Vec<u8>> = (0..512u32)
        .map(|i| {
            format!(
                "<https://blackcatinformatics.ca/gmeow/term{}> \
                     <https://blackcatinformatics.ca/gmeow/definition> \
                     \"a definition of term {i} in the gmeow ontology\" .\n",
                i % 41
            )
            .into_bytes()
        })
        .collect();
    let corpus: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
    crate::medium::train::build(
        crate::medium::registry::DictionaryStrategy::Trained,
        &corpus,
        4096,
    )
    .expect("the fixture dictionary trains")
}

fn registry(extra: &str) -> MediumRegistry {
    MediumRegistry::from_dataset(&fixture::dataset(extra)).expect("fixture registry")
}

/// A whole-artifact bundle authored through the production baseline door.
fn baseline_bundle() -> Vec<u8> {
    let dataset = purrdf::parse_dataset(
        b"<https://e/s> <https://e/p> <https://e/o> .\n",
        "application/n-triples",
        None,
    )
    .expect("fixture parses");
    let mut builder = purrdf::gts_compose::SnapshotBuilder::new();
    builder.add_dataset(&dataset).expect("add fixture");
    // gmeow-test-input: synthetic-only
    {
        let emission = gmeow_gts_profile::emit_gmeow_gts(
            builder,
            vec![purrdf::gts_compose::BlobRow {
                data: b"a whole-artifact payload".repeat(32),
                media_type: "text/plain".to_string(),
                rep: "cells-archive".to_string(),
            }],
            Vec::new(),
            None,
            &gmeow_gts_profile::baseline_medium_plan(),
        )
        .expect("the baseline door emits");
        assert!(
            emission.ingestion.declarations_omitted.is_empty(),
            "unexpected GMEOW fixture graph omissions: {:?}",
            emission.ingestion.declarations_omitted
        );
        emission.bytes
    }
}

#[test]
fn a_whole_artifact_bundle_passes_under_its_declared_dictionary_less_medium() {
    let registry = registry("");
    let bytes = baseline_bundle();
    // The universal Rule 6 check still holds on the very same bytes: the split is
    // an ADDITION, never a replacement.
    gmeow_gts_profile::validate_mandated_frames(&bytes).expect("universal rule holds");
    validate_declared_media(
        &bytes,
        &MediumDeclaration {
            medium: &crate::medium::registry::gm("mediumBaseline"),
            registry: &registry,
        },
    )
    .expect("an unprimed artifact under a dictionary-less medium is exactly conformant");
}

#[test]
fn a_whole_artifact_producer_declaring_a_primed_medium_must_prime_every_frame() {
    // The dist fixture medium DECLARES two dictionaries. An artifact authored
    // through the unprimed door and then claimed to be that medium is refused —
    // this is the branch's real obligation, not a formality.
    let registry = registry("");
    let diag = validate_declared_media(
        &baseline_bundle(),
        &MediumDeclaration {
            medium: &crate::medium::registry::gm("mediumDist"),
            registry: &registry,
        },
    )
    .expect_err("a primed medium with unprimed frames must be refused");
    assert_eq!(
        diag.code(),
        crate::error::MediumUndeclaredDictionary::register(),
        "{diag}"
    );
}

#[test]
fn a_producer_whose_declared_medium_is_undeclared_is_refused() {
    let registry = registry("");
    let diag = validate_declared_media(
        &baseline_bundle(),
        &MediumDeclaration {
            medium: &crate::medium::registry::gm("mediumInvented"),
            registry: &registry,
        },
    )
    .expect_err("an unresolvable declared medium has no audit to run");
    assert_eq!(diag.code(), crate::error::InvalidDeclaration::register());
}

/// A store-shaped file: one dict-primed segment authored through the production
/// `store_writer` door, audited under a header-dict medium.
fn primed_store(dictionary: &str) -> Vec<u8> {
    let medium = gmeow_gts_profile::StoreMedium {
        dictionary: dictionary.to_string(),
        bytes: dict_bytes(),
    };
    let mut writer =
        gmeow_gts_profile::store_writer("ai-package", &[], &medium).expect("the store door opens");
    writer
        .add_terms(&[purrdf::gts::model::Term {
            kind: purrdf::gts::model::TermKind::Iri,
            value: Some("https://e/claim".to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
            triple: None,
        }])
        .expect("terms frame");
    writer.into_bytes()
}

#[test]
fn a_header_dict_store_passes_when_its_pinned_dictionary_is_registered() {
    let registry = registry(HEADER_DICT_MEDIUM);
    let bytes = primed_store("gmeow-core-v1");
    gmeow_gts_profile::validate_mandated_frames(&bytes).expect("universal rule holds");
    validate_declared_media(
        &bytes,
        &MediumDeclaration {
            medium: &crate::medium::registry::gm("mediumStore"),
            registry: &registry,
        },
    )
    .expect("a primed store citing a registered dictionary conforms");
}

#[test]
fn a_header_dict_store_citing_an_unregistered_dictionary_is_refused() {
    let registry = registry(HEADER_DICT_MEDIUM);
    let diag = validate_declared_media(
        &primed_store("not-a-registered-dictionary"),
        &MediumDeclaration {
            medium: &crate::medium::registry::gm("mediumStore"),
            registry: &registry,
        },
    )
    .expect_err("a store citing an unregistered dictionary id must be refused");
    assert_eq!(
        diag.code(),
        crate::error::MediumUnknownDictionary::register(),
        "{diag}"
    );
}

#[test]
fn a_header_dict_store_with_unprimed_frames_is_refused() {
    // The counter-example the branch exists for: a store authored through the
    // deliberately unprimed writer, then claimed to be header-dict.
    let registry = registry(HEADER_DICT_MEDIUM);
    // gmeow-test-input: synthetic-only
    let mut writer = gmeow_gts_profile::GmeowGtsWriter::new("ai-package");
    writer
        .add_terms(&[purrdf::gts::model::Term {
            kind: purrdf::gts::model::TermKind::Iri,
            value: Some("https://e/claim".to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
            triple: None,
        }])
        .expect("terms frame");
    let diag = validate_declared_media(
        &writer.into_bytes(),
        &MediumDeclaration {
            medium: &crate::medium::registry::gm("mediumStore"),
            registry: &registry,
        },
    )
    .expect_err("an unprimed store under a header-dict medium must be refused");
    assert_eq!(
        diag.code(),
        crate::error::MediumUndeclaredDictionary::register(),
        "{diag}"
    );
}

/// A header-dict medium spliced into the registry fixture.
const HEADER_DICT_MEDIUM: &str = "\
gmeow:mediumStore a gmeow:ZstdDictMedium ;
    gmeow:mediumCodec gmeow:codecZstdRsyncable ;
    gmeow:mediumZstdLevel 12 ;
    gmeow:mediumSourceKind gmeow:mediumSourceHeaderDict ;
    gmeow:requiresReaderCapability \"zstd-dictionary\" , \"zstd-rsyncable\" ;
    gmeow:mediumDictionary gmeow:dictCore , gmeow:dictTerms .
";

/// A bundle whose catalog names a `dct` the header map does not carry: the reader
/// drops the entry, the frame degrades to an opaque node, and the fold silently
/// loses its content. Both halves of the refusal are proved — the wire check fires,
/// and the fold really would have degraded.
#[test]
fn a_bundle_whose_catalog_names_an_unpinned_dictionary_is_rejected() {
    let mut bytes = primed_store("gmeow-core-v1");
    strip_header_dct(&mut bytes);
    // The degradation is REAL, not hypothetical: fold the tampered bytes and watch
    // the reader recover instead of refusing.
    let graph = purrdf::gts::reader::read(&bytes, true, None);
    assert!(
        !graph.opaque.is_empty() || !graph.diagnostics.is_empty(),
        "the tampered store must genuinely degrade, or the gate below is vacuous"
    );
    let registry = registry(HEADER_DICT_MEDIUM);
    let diag = validate_declared_media(
        &bytes,
        &MediumDeclaration {
            medium: &crate::medium::registry::gm("mediumStore"),
            registry: &registry,
        },
    )
    .expect_err("a catalog naming an unpinned dictionary must be refused");
    assert_eq!(
        diag.code(),
        crate::error::MediumUnknownDictionary::register(),
        "{diag}"
    );
}

/// STRICTLY STRONGER, proved rather than asserted: every artifact the UNIVERSAL
/// Rule 6 check rejects is also rejected by the declared-media audit.
///
/// The split would be a regression if any of these slipped through — a second
/// check that accepted what the first refused would mean the gate that runs
/// second had quietly widened the contract. The set replays the profile crate's
/// own rejection fixtures (a bare writer, a level-declaring bare writer, a
/// payload frame with its transform chain removed, a mixed multi-segment file)
/// and adds the two the medium axis makes newly reachable: a frame naming a codec
/// id its catalog never declared, and a catalog whose `zstd-rsyncable` entry omits
/// the mandated level.
#[test]
fn every_universal_rejection_is_still_a_rejection_under_the_stronger_check() {
    let registry = registry(HEADER_DICT_MEDIUM);
    let declared = MediumDeclaration {
        medium: &crate::medium::registry::gm("mediumBaseline"),
        registry: &registry,
    };

    let mut cases: Vec<(&str, Vec<u8>)> = Vec::new();
    cases.push(("empty bytes", Vec::new()));
    cases.push(("not a header", b"\xa1\x61a\x01".to_vec()));
    cases.push(("a torn CBOR sequence", {
        let mut b = baseline_bundle();
        b.truncate(b.len() - 7);
        b
    }));
    // A bare purrdf writer: no declared level AND no transform chain.
    cases.push(("a bare purrdf writer", {
        let mut writer = purrdf::gts::writer::Writer::new("ai-package");
        writer.add_terms(&[iri_term("https://e/s")]);
        writer.into_bytes()
    }));
    // A level-declaring bare writer: the frame-level violation alone.
    cases.push(("a level-declaring bare writer", {
        let options = purrdf::gts::writer::WriterOptions {
            zstd_level: Some(12),
            ..Default::default()
        };
        let mut writer =
            purrdf::gts::writer::Writer::with_options("ai-package", options).expect("valid");
        writer.add_terms(&[iri_term("https://e/s")]);
        writer.into_bytes()
    }));
    // A conforming first segment followed by an unprofiled appended one.
    cases.push(("a mixed multi-segment file", {
        let mut mixed = primed_store("gmeow-core-v1");
        let mut bare = purrdf::gts::writer::Writer::new("ai-package");
        bare.add_terms(&[iri_term("https://e/b")]);
        mixed.extend_from_slice(&bare.into_bytes());
        mixed
    }));
    // A payload frame with its transform chain removed.
    cases.push(("a payload frame with no transform chain", {
        rewrite_items(&baseline_bundle(), |item| match item {
            Value::Map(mut entries) if map_get(&entries, "d").is_some() => {
                entries.retain(|(key, _)| !matches!(key, Value::Text(k) if k == "x"));
                Value::Map(entries)
            }
            other => other,
        })
    }));
    // NEW under the medium axis: a frame naming a codec id nothing declares.
    cases.push(("a frame naming an undeclared codec id", {
        rewrite_items(&baseline_bundle(), |item| match item {
            Value::Map(mut entries) if map_get(&entries, "d").is_some() => {
                for (key, value) in &mut entries {
                    if matches!(key, Value::Text(k) if k == "x") {
                        *value = Value::Array(vec![Value::Integer(9_999.into())]);
                    }
                }
                Value::Map(entries)
            }
            other => other,
        })
    }));
    // NEW: a catalog whose zstd-rsyncable entry omits the mandated level.
    cases.push(("a catalog entry with no declared level", {
        rewrite_items(&baseline_bundle(), strip_catalog_levels)
    }));

    for (label, bytes) in cases {
        let universal = gmeow_gts_profile::validate_mandated_frames(&bytes);
        assert!(
            universal.is_err(),
            "{label}: the universal rule must reject it, or this is not a replay"
        );
        assert!(
            validate_declared_media(&bytes, &declared).is_err(),
            "{label}: the declared-media audit accepted what the universal rule rejects — \
                 the split would then be a WIDENING rather than a strengthening"
        );
    }
}

fn iri_term(iri: &str) -> purrdf::gts::model::Term {
    purrdf::gts::model::Term {
        kind: purrdf::gts::model::TermKind::Iri,
        value: Some(iri.to_string()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
        triple: None,
    }
}

/// Re-serialize a CBOR sequence, applying `edit` to every item in order.
fn rewrite_items(bytes: &[u8], mut edit: impl FnMut(Value) -> Value) -> Vec<u8> {
    let (items, torn) = iter_items(bytes);
    assert!(torn.is_none());
    let mut out = Vec::new();
    for (_, item) in items {
        ciborium::ser::into_writer(&edit(item), &mut out).expect("re-serialize");
    }
    out
}

/// Drop `level` from every catalog entry of a header item, leaving frames alone.
fn strip_catalog_levels(item: Value) -> Value {
    fn strip(entries: &mut [(Value, Value)]) {
        for (key, value) in entries.iter_mut() {
            if !matches!(key, Value::Text(k) if k == "cat") {
                continue;
            }
            if let Value::Map(catalog) = value {
                for (_, descriptor) in catalog.iter_mut() {
                    if let Value::Map(fields) = descriptor {
                        fields.retain(|(key, _)| !matches!(key, Value::Text(k) if k == "level"));
                    }
                }
            }
        }
    }
    match item {
        Value::Tag(tag, inner) if tag == SELF_DESCRIBE_TAG => {
            let Value::Map(mut entries) = *inner else {
                return Value::Tag(tag, inner);
            };
            strip(&mut entries);
            Value::Tag(tag, Box::new(Value::Map(entries)))
        }
        Value::Map(mut entries) if map_get(&entries, "gts").is_some() => {
            strip(&mut entries);
            Value::Map(entries)
        }
        other => other,
    }
}

/// Drop the header's `"dct"` map while leaving the catalog's `dct` binding in
/// place — the exact shape a pack has when its dictionary went missing.
fn strip_header_dct(bytes: &mut Vec<u8>) {
    let (items, torn) = iter_items(bytes);
    assert!(torn.is_none());
    let mut rewritten = Vec::new();
    for (_, item) in items {
        let item = match item {
            Value::Tag(tag, inner) if tag == SELF_DESCRIBE_TAG => {
                let Value::Map(mut entries) = *inner else {
                    panic!("a tagged header wraps a map");
                };
                entries.retain(|(key, _)| !matches!(key, Value::Text(k) if k == "dct"));
                Value::Tag(tag, Box::new(Value::Map(entries)))
            }
            Value::Map(mut entries) if map_get(&entries, "gts").is_some() => {
                entries.retain(|(key, _)| !matches!(key, Value::Text(k) if k == "dct"));
                Value::Map(entries)
            }
            other => other,
        };
        ciborium::ser::into_writer(&item, &mut rewritten).expect("re-serialize");
    }
    *bytes = rewritten;
}
