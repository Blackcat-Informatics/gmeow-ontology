// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The mandatory GMEOW GTS authorship profile.
//!
//! Every payload-bearing frame emitted by production code uses one transform,
//! `zstd-rsyncable`, at compression level 12.  Keep this wrapper as the only
//! production entry to `purrdf::gts_compose::emit_gts`; direct calls belong only
//! in codec-specific tests.

use purrdf::gts_compose::{BlobRow, SnapshotBuilder};

/// Required transform on every payload-bearing GMEOW GTS frame.
pub const GMEOW_GTS_FRAME_TRANSFORM: &str = "zstd-rsyncable";

/// Required zstd compression level on every payload-bearing GMEOW GTS frame.
pub const GMEOW_GTS_ZSTD_LEVEL: i32 = 12;

// `emit_gts` applies its public dist-profile level to every blob and snapshot
// frame.  Turn an upstream default drift into a compile failure rather than a
// silently different committed bundle.
const _: () = assert!(purrdf::gts_compose::DIST_ZSTD_LEVEL == GMEOW_GTS_ZSTD_LEVEL);

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
    use ciborium::value::Value;
    use purrdf::gts::wire::{iter_items, map_get, unwrap_header};

    fn map(value: &Value) -> &[(Value, Value)] {
        match value {
            Value::Map(entries) => entries,
            _ => panic!("GTS item is not a map"),
        }
    }

    fn integer(value: &Value) -> i128 {
        match value {
            Value::Integer(value) => i128::from(*value),
            _ => panic!("catalog key is not an integer"),
        }
    }

    fn codec_id(header: &[(Value, Value)], name: &str) -> i128 {
        let catalog = map(map_get(header, "cat").expect("header carries codec catalog"));
        catalog
            .iter()
            .find_map(|(id, descriptor)| {
                let descriptor = map(descriptor);
                matches!(map_get(descriptor, "name"), Some(Value::Text(value)) if value == name)
                    .then(|| integer(id))
            })
            .unwrap_or_else(|| panic!("codec catalog has no {name}"))
    }

    fn assert_mandated_frames(bytes: &[u8]) {
        let (items, torn) = iter_items(bytes);
        assert!(torn.is_none(), "GTS CBOR sequence must be complete");
        let header = unwrap_header(&items.first().expect("GTS header").1).expect("valid header");
        let required = codec_id(header, GMEOW_GTS_FRAME_TRANSFORM);
        let mut payload_frames = 0usize;
        for (_, item) in items.iter().skip(1) {
            let frame = map(item);
            if map_get(frame, "d").is_none() {
                // A signed release may carry a metadata-only transport-key frame.
                // It has no payload bytes to transform and is not a codec exception.
                assert!(map_get(frame, "x").is_none());
                continue;
            }
            payload_frames += 1;
            let transforms = match map_get(frame, "x") {
                Some(Value::Array(values)) => values,
                _ => panic!("payload-bearing frame has no transform chain"),
            };
            assert_eq!(
                transforms.len(),
                1,
                "exactly one frame transform is permitted"
            );
            assert_eq!(
                integer(&transforms[0]),
                required,
                "frame transform must be zstd-rsyncable"
            );
        }
        assert!(
            payload_frames > 0,
            "bundle must carry payload-bearing frames"
        );
    }

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
        assert_mandated_frames(&bytes);
    }

    #[test]
    fn committed_bundle_uses_the_mandated_frame_profile() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("workspace root");
        let bytes = std::fs::read(root.join("generated/dist/gmeow.gts"))
            .expect("read committed GMEOW bundle");
        assert_mandated_frames(&bytes);
    }
}
