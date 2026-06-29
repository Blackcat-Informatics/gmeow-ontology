// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Content-addressed provenance IRI helpers.
//!
//! Every function here **must** produce byte-identical output to the canonical
//! native statements recipe.  The goldens in
//! `tests/fixtures/logic/determinism-goldens.json` are normative; any deviation
//! from them is a hard test failure.  (The retired `logic_materialize.py` was
//! the prior Python authority; it was superseded by this crate in #497/#636.)
//!
//! # N3 serialization rules (mirror of rdflib `.n3()`)
//!
//! rdflib's `.n3()` produces:
//! - IRI: `<iri>`
//! - Language-tagged literal: `"lex"@lang`  (rdflib lower-cases the lang subtag)
//! - `xsd:string` literal: `"lex"` (datatype **elided**)
//! - `rdf:langString` literal: `"lex"@lang` (datatype **elided**, lang kept)
//! - Any other typed literal: `"lex"^^<datatype_iri>`
//!
//! Lexical-form escaping:
//! - `\` → `\\`
//! - `"` → `\"`
//! - `\n` (newline) → `\n`
//! - `\r` (CR) → `\r`
//! - `\t` (tab) → `\t`
//!
//! No numeric normalization — the lexical form is preserved verbatim.
//!
//! # Reifier recipe
//!
//! `sha1(s.n3() + " " + p.n3() + " " + o.n3()).hexdigest()`
//! under `{NAMESPACE}reifier/`.
//!
//! # Derivation-ID recipe
//!
//! `sha1(rule_iri + "\n" + "\n".join(sorted(source_reifier_iris))).hexdigest()`
//! under `{NAMESPACE}derivation/`.
//! Sources are sorted for order-independence.

use gmeow_rdf::TermValue;
use sha1::{Digest, Sha1};

// ── Namespace constants ────────────────────────────────────────────────────────

/// Vocabulary namespace — term IRIs are `NAMESPACE + local`.
/// Matches `gmeow_tools.config.NAMESPACE` exactly.
pub const NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";

/// Logic vocabulary namespace.
/// Matches `gmeow_tools.config.LOGIC_NAMESPACE` exactly.
pub const LOGIC_NAMESPACE: &str = "https://blackcatinformatics.ca/logic/";

/// Sentinel rule IRI for asserted (input) facts.
/// The canonical assert-rule IRI (the recipe formerly carried by
/// `logic_materialize.py`, retired in #497):
/// `f"{_LOGIC_NS}assert"` where `_LOGIC_NS = PREFIXES["logic"]`.
pub const ASSERT_RULE_IRI: &str = "https://blackcatinformatics.ca/logic/assert";

/// Prefix for reifier IRIs.
pub const REIFIER_PREFIX: &str = "https://blackcatinformatics.ca/gmeow/reifier/";

/// Prefix for derivation IRIs.
pub const DERIVATION_PREFIX: &str = "https://blackcatinformatics.ca/gmeow/derivation/";

// ── XSD / RDF datatype IRIs ────────────────────────────────────────────────────

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

// ── SHA-1 helper ─────────────────────────────────────────────────────────────

/// The lowercase-hex SHA-1 of `s` — the content-addressing primitive the reifier,
/// derivation-id, and native reasoning-contract hashes all share.
pub fn sha1_hex(s: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ── N3 serialization ─────────────────────────────────────────────────────────

/// Escape a literal lexical form exactly as rdflib does in `.n3()`.
///
/// rdflib escapes: `\` → `\\`, `"` → `\"`, newline → `\n`, CR → `\r`, tab → `\t`.
/// No other escaping is applied.
fn escape_lexical(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

/// Serialize a native literal (lexical form + datatype IRI + optional language) to
/// rdflib `.n3()` form.
///
/// Rules:
/// - `xsd:string` → `"lex"` (datatype elided)
/// - `rdf:langString` (lang-tagged) → `"lex"@lang` (datatype elided, lang kept)
/// - Any other datatype → `"lex"^^<datatype_iri>`
///
/// rdflib lowercases the BCP-47 language subtag.  The native IR lowercases the
/// language tag for the identity key, but a `TermValue` constructed directly may
/// carry an un-lowercased tag, so we lowercase here to stay in sync regardless.
fn literal_n3_parts(lexical_form: &str, datatype: &str, language: Option<&str>) -> String {
    let lex = escape_lexical(lexical_form);

    if let Some(lang) = language {
        // Language-tagged literal — rdflib elides the rdf:langString datatype.
        // rdflib lowercases the language tag; mirror that.
        return format!("\"{}\"@{}", lex, lang.to_lowercase());
    }

    if datatype == XSD_STRING {
        // Plain xsd:string — rdflib elides the datatype.
        return format!("\"{}\"", lex);
    }
    if datatype == RDF_LANG_STRING {
        // rdf:langString without a language tag — treated like xsd:string by rdflib.
        // (In practice rdf:langString always has a lang tag; be defensive.)
        return format!("\"{}\"", lex);
    }

    // Typed literal with a non-elided datatype.
    format!("\"{}\"^^<{}>", lex, datatype)
}

/// Serialize a native [`TermValue`] to rdflib `.n3()` form.
///
/// - `Iri(iri)` → `<iri>`
/// - `Blank` → not expected after Skolemization; serialized as `_:label`
/// - `Literal` → delegated to [`literal_n3_parts`]
/// - `Triple` → hard error: RDF-star quoted-triple terms are out of scope for
///   gmeow-logic v1.  An empty hash would cause silent ID collisions; failing
///   closed is the correct behavior.
///
/// # Errors
///
/// Returns an error string if `term` is a `TermValue::Triple`.
pub fn term_n3(term: &TermValue) -> Result<String, String> {
    match term {
        TermValue::Iri(iri) => Ok(format!("<{}>", iri)),
        TermValue::Blank { label, scope } => Ok(format!("_:{}", scope.qualify_label(label))),
        TermValue::Literal {
            lexical_form,
            datatype,
            language,
            ..
        } => Ok(literal_n3_parts(
            lexical_form,
            datatype,
            language.as_deref(),
        )),
        TermValue::Triple { .. } => Err(
            "RDF-star quoted-triple terms are not supported in gmeow-logic v1 \
             (TermValue::Triple cannot be hashed without risking ID collisions)"
                .to_owned(),
        ),
    }
}

/// Serialize an IRI string to rdflib `.n3()` form: `<iri>`.
pub fn named_node_n3(iri: &str) -> String {
    format!("<{}>", iri)
}

/// Render a [`TermValue`] in oxigraph's Turtle term Display form — the exact byte
/// form the prior `Term::to_string()` produced. This is the canonical-surface used
/// for content-addressed dedup keys and sort keys (`rule_ir`, `foundation`) and for
/// the verify finding detail, so it MUST stay byte-identical to oxigraph's Display.
///
/// Unlike [`term_n3`] this does **not** lowercase the language tag (oxigraph's
/// Display preserves the stored tag verbatim) and renders a triple term in the
/// RDF-1.2 quoted form `<< s p o >>`.
///
/// - `Iri` → `<iri>`
/// - `Blank` → `_:label`
/// - `Literal` xsd:string / rdf:langString → `"lex"` ; lang → `"lex"@lang` ;
///   typed → `"lex"^^<dt>`
/// - `Triple` → `<< s p o >>` (recursive)
pub fn term_display(term: &TermValue) -> String {
    match term {
        TermValue::Iri(iri) => format!("<{iri}>"),
        TermValue::Blank { label, scope } => format!("_:{}", scope.qualify_label(label)),
        TermValue::Literal {
            lexical_form,
            datatype,
            language,
            ..
        } => {
            let lex = escape_lexical(lexical_form);
            if let Some(lang) = language {
                format!("\"{lex}\"@{lang}")
            } else if datatype == XSD_STRING || datatype == RDF_LANG_STRING {
                format!("\"{lex}\"")
            } else {
                format!("\"{lex}\"^^<{datatype}>")
            }
        }
        TermValue::Triple { s, p, o } => format!(
            "<< {} {} {} >>",
            term_display(s),
            term_display(p),
            term_display(o)
        ),
    }
}

// ── mint_reifier ─────────────────────────────────────────────────────────────

/// Compute the reifier IRI for an `(S, P, O)` triple.
///
/// Mirrors the native statement-stage reifier recipe exactly:
/// ```text
/// canonical = s.n3() + " " + p.n3() + " " + o.n3()
/// digest    = sha1(canonical.encode("utf-8")).hexdigest()
/// iri       = f"{NAMESPACE}reifier/{digest}"
/// ```
///
/// # Arguments
///
/// - `s` — Subject term (as [`TermValue`]; IRIs after Skolemization).
/// - `p` — Predicate IRI string.
/// - `o` — Object term.
///
/// # Errors
///
/// Returns an error string if either `s` or `o` is a `TermValue::Triple`
/// (RDF-star quoted triples are out of scope for gmeow-logic v1).
///
/// # Returns
///
/// The reifier IRI as a `String`.
pub fn mint_reifier(s: &TermValue, p: &str, o: &TermValue) -> Result<String, String> {
    let s_n3 = term_n3(s)?;
    let o_n3 = term_n3(o)?;
    let canonical = format!("{} {} {}", s_n3, named_node_n3(p), o_n3);
    let digest = sha1_hex(&canonical);
    Ok(format!("{}{}", REIFIER_PREFIX, digest))
}

/// Compute the reifier IRI from already-serialized N3 component strings.
///
/// `subject` and `predicate` are IRI strings (NOT N3-wrapped — this helper wraps
/// them in `<...>`); `obj_n3` is the object already in canonical N3 form (`<iri>`
/// for an IRI, `"lex"^^<dt>` for a literal, etc.) and is used **verbatim**.
///
/// The canonical reifier recipe (Python `_reifier_from_quad` in
/// `logic_explain.py` retired in #497):
/// ```text
/// payload = f"<{subject}> <{predicate}> {obj_n3}"
/// digest  = sha1(payload.encode("utf-8")).hexdigest()
/// iri     = f"{NAMESPACE}reifier/{digest}"
/// ```
///
/// Used by the explanation engine ([`crate::explain`]), whose rows carry the
/// object already as an N3 string (it never re-parses the object term).
pub(crate) fn reifier_from_strings(subject: &str, predicate: &str, obj_n3: &str) -> String {
    let canonical = format!("<{}> <{}> {}", subject, predicate, obj_n3);
    let digest = sha1_hex(&canonical);
    format!("{}{}", REIFIER_PREFIX, digest)
}

// ── mint_derivation_id ───────────────────────────────────────────────────────

/// Compute the derivation IRI for a rule firing.
///
/// The canonical derivation-id recipe (Python `derivation_id_iri` in
/// `gmeow_tools.logic_materialize` retired in #497):
/// ```text
/// payload = rule_iri + "\n" + "\n".join(sorted(source_reifier_iris))
/// digest  = sha1(payload.encode("utf-8")).hexdigest()
/// iri     = f"{NAMESPACE}derivation/{digest}"
/// ```
///
/// Sources are sorted (ascending lexicographic) for order-independence.
///
/// # Arguments
///
/// - `rule_iri` — The IRI of the fired rule (or the assert-sentinel).
/// - `source_reifier_iris` — The reifier IRIs of the consumed antecedent quads.
///
/// # Returns
///
/// The derivation IRI as a `String`.
pub fn mint_derivation_id(rule_iri: &str, source_reifier_iris: &[&str]) -> String {
    let mut sorted: Vec<&str> = source_reifier_iris.to_vec();
    sorted.sort_unstable();
    let joined = sorted.join("\n");
    let payload = format!("{}\n{}", rule_iri, joined);
    let digest = sha1_hex(&payload);
    format!("{}{}", DERIVATION_PREFIX, digest)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Test-only helper mirroring `literal_n3_parts` over a constructed `TermValue`.
    fn literal_n3(term: &TermValue) -> String {
        term_n3(term).expect("literal term must serialize")
    }

    // ── literal_n3 ────────────────────────────────────────────────────────────

    #[test]
    fn literal_n3_plain_string_elides_datatype() {
        // xsd:string datatype must be elided — matches rdflib .n3()
        let lit = TermValue::simple_literal("plain string");
        assert_eq!(literal_n3(&lit), "\"plain string\"");
    }

    #[test]
    fn literal_n3_language_tagged_lowercased() {
        // rdflib lowercases lang tags; we must mirror that
        let lit = TermValue::lang_literal("hello", "en");
        assert_eq!(literal_n3(&lit), "\"hello\"@en");
    }

    #[test]
    fn literal_n3_uppercase_lang_lowercased() {
        // Upper-case lang tag must be lowercased
        let lit = TermValue::lang_literal("Bonjour", "FR");
        assert_eq!(literal_n3(&lit), "\"Bonjour\"@fr");
    }

    #[test]
    fn literal_n3_decimal_not_elided() {
        // xsd:decimal must NOT be elided — only xsd:string and rdf:langString are
        let lit = TermValue::typed_literal("1.0", "http://www.w3.org/2001/XMLSchema#decimal");
        assert_eq!(
            literal_n3(&lit),
            "\"1.0\"^^<http://www.w3.org/2001/XMLSchema#decimal>"
        );
    }

    #[test]
    fn literal_n3_escape_backslash() {
        let lit = TermValue::simple_literal("a\\b");
        assert_eq!(literal_n3(&lit), "\"a\\\\b\"");
    }

    #[test]
    fn literal_n3_escape_quote() {
        let lit = TermValue::simple_literal("say \"hi\"");
        assert_eq!(literal_n3(&lit), "\"say \\\"hi\\\"\"");
    }

    #[test]
    fn literal_n3_escape_newline() {
        let lit = TermValue::simple_literal("line1\nline2");
        assert_eq!(literal_n3(&lit), "\"line1\\nline2\"");
    }

    #[test]
    fn literal_n3_escape_tab() {
        let lit = TermValue::simple_literal("col1\tcol2");
        assert_eq!(literal_n3(&lit), "\"col1\\tcol2\"");
    }

    // ── term_n3 ───────────────────────────────────────────────────────────────

    #[test]
    fn term_n3_iri() {
        let term = TermValue::iri("http://example.org/a");
        assert_eq!(term_n3(&term).unwrap(), "<http://example.org/a>");
    }

    #[test]
    fn term_n3_literal_string() {
        let term = TermValue::simple_literal("hello");
        assert_eq!(term_n3(&term).unwrap(), "\"hello\"");
    }

    // ── mint_reifier goldens ─────────────────────────────────────────────────

    /// Golden 1: three plain IRI terms.
    /// Python oracle: sha1("<http://example.org/a> <http://example.org/related> <http://example.org/b>")
    ///             = 10d9bdab72fe25cf3b81fe842b3a105077d98a6a
    #[test]
    fn mint_reifier_golden_1_iri_triple() {
        let s = TermValue::iri("http://example.org/a");
        let p = "http://example.org/related";
        let o = TermValue::iri("http://example.org/b");
        let got = mint_reifier(&s, p, &o).expect("IRI terms must not fail");
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/reifier/10d9bdab72fe25cf3b81fe842b3a105077d98a6a",
            "mint_reifier golden-1 mismatch"
        );
    }

    /// Golden 2: language-tagged literal object (lang tag lowercased).
    /// Python oracle: sha1("<http://example.org/x> <http://www.w3.org/2000/01/rdf-schema#label> \"hello\"@en")
    ///             = 61194b8ccffff3db1bbb81df91c55b7776ee4064
    #[test]
    fn mint_reifier_golden_2_lang_literal() {
        let s = TermValue::iri("http://example.org/x");
        let p = "http://www.w3.org/2000/01/rdf-schema#label";
        let o = TermValue::lang_literal("hello", "en");
        let got = mint_reifier(&s, p, &o).expect("lang literal terms must not fail");
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/reifier/61194b8ccffff3db1bbb81df91c55b7776ee4064",
            "mint_reifier golden-2 mismatch"
        );
    }

    /// Golden 3: xsd:decimal literal — datatype NOT elided.
    /// Python oracle: sha1("<http://example.org/m> <http://example.org/value> \"1.0\"^^<http://www.w3.org/2001/XMLSchema#decimal>")
    ///             = efbda8fbbb765e64c7f8ca2d690489a1ba70e569
    #[test]
    fn mint_reifier_golden_3_xsd_decimal() {
        let s = TermValue::iri("http://example.org/m");
        let p = "http://example.org/value";
        let o = TermValue::typed_literal("1.0", "http://www.w3.org/2001/XMLSchema#decimal");
        let got = mint_reifier(&s, p, &o).expect("typed literal terms must not fail");
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/reifier/efbda8fbbb765e64c7f8ca2d690489a1ba70e569",
            "mint_reifier golden-3 mismatch"
        );
    }

    /// Golden 4: plain string literal — xsd:string datatype ELIDED.
    /// Python oracle: sha1("<http://example.org/n> <http://example.org/name> \"plain string\"")
    ///             = 784c486d79b869539405a3f90f21126477b07f26
    #[test]
    fn mint_reifier_golden_4_plain_string() {
        let s = TermValue::iri("http://example.org/n");
        let p = "http://example.org/name";
        let o = TermValue::simple_literal("plain string");
        let got = mint_reifier(&s, p, &o).expect("plain literal terms must not fail");
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/reifier/784c486d79b869539405a3f90f21126477b07f26",
            "mint_reifier golden-4 mismatch"
        );
    }

    // ── mint_derivation_id goldens ────────────────────────────────────────────

    /// Golden 5: two-source rule firing (sources are sorted before hashing).
    /// payload = "https://blackcatinformatics.ca/logic/rules/transitivity\n
    ///            https://blackcatinformatics.ca/gmeow/reifier/10d9bdab72fe25cf3b81fe842b3a105077d98a6a\n
    ///            https://blackcatinformatics.ca/gmeow/reifier/61194b8ccffff3db1bbb81df91c55b7776ee4064"
    /// sha1 = e1379d93fd46357cc6a3be9e057528bb0d589f68
    #[test]
    fn mint_derivation_id_golden_5_two_sources() {
        let rule_iri = "https://blackcatinformatics.ca/logic/rules/transitivity";
        let sources = [
            "https://blackcatinformatics.ca/gmeow/reifier/10d9bdab72fe25cf3b81fe842b3a105077d98a6a",
            "https://blackcatinformatics.ca/gmeow/reifier/61194b8ccffff3db1bbb81df91c55b7776ee4064",
        ];
        let got = mint_derivation_id(rule_iri, &sources);
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/derivation/e1379d93fd46357cc6a3be9e057528bb0d589f68",
            "mint_derivation_id golden-5 mismatch"
        );
    }

    /// Golden 5b: same sources in reversed order → same result (sorted).
    #[test]
    fn mint_derivation_id_golden_5_order_independent() {
        let rule_iri = "https://blackcatinformatics.ca/logic/rules/transitivity";
        let sources_fwd = [
            "https://blackcatinformatics.ca/gmeow/reifier/10d9bdab72fe25cf3b81fe842b3a105077d98a6a",
            "https://blackcatinformatics.ca/gmeow/reifier/61194b8ccffff3db1bbb81df91c55b7776ee4064",
        ];
        let sources_rev = [
            "https://blackcatinformatics.ca/gmeow/reifier/61194b8ccffff3db1bbb81df91c55b7776ee4064",
            "https://blackcatinformatics.ca/gmeow/reifier/10d9bdab72fe25cf3b81fe842b3a105077d98a6a",
        ];
        assert_eq!(
            mint_derivation_id(rule_iri, &sources_fwd),
            mint_derivation_id(rule_iri, &sources_rev),
            "mint_derivation_id must be order-independent"
        );
    }

    /// Golden 6: assert-sentinel derivation (self-reifier as only source).
    /// payload = "https://blackcatinformatics.ca/logic/assert\n
    ///            https://blackcatinformatics.ca/gmeow/reifier/10d9bdab72fe25cf3b81fe842b3a105077d98a6a"
    /// sha1 = 5dd2eeebb9812618b81b5053f662c0756c57b2e6
    #[test]
    fn mint_derivation_id_golden_6_assert_sentinel() {
        let rule_iri = "https://blackcatinformatics.ca/logic/assert";
        let sources = [
            "https://blackcatinformatics.ca/gmeow/reifier/10d9bdab72fe25cf3b81fe842b3a105077d98a6a",
        ];
        let got = mint_derivation_id(rule_iri, &sources);
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/derivation/5dd2eeebb9812618b81b5053f662c0756c57b2e6",
            "mint_derivation_id golden-6 mismatch"
        );
    }

    // ── Goldens parity: load from JSON fixture ────────────────────────────────

    /// Load the authoritative goldens JSON and verify all entries match.
    ///
    /// This test is the normative gate: it reads the same file the Python oracle
    /// writes and asserts that every IRI the Rust engine would produce is
    /// byte-identical.
    #[test]
    fn goldens_parity_from_json_fixture() {
        // Path relative to the crate root (where Cargo.toml lives).
        // `include_str!` is relative to the source file, so use a path that
        // goes up from src/ to the repo root then down to the fixture.
        let json_text = include_str!("../../../tests/fixtures/logic/determinism-goldens.json");

        let root: serde_json::Value =
            serde_json::from_str(json_text).expect("determinism-goldens.json must be valid JSON");

        // ── Quad-reifier goldens ──────────────────────────────────────────────
        let reifier_goldens = root["quad_reifier_goldens"]
            .as_array()
            .expect("quad_reifier_goldens must be an array");

        for entry in reifier_goldens {
            let id = entry["_id"].as_str().unwrap_or("?");
            let subj_iri = entry["subject"].as_str().expect("subject");
            let pred_iri = entry["predicate"].as_str().expect("predicate");
            let expected_reifier = entry["reifier_iri"].as_str().expect("reifier_iri");
            let is_literal = entry["object_is_literal"].as_bool().unwrap_or(false);

            let s = TermValue::iri(subj_iri);

            let o: TermValue = if is_literal {
                let lex = entry["object"].as_str().expect("object lexical");
                if let Some(lang) = entry["object_lang"].as_str() {
                    TermValue::lang_literal(lex, lang)
                } else if let Some(dt_iri) = entry["object_datatype"].as_str() {
                    TermValue::typed_literal(lex, dt_iri)
                } else {
                    // Plain xsd:string
                    TermValue::simple_literal(lex)
                }
            } else {
                let obj_iri = entry["object"].as_str().expect("object IRI");
                TermValue::iri(obj_iri)
            };

            let got = mint_reifier(&s, pred_iri, &o)
                .unwrap_or_else(|e| panic!("{id}: mint_reifier failed: {e}"));
            assert_eq!(
                got, expected_reifier,
                "goldens parity FAIL for {id}: got {got:?}, expected {expected_reifier:?}"
            );
        }

        // ── Derivation-ID goldens ─────────────────────────────────────────────
        let derivation_goldens = root["derivation_id_goldens"]
            .as_array()
            .expect("derivation_id_goldens must be an array");

        for entry in derivation_goldens {
            let id = entry["_id"].as_str().unwrap_or("?");
            let rule_iri = entry["rule_iri"].as_str().expect("rule_iri");
            let expected_derivation = entry["derivation_iri"].as_str().expect("derivation_iri");
            let sources: Vec<&str> = entry["source_reifier_iris"]
                .as_array()
                .expect("source_reifier_iris")
                .iter()
                .map(|v| v.as_str().expect("source IRI"))
                .collect();

            let got = mint_derivation_id(rule_iri, &sources);
            assert_eq!(
                got, expected_derivation,
                "goldens parity FAIL for {id}: got {got:?}, expected {expected_derivation:?}"
            );
        }
    }
}
