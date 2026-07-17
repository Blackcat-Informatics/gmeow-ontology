// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared identifier helper for the developer-surface schema emitters.
//!
//! [`local_name`] is the deterministic IRI→local-name rule the LinkML/TypeScript/
//! GraphQL renderer ([`crate::stages::schemas`]) shares with the rest of the
//! pipeline. Keeping the ONE copy here means every surface localises an IRI by
//! the identical rule and can never drift.

/// The bare local name of an IRI: the substring after the last `#` or `/`.
pub(crate) fn local_name(iri: &str) -> &str {
    iri.rsplit(['#', '/']).next().unwrap_or(iri)
}
