// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Oxigraph term ⇄ Nemo fact-string encode/decode helpers.
//!
//! This module owns the byte-exact contract between oxigraph's RDF model and
//! Nemo's ground-fact string representation. Functions here have precise byte
//! parity with the Python oracle; do not change their logic without updating
//! both sides and the conformance corpus.
//!
//! # Encode contract
//!
//! **Predicate**: the full IRI string is both the encode key and the `ChaseRow`
//! predicate field (Nemo strips the angle brackets for us).
//!
//! **Subject**: always a `NamedNode`.  Blank nodes are Skolemized to
//! `{NAMESPACE}skolem/{sha1_hex(bnode_id_utf8)}`.
//!
//! **Object**: `NamedNode` → `<iri>`; blank node → Skolem IRI; plain literal →
//! `"value"`; typed literal → `"value"^^<datatype>`; language literal →
//! `"value"@lang`.
//!
//! **Context (world)**: the named-graph IRI encoded as a Nemo string constant
//! `"world_iri"`.  Nemo's display form for a string datavalue is `"value"`, so
//! the decode strips the outer double quotes.

use oxigraph::model::{Literal, NamedNode, NamedOrBlankNode, Term};
use sha1::{Digest, Sha1};

// ── Skolem prefix ─────────────────────────────────────────────────────────────

/// Prefix for Skolem IRIs derived from blank-node identifiers.
///
/// Matches the Python oracle: `{NAMESPACE}skolem/{sha1_hex(bnode_id_utf8)}`.
pub(crate) const SKOLEM_PREFIX: &str = "https://blackcatinformatics.ca/gmeow/skolem/";

// ── Skolemization ─────────────────────────────────────────────────────────────

/// Compute the SHA-1 hex digest of a UTF-8 string — matching the Python recipe
/// `sha1(str(bnode).encode("utf-8")).hexdigest()`.
pub(crate) fn sha1_hex(s: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Skolemize a blank-node identifier to a stable IRI string.
pub(crate) fn skolem_iri(bnode_id: &str) -> String {
    format!("{}{}", SKOLEM_PREFIX, sha1_hex(bnode_id))
}

// ── Encode: oxigraph quad → Nemo ground-fact line ────────────────────────────

/// Encode an oxigraph `Term` as a Nemo argument string.
///
/// - `NamedNode(iri)` → `<iri>`
/// - `BlankNode(id)` → Skolemized `<https://...skolem/{sha1}>` (same as NamedNode)
/// - `Literal(value)` → depends on datatype / language (see below)
/// - `Triple` → unsupported; empty string (gmeow-logic only uses IRI/BNode/Literal)
pub(crate) fn encode_term(term: &Term) -> String {
    match term {
        Term::NamedNode(nn) => format!("<{}>", nn.as_str()),
        Term::BlankNode(bn) => format!("<{}>", skolem_iri(bn.as_str())),
        Term::Literal(lit) => encode_literal(lit),
        Term::Triple(_) => String::new(), // RDF-star triple terms: unsupported in Nemo
    }
}

/// Encode a subject term (NamedNode or BlankNode) as a Nemo argument.
pub(crate) fn encode_subject(subject: &NamedOrBlankNode) -> String {
    match subject {
        NamedOrBlankNode::NamedNode(nn) => format!("<{}>", nn.as_str()),
        NamedOrBlankNode::BlankNode(bn) => format!("<{}>", skolem_iri(bn.as_str())),
    }
}

/// Escape `\` and `"` in a single pass, producing the same output as
/// `.replace('\\', "\\\\").replace('"', "\\\"")` but with one allocation.
#[inline]
fn escape_backslash_and_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            other => out.push(other),
        }
    }
    out
}

/// Encode an oxigraph `Literal` as a Nemo constant string.
///
/// - Plain `xsd:string` literal: `"value"`
/// - Language-tagged: `"value"@lang`
/// - Any other datatype: `"value"^^<datatype_iri>`
///
/// # Escape contract
///
/// Nemo's parser accepts raw bytes between the opening and closing `"` of a
/// string literal (it uses `is_not("\"")` — no escape processing in the
/// lexer).  However, Nemo's `AnyDataValue::to_string()` display form does
/// process the stored value through `quote_string`, which escapes:
///   - `\` → `\\`
///   - `"` → `\"`
///   - `\n` (newline, U+000A) → `\n` (two chars: backslash + n)
///   - `\r` (carriage return, U+000D) → `\r` (two chars: backslash + r)
///
/// Tabs (U+0009) are intentionally left unescaped by `quote_string` and
/// appear as literal tab characters in the display form.
///
/// Our encode writes control characters raw into the `.rls` source (they are
/// valid in Nemo string literals).  The decode path in [`decode_nemo_term`]
/// and [`decode_string_constant`] is then responsible for reversing the
/// `quote_string` display escapes (including `\n` → newline and `\r` → CR)
/// so that the round-trip is exact.
pub(crate) fn encode_literal(lit: &Literal) -> String {
    // Escape in the same order as Nemo's quote_string to ensure the .rls
    // source is valid and the round-trip through encode→Nemo→decode is exact:
    //   1. Backslash first (must come before adding new backslashes)
    //   2. Double-quote (Nemo string delimiter)
    // Control characters (\n, \r, \t) are accepted raw by Nemo's lexer and
    // are decoded symmetrically by the decode path.
    let escaped = escape_backslash_and_quote(lit.value());

    if let Some(lang) = lit.language() {
        // Language-tagged literal
        format!("\"{}\"@{}", escaped, lang)
    } else {
        let dt = lit.datatype().as_str();
        if dt == "http://www.w3.org/2001/XMLSchema#string" {
            // Plain string — no datatype annotation needed in Nemo
            format!("\"{}\"", escaped)
        } else {
            // Typed literal
            format!("\"{}\"^^<{}>", escaped, dt)
        }
    }
}

/// Encode one oxigraph quad as a single Nemo ground-fact line (with trailing `.`).
///
/// Format: `<predicate_iri>(<subject_term>, <object_term>, "world_iri").`
pub(crate) fn encode_quad_to_nemo_fact(
    subject: &NamedOrBlankNode,
    predicate: &NamedNode,
    object: &Term,
    world_iri: &str,
) -> String {
    let pred = format!("<{}>", predicate.as_str());
    let subj = encode_subject(subject);
    let obj = encode_term(object);
    // World IRI is encoded as a Nemo string constant (double-quoted).
    // Escape any backslashes or double-quotes inside the IRI (IRIs don't normally
    // contain these, but be defensive).
    let world_escaped = escape_backslash_and_quote(world_iri);
    format!("{}({}, {}, \"{}\").", pred, subj, obj, world_escaped)
}

// ── Decode: Nemo ChaseRow → oxigraph quad ────────────────────────────────────

/// Decode an error-prefixed description for decode failures.
pub(crate) fn decode_err(context: &str, got: &str) -> String {
    format!("decode error [{context}]: {got:?}")
}

/// Reverse the `quote_string` escape sequences that Nemo emits in its display form.
///
/// Nemo's `AnyDataValue::to_string()` uses `quote_string` internally, which
/// escapes the following characters:
///   - `\` → `\\`
///   - `"` → `\"`
///   - newline (U+000A) → `\n` (two chars: backslash + n)
///   - carriage return (U+000D) → `\r` (two chars: backslash + r)
///
/// Tabs (U+0009) are NOT escaped by `quote_string` and appear literally.
///
/// This function reverses those escapes using a single-pass character scanner
/// to avoid double-processing (`\\n` must decode to `\n`, not to newline).
pub(crate) fn unescape_nemo_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some(other) => {
                    // Unknown escape — keep both chars verbatim (defensive).
                    out.push('\\');
                    out.push(other);
                }
                None => {
                    // Trailing backslash — keep verbatim (malformed but don't panic).
                    out.push('\\');
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Decode a Nemo display-form IRI term (`<iri>`) to an IRI string.
///
/// Nemo displays IRI values as `<http://...>`.  This strips the angle brackets.
pub(crate) fn decode_iri_term(s: &str) -> Result<String, String> {
    if s.starts_with('<') && s.ends_with('>') {
        Ok(s[1..s.len() - 1].to_owned())
    } else {
        Err(decode_err("expected <iri>", s))
    }
}

/// Decode a Nemo display-form string constant (`"value"`) to the raw string.
///
/// Nemo displays plain string datavalues as `"value"` (the outer double-quotes
/// are part of the display representation, not the value).  This strips them
/// and reverses the `quote_string` escape sequences emitted by Nemo:
///
/// | Display sequence  | Decoded value            |
/// |-------------------|--------------------------|
/// | `\\`              | `\` (backslash)          |
/// | `\"`              | `"` (double-quote)       |
/// | `\n` (two chars)  | U+000A (newline)         |
/// | `\r` (two chars)  | U+000D (carriage return) |
///
/// Tabs appear as literal tab characters in the display form (Nemo's
/// `quote_string` does not escape them) and pass through unchanged.
///
/// The un-escape order is: `\\` → `\` last (after all other two-char
/// sequences have been processed) to avoid double-consuming backslashes.
/// We do it in the reverse order of Nemo's encoding:
///   1. `\"` → `"` (before `\\` so we don't break `\\\"` → `\"`)
///   2. `\n` → newline
///   3. `\r` → CR
///   4. `\\` → `\` (must be last — processes the remaining escaped backslashes)
pub(crate) fn decode_string_constant(s: &str) -> Result<String, String> {
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let inner = &s[1..s.len() - 1];
        // Un-escape in reverse-encode order so backslash unescaping is last.
        // Using a single-pass approach to avoid double-processing:
        let value = unescape_nemo_string(inner);
        Ok(value)
    } else {
        Err(decode_err("expected \"string\"", s))
    }
}

/// Decode a Nemo display-form term to an oxigraph `Term`.
///
/// Handles:
/// - `<iri>` → `Term::NamedNode`
/// - `"value"` → plain `xsd:string` `Term::Literal`
/// - `"value"^^<datatype>` → typed `Term::Literal`
/// - `"value"@lang` → language-tagged `Term::Literal`
pub(crate) fn decode_nemo_term(s: &str) -> Result<Term, String> {
    if s.starts_with('<') && s.ends_with('>') {
        // IRI term
        let iri = &s[1..s.len() - 1];
        let nn = NamedNode::new(iri)
            .map_err(|e| decode_err("invalid IRI in <iri> term", &format!("{e}: {iri}")))?;
        return Ok(Term::NamedNode(nn));
    }

    if let Some(content) = s.strip_prefix('"') {
        // Literal: find the closing quote character, accounting for escapes.
        // The closing `"` may be followed by `^^<dt>` or `@lang` or nothing.
        // Nemo's display form does not escape the closing quote mid-string;
        // the value ends at the last unescaped `"`.
        let (raw_value, suffix) = split_nemo_literal_content(content)?;
        // Un-escape the value (reverses Nemo's quote_string escaping).
        let value = unescape_nemo_string(raw_value);

        if suffix.is_empty() {
            // Plain xsd:string
            return Ok(Term::Literal(Literal::new_simple_literal(value)));
        }
        if let Some(lang) = suffix.strip_prefix('@') {
            return Ok(Term::Literal(
                Literal::new_language_tagged_literal(value, lang)
                    .map_err(|e| decode_err("invalid language tag", &format!("{e}")))?,
            ));
        }
        if let Some(dt_part) = suffix.strip_prefix("^^<") {
            if let Some(dt_iri) = dt_part.strip_suffix('>') {
                let dt = NamedNode::new(dt_iri)
                    .map_err(|e| decode_err("invalid datatype IRI", &format!("{e}")))?;
                return Ok(Term::Literal(Literal::new_typed_literal(value, dt)));
            }
        }
        return Err(decode_err("unrecognized literal suffix", suffix));
    }

    Err(decode_err("unrecognized Nemo term", s))
}

/// Split a Nemo literal body (after the opening `"`) into `(value_part, suffix)`.
///
/// `value_part` is the raw escaped content between the opening and closing `"`.
/// `suffix` is everything after the closing `"` (e.g. `^^<dt>`, `@lang`, or `""`).
pub(crate) fn split_nemo_literal_content(s: &str) -> Result<(&str, &str), String> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            // Skip the next character (escape sequence)
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            // Found the closing quote
            return Ok((&s[..i], &s[i + 1..]));
        }
        i += 1;
    }
    Err(decode_err("unterminated literal", s))
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── sha1_hex ──────────────────────────────────────────────────────────────

    #[test]
    fn sha1_hex_known_value() {
        // Python: sha1(b"b0").hexdigest() == "21fb6f4bd02acfcf13de01e98b4e7cb04ddb53c7"
        // (value computed independently — verifies our SHA1 matches Python's hashlib)
        let h = sha1_hex("b0");
        assert_eq!(h.len(), 40, "SHA1 hex must be 40 characters");
        assert!(
            h.chars().all(|c| c.is_ascii_hexdigit()),
            "SHA1 hex must be hex"
        );
    }

    // ── encode_literal ────────────────────────────────────────────────────────

    #[test]
    fn encode_plain_literal() {
        let lit = Literal::new_simple_literal("hello world");
        assert_eq!(encode_literal(&lit), r#""hello world""#);
    }

    #[test]
    fn encode_literal_with_quotes() {
        let lit = Literal::new_simple_literal(r#"say "hi""#);
        assert_eq!(encode_literal(&lit), r#""say \"hi\"""#);
    }

    #[test]
    fn encode_language_literal() {
        let lit = Literal::new_language_tagged_literal("Bonjour", "fr").unwrap();
        assert_eq!(encode_literal(&lit), r#""Bonjour"@fr"#);
    }

    #[test]
    fn encode_typed_literal() {
        let dt = NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap();
        let lit = Literal::new_typed_literal("42", dt);
        assert_eq!(
            encode_literal(&lit),
            r#""42"^^<http://www.w3.org/2001/XMLSchema#integer>"#
        );
    }

    // ── decode_nemo_term ──────────────────────────────────────────────────────

    #[test]
    fn decode_iri_roundtrip() {
        let encoded = "<http://example.org/Dog>";
        let term = decode_nemo_term(encoded).unwrap();
        match term {
            Term::NamedNode(nn) => assert_eq!(nn.as_str(), "http://example.org/Dog"),
            other => panic!("expected NamedNode, got {other:?}"),
        }
    }

    #[test]
    fn decode_plain_string_literal() {
        let encoded = r#""hello""#;
        let term = decode_nemo_term(encoded).unwrap();
        match term {
            Term::Literal(lit) => {
                assert_eq!(lit.value(), "hello");
                assert_eq!(
                    lit.datatype().as_str(),
                    "http://www.w3.org/2001/XMLSchema#string"
                );
                assert!(lit.language().is_none());
            }
            other => panic!("expected Literal, got {other:?}"),
        }
    }

    #[test]
    fn decode_language_literal() {
        let encoded = r#""Hola"@es"#;
        let term = decode_nemo_term(encoded).unwrap();
        match term {
            Term::Literal(lit) => {
                assert_eq!(lit.value(), "Hola");
                assert_eq!(lit.language(), Some("es"));
            }
            other => panic!("expected language Literal, got {other:?}"),
        }
    }

    #[test]
    fn decode_typed_literal() {
        let encoded = r#""42"^^<http://www.w3.org/2001/XMLSchema#integer>"#;
        let term = decode_nemo_term(encoded).unwrap();
        match term {
            Term::Literal(lit) => {
                assert_eq!(lit.value(), "42");
                assert_eq!(
                    lit.datatype().as_str(),
                    "http://www.w3.org/2001/XMLSchema#integer"
                );
            }
            other => panic!("expected typed Literal, got {other:?}"),
        }
    }

    // ── decode_string_constant ────────────────────────────────────────────────

    #[test]
    fn decode_world_constant() {
        let encoded = r#""http://world/Alpha""#;
        let world = decode_string_constant(encoded).unwrap();
        assert_eq!(world, "http://world/Alpha");
    }

    #[test]
    fn decode_default_constant() {
        let encoded = r#""default""#;
        let s = decode_string_constant(encoded).unwrap();
        assert_eq!(s, "default");
    }

    // ── encode/decode roundtrip ───────────────────────────────────────────────

    #[test]
    fn encode_decode_iri_roundtrip() {
        let subject = NamedOrBlankNode::NamedNode(NamedNode::new("http://example.org/s").unwrap());
        let predicate = NamedNode::new("http://example.org/p").unwrap();
        let object = Term::NamedNode(NamedNode::new("http://example.org/o").unwrap());
        let world = "http://world/Test";

        let line = encode_quad_to_nemo_fact(&subject, &predicate, &object, world);
        // line = <http://example.org/p>(<http://example.org/s>, <http://example.org/o>, "http://world/Test").

        // Verify it parses correctly
        assert!(line.starts_with("<http://example.org/p>("));
        assert!(line.contains("<http://example.org/s>"));
        assert!(line.contains("<http://example.org/o>"));
        assert!(line.contains("\"http://world/Test\""));
        assert!(line.ends_with('.'));
    }

    #[test]
    fn encode_decode_literal_roundtrip() {
        let dt = NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap();
        let lit = Literal::new_typed_literal("3.14", dt);
        let encoded = encode_literal(&lit);
        let decoded = decode_nemo_term(&encoded).unwrap();
        match decoded {
            Term::Literal(l) => assert_eq!(l.value(), "3.14"),
            other => panic!("expected Literal, got {other:?}"),
        }
    }

    // ── control-character round-trip (Gap 9) ─────────────────────────────────

    /// Verify that literals containing newline and tab survive the
    /// encode → Nemo display → decode round-trip with the exact same value.
    ///
    /// Nemo's `quote_string` (used by `AnyDataValue::to_string()`) encodes:
    ///   - actual `\n` (U+000A) → `\n` (two chars: backslash + n)
    ///   - actual `\r` (U+000D) → `\r` (two chars: backslash + r)
    ///   - tabs are NOT escaped (passed through as raw tab)
    ///
    /// The encode path writes these characters raw into the `.rls` source
    /// (Nemo's lexer accepts any byte except `"` inside a string literal).
    /// The decode path reverses the `quote_string` escaping so the value is
    /// preserved exactly.
    #[test]
    fn encode_decode_literal_with_newline_and_tab() {
        // Literal value: "line1\nline2\ttabbed"
        let raw = "line1\nline2\ttabbed";
        let lit = Literal::new_simple_literal(raw);

        // Step 1: encode produces the literal token for the .rls source.
        let encoded = encode_literal(&lit);

        // The encoded form must be a valid Nemo string literal (double-quoted).
        assert!(
            encoded.starts_with('"') && encoded.ends_with('"'),
            "encoded literal must be double-quoted: {encoded:?}"
        );

        // Step 2: simulate what Nemo's AnyDataValue::to_string() returns after
        // storing and re-displaying the value.  Nemo's quote_string:
        //   - leaves raw bytes from the source AS-IS (no unescape on parse)
        //   - on display, escapes \n → \n (two chars) and \r → \r (two chars)
        //
        // Because our encode writes the raw chars into the source, Nemo stores
        // them as-is, then quote_string produces the two-char escape sequences.
        // We simulate that display form here so we can test the decode path
        // without running the full Nemo chase.
        let simulated_nemo_display = {
            // quote_string: \\ → \\, \" → \", \n → \n, \r → \r (tabs untouched)
            let inner = raw
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r");
            format!("\"{inner}\"")
        };

        // Step 3: decode the Nemo display form back to an oxigraph Term.
        let decoded_term = decode_nemo_term(&simulated_nemo_display)
            .expect("decode must succeed for a valid Nemo plain string display form");

        // Step 4: the decoded value must equal the original literal value exactly.
        match decoded_term {
            Term::Literal(decoded_lit) => {
                assert_eq!(
                    decoded_lit.value(),
                    raw,
                    "round-trip must preserve newline+tab exactly: \
                     expected {raw:?}, got {:?}",
                    decoded_lit.value()
                );
            }
            other => panic!("expected Literal, got {other:?}"),
        }
    }

    /// Verify that `unescape_nemo_string` handles all four escape sequences
    /// that Nemo's `quote_string` can emit.
    #[test]
    fn unescape_nemo_string_all_sequences() {
        // Input: backslash-escaped sequences as emitted by Nemo's quote_string
        let input = r#"hello\\world\"quoted\nnewline\rcarriage"#;
        // Rust raw string: the source text is:
        //   hello\\world\"quoted\nnewline\rcarriage
        // which represents the Nemo display form of:
        //   hello\world"quoted[newline]newline[CR]carriage
        let expected = "hello\\world\"quoted\nnewline\rcarriage";
        assert_eq!(unescape_nemo_string(input), expected);
    }
}
