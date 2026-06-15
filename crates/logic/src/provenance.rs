// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! Content-addressed provenance IRI helpers — Rust mirror of the Python oracle.
//!
//! Every function here **must** produce byte-identical output to the corresponding
//! Python function in `gmeow_tools.statement_dsl` / `gmeow_tools.logic_materialize`.
//! The goldens in `tests/fixtures/logic/determinism-goldens.json` are normative;
//! any deviation from them is a hard test failure.
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

use oxigraph::model::{Literal, NamedNode, Term};
use sha1::{Digest, Sha1};

// ── Namespace constants ────────────────────────────────────────────────────────

/// Vocabulary namespace — term IRIs are `NAMESPACE + local`.
/// Matches `gmeow_tools.config.NAMESPACE` exactly.
pub const NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";

/// Logic vocabulary namespace.
/// Matches `gmeow_tools.config.LOGIC_NAMESPACE` exactly.
pub const LOGIC_NAMESPACE: &str = "https://blackcatinformatics.ca/logic/";

/// Sentinel rule IRI for asserted (input) facts.
/// Matches `_ASSERT_RULE_IRI` in `logic_materialize.py` exactly:
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

fn sha1_hex(s: &str) -> String {
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

/// Serialize an oxigraph `Literal` to rdflib `.n3()` form.
///
/// Rules:
/// - `xsd:string` → `"lex"` (datatype elided)
/// - `rdf:langString` (lang-tagged) → `"lex"@lang` (datatype elided, lang kept)
/// - Any other datatype → `"lex"^^<datatype_iri>`
///
/// rdflib lowercases the BCP-47 language subtag.  oxigraph already stores the
/// language tag exactly as parsed, but the Python oracle uses rdflib which
/// lowercases on parse.  We lowercase here to stay in sync.
pub fn literal_n3(lit: &Literal) -> String {
    let lex = escape_lexical(lit.value());

    if let Some(lang) = lit.language() {
        // Language-tagged literal — rdflib elides the rdf:langString datatype.
        // rdflib lowercases the language tag; mirror that.
        return format!("\"{}\"@{}", lex, lang.to_lowercase());
    }

    let dt = lit.datatype().as_str();
    if dt == XSD_STRING {
        // Plain xsd:string — rdflib elides the datatype.
        return format!("\"{}\"", lex);
    }
    if dt == RDF_LANG_STRING {
        // rdf:langString without a language tag — treated like xsd:string by rdflib.
        // (In practice rdf:langString always has a lang tag; be defensive.)
        return format!("\"{}\"", lex);
    }

    // Typed literal with a non-elided datatype.
    format!("\"{}\"^^<{}>", lex, dt)
}

/// Serialize an oxigraph `Term` to rdflib `.n3()` form.
///
/// - `NamedNode(iri)` → `<iri>`
/// - `BlankNode` → not expected after Skolemization; serialized as `_:id`
/// - `Literal` → delegated to [`literal_n3`]
/// - `Triple` → not supported; returns empty string
pub fn term_n3(term: &Term) -> String {
    match term {
        Term::NamedNode(nn) => format!("<{}>", nn.as_str()),
        Term::BlankNode(bn) => format!("_:{}", bn.as_str()),
        Term::Literal(lit) => literal_n3(lit),
        Term::Triple(_) => String::new(),
    }
}

/// Serialize a `NamedNode` to rdflib `.n3()` form: `<iri>`.
pub fn named_node_n3(nn: &NamedNode) -> String {
    format!("<{}>", nn.as_str())
}

// ── mint_reifier ─────────────────────────────────────────────────────────────

/// Compute the reifier IRI for an `(S, P, O)` triple.
///
/// Mirrors `mint_reifier` in `gmeow_tools.statement_dsl` exactly:
/// ```text
/// canonical = s.n3() + " " + p.n3() + " " + o.n3()
/// digest    = sha1(canonical.encode("utf-8")).hexdigest()
/// iri       = f"{NAMESPACE}reifier/{digest}"
/// ```
///
/// # Arguments
///
/// - `s` — Subject term (as `Term`; IRIs after Skolemization).
/// - `p` — Predicate named node.
/// - `o` — Object term.
///
/// # Returns
///
/// The reifier IRI as a `String`.
pub fn mint_reifier(s: &Term, p: &NamedNode, o: &Term) -> String {
    let canonical = format!("{} {} {}", term_n3(s), named_node_n3(p), term_n3(o));
    let digest = sha1_hex(&canonical);
    format!("{}{}", REIFIER_PREFIX, digest)
}

// ── mint_derivation_id ───────────────────────────────────────────────────────

/// Compute the derivation IRI for a rule firing.
///
/// Mirrors `derivation_id_iri` in `gmeow_tools.logic_materialize` exactly:
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
    use oxigraph::model::{Literal, NamedNode, Term};

    // ── literal_n3 ────────────────────────────────────────────────────────────

    #[test]
    fn literal_n3_plain_string_elides_datatype() {
        // xsd:string datatype must be elided — matches rdflib .n3()
        let lit = Literal::new_simple_literal("plain string");
        assert_eq!(literal_n3(&lit), "\"plain string\"");
    }

    #[test]
    fn literal_n3_language_tagged_lowercased() {
        // rdflib lowercases lang tags; we must mirror that
        let lit = Literal::new_language_tagged_literal("hello", "en").unwrap();
        assert_eq!(literal_n3(&lit), "\"hello\"@en");
    }

    #[test]
    fn literal_n3_uppercase_lang_lowercased() {
        // Upper-case lang tag must be lowercased
        let lit = Literal::new_language_tagged_literal("Bonjour", "FR").unwrap();
        assert_eq!(literal_n3(&lit), "\"Bonjour\"@fr");
    }

    #[test]
    fn literal_n3_decimal_not_elided() {
        // xsd:decimal must NOT be elided — only xsd:string and rdf:langString are
        let dt = NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap();
        let lit = Literal::new_typed_literal("1.0", dt);
        assert_eq!(
            literal_n3(&lit),
            "\"1.0\"^^<http://www.w3.org/2001/XMLSchema#decimal>"
        );
    }

    #[test]
    fn literal_n3_escape_backslash() {
        let lit = Literal::new_simple_literal("a\\b");
        assert_eq!(literal_n3(&lit), "\"a\\\\b\"");
    }

    #[test]
    fn literal_n3_escape_quote() {
        let lit = Literal::new_simple_literal("say \"hi\"");
        assert_eq!(literal_n3(&lit), "\"say \\\"hi\\\"\"");
    }

    #[test]
    fn literal_n3_escape_newline() {
        let lit = Literal::new_simple_literal("line1\nline2");
        assert_eq!(literal_n3(&lit), "\"line1\\nline2\"");
    }

    #[test]
    fn literal_n3_escape_tab() {
        let lit = Literal::new_simple_literal("col1\tcol2");
        assert_eq!(literal_n3(&lit), "\"col1\\tcol2\"");
    }

    // ── term_n3 ───────────────────────────────────────────────────────────────

    #[test]
    fn term_n3_iri() {
        let nn = NamedNode::new("http://example.org/a").unwrap();
        let term = Term::NamedNode(nn);
        assert_eq!(term_n3(&term), "<http://example.org/a>");
    }

    #[test]
    fn term_n3_literal_string() {
        let lit = Literal::new_simple_literal("hello");
        let term = Term::Literal(lit);
        assert_eq!(term_n3(&term), "\"hello\"");
    }

    // ── mint_reifier goldens ─────────────────────────────────────────────────

    /// Golden 1: three plain IRI terms.
    /// Python oracle: sha1("<http://example.org/a> <http://example.org/related> <http://example.org/b>")
    ///             = 10d9bdab72fe25cf3b81fe842b3a105077d98a6a
    #[test]
    fn mint_reifier_golden_1_iri_triple() {
        let s = Term::NamedNode(NamedNode::new("http://example.org/a").unwrap());
        let p = NamedNode::new("http://example.org/related").unwrap();
        let o = Term::NamedNode(NamedNode::new("http://example.org/b").unwrap());
        let got = mint_reifier(&s, &p, &o);
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
        let s = Term::NamedNode(NamedNode::new("http://example.org/x").unwrap());
        let p = NamedNode::new("http://www.w3.org/2000/01/rdf-schema#label").unwrap();
        let lit = Literal::new_language_tagged_literal("hello", "en").unwrap();
        let o = Term::Literal(lit);
        let got = mint_reifier(&s, &p, &o);
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
        let s = Term::NamedNode(NamedNode::new("http://example.org/m").unwrap());
        let p = NamedNode::new("http://example.org/value").unwrap();
        let dt = NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap();
        let lit = Literal::new_typed_literal("1.0", dt);
        let o = Term::Literal(lit);
        let got = mint_reifier(&s, &p, &o);
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
        let s = Term::NamedNode(NamedNode::new("http://example.org/n").unwrap());
        let p = NamedNode::new("http://example.org/name").unwrap();
        let lit = Literal::new_simple_literal("plain string");
        let o = Term::Literal(lit);
        let got = mint_reifier(&s, &p, &o);
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

            let s = Term::NamedNode(
                NamedNode::new(subj_iri)
                    .unwrap_or_else(|e| panic!("{id}: invalid subject IRI {subj_iri:?}: {e}")),
            );
            let p = NamedNode::new(pred_iri)
                .unwrap_or_else(|e| panic!("{id}: invalid predicate IRI {pred_iri:?}: {e}"));

            let o: Term = if is_literal {
                let lex = entry["object"].as_str().expect("object lexical");
                if let Some(lang) = entry["object_lang"].as_str() {
                    Term::Literal(
                        Literal::new_language_tagged_literal(lex, lang)
                            .unwrap_or_else(|e| panic!("{id}: invalid lang tag: {e}")),
                    )
                } else if let Some(dt_iri) = entry["object_datatype"].as_str() {
                    let dt = NamedNode::new(dt_iri)
                        .unwrap_or_else(|e| panic!("{id}: invalid datatype IRI: {e}"));
                    Term::Literal(Literal::new_typed_literal(lex, dt))
                } else {
                    // Plain xsd:string
                    Term::Literal(Literal::new_simple_literal(lex))
                }
            } else {
                let obj_iri = entry["object"].as_str().expect("object IRI");
                Term::NamedNode(
                    NamedNode::new(obj_iri)
                        .unwrap_or_else(|e| panic!("{id}: invalid object IRI: {e}")),
                )
            };

            let got = mint_reifier(&s, &p, &o);
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
