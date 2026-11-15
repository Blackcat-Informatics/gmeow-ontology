// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The canonical profile GMEOW's persisted values assume.
//!
//! GMEOW persists and ships values derived from PurRDF's canonical blank-node
//! labels and canonical documents: the coherence certificate's bundle hash, the
//! snapshot stratum digest, every term definition digest, skolem and genid IRIs,
//! and the pinned conformance goldens. Each of them is a function of the
//! canonicalization profile PurRDF implements, so a substrate release that changes
//! the profile changes those values without changing a line of this repository.
//!
//! The constants below state the coordinates those values were produced under, and
//! the assertions compare each one with the coordinate the linked PurRDF declares.
//! This module is their single owner: every crate that canonicalizes for a
//! persisted value links this leaf, so a profile change stops the build here.
//!
//! # When an assertion in this module fails
//!
//! The canonical labelling or digest profile changed upstream. Changing a constant
//! here is only the last step: every persisted value derived from the profile must
//! be regenerated in the same change. The classes of such values, and how each is
//! regenerated, are listed in `docs/GATE-AND-PIPELINE.md` under "Values derived
//! from the canonical profile".

/// The canonicalization profile every persisted GMEOW identity is produced under.
pub const GMEOW_CANON_PROFILE_ID: &str = "purrdf-rdfc12";

/// The version of [`GMEOW_CANON_PROFILE_ID`] every persisted GMEOW identity is
/// produced under. PurRDF increments its side for any change to canonical bytes,
/// to the reserved vocabulary, to the refusal rule or to the search bounds.
pub const GMEOW_CANON_PROFILE_VERSION: u32 = 3;

/// The identity of the normative vector corpus that demonstrates
/// [`GMEOW_CANON_PROFILE_ID`] at [`GMEOW_CANON_PROFILE_VERSION`]: the SHA-256 of
/// PurRDF's corpus freeze manifest.
pub const GMEOW_CANON_CORPUS_DIGEST: &str =
    "b9f367a47ebbf389f76efddb083040dae77b1987f96709bebb8af9fdb5818d3d";

/// The overlay presentation, which renders the RDF 1.2 statement layer through
/// the profile's reserved sentinel IRIs. Graph, bundle and pack digests and the
/// GMN content digest are taken over it.
pub const GMEOW_CANON_OVERLAY_PRESENTATION_ID: &str = "overlay";

/// The version of [`GMEOW_CANON_OVERLAY_PRESENTATION_ID`] those values assume.
pub const GMEOW_CANON_OVERLAY_PRESENTATION_VERSION: u32 = 1;

/// The flat-assertion presentation, which lowers the statement layer to ordinary
/// quads. The bundle hash, the snapshot stratum digest and the term definition
/// digests are taken over it.
pub const GMEOW_CANON_FLAT_PRESENTATION_ID: &str = "flat-assertion";

/// The version of [`GMEOW_CANON_FLAT_PRESENTATION_ID`] those values assume.
pub const GMEOW_CANON_FLAT_PRESENTATION_VERSION: u32 = 1;

/// Byte equality of two strings in const context: the compile-time proof that two
/// constants which must name the same value have not drifted apart.
#[must_use]
pub const fn const_str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

// A failure of any assertion below means the canonical labelling or digest
// profile of the linked PurRDF differs from the one GMEOW's persisted values were
// produced under. Regenerate every value listed in `docs/GATE-AND-PIPELINE.md`
// under "Values derived from the canonical profile" in the same change that
// updates the constant; never update the constant alone.
const _: () = assert!(
    const_str_eq(purrdf::CANON_PROFILE_ID, GMEOW_CANON_PROFILE_ID),
    "PurRDF canonical profile identifier changed: regenerate every persisted value derived from it"
);
const _: () = assert!(
    purrdf::CANON_PROFILE_VERSION == GMEOW_CANON_PROFILE_VERSION,
    "PurRDF canonical profile version changed: regenerate every persisted value derived from it"
);
const _: () = assert!(
    const_str_eq(purrdf::CANON_CORPUS_DIGEST, GMEOW_CANON_CORPUS_DIGEST),
    "PurRDF canonical vector corpus changed: regenerate every persisted value derived from it"
);
const _: () = assert!(
    const_str_eq(
        purrdf::CANON_PRESENTATION_OVERLAY_ID,
        GMEOW_CANON_OVERLAY_PRESENTATION_ID
    ),
    "PurRDF overlay presentation identifier changed: regenerate every persisted value derived from it"
);
const _: () = assert!(
    purrdf::CANON_PRESENTATION_OVERLAY_VERSION == GMEOW_CANON_OVERLAY_PRESENTATION_VERSION,
    "PurRDF overlay presentation version changed: regenerate every persisted value derived from it"
);
const _: () = assert!(
    const_str_eq(
        purrdf::CANON_PRESENTATION_FLAT_ASSERTION_ID,
        GMEOW_CANON_FLAT_PRESENTATION_ID
    ),
    "PurRDF flat-assertion presentation identifier changed: regenerate every persisted value derived from it"
);
const _: () = assert!(
    purrdf::CANON_PRESENTATION_FLAT_ASSERTION_VERSION == GMEOW_CANON_FLAT_PRESENTATION_VERSION,
    "PurRDF flat-assertion presentation version changed: regenerate every persisted value derived from it"
);
