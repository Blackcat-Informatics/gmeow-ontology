// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The mandatory GMEOW GTS authorship profile.
//!
//! Every payload-bearing frame emitted by production code uses one transform,
//! `zstd-rsyncable`, at compression level 12.  Keep this wrapper as the only
//! production entry to `purrdf::gts_compose::emit_gts`; direct calls belong only
//! in codec-specific tests.

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
    gmeow_errors::Diag::of_kind(crate::error::Transform {
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
/// validation gate. The compile-time assertion above separately pins the only
/// production emitter's zstd level to 12.
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
        if matches!(
            map_get(descriptor, "name"),
            Some(Value::Text(name)) if name == GMEOW_GTS_FRAME_TRANSFORM
        ) {
            required_codec_ids.push(integer(id, "GTS codec id")?);
        }
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
pub(crate) fn emit_gmeow_gts(
    builder: &SnapshotBuilder,
    doc_blobs: Vec<BlobRow>,
    report_blobs: Vec<BlobRow>,
    signer_secret: Option<[u8; 32]>,
    signer_kid: Option<String>,
    public_key_armor: Option<String>,
) -> gmeow_errors::Result<Vec<u8>> {
    purrdf::gts_compose::emit_gts(
        builder,
        "dist",
        Some(vec![GMEOW_GTS_FRAME_TRANSFORM.to_string()]),
        doc_blobs,
        report_blobs,
        signer_secret,
        signer_kid,
        public_key_armor,
        purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
    )
    .map_err(|message| gmeow_errors::Diag::of_kind(crate::error::Transform { message }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_profile_pins_transform_and_level() {
        assert_eq!(GMEOW_GTS_FRAME_TRANSFORM, "zstd-rsyncable");
        assert_eq!(GMEOW_GTS_ZSTD_LEVEL, 12);
        assert_eq!(purrdf::gts_compose::DIST_ZSTD_LEVEL, 12);

        let dataset = purrdf::parse_dataset(
            b"<https://e/s> <https://e/p> <https://e/o> .\n",
            purrdf::NativeRdfFormat::NTriples.media_type(),
            None,
        )
        .expect("parse fixture");
        let mut builder = SnapshotBuilder::new();
        builder.add_dataset(&dataset).expect("add fixture");
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
    fn committed_bundle_uses_the_mandated_frame_profile() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("workspace root");
        let bytes = std::fs::read(root.join("generated/dist/gmeow.gts"))
            .expect("read committed GMEOW bundle");
        validate_mandated_frames(&bytes).expect("committed bundle uses mandated frame profile");
    }

    #[test]
    fn profile_validator_rejects_a_payload_without_a_transform_chain() {
        let dataset = purrdf::parse_dataset(
            b"<https://e/s> <https://e/p> <https://e/o> .\n",
            purrdf::NativeRdfFormat::NTriples.media_type(),
            None,
        )
        .expect("parse fixture");
        let mut builder = SnapshotBuilder::new();
        builder.add_dataset(&dataset).expect("add fixture");
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
}
