// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Content-key computation.
//!
//! A form's content key is a deterministic string built by a recursive, tagged,
//! *length-prefixed* (netstring-style) walk of its structural content. Every
//! variable-length component is emitted as a self-delimiting prefix code — a raw
//! string as `<byte-len>:<bytes>`, an optional as `N`/`S<field>`, an ordered list as
//! `<count>:` followed by each element length-prefixed — so concatenations are
//! unambiguously decodable and the key is *injective by construction*: two structurally
//! distinct forms can never share a key. This replaces the earlier in-band
//! delimiter scheme (`\u{0}`, `,`, `|`, `=`), which was not injective — `None` and
//! `Some("")` aliased, and a raw field containing a delimiter aliased a different
//! structure. The composition rules still mirror the `logic:` IR discipline: each node
//! kind gets a leading constant tag so keys of different kinds never collide; unordered
//! content (feature sets, slots) is canonicalized (sorted) before encoding so authoring
//! order is immaterial; ordered content (orthographic spans) is encoded in order.
//! Nothing from the surface stratum enters a form key. Digesting to a fixed-width
//! identifier happens only at the boundary, in [`Form::stable_id`], via SHA-256.

use crate::ast::{Form, MorphFeature, Slot, SurfaceForm};
use sha2::{Digest, Sha256};

/// Length-prefixed encoding of a raw string field: `<byte-len>:<bytes>`. A prefix
/// code — concatenations of these are unambiguously decodable, so the key is injective.
fn field(s: &str) -> String {
    format!("{}:{}", s.len(), s)
}

/// Injective encoding of an optional string: a bare `N` for `None`, `S<field>` for
/// `Some`, so `None` and `Some("")` differ (`N` vs `S0:`).
fn opt(o: Option<&str>) -> String {
    match o {
        None => "N".to_owned(),
        Some(s) => format!("S{}", field(s)),
    }
}

/// Count-prefixed encoding of an ordered list of element strings: `<count>:` then each
/// element length-prefixed via [`field`], so nested lengths can never be confused with
/// the surrounding structure.
fn seq(items: &[String]) -> String {
    let mut out = format!("{}:", items.len());
    for it in items {
        out.push_str(&field(it));
    }
    out
}

impl MorphFeature {
    /// The order-independent key contribution of a feature: the feature key, its value
    /// set (sorted, so authoring order is immaterial), and its optional layer, each
    /// encoded as a self-delimiting prefix code.
    fn key_str(&self) -> String {
        let mut values = self.values.clone();
        values.sort();
        format!(
            "{}{}{}",
            field(&self.key),
            seq(&values),
            opt(self.layer.as_deref()),
        )
    }
}

/// Fold a feature set into an order-independent key fragment: each feature's element
/// string is built, the element strings are sorted, then count-and-length prefixed.
fn features_key(features: &[MorphFeature]) -> String {
    let mut parts: Vec<String> = features.iter().map(MorphFeature::key_str).collect();
    parts.sort();
    seq(&parts)
}

impl Slot {
    /// The key of a slot: its index (identity-bearing), role, dependency edge, and the
    /// content key of the constituent form — each a self-delimiting prefix code, so a
    /// missing role/dep/head differs from any present value.
    fn content_key(&self) -> String {
        format!(
            "SLOT{}{}{}{}{}",
            field(&self.index.to_string()),
            opt(self.role.as_deref()),
            opt(self.dep_relation.as_deref()),
            opt(self.depends_on.map(|h| h.to_string()).as_deref()),
            field(&self.form.content_key()),
        )
    }
}

impl Form {
    /// The sign system this form names — every form names exactly one.
    #[must_use]
    pub fn sign_system(&self) -> &str {
        match self {
            Form::Morpheme { sign_system, .. }
            | Form::Morph { sign_system, .. }
            | Form::Lexeme { sign_system, .. }
            | Form::WordForm { sign_system, .. }
            | Form::OrthographicWord { sign_system, .. }
            | Form::Covert { sign_system, .. }
            | Form::Composed { sign_system, .. } => sign_system,
        }
    }

    /// The deterministic content key: structural content only, never any surface,
    /// encoding, script, casing, normalization, or rendering.
    #[must_use]
    pub fn content_key(&self) -> String {
        match self {
            Form::Morpheme { sign_system, id } => {
                format!("MORPHEME{}{}", field(sign_system), field(id))
            }
            Form::Morph {
                sign_system,
                morpheme,
                features,
            } => format!(
                "MORPH{}{}{}",
                field(sign_system),
                field(&morpheme.content_key()),
                features_key(features),
            ),
            Form::Lexeme {
                sign_system,
                lemma,
                part_of_speech,
            } => format!(
                "LEXEME{}{}{}",
                field(sign_system),
                field(lemma),
                opt(part_of_speech.as_deref()),
            ),
            Form::WordForm {
                sign_system,
                lexeme,
                features,
            } => format!(
                "WORDFORM{}{}{}",
                field(sign_system),
                field(&lexeme.content_key()),
                features_key(features),
            ),
            Form::OrthographicWord { sign_system, spans } => {
                // Spans are ordered (the token's decomposition sequence), so keys are
                // encoded in order, not sorted.
                let inner: Vec<String> = spans.iter().map(Form::content_key).collect();
                format!("ORTHWORD{}{}", field(sign_system), seq(&inner))
            }
            Form::Covert {
                sign_system,
                features,
            } => format!("COVERT{}{}", field(sign_system), features_key(features),),
            Form::Composed {
                sign_system,
                level,
                analysis,
                head,
                slots,
            } => {
                // Order slots by their content key (which leads with the index), so
                // authoring order is immaterial while index order is preserved.
                let mut slot_keys: Vec<String> = slots.iter().map(Slot::content_key).collect();
                slot_keys.sort();
                format!(
                    "COMPOSED{}{}{}{}{}",
                    field(sign_system),
                    field(level),
                    opt(analysis.as_deref()),
                    opt(head.map(|h| h.to_string()).as_deref()),
                    seq(&slot_keys),
                )
            }
        }
    }

    /// A stable, fixed-width identifier for the form: the first 12 bytes of the
    /// SHA-256 of its content key, hex-encoded. The only place a hasher is applied —
    /// keys compose by string concatenation, and digesting happens at the boundary.
    #[must_use]
    pub fn stable_id(&self) -> String {
        let digest = Sha256::digest(self.content_key().as_bytes());
        digest[..12]
            .iter()
            .fold(String::with_capacity(24), |mut acc, b| {
                use std::fmt::Write as _;
                let _ = write!(acc, "{b:02x}");
                acc
            })
    }
}

impl SurfaceForm {
    /// The material-identity key of a surface: text, script, encoding, normalization,
    /// and collation locale — the frame in which byte identity is load-bearing. Two
    /// surfaces of one form (an NFC and an NFD copy, say) have distinct surface keys
    /// but realize a form with a single content key.
    #[must_use]
    pub fn surface_key(&self) -> String {
        format!(
            "SURFACE{}{}{}{}{}",
            field(&self.text),
            field(&self.script),
            field(&self.encoding),
            field(&self.normalization),
            field(&self.collation),
        )
    }
}
