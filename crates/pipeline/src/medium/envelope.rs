// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Sealing and opening a `gmeow:MediumEnvelope`.
//!
//! An envelope is a PROJECTION of facts a frame already carries in band — its rep,
//! its content digest, the `Dictionary_ID` in its zstd header — never a settings
//! record chosen at projection time. [`seal`] builds one from those facts plus the
//! registry; [`open`] is the reader-side inverse that refuses BEFORE any decode is
//! attempted.
//!
//! # Why the digest is stratified
//!
//! The snapshot frame's own envelope is SELF-REFERENTIAL: the snapshot content id is
//! a digest over the entire snapshot payload, which folds the very graph the envelope
//! lives in, so writing that digest into the envelope changes the payload it digests.
//! Iterating to a fixed point does not converge — BLAKE3 is not a contraction, so
//! successive passes wander rather than settle. Stratifying fixes it:
//! [`DigestStratum::PayloadExcludingMediumEnvelope`] names a region the envelope is
//! NOT inside, so pass one emits the payload without the envelope, pass two adds the
//! envelope carrying the pass-one stratum digest, and that addition provably cannot
//! change the digest. The emission converges in exactly two passes.
//!
//! # Nothing here ever degrades to a dictionary-less decode
//!
//! A payload written through a primed medium is not readable at lower fidelity
//! without its dictionary — it is not readable AT ALL. Priming changes the CODE, not
//! the framing, so an unprimed decode of a primed payload yields plausible-looking
//! garbage that no checksum inside the payload would catch. Every refusal below is
//! therefore terminal.

use std::collections::BTreeSet;

use super::registry::{DictSelection, DictionaryDef, MediumRegistry, MediumSelection};
use super::{
    GMEOW, blake3_digest, digest_mismatch, is_canonical_digest, opaque_frame,
    undeclared_dictionary, unknown_dictionary,
};

/// The named sub-payload a `gmeow:strataDigest` commits to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DigestStratum {
    /// The entire frame payload. Legal only where the envelope is NOT inside the
    /// payload it digests.
    WholePayload,
    /// The payload MINUS the medium-envelope subgraph — the stratum that breaks the
    /// snapshot envelope's self-reference.
    PayloadExcludingMediumEnvelope,
}

impl DigestStratum {
    /// The `gmeow:DigestStratum` individual naming this stratum.
    #[must_use]
    pub fn iri(self) -> String {
        let local = match self {
            Self::WholePayload => "stratumWholePayload",
            Self::PayloadExcludingMediumEnvelope => "stratumPayloadExcludingMediumEnvelope",
        };
        format!("{GMEOW}{local}")
    }
}

/// The facts a frame carries IN BAND, which an envelope projects.
#[derive(Debug, Clone, Copy)]
pub struct FrameFacts<'a> {
    /// The frame's own identity (its GTS frame id), as an IRI.
    pub frame: &'a str,
    /// The frame's `pub.rep` content-representation tag.
    pub rep: &'a str,
    /// The frame's decoded payload bytes.
    pub payload: &'a [u8],
    /// The bytes of the stratum the envelope's `gmeow:strataDigest` commits to.
    /// Equals `payload` exactly when the stratum is
    /// [`DigestStratum::WholePayload`].
    pub stratum_bytes: &'a [u8],
    /// The stratum those bytes are.
    pub stratum: DigestStratum,
    /// The pack dictionary id the frame's zstd header cites, when it is primed.
    /// `None` means the frame declares no dictionary — legal only under a medium
    /// that declares none either.
    pub dictionary_id: Option<&'a str>,
}

/// The already-computed content identities of one frame.
///
/// This is the ownership-safe form used by the terminal snapshot emitter.  Its
/// snapshot payload and canonical RDF stratum are both whole-carrier values; keeping
/// either byte buffer alive until the other has been rendered needlessly adds their
/// sizes to peak residency.  The emitter therefore hashes each representation in its
/// own scope, drops the bytes, and seals the same facts through this typed record.
#[derive(Debug, Clone, Copy)]
pub struct FrameDigestFacts<'a> {
    /// The content-addressed frame IRI.
    pub frame: &'a str,
    /// The frame's `pub.rep` content-representation tag.
    pub rep: &'a str,
    /// Canonical `blake3:<hex>` identity of the decoded frame payload.
    pub content_digest: &'a str,
    /// Canonical `blake3:<hex>` identity of the selected digest stratum.
    pub strata_digest: &'a str,
    /// The stratum whose identity [`Self::strata_digest`] carries.
    pub stratum: DigestStratum,
    /// The in-band dictionary id, if the frame is primed.
    pub dictionary_id: Option<&'a str>,
}

/// Owned digest facts used when the caller has already allocated the frame identity
/// and canonical digests and can transfer them directly into the envelope.
pub(crate) struct OwnedFrameDigestFacts<'a> {
    /// The content-addressed frame IRI.
    pub frame: String,
    /// The frame's registered representation tag.
    pub rep: &'a str,
    /// Canonical decoded-payload identity.
    pub content_digest: String,
    /// Canonical selected-stratum identity.
    pub strata_digest: String,
    /// The stratum named by `strata_digest`.
    pub stratum: DigestStratum,
    /// The in-band dictionary id, when primed.
    pub dictionary_id: Option<&'a str>,
}

/// The seven-field medium-envelope contract, as a typed record.
///
/// Every field is exactly-one because a projection missing a coordinate is a
/// DIFFERENT claim rather than a weaker one. [`Self::dictionary`] is the single
/// nuance: it is absent exactly when the assigned medium declares an empty
/// `gmeow:mediumDictionary` set, which is that medium's explicit "no dictionary"
/// SELECTION — not an omission.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MediumEnvelope {
    /// `gmeow:envelopePayloadFrame` — the frame whose payload this describes.
    pub frame: String,
    /// `gmeow:envelopeSchema` — the registered `gmeow:PayloadSchema` IRI.
    pub schema: String,
    /// `gmeow:envelopeMedium` — the `gmeow:Medium` IRI the bytes were written
    /// through.
    pub medium: String,
    /// `gmeow:envelopeDictionary` — the priming dictionary's IRI.
    pub dictionary: Option<String>,
    /// `gmeow:envelopeDigestStratum` — WHICH region the stratum digest commits to.
    pub stratum: DigestStratum,
    /// `gmeow:strataDigest` over exactly that stratum.
    pub strata_digest: String,
    /// `gmeow:contentDigest` — the frame's own in-band byte identity, carried
    /// beside the stratum digest rather than replaced by it.
    pub content_digest: String,
}

/// Project a `gmeow:MediumEnvelope` from the facts a frame already carries, under the
/// [`MediumSelection`] the emission was written through.
///
/// The selection is a PARAMETER rather than always the authored assignment because an
/// envelope describes the frame this emission actually wrote. An emission through the
/// declared no-dictionary medium writes unprimed frames, and an envelope that named the
/// authored `gmeow:mediumProfileDistL12` over them would be a projection of an intention
/// instead of a projection of the wire — precisely the failure the whole class exists to
/// exclude.
///
/// # Errors
/// `MediumUnknownSchema` (unregistered rep), `MediumUndeclaredDictionary` (the rep
/// has no assignment, or a primed frame declares no dictionary), or
/// `MediumUnknownDictionary` (the frame's in-band dictionary does not resolve, or
/// disagrees with the one the registry assigns that rep).
pub fn seal(
    registry: &MediumRegistry,
    selection: &MediumSelection,
    facts: &FrameFacts<'_>,
) -> Result<MediumEnvelope, gmeow_errors::Diag> {
    let content_digest = blake3_digest(facts.payload);
    let strata_digest = blake3_digest(facts.stratum_bytes);
    seal_digests(
        registry,
        selection,
        &FrameDigestFacts {
            frame: facts.frame,
            rep: facts.rep,
            content_digest: &content_digest,
            strata_digest: &strata_digest,
            stratum: facts.stratum,
            dictionary_id: facts.dictionary_id,
        },
    )
}

/// Project a [`MediumEnvelope`] from canonical frame digests already computed by
/// the caller.
///
/// This is semantically identical to [`seal`]; it changes only ownership.  A caller
/// may release a whole-frame serialization immediately after hashing it instead of
/// retaining two multi-gigabyte preimages until envelope projection finishes.
///
/// # Errors
/// The same registry/medium mismatches as [`seal`], or a non-canonical supplied
/// digest.
pub fn seal_digests(
    registry: &MediumRegistry,
    selection: &MediumSelection,
    facts: &FrameDigestFacts<'_>,
) -> Result<MediumEnvelope, gmeow_errors::Diag> {
    seal_owned_digests(
        registry,
        selection,
        OwnedFrameDigestFacts {
            frame: facts.frame.to_string(),
            rep: facts.rep,
            content_digest: facts.content_digest.to_string(),
            strata_digest: facts.strata_digest.to_string(),
            stratum: facts.stratum,
            dictionary_id: facts.dictionary_id,
        },
    )
}

/// Seal canonical identities while transferring their owned strings into the result.
///
/// Blob authors use this after computing the in-band digest once. It preserves the
/// validation and registry checks of [`seal_digests`] while avoiding another frame-IRI
/// copy and one of the two equal whole-payload digest copies.
pub(crate) fn seal_owned_digests(
    registry: &MediumRegistry,
    selection: &MediumSelection,
    facts: OwnedFrameDigestFacts<'_>,
) -> Result<MediumEnvelope, gmeow_errors::Diag> {
    for (label, digest) in [
        ("content digest", facts.content_digest.as_str()),
        ("strata digest", facts.strata_digest.as_str()),
    ] {
        if !is_canonical_digest(digest) {
            return Err(digest_mismatch(format!(
                "frame <{}> (rep {:?}) carries a non-canonical {label} {digest:?}; expected \
                 'blake3:<64 lowercase hex>'",
                facts.frame, facts.rep
            )));
        }
    }

    let row = registry.resolved_assignment(selection, facts.rep)?;

    let dictionary = match (&row.dictionary, facts.dictionary_id) {
        (DictSelection::Baseline, None) => None,
        (DictSelection::Baseline, Some(id)) => {
            return Err(unknown_dictionary(format!(
                "frame <{}> (rep {:?}) is primed with dictionary {id:?}, but its assigned medium \
                 <{}> declares NO dictionary — the medium's declared set is the bound on what it \
                 may prime with",
                facts.frame, facts.rep, row.medium
            )));
        }
        (DictSelection::Named(assigned), None) => {
            return Err(undeclared_dictionary(format!(
                "frame <{}> (rep {:?}) declares no dictionary in band, but its rep is assigned \
                 <{assigned}> — an undeclared dictionary is undiscoverable, so the payload would \
                 be permanently undecodable even with its bytes intact",
                facts.frame, facts.rep
            )));
        }
        (DictSelection::Named(assigned), Some(id)) => {
            let resolved = registry.dictionary_by_id(id)?;
            if resolved.iri != *assigned {
                return Err(unknown_dictionary(format!(
                    "frame <{}> (rep {:?}) is primed with dictionary {id:?} (<{}>), but its rep is \
                     assigned <{assigned}> — decoding a payload against a DIFFERENT dictionary \
                     produces plausible-looking garbage rather than an error, so the disagreement \
                     is refused here",
                    facts.frame, facts.rep, resolved.iri
                )));
            }
            Some(resolved.iri.clone())
        }
    };

    Ok(MediumEnvelope {
        frame: facts.frame,
        schema: row.schema.clone(),
        medium: row.medium.clone(),
        dictionary,
        stratum: facts.stratum,
        strata_digest: facts.strata_digest,
        content_digest: facts.content_digest,
    })
}

/// The reader capabilities a decoder holds, checked against a medium's
/// `gmeow:requiresReaderCapability`.
pub type ReaderCapabilities = BTreeSet<String>;

/// Open an envelope: prove every claim it makes BEFORE any decode is attempted, and
/// return the dictionary the decode must be primed with (`None` = the declared
/// baseline selection).
///
/// `payload` and `stratum_bytes` are the bytes at hand; the digests are RECOMPUTED
/// over them rather than trusted, because a digest that is never checked is a
/// comment.
///
/// # Errors
/// `MediumUnknownSchema`, `MediumUnknownDictionary`, `MediumDigestMismatch` (a
/// malformed digest literal or one that disagrees with the bytes), or
/// `MediumOpaqueFrame` (the reader lacks a capability the medium declares).
pub fn open<'r>(
    envelope: &MediumEnvelope,
    registry: &'r MediumRegistry,
    capabilities: &ReaderCapabilities,
    payload: &[u8],
    stratum_bytes: &[u8],
) -> Result<Option<&'r DictionaryDef>, gmeow_errors::Diag> {
    let schema = registry.schemas().get(&envelope.schema).ok_or_else(|| {
        super::unknown_schema(format!(
            "envelope for frame <{}> names schema <{}>, which is not a registered \
             gmeow:PayloadSchema",
            envelope.frame, envelope.schema
        ))
    })?;
    let medium = registry.media().get(&envelope.medium).ok_or_else(|| {
        super::invalid_declaration(format!(
            "envelope for frame <{}> names medium <{}>, which is not a declared gmeow:Medium",
            envelope.frame, envelope.medium
        ))
    })?;

    // The reader contract FIRST: a reader that cannot apply the medium must not
    // proceed to compare digests it has no way to act on. Raising the reader
    // contract is a declared property of the deliverable, never something a
    // consumer discovers mid-decode.
    let missing: Vec<&String> = medium
        .reader_capabilities
        .iter()
        .filter(|capability| !capabilities.contains(*capability))
        .collect();
    if !missing.is_empty() {
        return Err(opaque_frame(format!(
            "frame <{}> (rep {:?}) was written through medium <{}>, which requires reader \
             capabilit(y/ies) {missing:?} this reader does not hold — surface the region as a \
             gmeow:OpaqueFrame with gmeow:opacityUnknownCodec rather than decoding it",
            envelope.frame, schema.rep, medium.iri
        )));
    }

    for (label, digest) in [
        ("gmeow:strataDigest", &envelope.strata_digest),
        ("gmeow:contentDigest", &envelope.content_digest),
    ] {
        if !is_canonical_digest(digest) {
            return Err(digest_mismatch(format!(
                "envelope for frame <{}> carries {label} {digest:?}, which is not written \
                 'blake3:<64 lowercase hex>' — a free-form digest cannot be compared against the \
                 bytes it claims to commit to, so it is a mismatch by construction",
                envelope.frame
            )));
        }
    }

    for (label, expected, bytes) in [
        ("gmeow:strataDigest", &envelope.strata_digest, stratum_bytes),
        ("gmeow:contentDigest", &envelope.content_digest, payload),
    ] {
        let actual = blake3_digest(bytes);
        if actual != *expected {
            return Err(digest_mismatch(format!(
                "envelope for frame <{}> carries {label} {expected}, but the {} bytes at hand \
                 digest to {actual} — the reader's premises are wrong, so it refuses rather than \
                 searching for a decode that appears to work",
                envelope.frame,
                bytes.len()
            )));
        }
    }

    match &envelope.dictionary {
        None => {
            if !medium.dictionaries.is_empty() {
                return Err(undeclared_dictionary(format!(
                    "envelope for frame <{}> declares no gmeow:envelopeDictionary, but its medium \
                     <{}> declares {} — the payload's priming dictionary was never written down",
                    envelope.frame,
                    medium.iri,
                    medium.dictionaries.len()
                )));
            }
            Ok(None)
        }
        Some(iri) => {
            let def = registry.dictionaries().get(iri).ok_or_else(|| {
                unknown_dictionary(format!(
                    "envelope for frame <{}> declares dictionary <{iri}>, which resolves to no \
                     registered gmeow:CompressionDictionary",
                    envelope.frame
                ))
            })?;
            if !medium.dictionaries.contains(iri) {
                return Err(unknown_dictionary(format!(
                    "envelope for frame <{}> primes with <{iri}>, which its medium <{}> does not \
                     declare",
                    envelope.frame, medium.iri
                )));
            }
            Ok(Some(def))
        }
    }
}

#[path = "envelope.tests.rs"]
#[cfg(test)]
mod tests;
