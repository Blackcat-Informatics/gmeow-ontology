// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The XCL (eXtended Common Logic Markup Language, XML) dialect.
//!
//! XCL is the ISO/IEC 24707 XML serialization for full first-order Common Logic. This module
//! is a **bidirectional, `PreservationKind::Exact`** dialect, a sibling of [`crate::clif`] and
//! [`crate::cgif`]: [`project_xcl`] lowers a
//! [`LogicProgram`](crate::ir::LogicProgram) to XCL XML and
//! [`parse_xcl_str`] lifts it back, and the two are inverses on the
//! canonical IR (the production round-trip test pins this).
//!
//! ## The two-channel split that makes Exact genuine
//!
//! Identical in shape to CLIF's and CGIF's — the IR is round-tripped through two disjoint
//! channels, but XML's single-root, entity-escaped structure changes *how* the channels are
//! delimited (see below):
//!
//! 1. **Idiomatic sentence channel** — `program.rules` + `program.formulas`. These become
//!    readable XCL2 elements (`<forall>` / `<exists>` quantifiers, `<and>` / `<or>` / `<not>` /
//!    `<implies>` / `<iff>` connectives, `<atom>` predications with `<name>` / `<var>` /
//!    `<literal>` terms, `<rule>` for Horn rules). This channel is **WRITE-ONLY / validated-only
//!    on read**: the canonical IR carries an `obj_is_literal` bit and minted reifier-node
//!    identities that idiomatic sentence syntax cannot express, so reconstructing the byte-exact
//!    IR from the sentences alone would be lossy.
//! 2. **RDF / predication channel** — everything else (axioms + scope, contracts, path shapes,
//!    correspondences, transaction programs). These are already flat RDF; the writer serializes
//!    them through the lossless canonical-RDF-1.2 projection and emits them as canonical
//!    N-Triples inside a single `<gmeow-rdf-meta>` element, and the reader reads that element's
//!    text back through the canonical RDF frontend. The bidirectional faithfulness of that leg
//!    is therefore exactly the canonical-RDF-1.2 target's (already `Exact`).
//!
//! ## Why the meta carrier is an element (not a comment) holding N-Triples
//!
//! CLIF and CGIF append their predication channel after a *comment* sentinel in a flat text
//! stream. XML forbids both moves: a document has exactly one root element, and an XML comment
//! cannot contain the substring `--` (which real IRIs do). So the carrier is a dedicated
//! `<gmeow-rdf-meta>` child element — a generic XCL/XML consumer ignores it, exactly as a
//! generic CGIF consumer ignores the comment-delimited block. Its payload is **canonical
//! N-Triples** rather than a per-dialect re-encoding of each triple: N-Triples already escapes
//! every IRI/literal character losslessly (the proven kernel codec), and the surrounding XML
//! text node adds one further total escape layer (`&`, `<`, `>`). Two composable, standard
//! escaping layers keep the Exact claim honest across arbitrary IRIs without a bespoke
//! character model. Reading uses a real XML parser (`roxmltree`), never a hand-rolled scanner.

pub mod reader;
pub mod writer;

pub use reader::parse_xcl_str;
pub use writer::project_xcl;

#[cfg(test)]
mod tests;

// --------------------------------------------------------------------------- //
// Document element names
// --------------------------------------------------------------------------- //

/// The document root element. Wraps the idiomatic sentence channel and the meta carrier so the
/// whole projection is a single well-formed XML document.
pub(crate) const ROOT_ELEMENT: &str = "gmeow-xcl";

/// The dedicated element carrying the RDF/predication meta channel as canonical N-Triples text.
/// The reader routes this element's text through the canonical RDF frontend; a generic XCL
/// consumer ignores it. Always emitted (possibly empty) so the reader can distinguish a
/// legitimately empty program from a corrupted document that lost its carrier.
pub(crate) const RDF_META_ELEMENT: &str = "gmeow-rdf-meta";

// --------------------------------------------------------------------------- //
// XML escaping (writer side; the reader decodes via roxmltree)
// --------------------------------------------------------------------------- //

/// Whether `c` is an XML 1.0 `Char` production code point — i.e. representable at all in a
/// well-formed XML document (raw or via a numeric character reference). XML 1.0 excludes the C0
/// control range except tab/LF/CR (`0x00-0x08`, `0x0B-0x0C`, `0x0E-0x1F`) outright; there is no
/// legal encoding for those code points anywhere in an XML document. DEL (`0x7F`) and the C1
/// block (`0x80-0x9F`) ARE legal `Char` code points (merely discouraged), so they ride through a
/// numeric character reference losslessly.
fn is_xml_char(c: char) -> bool {
    // Tab/LF/CR are the only sub-0x20 code points XML 1.0 admits; every code point at 0x20 or
    // above — including DEL (0x7F) and the C1 block (0x80-0x9F) — is a legal `Char`. (Not
    // `!c.is_control()`, which would wrongly exclude DEL/C1 and force them to U+FFFD.)
    matches!(c, '\t' | '\n' | '\r') || (c as u32) >= 0x20
}

/// Escape a string for an XML **text node**: `&`, `<`, `>`, plus every control character. This
/// channel is the human-readable, WRITE-ONLY/validated-only idiomatic sentence view (see the
/// module doc) — the lossless round-trip authority is the separate `<gmeow-rdf-meta>` N-Triples
/// carrier, whose payload is already `\uXXXX`-escaped at the source (never carries a raw control
/// character), so this function only needs to keep the *document* well-formed, not perform a
/// reversible escape. `\t`/`\n`/`\r` pass through raw; DEL/C1 controls are legal XML `Char` code
/// points and ride as a numeric character reference (`&#xHH;`); the handful of C0 controls XML
/// forbids outright (even referenced) are replaced with U+FFFD REPLACEMENT CHARACTER, since no
/// well-formed XML encoding of them exists.
pub(crate) fn xml_escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if c.is_control() => {
                if is_xml_char(c) {
                    out.push_str(&format!("&#x{:X};", c as u32));
                } else {
                    out.push('\u{FFFD}');
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// Escape a string for a double-quoted XML **attribute value**: the text set plus `"`, plus
/// every control character (see [`xml_escape_text`] — same rationale, same view-only channel).
pub(crate) fn xml_escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if c.is_control() => {
                if is_xml_char(c) {
                    out.push_str(&format!("&#x{:X};", c as u32));
                } else {
                    out.push('\u{FFFD}');
                }
            }
            c => out.push(c),
        }
    }
    out
}
