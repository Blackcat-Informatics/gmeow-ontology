// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW-owned literal interpretations shared by compilation and reasoning.

use purrdf::RdfTextDirection;

/// The algorithm identity is part of every plain BLAKE3-256 value.
pub const BLAKE3_256_DIGEST: &str = "https://blackcatinformatics.ca/gmeow/blake3_256Digest";

/// The algorithm identity is part of every SHA-256 value.
pub const SHA256_DIGEST: &str = "https://blackcatinformatics.ca/gmeow/sha256Digest";

/// The algorithm identity is part of every MD5 value.
pub const MD5_DIGEST: &str = "https://blackcatinformatics.ca/gmeow/md5Digest";

/// A SHA-256 value carries its algorithm independently of generic binary values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    /// Decode exactly 64 lowercase hexadecimal digits, without normalization.
    /// An independently selected algorithm (such as SPDX) can use this boundary.
    pub fn from_canonical_hex(lexical: &str) -> Option<Self> {
        purrdf_hash::hex::decode_32_canonical(lexical).map(Self)
    }

    /// Admit the complete native literal, preserving its algorithm and facets.
    pub fn from_parts(
        lexical: &str,
        datatype: &str,
        language: Option<&str>,
        direction: Option<RdfTextDirection>,
    ) -> Option<Self> {
        if datatype != SHA256_DIGEST || language.is_some() || direction.is_some() {
            return None;
        }
        Self::from_canonical_hex(lexical)
    }
}

/// A plain BLAKE3-256 value retains its algorithm independently of SHA-256.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blake3Digest([u8; 32]);

impl Blake3Digest {
    /// Admit the complete native literal, preserving its algorithm and facets.
    pub fn from_parts(
        lexical: &str,
        datatype: &str,
        language: Option<&str>,
        direction: Option<RdfTextDirection>,
    ) -> Option<Self> {
        if datatype != BLAKE3_256_DIGEST || language.is_some() || direction.is_some() {
            return None;
        }
        purrdf_hash::hex::decode_32_canonical(lexical).map(Self)
    }
}

/// An MD5 value carries its algorithm independently of generic binary values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Md5Digest([u8; 16]);

impl Md5Digest {
    /// Admit exactly 32 lowercase hexadecimal digits with their MD5 datatype.
    /// The length guard bounds the canonical decoder's allocation to 16 bytes.
    pub fn from_parts(
        lexical: &str,
        datatype: &str,
        language: Option<&str>,
        direction: Option<RdfTextDirection>,
    ) -> Option<Self> {
        if datatype != MD5_DIGEST
            || language.is_some()
            || direction.is_some()
            || lexical.len() != 32
        {
            return None;
        }
        purrdf_hash::hex::decode_canonical(lexical)
            .ok()?
            .try_into()
            .ok()
            .map(Self)
    }
}
