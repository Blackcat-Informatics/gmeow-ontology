// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The mandatory GMEOW GTS authorship profile.
//!
//! Every payload-bearing frame emitted by production code uses one transform,
//! `zstd-rsyncable`, at compression level 12. [`emit_gmeow_gts`] is the ONLY
//! production entry to `purrdf::gts_compose::emit_gts` anywhere in the workspace;
//! direct calls belong only in codec-specific tests.
//!
//! This is a leaf crate — `purrdf` + `gmeow-errors` + `ciborium`, nothing else —
//! precisely so that every bundle author can depend on it. The profile previously
//! lived inside `gmeow-pipeline`, which put it out of reach of `gmeow-math` (which
//! `gmeow-pipeline` itself depends on, so the edge cannot be reversed) and of
//! `gmeow-music`; both consequently called the writer directly and the
//! single-entry claim was false. A narrow-waist leaf is what makes the claim
//! true rather than aspirational.

pub mod error;

use ciborium::value::Value;
use purrdf::gts::wire::{iter_items, map_get, unwrap_header};
use purrdf::gts_compose::{BlobRow, SnapshotBuilder};

/// Required transform on every payload-bearing GMEOW GTS frame.
pub const GMEOW_GTS_FRAME_TRANSFORM: &str = "zstd-rsyncable";

/// Required zstd compression level on every payload-bearing GMEOW GTS frame.
pub const GMEOW_GTS_ZSTD_LEVEL: i32 = 12;

// `emit_gts` applies its public dist-profile level to every blob and snapshot
// frame.  Turn an upstream default drift into a compile failure rather than a
// silently different committed bundle.
const _: () = assert!(purrdf::gts_compose::DIST_ZSTD_LEVEL == GMEOW_GTS_ZSTD_LEVEL);

fn profile_error(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(error::Profile {
        message: message.into(),
    })
}

fn map<'a>(value: &'a Value, context: &str) -> gmeow_errors::Result<&'a [(Value, Value)]> {
    match value {
        Value::Map(entries) => Ok(entries),
        _ => Err(profile_error(format!("{context} is not a CBOR map"))),
    }
}

fn integer(value: &Value, context: &str) -> gmeow_errors::Result<i128> {
    match value {
        Value::Integer(value) => Ok(i128::from(*value)),
        _ => Err(profile_error(format!("{context} is not a CBOR integer"))),
    }
}

/// Validate the mandatory transform profile on a materialized GMEOW GTS bundle.
///
/// This is intentionally a cheap wire-level audit: it does not fold or parse the
/// RDF payload. Deep semantic validation remains the responsibility of the
/// validation gate. It DOES read the codec catalog's declared `level` field and
/// hard-fail when it is missing or disagrees with [`GMEOW_GTS_ZSTD_LEVEL`] — the
/// compile-time assertion above only pins the emitter's own default; without this
/// wire read a bundle authored at a different level, or with no declared level at
/// all, would pass the audit despite the documented level-12 contract.
pub fn validate_mandated_frames(bytes: &[u8]) -> gmeow_errors::Result<()> {
    let (items, torn) = iter_items(bytes);
    if let Some(offset) = torn {
        return Err(profile_error(format!(
            "GTS CBOR sequence is torn at byte offset {offset}"
        )));
    }

    let header_item = items
        .first()
        .ok_or_else(|| profile_error("GTS bundle has no header"))?;
    let header = unwrap_header(&header_item.1)
        .map_err(|message| profile_error(format!("invalid header: {message}")))?;
    let catalog = map(
        map_get(header, "cat").ok_or_else(|| profile_error("GTS header has no codec catalog"))?,
        "GTS codec catalog",
    )?;

    let mut required_codec_ids = Vec::new();
    for (id, descriptor) in catalog {
        let descriptor = map(descriptor, "GTS codec descriptor")?;
        if !matches!(
            map_get(descriptor, "name"),
            Some(Value::Text(name)) if name == GMEOW_GTS_FRAME_TRANSFORM
        ) {
            continue;
        }
        let declared_level = match map_get(descriptor, "level") {
            Some(value) => Some(integer(value, "GTS codec level")?),
            None => None,
        };
        if declared_level != Some(i128::from(GMEOW_GTS_ZSTD_LEVEL)) {
            return Err(profile_error(format!(
                "GTS codec catalog entry for {} must declare compression level {}; found {}",
                GMEOW_GTS_FRAME_TRANSFORM,
                GMEOW_GTS_ZSTD_LEVEL,
                declared_level.map_or_else(|| "none".to_string(), |level| level.to_string())
            )));
        }
        required_codec_ids.push(integer(id, "GTS codec id")?);
    }
    if required_codec_ids.len() != 1 {
        return Err(profile_error(format!(
            "GTS codec catalog must contain exactly one {} entry; found {}",
            GMEOW_GTS_FRAME_TRANSFORM,
            required_codec_ids.len()
        )));
    }
    let required_codec_id = required_codec_ids[0];

    let mut payload_frames = 0usize;
    for (offset, item) in items.iter().skip(1) {
        let frame = map(item, &format!("GTS frame at byte offset {offset}"))?;
        if map_get(frame, "d").is_none() {
            // A signed release may carry a metadata-only transport-key frame.
            // It has no payload bytes to transform and is not a codec exception.
            if map_get(frame, "x").is_some() {
                return Err(profile_error(format!(
                    "payload-free GTS frame at byte offset {offset} carries a transform chain"
                )));
            }
            continue;
        }

        payload_frames += 1;
        let transforms = match map_get(frame, "x") {
            Some(Value::Array(values)) => values,
            _ => {
                return Err(profile_error(format!(
                    "payload-bearing GTS frame at byte offset {offset} has no transform chain"
                )));
            }
        };
        if transforms.len() != 1 {
            return Err(profile_error(format!(
                "payload-bearing GTS frame at byte offset {offset} must carry exactly one transform; found {}",
                transforms.len()
            )));
        }
        let transform = integer(&transforms[0], "GTS frame transform id")?;
        if transform != required_codec_id {
            return Err(profile_error(format!(
                "payload-bearing GTS frame at byte offset {offset} uses codec id {transform}, not {} ({required_codec_id})",
                GMEOW_GTS_FRAME_TRANSFORM
            )));
        }
    }

    if payload_frames == 0 {
        return Err(profile_error(
            "GTS bundle carries no payload-bearing frames",
        ));
    }
    Ok(())
}

/// Emit a GMEOW snapshot using the one permitted production frame profile.
#[allow(clippy::too_many_arguments)]
pub fn emit_gmeow_gts(
    builder: &SnapshotBuilder,
    archive_blobs: Vec<BlobRow>,
    report_blobs: Vec<BlobRow>,
    signer_secret: Option<[u8; 32]>,
    signer_kid: Option<String>,
    public_key_armor: Option<String>,
) -> gmeow_errors::Result<Vec<u8>> {
    let transform = vec![GMEOW_GTS_FRAME_TRANSFORM.to_string()];
    // The medium plan is now CALLER-STATED data. Before purrdf 0.8.5 the writer
    // INFERRED the zstd level from `profile == "dist"` and applied at most one
    // dictionary by name-matching internally. `dist_default` reproduces the level
    // derivation exactly for a zstd-family chain, and pins no dictionary — which is
    // what the pre-0.8.5 writer did for every bundle this workspace has ever
    // authored (it never assigned `WriterOptions::dict`). `medium_plan_dist_default_*`
    // below holds both halves of that equivalence.
    let plan = purrdf::gts_compose::MediumPlan::dist_default(Some(&transform));
    purrdf::gts_compose::emit_gts(
        builder,
        "dist",
        Some(transform),
        archive_blobs,
        report_blobs,
        signer_secret,
        signer_kid,
        public_key_armor,
        purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
        &plan,
    )
    .map_err(|message| gmeow_errors::Diag::of_kind(error::Profile { message }))
}

#[cfg(test)]
mod tests {
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

    /// The pre-0.8.5 level derivation, reproduced verbatim as the oracle.
    ///
    /// The writer computed, for a caller-supplied `transform`:
    /// ```text
    /// let base_chain    = transform.unwrap_or_else(|| vec!["zstd".to_string()]);
    /// let chain_is_zstd = base_chain.iter().any(|t| t == "zstd" || t == "zstd-rsyncable");
    /// let zstd_level    = if profile == "dist" && chain_is_zstd { Some(DIST_ZSTD_LEVEL) } else { None };
    /// ```
    /// Every `emit_gts` call in this workspace passes `profile == "dist"`, so the
    /// profile term is constant and the derivation reduces to the chain test.
    fn legacy_zstd_level(transform: Option<&[String]>) -> Option<i32> {
        let owned;
        let base_chain: &[String] = match transform {
            Some(chain) => chain,
            None => {
                owned = vec!["zstd".to_string()];
                &owned
            }
        };
        let chain_is_zstd = base_chain
            .iter()
            .any(|t| t == "zstd" || t == "zstd-rsyncable");
        chain_is_zstd.then_some(purrdf::gts_compose::DIST_ZSTD_LEVEL)
    }

    /// Every transform-chain literal any `emit_gts` call in this workspace supplies.
    fn chain_literals_in_use() -> Vec<Option<Vec<String>>> {
        vec![
            None,
            Some(vec!["zstd-rsyncable".to_string()]),
            Some(vec!["zstd".to_string()]),
            Some(vec!["gzip".to_string()]),
            Some(vec!["identity".to_string()]),
        ]
    }

    /// `MediumPlan::dist_default` must reproduce the pre-0.8.5 level derivation for
    /// every chain this workspace actually emits. `MediumPlan` replaced TWO implicit
    /// behaviours, so both are pinned here: the level, and the dictionary.
    ///
    /// The dictionary half is the one that could silently move bundle bytes. At the
    /// pre-bump revision the dictionary was `purrdf_gts::writer::WriterOptions::dict`,
    /// defaulting to `None`, and `gts_compose::emit_gts` never assigned it — so every
    /// bundle this workspace has authored is baseline-encoded by construction and
    /// `undicted` is dictionary-equivalent. If an upstream `dist_default` ever starts
    /// pinning a dictionary, this reds instead of re-encoding the shipped bundle.
    #[test]
    fn medium_plan_dist_default_matches_the_pre_0_8_5_level_derivation() {
        for chain in chain_literals_in_use() {
            let plan = purrdf::gts_compose::MediumPlan::dist_default(chain.as_deref());
            assert_eq!(
                plan.zstd_level,
                legacy_zstd_level(chain.as_deref()),
                "level derivation drifted for chain {chain:?}"
            );
            assert!(
                plan.dicts.is_empty() && plan.assignment.is_empty(),
                "dist_default pinned a dictionary for chain {chain:?}; the pre-0.8.5 \
                 writer never assigned WriterOptions::dict, so this would re-encode \
                 every shipped bundle"
            );
        }
    }

    /// The mandated production chain specifically resolves to level 12 — the value
    /// the distribution contract and the compile-time assertion both name.
    #[test]
    fn the_mandated_chain_resolves_to_the_dist_level() {
        let transform = vec![GMEOW_GTS_FRAME_TRANSFORM.to_string()];
        let plan = purrdf::gts_compose::MediumPlan::dist_default(Some(&transform));
        assert_eq!(plan.zstd_level, Some(GMEOW_GTS_ZSTD_LEVEL));
    }

    #[test]
    fn production_profile_pins_transform_and_level() {
        assert_eq!(GMEOW_GTS_FRAME_TRANSFORM, "zstd-rsyncable");
        assert_eq!(GMEOW_GTS_ZSTD_LEVEL, 12);
        assert_eq!(purrdf::gts_compose::DIST_ZSTD_LEVEL, 12);

        let builder = fixture_builder();
        let bytes = emit_gmeow_gts(
            &builder,
            vec![BlobRow {
                data: b"small payload must not fall back to plain zstd".to_vec(),
                media_type: "text/plain".to_string(),
                rep: "profile-test".to_string(),
            }],
            Vec::new(),
            None,
            None,
            None,
        )
        .expect("emit fixture");
        validate_mandated_frames(&bytes).expect("fixture uses mandated frame profile");
    }

    #[test]
    fn profile_validator_rejects_a_payload_without_a_transform_chain() {
        let builder = fixture_builder();
        let bytes = emit_gmeow_gts(&builder, Vec::new(), Vec::new(), None, None, None)
            .expect("emit fixture");
        let (mut items, torn) = iter_items(&bytes);
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

    // ---- Shared fixtures for the negative-branch tests below ----------------
    //
    // Every hard-fail branch in `validate_mandated_frames` is proven with a
    // malformed input constructed by mutating a genuinely valid bundle, the
    // same way `profile_validator_rejects_a_payload_without_a_transform_chain`
    // does above — never a hand-typed CBOR literal, so the "malformed" input
    // differs from a real bundle in exactly the one respect under test.

    /// A materialized bundle with at least one payload (blob) frame.
    fn valid_bundle() -> Vec<u8> {
        let builder = fixture_builder();
        emit_gmeow_gts(
            &builder,
            vec![BlobRow {
                data: b"small payload for the profile audit fixture".to_vec(),
                media_type: "text/plain".to_string(),
                rep: "profile-test".to_string(),
            }],
            Vec::new(),
            None,
            None,
            None,
        )
        .expect("emit fixture")
    }

    /// `valid_bundle`, parsed into its raw `(offset, item)` pairs.
    fn valid_items() -> Vec<(usize, Value)> {
        let bytes = valid_bundle();
        let (items, torn) = iter_items(&bytes);
        assert!(torn.is_none(), "fixture bundle must not be torn");
        items
    }

    /// Re-serialize a `(offset, item)` sequence as a CBOR item sequence,
    /// discarding the (now-stale) offsets exactly as the existing negative
    /// test above does.
    fn serialize_items(items: &[(usize, Value)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (_, item) in items {
            ciborium::ser::into_writer(item, &mut out).expect("serialize fixture item");
        }
        out
    }

    /// The header item's map entries, unwrapping the optional self-describe tag.
    fn header_entries(item: &Value) -> Vec<(Value, Value)> {
        match item {
            Value::Tag(_, inner) => match inner.as_ref() {
                Value::Map(entries) => entries.clone(),
                other => panic!("header inner is not a CBOR map: {other:?}"),
            },
            Value::Map(entries) => entries.clone(),
            other => panic!("header item is not a CBOR map or tag: {other:?}"),
        }
    }

    /// Wrap mutated header entries the same way the original header item was
    /// wrapped (tagged or bare), so the mutation under test is the only change.
    fn rewrap_like(original: &Value, entries: Vec<(Value, Value)>) -> Value {
        match original {
            Value::Tag(tag, _) => Value::Tag(*tag, Box::new(Value::Map(entries))),
            _ => Value::Map(entries),
        }
    }

    /// Build a header-only bundle (no frames) from `valid_bundle`'s header,
    /// after applying `mutate`. Every header-processing hard fail (missing
    /// catalog, catalog cardinality, declared level) is reached before the
    /// frame loop ever runs, so a header-only sequence suffices to trip them.
    fn header_only_bytes(mutate: impl FnOnce(&mut Vec<(Value, Value)>)) -> Vec<u8> {
        let items = valid_items();
        let original = &items[0].1;
        let mut header = header_entries(original);
        mutate(&mut header);
        let item = rewrap_like(original, header);
        let mut out = Vec::new();
        ciborium::ser::into_writer(&item, &mut out).expect("serialize header");
        out
    }

    /// Set (or insert) a text-keyed field on a CBOR map's entry list.
    fn set_map_field(entries: &mut Vec<(Value, Value)>, key: &str, value: Value) {
        if let Some(entry) = entries
            .iter_mut()
            .find(|(k, _)| matches!(k, Value::Text(t) if t == key))
        {
            entry.1 = value;
        } else {
            entries.push((Value::Text(key.to_string()), value));
        }
    }

    /// Remove a text-keyed field from a CBOR map's entry list, if present.
    fn remove_map_field(entries: &mut Vec<(Value, Value)>, key: &str) {
        entries.retain(|(k, _)| !matches!(k, Value::Text(t) if t == key));
    }

    /// The codec catalog entries (the header's `"cat"` map), as an entry list.
    fn cat_entries(header: &[(Value, Value)]) -> Vec<(Value, Value)> {
        match map_get(header, "cat") {
            Some(Value::Map(entries)) => entries.clone(),
            other => panic!("codec catalog is not a CBOR map: {other:?}"),
        }
    }

    /// True when a codec descriptor's `"name"` is the mandated transform.
    fn descriptor_matches_transform(descriptor: &Value) -> bool {
        match descriptor {
            Value::Map(entries) => matches!(
                map_get(entries, "name"),
                Some(Value::Text(name)) if name == GMEOW_GTS_FRAME_TRANSFORM
            ),
            _ => false,
        }
    }

    /// Apply `mutate` to the first payload-bearing frame (has a `"d"` entry)
    /// found among `items[1..]`, in place. Panics if none is found, since
    /// every test using this expects the fixture to carry one.
    fn mutate_first_payload_frame(
        items: Vec<(usize, Value)>,
        mutate: impl FnOnce(&mut Vec<(Value, Value)>),
    ) -> Vec<(usize, Value)> {
        let mut mutate = Some(mutate);
        let out = items
            .into_iter()
            .map(|(offset, item)| {
                if mutate.is_some()
                    && let Value::Map(entries) = &item
                    && map_get(entries, "d").is_some()
                {
                    let mut entries = entries.clone();
                    mutate.take().expect("checked above")(&mut entries);
                    return (offset, Value::Map(entries));
                }
                (offset, item)
            })
            .collect();
        assert!(
            mutate.is_none(),
            "fixture must contain a payload frame to mutate"
        );
        out
    }

    #[test]
    fn profile_validator_rejects_a_torn_cbor_sequence() {
        let bytes = valid_bundle();
        let malformed = &bytes[..bytes.len() - 1];
        let error = validate_mandated_frames(malformed).expect_err("torn sequence must fail");
        assert!(error.to_string().contains("torn at byte offset"), "{error}");
    }

    #[test]
    fn profile_validator_rejects_a_bundle_with_no_header() {
        let error = validate_mandated_frames(&[]).expect_err("empty bundle must fail");
        assert!(error.to_string().contains("has no header"), "{error}");
    }

    #[test]
    fn profile_validator_rejects_a_header_with_no_codec_catalog() {
        let malformed = header_only_bytes(|header| remove_map_field(header, "cat"));
        let error =
            validate_mandated_frames(&malformed).expect_err("missing codec catalog must fail");
        assert!(
            error.to_string().contains("has no codec catalog"),
            "{error}"
        );
    }

    #[test]
    fn profile_validator_rejects_a_catalog_without_exactly_one_mandated_codec() {
        let malformed = header_only_bytes(|header| {
            let cat = cat_entries(header);
            let filtered: Vec<(Value, Value)> = cat
                .into_iter()
                .filter(|(_, descriptor)| !descriptor_matches_transform(descriptor))
                .collect();
            set_map_field(header, "cat", Value::Map(filtered));
        });
        let error = validate_mandated_frames(&malformed)
            .expect_err("catalog without the mandated codec must fail");
        assert!(
            error.to_string().contains("must contain exactly one"),
            "{error}"
        );
    }

    #[test]
    fn profile_validator_rejects_a_codec_catalog_entry_with_the_wrong_compression_level() {
        let malformed = header_only_bytes(|header| {
            let cat = cat_entries(header);
            let mutated: Vec<(Value, Value)> = cat
                .into_iter()
                .map(|(id, descriptor)| {
                    if !descriptor_matches_transform(&descriptor) {
                        return (id, descriptor);
                    }
                    let mut entries = match descriptor {
                        Value::Map(entries) => entries,
                        other => panic!("descriptor is not a CBOR map: {other:?}"),
                    };
                    set_map_field(&mut entries, "level", Value::Integer(3.into()));
                    (id, Value::Map(entries))
                })
                .collect();
            set_map_field(header, "cat", Value::Map(mutated));
        });
        let error = validate_mandated_frames(&malformed)
            .expect_err("wrong declared compression level must fail");
        assert!(
            error.to_string().contains("must declare compression level"),
            "{error}"
        );
    }

    #[test]
    fn profile_validator_rejects_a_payload_free_frame_carrying_a_transform_chain() {
        let items =
            mutate_first_payload_frame(valid_items(), |entries| remove_map_field(entries, "d"));
        let malformed = serialize_items(&items);
        let error = validate_mandated_frames(&malformed)
            .expect_err("payload-free frame with a transform chain must fail");
        assert!(
            error.to_string().contains("carries a transform chain"),
            "{error}"
        );
    }

    #[test]
    fn profile_validator_rejects_a_transform_chain_with_the_wrong_length() {
        let items = mutate_first_payload_frame(valid_items(), |entries| {
            set_map_field(entries, "x", Value::Array(Vec::new()));
        });
        let malformed = serialize_items(&items);
        let error =
            validate_mandated_frames(&malformed).expect_err("empty transform chain must fail");
        assert!(
            error
                .to_string()
                .contains("must carry exactly one transform"),
            "{error}"
        );
    }

    #[test]
    fn profile_validator_rejects_a_frame_using_the_wrong_codec_id() {
        let items = mutate_first_payload_frame(valid_items(), |entries| {
            set_map_field(
                entries,
                "x",
                Value::Array(vec![Value::Integer(999_999.into())]),
            );
        });
        let malformed = serialize_items(&items);
        let error = validate_mandated_frames(&malformed).expect_err("wrong codec id must fail");
        assert!(error.to_string().contains("uses codec id"), "{error}");
    }

    #[test]
    fn profile_validator_rejects_a_bundle_with_no_payload_frames() {
        let items = valid_items();
        let mut mutated_any = false;
        let stripped: Vec<(usize, Value)> = items
            .into_iter()
            .map(|(offset, item)| match &item {
                Value::Map(entries) if map_get(entries, "d").is_some() => {
                    mutated_any = true;
                    let mut entries = entries.clone();
                    entries
                        .retain(|(key, _)| !matches!(key, Value::Text(v) if v == "d" || v == "x"));
                    (offset, Value::Map(entries))
                }
                _ => (offset, item),
            })
            .collect();
        assert!(
            mutated_any,
            "fixture must contain at least one payload frame to strip"
        );
        let malformed = serialize_items(&stripped);
        let error =
            validate_mandated_frames(&malformed).expect_err("zero payload frames must fail");
        assert!(
            error.to_string().contains("no payload-bearing frames"),
            "{error}"
        );
    }
}
