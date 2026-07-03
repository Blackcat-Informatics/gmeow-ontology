// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Content-key computation.
//!
//! A form's content key is a deterministic string built by a recursive, tagged,
//! `SEP`-joined walk of its structural content. The composition rules mirror the
//! `logic:` IR discipline: each node kind gets a leading tag so keys of different
//! kinds never collide; unordered content (feature sets) is sorted before joining so
//! order is immaterial; ordered content (slot indexes) is carried explicitly. Nothing
//! from the surface stratum enters a form key. Digesting to a fixed-width identifier
//! happens only at the boundary, in [`Form::stable_id`], via SHA-256.

use crate::ast::{Form, MorphFeature, Slot, SurfaceForm};
use sha2::{Digest, Sha256};

/// The null-byte field separator, matching the `logic:` IR `sort_key` convention.
const SEP: char = '\u{0}';

impl MorphFeature {
    /// The order-independent key contribution of a feature: `key=v1,v2|layer=…` with
    /// the value set sorted so authoring order is immaterial.
    fn key_str(&self) -> String {
        let mut values = self.values.clone();
        values.sort();
        format!(
            "{}={}|layer={}",
            self.key,
            values.join(","),
            self.layer.as_deref().unwrap_or("")
        )
    }
}

/// Fold a feature set into an order-independent key fragment.
fn features_key(features: &[MorphFeature]) -> String {
    let mut parts: Vec<String> = features.iter().map(MorphFeature::key_str).collect();
    parts.sort();
    parts.join(",")
}

impl Slot {
    /// The key of a slot: its index (identity-bearing), role, dependency edge, and the
    /// content key of the constituent form.
    fn content_key(&self) -> String {
        format!(
            "SLOT{SEP}{}{SEP}role={}{SEP}dep={}{SEP}head={}{SEP}{}",
            self.index,
            self.role.as_deref().unwrap_or(""),
            self.dep_relation.as_deref().unwrap_or(""),
            self.depends_on.map_or_else(String::new, |h| h.to_string()),
            self.form.content_key(),
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
                format!("MORPHEME{SEP}{sign_system}{SEP}{id}")
            }
            Form::Morph {
                sign_system,
                morpheme,
                features,
            } => format!(
                "MORPH{SEP}{sign_system}{SEP}{}{SEP}feats[{}]",
                morpheme.content_key(),
                features_key(features),
            ),
            Form::Lexeme {
                sign_system,
                lemma,
                part_of_speech,
            } => format!(
                "LEXEME{SEP}{sign_system}{SEP}{lemma}{SEP}pos={}",
                part_of_speech.as_deref().unwrap_or(""),
            ),
            Form::WordForm {
                sign_system,
                lexeme,
                features,
            } => format!(
                "WORDFORM{SEP}{sign_system}{SEP}{}{SEP}feats[{}]",
                lexeme.content_key(),
                features_key(features),
            ),
            Form::OrthographicWord { sign_system, spans } => {
                // Spans are ordered (the token's decomposition sequence), so keys are
                // joined in order, not sorted.
                let inner: Vec<String> = spans.iter().map(Form::content_key).collect();
                format!("ORTHWORD{SEP}{sign_system}{SEP}({})", inner.join(","))
            }
            Form::Covert {
                sign_system,
                features,
            } => format!(
                "COVERT{SEP}{sign_system}{SEP}feats[{}]",
                features_key(features),
            ),
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
                    "COMPOSED{SEP}{sign_system}{SEP}level={level}{SEP}analysis={}{SEP}head={}{SEP}slots[{}]",
                    analysis.as_deref().unwrap_or(""),
                    head.map_or_else(String::new, |h| h.to_string()),
                    slot_keys.join("|"),
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
        digest[..12].iter().fold(String::new(), |mut acc, b| {
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
            "SURFACE{SEP}{}{SEP}script={}{SEP}enc={}{SEP}norm={}{SEP}coll={}",
            self.text, self.script, self.encoding, self.normalization, self.collation,
        )
    }
}
