// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The MEDIUM axis: the bundle's zstd dictionaries as a projection of its own
//! ontology.
//!
//! `slices/core/gts/module.ttl` models a medium as a lawful `(encode, decode)`
//! pair, splits the AUTHORED `gmeow:CompressionDictionary` from its GENERATED
//! `gmeow:CompressionDictionaryRealization`, declares a `gmeow:DictionaryCorpus`
//! SELECTOR per dictionary, and registers one `gmeow:PayloadSchema` per blob
//! representation the carrier can emit. This module is the executable twin of that
//! declaration, split so each file has exactly ONE reason to change:
//!
//! * [`audit`] — the DECLARED-MEDIA check: does an emitted artifact's wire agree
//!   with the `gmeow:Medium` its producer declared? The dictionary half of the
//!   codec gate, split from the universal
//!   [`gmeow_gts_profile::validate_mandated_frames`] rule so the latter stays
//!   applicable to every GMEOW-authored artifact, registry or no registry;
//! * [`registry`] — the carrier dataset → the typed registry (dictionaries,
//!   corpora, payload schemas, media, and the TOTAL rep→medium assignment), plus
//!   the [`purrdf::gts_compose::MediumPlan`] that assignment renders to;
//! * [`corpus`] — the selector VOCABULARY and its evaluation: which bundle-internal
//!   bytes a declared corpus resolves to;
//! * [`train`] — a thin, verbatim adapter over [`purrdf::gts::dict`]; a PURE
//!   `&[&[u8]] -> Vec<u8>` function and nothing else;
//! * [`envelope`] — sealing and opening a `gmeow:MediumEnvelope`, projected from
//!   the facts a frame already carries in band;
//! * [`rdf`] — the projection of realizations and envelopes into the build-time
//!   [`MEDIUM_REGISTRY_GRAPH`] named graph;
//! * [`measure`] — the two-part code: does each shipped dictionary PAY FOR ITSELF
//!   over the population it primes, measured on the mandated chain and projected
//!   into [`MEDIUM_MEASUREMENT_GRAPH`];
//! * [`sweep`] — the off-gate `(strategy × target length)` grid and the committed
//!   winner table the build consumes, so the shipped dictionaries are a measured
//!   choice rather than a per-build search.
//!
//! # Everything is read from the IN-MEMORY carrier
//!
//! The registry is read off the live [`purrdf::RdfDataset`] a stage was handed,
//! never re-parsed from a committed file, and every corpus sample comes from an
//! upstream product's in-memory lane — the same "stale-disk-fold" refusal
//! [`crate::docs_measure`] documents. The single exception is deliberate and
//! narrow: a `gmeow:corpusSelectsPathPrefix` naming an AUTHORED source tree
//! (`slices/…`) reads the repo, exactly as `stage-archive-blobs` does for the
//! authored trees it tars — a prefix under `generated/` NEVER does, because the
//! committed tree is not flushed until the post-run reconcile and tarring it would
//! train the dictionary on the PREVIOUS build's bytes.
//!
//! # Why the fixpoint exclusion exists
//!
//! A dictionary is trained from a corpus, the corpus is declared in the registry,
//! and the registry's own realizations are projected back into the carrier. If a
//! corpus could select that projection, dictionary → registry → corpus → dictionary
//! would close a cycle: the build would either oscillate or land on whichever
//! accidental fixpoint the machine happened to reach, and neither is reproducible.
//! [`corpus`] therefore REJECTS any selector that transitively covers
//! [`MEDIUM_REGISTRY_GRAPH`], [`MEDIUM_MEASUREMENT_GRAPH`], or
//! [`MEDIUM_GENERATED_PREFIX`] — statically where the selector says so, and by
//! content inspection where only the selected material can tell.

pub mod audit;
pub mod corpus;
pub mod envelope;
pub mod measure;
pub mod rdf;
pub mod registry;
pub mod sweep;
pub mod train;

use gmeow_errors::Diag;

/// The GMEOW ontology namespace every medium term lives under.
pub(crate) const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

/// The build-time named graph the generated
/// `gmeow:CompressionDictionaryRealization` / `gmeow:MediumEnvelope` records are
/// projected into. Never authored by hand — a hand-written realization would be a
/// second source of truth for bytes the build already owns.
pub const MEDIUM_REGISTRY_GRAPH: &str =
    "https://blackcatinformatics.ca/gmeow/graph/medium-registry";

/// The named graph the medium MEASUREMENT records (per-dictionary byte deltas)
/// are projected into. Excluded from every corpus for the same reason as
/// [`MEDIUM_REGISTRY_GRAPH`]: a measurement of a dictionary is downstream of that
/// dictionary, so training on it closes the loop.
pub const MEDIUM_MEASUREMENT_GRAPH: &str =
    "https://blackcatinformatics.ca/gmeow/graph/medium-measurement";

/// The repo-relative path family the materialized dictionary bytes are projected
/// onto. A corpus selector that covers it — in either direction — is a fixpoint.
///
/// A dictionary's CHANNEL is the segment header's in-band `"dct"` map, and
/// [`crate::stages::medium_dictionaries`] keeps the trained bytes on the internal
/// `pipeline/` lane, so the bundle carries them exactly once. What lands under this
/// prefix is a PROJECTION of that one copy: the superset gate's `header-dict`
/// fanout family reconstructs `generated/medium/<dict-id>.zdict` from the header
/// entry itself (never from the archive lane, which would carry the same
/// high-entropy bytes a second time — Constitution §18).
///
/// The exclusion is therefore live rather than defensive: the projected files ARE
/// on disk, so a corpus that selected this prefix would train a dictionary on the
/// previous build's dictionaries and close the cycle
/// dictionary → registry → corpus → dictionary. [`corpus`] refuses it statically.
pub const MEDIUM_GENERATED_PREFIX: &str = "generated/medium/";

/// The `gmeow:payloadSchemaId` of the snapshot wire schema — the rep whose
/// assignment governs [`purrdf::gts_compose::FrameSlot::Snapshot`].
pub const SNAPSHOT_WIRE_REP: &str = "gmeow:snapshot/wire";

/// A rep with no medium assignment, or a declared dictionary whose corpus resolves
/// to nothing. Both are the same defect: a declaration that names no bytes.
pub(crate) fn undeclared_dictionary(detail: impl Into<String>) -> Diag {
    Diag::of_kind(crate::error::MediumUndeclaredDictionary {
        detail: detail.into(),
    })
}

/// A dictionary that does not resolve to a registered `gmeow:CompressionDictionary`
/// — or one outside the declared `gmeow:mediumDictionary` bound of the medium the
/// frame was written through.
pub(crate) fn unknown_dictionary(detail: impl Into<String>) -> Diag {
    Diag::of_kind(crate::error::MediumUnknownDictionary {
        detail: detail.into(),
    })
}

/// A blob rep the `gmeow:PayloadSchema` registry does not know.
pub(crate) fn unknown_schema(detail: impl Into<String>) -> Diag {
    Diag::of_kind(crate::error::MediumUnknownSchema {
        detail: detail.into(),
    })
}

/// A carried digest that disagrees with the bytes it commits to.
pub(crate) fn digest_mismatch(detail: impl Into<String>) -> Diag {
    Diag::of_kind(crate::error::MediumDigestMismatch {
        detail: detail.into(),
    })
}

/// A frame whose medium demands a reader capability that is not held.
pub(crate) fn opaque_frame(detail: impl Into<String>) -> Diag {
    Diag::of_kind(crate::error::MediumOpaqueFrame {
        detail: detail.into(),
    })
}

/// An emitted dictionary version no authored definition still declares.
pub(crate) fn dictionary_regression(detail: impl Into<String>) -> Diag {
    Diag::of_kind(crate::error::MediumDictionaryRegression {
        detail: detail.into(),
    })
}

/// A malformed or unrecognized medium DECLARATION in the carrier: an unknown
/// strategy individual, an unknown selector predicate, a missing exactly-one field.
/// The RDF parsed cleanly — the declaration is what is wrong — so this is
/// [`crate::error::InvalidDeclaration`], the kind that already names exactly that.
pub(crate) fn invalid_declaration(message: impl Into<String>) -> Diag {
    Diag::of_kind(crate::error::InvalidDeclaration {
        message: message.into(),
    })
}

/// The canonical `blake3:<64 lowercase hex>` form every medium digest is written in
/// (`logic:MediumStrataDigestFormConstraint`). A free-form digest cannot be compared
/// against the bytes it claims to commit to, so the SHAPE is part of the claim.
#[must_use]
pub fn blake3_digest(bytes: &[u8]) -> String {
    format!("blake3:{}", blake3::hash(bytes).to_hex())
}

/// Whether `digest` is written in the canonical `blake3:<64 lowercase hex>` form.
#[must_use]
pub fn is_canonical_digest(digest: &str) -> bool {
    match digest.strip_prefix("blake3:") {
        Some(hex) => {
            hex.len() == 64
                && hex
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_render_and_validate_in_the_canonical_form() {
        let digest = blake3_digest(b"medium");
        assert!(is_canonical_digest(&digest), "{digest}");
        assert!(!is_canonical_digest("blake3:CAFE"));
        assert!(!is_canonical_digest(&digest.replace("blake3:", "sha256:")));
        // Upper-case hex is NOT canonical: two spellings of one digest would
        // compare unequal while naming the same bytes.
        assert!(!is_canonical_digest(&digest.to_uppercase()));
    }
}
