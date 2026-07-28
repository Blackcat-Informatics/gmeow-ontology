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
//! One helper deliberately did NOT move: `site_sub_asset_iri`. It is defined over
//! `gmeow_docs::formats::DocFormat::Site`, so hoisting it here would pull `gmeow-docs`
//! (and its ~13.6 MB of embedded vendored wasm) into a wasm-clean leaf. It stays with the
//! emitter, defined in terms of [`dist_iri`] — still one definition site, on the side of
//! the seam that already owns the `DocFormat` vocabulary.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dist_iri_hangs_off_the_declared_base() {
        assert_eq!(
            dist_iri("site"),
            "https://blackcatinformatics.ca/gmeow/distribution/dist/site"
        );
    }

    #[test]
    fn local_name_recovers_every_catalog_subject_tail() {
        assert_eq!(
            local_name("https://blackcatinformatics.ca/gmeow/distribution/family/doc-render"),
            "doc-render"
        );
        assert_eq!(
            local_name("https://blackcatinformatics.ca/gmeow/consumerPublicSite"),
            "consumerPublicSite"
        );
        assert_eq!(local_name("bare"), "bare");
    }

    #[test]
    fn nt_literal_escapes_the_five_reserved_forms() {
        assert_eq!(
            triple_lit("s", "p", "a\"b\\c\nd\re\tf"),
            "<s> <p> \"a\\\"b\\\\c\\nd\\re\\tf\" ."
        );
    }

    #[test]
    fn triple_is_the_plain_iri_form() {
        assert_eq!(triple("s", "p", "o"), "<s> <p> <o> .");
    }
}
