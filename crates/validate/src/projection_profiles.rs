// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The canonical set of projection-profile names subject to leak conformance.
//!
//! This is the single source of truth for *which* projection profiles the
//! suppression leak sweep (CONSTITUTION P10) covers. Two on-gate assertions pin
//! it in both directions so the coverage cannot silently drift:
//!
//!   - `gmeow_pipeline::projections::profiles()` MUST register exactly these
//!     names — an on-gate pipeline test asserts full set-equality, so a new
//!     projection profile cannot be added to the registry without also entering
//!     the sweep.
//!   - the suppression conformance sweep iterates exactly these names and reads
//!     each `generated/queries/{name}.rq`, so a name here without a real
//!     projection CONSTRUCT hard-fails on the missing query file.
//!
//! Ordered as in the projection registry (phase order); the pins compare as sets.

/// The projection profiles that must pass suppression leak conformance.
pub const PROJECTION_PROFILES: &[&str] = &[
    "odrl",
    "cc",
    "dcterms",
    "oai_dc",
    "spdx",
    "schema-org",
    "foaf",
    "vcard",
    "ical",
    "owl-time",
    "ontolex",
    "web-annotation",
    "skos",
    "bot",
    "mailmap",
    "exif",
    "iiif",
    "dcat",
    "org",
    "bibo",
    "bibframe",
    "gedcom",
    "sioc",
    "doap",
    "codemeta",
    "prov",
    "geosparql",
];
