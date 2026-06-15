// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! The GTS transform catalog (§8) — mirror of `src/gmeow_tools/gts/codec.py`.
//!
//! Each catalog entry is a codec with a canonical `name` and a `cls` of
//! `encode`, `compress` or `encrypt`. The baseline implements the core
//! `identity`/`gzip`/`zstd` codecs; an unknown codec or an `encrypt` codec
//! (no keys in the baseline) degrades to an opaque node (§7.6, §8.3).

use std::io::Read;

/// A catalog entry (§5, §8.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Codec {
    pub name: String,
    /// `"encode"` | `"compress"` | `"encrypt"`.
    pub cls: String,
}

/// Why a transform chain could not be reversed.
#[derive(Debug)]
pub enum CodecError {
    /// A missing capability: `reason` is `"unknown-codec"` or `"missing-key"`
    /// — the frame degrades to an opaque node with that reason (§8.3).
    Unavailable {
        reason: &'static str,
        detail: String,
    },
    /// The codec is known but the data is corrupt — the frame is damaged.
    Failed(String),
}

fn decode_one(codec: &Codec, data: &[u8]) -> Result<Vec<u8>, CodecError> {
    if codec.cls == "encrypt" {
        return Err(CodecError::Unavailable {
            reason: "missing-key",
            detail: format!("no key for encrypt codec '{}'", codec.name),
        });
    }
    match codec.name.as_str() {
        "identity" => Ok(data.to_vec()),
        "gzip" => {
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(data)
                .read_to_end(&mut out)
                .map_err(|e| CodecError::Failed(format!("gzip decode failed: {e}")))?;
            Ok(out)
        }
        "zstd" | "zstd-rsyncable" => {
            // ruzstd's StreamingDecoder only handles a single zstd frame.
            // zstd-rsyncable concatenates independent frames (one per block),
            // so use FrameDecoder::decode_all_to_vec, which loops over frames
            // while input remains (see ruzstd src/decoding/frame_decoder.rs).
            let mut decoder = ruzstd::decoding::FrameDecoder::new();
            // Start with a generous expansion factor and allow bounded growth.
            const MAX_ZSTD_DECODED_SIZE: usize = 16 * 1024 * 1024;
            let max_capacity = data
                .len()
                .saturating_mul(4)
                .clamp(4096, MAX_ZSTD_DECODED_SIZE);
            let mut capacity = max_capacity;
            loop {
                let mut out = Vec::with_capacity(capacity);
                match decoder.decode_all_to_vec(data, &mut out) {
                    Ok(()) => return Ok(out),
                    Err(ruzstd::decoding::errors::FrameDecoderError::TargetTooSmall) => {
                        if capacity >= max_capacity {
                            return Err(CodecError::Failed(
                                "zstd decode failed: decompressed size exceeds safety bound".into(),
                            ));
                        }
                        capacity = (capacity * 2).min(max_capacity);
                        continue;
                    }
                    Err(e) => return Err(CodecError::Failed(format!("zstd decode failed: {e}"))),
                }
            }
        }
        other => Err(CodecError::Unavailable {
            reason: "unknown-codec",
            detail: format!("unknown codec '{other}'"),
        }),
    }
}

/// Reverse a resolved codec chain, last to first (§6.1, §8.2).
///
/// The baseline carries no keys, so every `encrypt`-class codec degrades to
/// `missing-key` (matching the Python reader with `keys=None`).
pub fn decode_chain(chain: &[Codec], data: &[u8]) -> Result<Vec<u8>, CodecError> {
    let mut current = data.to_vec();
    for codec in chain.iter().rev() {
        current = decode_one(codec, &current)?;
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zstd_rsyncable_decodes_concatenated_frames() {
        // Build a multi-frame zstd stream that mirrors zstd-rsyncable output.
        let block1 = b"first block of rsyncable data ";
        let block2 = b"second block of rsyncable data";
        let mut encoded = ruzstd::encoding::compress_to_vec(
            &block1[..],
            ruzstd::encoding::CompressionLevel::Uncompressed,
        );
        encoded.extend(ruzstd::encoding::compress_to_vec(
            &block2[..],
            ruzstd::encoding::CompressionLevel::Uncompressed,
        ));

        let decoded = decode_one(
            &Codec {
                name: "zstd-rsyncable".into(),
                cls: "compress".into(),
            },
            &encoded,
        )
        .expect("multi-frame zstd must decode");

        let mut expected = block1.to_vec();
        expected.extend_from_slice(block2);
        assert_eq!(decoded, expected);
    }
}
