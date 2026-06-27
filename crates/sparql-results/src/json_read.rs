// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! SPARQL Results **JSON** (SRJ) reader — the inverse of [`crate::json`].
//!
//! Parses a W3C SPARQL 1.1 Query Results JSON document
//! (<https://www.w3.org/TR/sparql11-results-json/>) into a [`ParsedSolutions`]
//! (for `SELECT`) or a boolean (for `ASK`). This is what SPARQL `SERVICE`
//! federation (S6b #928) uses to ingest a remote endpoint's response, and what
//! the W3C conformance harness uses to read expected `.srj` results.
//!
//! # Wasm discipline
//!
//! A hand-rolled recursive-descent parser over `&[u8]` — **no `serde`, no
//! `std::io`** — symmetric with the hand-rolled writers and keeping the crate
//! wasm-clean and oxigraph-free.

use gmeow_rdf_core::{BlankScope, RdfTextDirection, TermValue};

use crate::error::Error;

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const RDF_LANGSTRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";
const RDF_DIR_LANGSTRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString";

/// A decoded `SELECT` result set: ordered variable names plus dense rows. A
/// `None` cell is an unbound (absent) binding for that variable in that row.
///
/// This is the structural inverse of [`gmeow_rdf_core::SparqlResult::Solutions`]
/// and the shape the SERVICE evaluator interns into a solution sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSolutions {
    /// The result variables, in `head.vars` order.
    pub variables: Vec<String>,
    /// One row per binding; `rows[i][j]` is the value of `variables[j]`.
    pub rows: Vec<Vec<Option<TermValue>>>,
}

/// Parse a SPARQL Results JSON `SELECT` document into [`ParsedSolutions`].
///
/// # Errors
///
/// Returns [`Error::Format`] on malformed JSON, a non-object document, an `ASK`
/// (`boolean`) document (use [`from_json_boolean`]), or a binding object whose
/// `type`/`value` shape is invalid.
pub fn from_json(bytes: &[u8]) -> Result<ParsedSolutions, Error> {
    let doc = JsonParser::new(bytes).parse_document()?;
    let obj = doc
        .as_object()
        .ok_or_else(|| fmt("top level is not an object"))?;
    if obj_get(obj, "boolean").is_some() {
        return Err(fmt(
            "expected SELECT results, got an ASK (boolean) document",
        ));
    }
    let head = obj_get(obj, "head")
        .and_then(Json::as_object)
        .ok_or_else(|| fmt("missing `head` object"))?;
    let variables = match obj_get(head, "vars") {
        Some(Json::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| fmt("`head.vars` entry is not a string"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        // A results doc with no `vars` is degenerate but valid (zero columns).
        _ => Vec::new(),
    };

    let results = obj_get(obj, "results")
        .and_then(Json::as_object)
        .ok_or_else(|| fmt("missing `results` object"))?;
    let bindings = match obj_get(results, "bindings") {
        Some(Json::Array(items)) => items.as_slice(),
        _ => return Err(fmt("missing `results.bindings` array")),
    };

    let mut rows = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let row_obj = binding
            .as_object()
            .ok_or_else(|| fmt("`results.bindings` entry is not an object"))?;
        let mut row = vec![None; variables.len()];
        for (j, var) in variables.iter().enumerate() {
            if let Some(cell) = obj_get(row_obj, var) {
                row[j] = Some(decode_binding(cell)?);
            }
        }
        rows.push(row);
    }
    Ok(ParsedSolutions { variables, rows })
}

/// Parse a SPARQL Results JSON `ASK` document into its boolean.
///
/// # Errors
///
/// Returns [`Error::Format`] on malformed JSON or a document without a boolean
/// `boolean` field.
pub fn from_json_boolean(bytes: &[u8]) -> Result<bool, Error> {
    let doc = JsonParser::new(bytes).parse_document()?;
    let obj = doc
        .as_object()
        .ok_or_else(|| fmt("top level is not an object"))?;
    match obj_get(obj, "boolean") {
        Some(Json::Bool(b)) => Ok(*b),
        _ => Err(fmt("missing boolean `boolean` field")),
    }
}

/// Decode one SPARQL-JSON binding object into a [`TermValue`] (recursive for
/// RDF 1.2 triple terms).
fn decode_binding(value: &Json) -> Result<TermValue, Error> {
    let obj = value
        .as_object()
        .ok_or_else(|| fmt("binding is not an object"))?;
    let ty = obj_get(obj, "type")
        .and_then(Json::as_str)
        .ok_or_else(|| fmt("binding has no string `type`"))?;
    match ty {
        "uri" => {
            let v = binding_value(obj)?;
            Ok(TermValue::Iri(v.to_owned()))
        }
        "bnode" => {
            let v = binding_value(obj)?;
            Ok(TermValue::Blank {
                label: v.to_owned(),
                scope: BlankScope::DEFAULT,
            })
        }
        "literal" | "typed-literal" => {
            let v = binding_value(obj)?;
            let language = obj_get(obj, "xml:lang").and_then(Json::as_str);
            let direction = match obj_get(obj, "dir").and_then(Json::as_str) {
                Some("ltr") => Some(RdfTextDirection::Ltr),
                Some("rtl") => Some(RdfTextDirection::Rtl),
                Some(other) => return Err(fmt(&format!("unknown base direction `{other}`"))),
                None => None,
            };
            let datatype = obj_get(obj, "datatype").and_then(Json::as_str);
            let datatype = resolve_datatype(datatype, language.is_some(), direction.is_some());
            Ok(TermValue::Literal {
                lexical_form: v.to_owned(),
                datatype,
                language: language.map(str::to_owned),
                direction,
            })
        }
        "triple" => {
            let inner = obj_get(obj, "value")
                .and_then(Json::as_object)
                .ok_or_else(|| fmt("triple binding has no object `value`"))?;
            let s = decode_binding(
                obj_get(inner, "subject").ok_or_else(|| fmt("triple has no subject"))?,
            )?;
            let p = decode_binding(
                obj_get(inner, "predicate").ok_or_else(|| fmt("triple has no predicate"))?,
            )?;
            let o = decode_binding(
                obj_get(inner, "object").ok_or_else(|| fmt("triple has no object"))?,
            )?;
            if !matches!(p, TermValue::Iri(_)) {
                return Err(fmt("triple-term predicate is not an IRI"));
            }
            Ok(TermValue::Triple {
                s: Box::new(s),
                p: Box::new(p),
                o: Box::new(o),
            })
        }
        other => Err(fmt(&format!("unknown binding type `{other}`"))),
    }
}

/// Read the required string `value` field of a binding object.
fn binding_value(obj: &[(String, Json)]) -> Result<&str, Error> {
    obj_get(obj, "value")
        .and_then(Json::as_str)
        .ok_or_else(|| fmt("binding has no string `value`"))
}

/// Resolve a literal's datatype: an explicit `datatype` wins; otherwise a
/// language-tagged literal is `rdf:langString` (or `rdf:dirLangString` with a
/// base direction), and a plain literal is `xsd:string`.
fn resolve_datatype(datatype: Option<&str>, has_lang: bool, has_dir: bool) -> String {
    match datatype {
        Some(dt) => dt.to_owned(),
        None if has_lang && has_dir => RDF_DIR_LANGSTRING.to_owned(),
        None if has_lang => RDF_LANGSTRING.to_owned(),
        None => XSD_STRING.to_owned(),
    }
}

/// Build a `Format` error.
fn fmt(msg: &str) -> Error {
    Error::Format(format!("SPARQL-JSON: {msg}"))
}

/// Look up a key in an object's `(key, value)` pairs (first match).
fn obj_get<'a>(obj: &'a [(String, Json)], key: &str) -> Option<&'a Json> {
    obj.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// A minimal JSON value (numbers retained as their lexical form — SPARQL-JSON
/// never needs them numerically).
#[derive(Debug, Clone, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }
    fn as_object(&self) -> Option<&[(String, Json)]> {
        match self {
            Json::Object(o) => Some(o),
            _ => None,
        }
    }
}

/// A hand-rolled recursive-descent JSON parser over `&[u8]`.
struct JsonParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> JsonParser<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    /// Parse the whole document; trailing non-whitespace is an error.
    fn parse_document(&mut self) -> Result<Json, Error> {
        self.skip_ws();
        let value = self.parse_value()?;
        self.skip_ws();
        if self.pos != self.bytes.len() {
            return Err(fmt("trailing data after JSON value"));
        }
        Ok(value)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if matches!(c, b' ' | b'\t' | b'\n' | b'\r') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn parse_value(&mut self) -> Result<Json, Error> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => Ok(Json::String(self.parse_string()?)),
            Some(b't') => self.parse_lit("true", Json::Bool(true)),
            Some(b'f') => self.parse_lit("false", Json::Bool(false)),
            Some(b'n') => self.parse_lit("null", Json::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.parse_number(),
            _ => Err(fmt("unexpected token while parsing a value")),
        }
    }

    fn parse_lit(&mut self, word: &str, value: Json) -> Result<Json, Error> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(fmt(&format!("expected `{word}`")))
        }
    }

    fn parse_number(&mut self) -> Result<Json, Error> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.' | b'e' | b'E') {
                self.pos += 1;
            } else {
                break;
            }
        }
        let raw = core::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| fmt("non-UTF-8 number"))?;
        Ok(Json::Number(raw.to_owned()))
    }

    fn parse_array(&mut self) -> Result<Json, Error> {
        self.pos += 1; // consume '['
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(fmt("expected `,` or `]` in array")),
            }
        }
        Ok(Json::Array(items))
    }

    fn parse_object(&mut self) -> Result<Json, Error> {
        self.pos += 1; // consume '{'
        let mut entries = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Object(entries));
        }
        loop {
            self.skip_ws();
            if self.peek() != Some(b'"') {
                return Err(fmt("expected string key in object"));
            }
            let key = self.parse_string()?;
            self.skip_ws();
            if self.peek() != Some(b':') {
                return Err(fmt("expected `:` after object key"));
            }
            self.pos += 1;
            let value = self.parse_value()?;
            entries.push((key, value));
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(fmt("expected `,` or `}` in object")),
            }
        }
        Ok(Json::Object(entries))
    }

    fn parse_string(&mut self) -> Result<String, Error> {
        self.pos += 1; // consume opening '"'
        let mut s = String::new();
        loop {
            let Some(c) = self.peek() else {
                return Err(fmt("unterminated string"));
            };
            match c {
                b'"' => {
                    self.pos += 1;
                    return Ok(s);
                }
                b'\\' => {
                    self.pos += 1;
                    let Some(esc) = self.peek() else {
                        return Err(fmt("unterminated escape"));
                    };
                    self.pos += 1;
                    match esc {
                        b'"' => s.push('"'),
                        b'\\' => s.push('\\'),
                        b'/' => s.push('/'),
                        b'b' => s.push('\u{0008}'),
                        b'f' => s.push('\u{000C}'),
                        b'n' => s.push('\n'),
                        b'r' => s.push('\r'),
                        b't' => s.push('\t'),
                        b'u' => s.push(self.parse_unicode_escape()?),
                        other => {
                            return Err(fmt(&format!("bad escape \\{}", other as char)));
                        }
                    }
                }
                // A raw multibyte UTF-8 sequence: copy the whole code point.
                _ => {
                    let ch = self.next_utf8_char()?;
                    s.push(ch);
                }
            }
        }
    }

    /// Decode a `\uXXXX` escape (with surrogate-pair handling), positioned just
    /// after the `u`.
    fn parse_unicode_escape(&mut self) -> Result<char, Error> {
        let hi = self.read_hex4()?;
        if (0xD800..=0xDBFF).contains(&hi) {
            // High surrogate: expect a following `\uXXXX` low surrogate.
            if self.peek() != Some(b'\\') {
                return Err(fmt("lone high surrogate"));
            }
            self.pos += 1;
            if self.peek() != Some(b'u') {
                return Err(fmt("lone high surrogate"));
            }
            self.pos += 1;
            let lo = self.read_hex4()?;
            if !(0xDC00..=0xDFFF).contains(&lo) {
                return Err(fmt("invalid low surrogate"));
            }
            let c = 0x1_0000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
            char::from_u32(c).ok_or_else(|| fmt("invalid surrogate pair"))
        } else {
            char::from_u32(hi).ok_or_else(|| fmt("invalid \\u escape"))
        }
    }

    fn read_hex4(&mut self) -> Result<u32, Error> {
        if self.pos + 4 > self.bytes.len() {
            return Err(fmt("truncated \\u escape"));
        }
        let mut value: u32 = 0;
        for _ in 0..4 {
            let c = self.bytes[self.pos];
            let digit = match c {
                b'0'..=b'9' => u32::from(c - b'0'),
                b'a'..=b'f' => u32::from(c - b'a' + 10),
                b'A'..=b'F' => u32::from(c - b'A' + 10),
                _ => return Err(fmt("non-hex digit in \\u escape")),
            };
            value = value * 16 + digit;
            self.pos += 1;
        }
        Ok(value)
    }

    /// Consume one UTF-8 code point starting at `pos`.
    fn next_utf8_char(&mut self) -> Result<char, Error> {
        let rest = core::str::from_utf8(&self.bytes[self.pos..])
            .map_err(|_| fmt("invalid UTF-8 in string"))?;
        let ch = rest
            .chars()
            .next()
            .ok_or_else(|| fmt("unterminated string"))?;
        self.pos += ch.len_utf8();
        Ok(ch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_select_with_mixed_terms() {
        let srj = r#"{
          "head": { "vars": [ "s", "name", "label", "age" ] },
          "results": { "bindings": [
            {
              "s": { "type": "uri", "value": "http://example.org/s" },
              "name": { "type": "literal", "value": "Ada" },
              "label": { "type": "literal", "value": "bonjour", "xml:lang": "fr" },
              "age": { "type": "literal", "value": "42",
                       "datatype": "http://www.w3.org/2001/XMLSchema#integer" }
            },
            {
              "s": { "type": "bnode", "value": "b0" }
            }
          ] }
        }"#;
        let parsed = from_json(srj.as_bytes()).expect("parse");
        assert_eq!(parsed.variables, vec!["s", "name", "label", "age"]);
        assert_eq!(parsed.rows.len(), 2);
        assert_eq!(
            parsed.rows[0][0],
            Some(TermValue::Iri("http://example.org/s".to_owned()))
        );
        assert_eq!(
            parsed.rows[0][1],
            Some(TermValue::Literal {
                lexical_form: "Ada".to_owned(),
                datatype: XSD_STRING.to_owned(),
                language: None,
                direction: None,
            })
        );
        assert_eq!(
            parsed.rows[0][2],
            Some(TermValue::Literal {
                lexical_form: "bonjour".to_owned(),
                datatype: RDF_LANGSTRING.to_owned(),
                language: Some("fr".to_owned()),
                direction: None,
            })
        );
        assert_eq!(
            parsed.rows[0][3],
            Some(TermValue::Literal {
                lexical_form: "42".to_owned(),
                datatype: "http://www.w3.org/2001/XMLSchema#integer".to_owned(),
                language: None,
                direction: None,
            })
        );
        // Second row: only `s` bound (a bnode), the rest unbound.
        assert_eq!(
            parsed.rows[1][0],
            Some(TermValue::Blank {
                label: "b0".to_owned(),
                scope: BlankScope::DEFAULT,
            })
        );
        assert_eq!(parsed.rows[1][1], None);
        assert_eq!(parsed.rows[1][3], None);
    }

    #[test]
    fn reads_directional_literal() {
        let srj = r#"{"head":{"vars":["x"]},"results":{"bindings":[
          {"x":{"type":"literal","value":"שלום","xml:lang":"he","dir":"rtl"}}]}}"#;
        let parsed = from_json(srj.as_bytes()).expect("parse");
        assert_eq!(
            parsed.rows[0][0],
            Some(TermValue::Literal {
                lexical_form: "שלום".to_owned(),
                datatype: RDF_DIR_LANGSTRING.to_owned(),
                language: Some("he".to_owned()),
                direction: Some(RdfTextDirection::Rtl),
            })
        );
    }

    #[test]
    fn reads_triple_term() {
        let srj = r#"{"head":{"vars":["t"]},"results":{"bindings":[
          {"t":{"type":"triple","value":{
            "subject":{"type":"uri","value":"http://ex/s"},
            "predicate":{"type":"uri","value":"http://ex/p"},
            "object":{"type":"uri","value":"http://ex/o"}}}}]}}"#;
        let parsed = from_json(srj.as_bytes()).expect("parse");
        assert_eq!(
            parsed.rows[0][0],
            Some(TermValue::Triple {
                s: Box::new(TermValue::Iri("http://ex/s".to_owned())),
                p: Box::new(TermValue::Iri("http://ex/p".to_owned())),
                o: Box::new(TermValue::Iri("http://ex/o".to_owned())),
            })
        );
    }

    #[test]
    fn reads_ask_boolean() {
        assert!(from_json_boolean(br#"{"head":{},"boolean":true}"#).expect("ask"));
        assert!(!from_json_boolean(br#"{"head":{},"boolean":false}"#).expect("ask"));
    }

    #[test]
    fn select_reader_rejects_ask_document() {
        let err = from_json(br#"{"head":{},"boolean":true}"#).unwrap_err();
        assert!(matches!(err, Error::Format(_)));
    }

    #[test]
    fn handles_escapes_and_unicode() {
        let srj = r#"{"head":{"vars":["x"]},"results":{"bindings":[
          {"x":{"type":"literal","value":"a\"b\\c\nA😀"}}]}}"#;
        let parsed = from_json(srj.as_bytes()).expect("parse");
        let TermValue::Literal { lexical_form, .. } = parsed.rows[0][0].clone().unwrap() else {
            panic!("expected literal");
        };
        assert_eq!(lexical_form, "a\"b\\c\nA😀");
    }

    #[test]
    fn rejects_trailing_garbage() {
        assert!(from_json(br#"{"head":{"vars":[]},"results":{"bindings":[]}} oops"#).is_err());
    }
}
