// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The distribution catalog's identity + N-Triples formatting helpers.
//!
//! These are the SINGLE definition site for the strings both ends of the catalog agree
//! on: the subject IRIs the carrier-time emitter mints and the release-time instance
//! producer references, the `gmeow:` predicate/class IRI concatenation the readers select
//! on, and the N-Triples escaping convention the emitter serializes with.
//!
//! They live in this leaf rather than with the emitter because the READ side needs
//! [`iri`] to spell the predicates it filters on, and a reader may not depend on the build
//! executor. `gmeow_pipeline::stages::distribution_catalog` re-exports every one of them
//! at its original `pub(crate)` visibility, so the writer is unchanged and the two sides
//! can never fork an escaping rule or a subject namespace.
//!
//! One helper deliberately did NOT move: `sub_asset_iri`. It is defined over the
//! emitter's own sub-asset vocabulary (which slug ships at which owner-tree prefix), and
//! that vocabulary is authored beside the distribution table in `gmeow-pipeline`. It stays
//! with the emitter, defined in terms of [`DISTRIBUTION_BASE`] — still one definition
//! site, on the side of the seam that owns the thing being named.

/// The instance subject base every distribution / family / loss / capability IRI the
/// catalog mints lives under.
pub const DISTRIBUTION_BASE: &str = "https://blackcatinformatics.ca/gmeow/distribution/";

/// The canonical distribution-catalog subject IRI for a distribution slug
/// (`https://blackcatinformatics.ca/gmeow/distribution/dist/<slug>`).
///
/// The carrier-time emitter mints these; the release-time instance producer hangs its
/// `gmeow:corpusMember` rows off the SAME subject rather than a re-derived literal.
#[must_use]
pub fn dist_iri(slug: &str) -> String {
    format!("{DISTRIBUTION_BASE}dist/{slug}")
}

/// Concatenate a namespace and a local name into a full IRI.
///
/// The readers and the emitter address the SAME `gmeow:` predicate/class IRIs through
/// this one helper rather than each re-deriving the concatenation.
#[must_use]
pub fn iri(ns: &str, local: &str) -> String {
    format!("{ns}{local}")
}

/// The IRI-namespace-local-name tail of `iri` (the segment after its final `/`).
///
/// Uniformly recovers a slug from every kind of subject the catalog mints: a
/// `…/family/<slug>` family IRI, a `…/capability/<slug>` capability IRI, and a
/// `{GMEOW_NS}<name>` consumer IRI (`GMEOW_NS` itself ends in `/`, so the tail IS the bare
/// local name in that case too).
#[must_use]
pub fn local_name(iri: &str) -> String {
    iri.rsplit('/').next().unwrap_or(iri).to_string()
}

/// The single N-Triples subject/predicate/object-IRI triple formatter.
#[must_use]
pub fn triple(subject: &str, predicate: &str, object: &str) -> String {
    format!("<{subject}> <{predicate}> <{object}> .")
}

/// The single N-Triples subject/predicate/literal-object formatter — see [`triple`].
#[must_use]
pub fn triple_lit(subject: &str, predicate: &str, literal: &str) -> String {
    format!("<{subject}> <{predicate}> {} .", nt_literal(literal))
}

/// The single N-Triples subject/predicate/TYPED-literal formatter — see [`triple`]. Shares
/// [`nt_literal`]'s escaping so a datatyped value can never fork the quoting convention.
#[must_use]
pub fn triple_typed(subject: &str, predicate: &str, literal: &str, datatype: &str) -> String {
    format!(
        "<{subject}> <{predicate}> {}^^<{datatype}> .",
        nt_literal(literal)
    )
}

/// `xsd:integer` — the datatype the authored `logic:` formula ASTs give a `logic:termIndex`
/// (Turtle's bare integer literal), so an emitted AST is byte-comparable with a hand-written
/// one.
pub const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

/// Escape a string as an N-Triples quoted literal (UTF-8 passes through verbatim).
fn nt_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

#[path = "identity.tests.rs"]
#[cfg(test)]
mod tests;
