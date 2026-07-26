// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The IRI vocabulary a lift emits into.
//!
//! Every term here is one the grounding slices ALREADY declare — this module names them,
//! it does not mint them. A lifter that needs a term the ontology does not have is a
//! signal to author the term in the slice, never to invent one here: a Rust-side IRI with
//! no `module.ttl` declaration is a second source of truth.

/// The `math:` grounding namespace.
pub const MATH: &str = "https://blackcatinformatics.ca/math/";
/// The `logic:` grounding namespace — the canonical reasoning language.
pub const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
/// The core `gmeow:` namespace.
pub const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

/// `rdf:type`.
pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// `xsd:boolean`.
pub const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
/// `xsd:integer`.
pub const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
/// `xsd:decimal`.
pub const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
/// `xsd:base64Binary` — the lexical form a retained binary source rides in.
pub const XSD_BASE64: &str = "http://www.w3.org/2001/XMLSchema#base64Binary";

/// A `math:` term IRI.
#[must_use]
pub fn math(local: &str) -> String {
    format!("{MATH}{local}")
}

/// A `logic:` term IRI.
#[must_use]
pub fn logic(local: &str) -> String {
    format!("{LOGIC}{local}")
}

/// A `gmeow:` term IRI.
#[must_use]
pub fn gmeow(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// FNV-1a 64-bit, lower-case hex.
///
/// A stable, portable, dependency-free content hash used ONLY to disambiguate minted
/// IRIs. It is not a security primitive and never a term's semantic identity — the
/// semantic identity of a lifted expression is its
/// [`ContentKey`](gmeow_term_arena::ContentKey), produced by the arena's fold.
///
/// Determinism is the point: the same source bytes mint the same IRI, so a re-lift is
/// byte-identical and the producer stays idempotent. No clock, no counter, no randomness.
#[must_use]
pub fn fnv1a_hex(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}
