// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Canonicalization coordinates for persisted GMEOW values.
//!
//! GMEOW persists and ships values derived from PurRDF's canonical blank-node
//! labels and canonical documents: the coherence certificate's bundle hash, the
//! snapshot stratum digest, every term definition digest, reasoner skolem IRIs,
//! canonical labels in the compiled projections, and the pinned conformance
//! goldens. Each of them is a function of the canonicalization profile PurRDF
//! implements, so a substrate release that changes the profile changes those
//! values without changing a line of this repository.
//!
//! New values from the PurRDF default use the line-sequence presentation.
//! The medium stratum, coherence certificate, and term release authority use
//! the explicit flat RDFC profile. The GMN content digest uses the RDFC overlay.
//! The assertions pin both families.
//!
//! # When an assertion in this module fails
//!
//! The canonical labelling or digest profile changed upstream. Changing a constant
//! here is only the last step: the persisted values derived from the profile are
//! regenerated or re-blessed in the same change, except those the document says
//! must not be refreshed (the term release authority file is one). The classes of
//! such values, how each is regenerated, and the exceptions are listed in
//! `docs/GATE-AND-PIPELINE.md` under "Values derived from the canonical profile".

use crate::const_str::const_str_eq;

const _: () = assert!(
    const_str_eq(purrdf::CANON_LABELLING_NAME, "nested-refinement-labelling")
        && purrdf::CANON_LABELLING_VERSION == 1
        && const_str_eq(purrdf::CANON_PRESENTATION_LINES_ID, "line-sequence")
        && purrdf::CANON_PRESENTATION_LINES_VERSION == 1,
    "PurRDF default canonicalization changed: check the persisted values in docs/GATE-AND-PIPELINE.md"
);

/// The explicit RDFC profile recorded by medium envelopes and coherence certificates.
pub const GMEOW_CANON_PROFILE_ID: &str = "purrdf-rdfc12";

/// The version of [`GMEOW_CANON_PROFILE_ID`] used by the flat RDFC contracts.
pub const GMEOW_CANON_PROFILE_VERSION: u32 = 3;

/// The identity of the normative vector corpus that demonstrates
/// [`GMEOW_CANON_PROFILE_ID`] at [`GMEOW_CANON_PROFILE_VERSION`]: the SHA-256 of
/// PurRDF's corpus freeze manifest.
pub const GMEOW_CANON_CORPUS_DIGEST: &str =
    "b9f367a47ebbf389f76efddb083040dae77b1987f96709bebb8af9fdb5818d3d";

/// The overlay presentation, which renders the RDF 1.2 statement layer through
/// the profile's reserved sentinel IRIs. The GMN content digest uses this
/// presentation.
pub const GMEOW_CANON_OVERLAY_PRESENTATION_ID: &str = "overlay";

/// The version of [`GMEOW_CANON_OVERLAY_PRESENTATION_ID`] those values assume.
pub const GMEOW_CANON_OVERLAY_PRESENTATION_VERSION: u32 = 1;

/// The flat-assertion presentation, which lowers the statement layer to ordinary
/// quads. The bundle hash, the snapshot stratum digest and the term definition
/// digests are taken over it.
pub const GMEOW_CANON_FLAT_PRESENTATION_ID: &str = "flat-assertion";

/// The version of [`GMEOW_CANON_FLAT_PRESENTATION_ID`] those values assume.
pub const GMEOW_CANON_FLAT_PRESENTATION_VERSION: u32 = 1;

// A failure of any assertion below means the canonical labelling or digest
// profile of the linked PurRDF differs from the one GMEOW's persisted values were
// produced under. Follow `docs/GATE-AND-PIPELINE.md`, section "Values derived from
// the canonical profile", in the same change that updates the constant: it lists
// what to regenerate and what must not be refreshed. Never update the constant
// alone. The corpus digest is the
// exception: no persisted value is derived from the vector corpus, so its message
// asks for confirmation rather than regeneration.
const _: () = assert!(
    const_str_eq(purrdf::rdfc12::CANON_PROFILE_ID, GMEOW_CANON_PROFILE_ID),
    "PurRDF canonical profile identifier changed: follow docs/GATE-AND-PIPELINE.md, section \"Values derived from the canonical profile\", which lists what to regenerate and what not to refresh"
);
const _: () = assert!(
    purrdf::rdfc12::CANON_PROFILE_VERSION == GMEOW_CANON_PROFILE_VERSION,
    "PurRDF canonical profile version changed: follow docs/GATE-AND-PIPELINE.md, section \"Values derived from the canonical profile\", which lists what to regenerate and what not to refresh"
);
const _: () = assert!(
    const_str_eq(
        purrdf::rdfc12::CANON_CORPUS_DIGEST,
        GMEOW_CANON_CORPUS_DIGEST
    ),
    "PurRDF canonical vector corpus changed: confirm the profile and presentation versions are unchanged and that persisted values still reproduce before updating this constant; see docs/GATE-AND-PIPELINE.md, Values derived from the canonical profile"
);
const _: () = assert!(
    const_str_eq(
        purrdf::rdfc12::CANON_PRESENTATION_OVERLAY_ID,
        GMEOW_CANON_OVERLAY_PRESENTATION_ID
    ),
    "PurRDF overlay presentation identifier changed: follow docs/GATE-AND-PIPELINE.md, section \"Values derived from the canonical profile\", which lists what to regenerate and what not to refresh"
);
const _: () = assert!(
    purrdf::rdfc12::CANON_PRESENTATION_OVERLAY_VERSION == GMEOW_CANON_OVERLAY_PRESENTATION_VERSION,
    "PurRDF overlay presentation version changed: follow docs/GATE-AND-PIPELINE.md, section \"Values derived from the canonical profile\", which lists what to regenerate and what not to refresh"
);
const _: () = assert!(
    const_str_eq(
        purrdf::rdfc12::CANON_PRESENTATION_FLAT_ASSERTION_ID,
        GMEOW_CANON_FLAT_PRESENTATION_ID
    ),
    "PurRDF flat-assertion presentation identifier changed: follow docs/GATE-AND-PIPELINE.md, section \"Values derived from the canonical profile\", which lists what to regenerate and what not to refresh"
);
const _: () = assert!(
    purrdf::rdfc12::CANON_PRESENTATION_FLAT_ASSERTION_VERSION
        == GMEOW_CANON_FLAT_PRESENTATION_VERSION,
    "PurRDF flat-assertion presentation version changed: follow docs/GATE-AND-PIPELINE.md, section \"Values derived from the canonical profile\", which lists what to regenerate and what not to refresh"
);
