// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared N-Triples term codecs used by every text dialect (`xcl`, `clif`, `cgif`)
//! that embeds a canonical N-Triples RDF channel. One escaper, three consumers —
//! see `crate::xcl::writer`, `crate::clif::reader`, and `crate::cgif::reader`.

/// Escape an IRI's inner content for an N-Triples `<…>` reference (does NOT add the
/// angle brackets). The `IRIREF` grammar forbids `< > " { } | ^ ` \` and every
/// control character or space appearing raw; each rides as a `\uXXXX` UCHAR, or the
/// re-parse hard-fails.
pub(crate) fn nt_escape_iri(iri: &str) -> String {
    let mut out = String::with_capacity(iri.len());
    for c in iri.chars() {
        match c {
            '\\' => out.push_str("\\u005C"),
            '"' => out.push_str("\\u0022"),
            '<' => out.push_str("\\u003C"),
            '>' => out.push_str("\\u003E"),
            '{' => out.push_str("\\u007B"),
            '}' => out.push_str("\\u007D"),
            '|' => out.push_str("\\u007C"),
            '^' => out.push_str("\\u005E"),
            '`' => out.push_str("\\u0060"),
            // `is_control()` covers C0 (0x00–0x1F), DEL (0x7F), and C1 (0x80–0x9F);
            // all are IRIREF-illegal raw. Space is legal-looking but also forbidden raw.
            c if c.is_control() || c == ' ' => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Escape a literal lexical form for an N-Triples `"…"` string (does NOT add the quotes).
pub(crate) fn nt_escape_literal(lex: &str) -> String {
    let mut out = String::with_capacity(lex.len());
    for c in lex.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Any OTHER control char (C0 minus \t\n\r, DEL, C1) must not reach the XML
            // text node raw — XML 1.0 forbids most of them entirely — so ride as UCHAR.
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
