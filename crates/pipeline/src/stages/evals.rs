// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `evals` export leaf (P4): scorecard + leaderboard + scores.ttl.
//!
//! A genuine port of the render half of `src/gmeow_tools/evals.py` (the
//! claim-extraction eval suite). The generator reads NON-fold inputs —
//! the emission schema (`evals/claim-emission.schema.json`), the corpus manifest
//! (`evals/corpus.ttl`), the ground-truth expectations (`evals/expectations.json`),
//! and the recorded model emissions (`evals/outputs/<model>/claims.jsonl`) — and
//! mechanically scores each model against the published contract, with ZERO
//! human judgment. The scores are themselves GMEOW claims (vantage-indexed
//! `gmeow:Assessment` individuals), dogfooded.
//!
//! The committed outputs are (`GENERATED_EVALS_DIR` = `generated/evals`):
//!   * `leaderboard.md` — the ranked Markdown leaderboard,
//!   * `scores.ttl` — the meta-claim Assessments (RDF/Turtle), and
//!   * `<model>.scorecard.json` — one JSON scorecard per model.
//!
//! The `EvalsGenerator` does NOT override `compare`, so all three outputs go
//! through the default BYTE comparator: every artifact is rendered
//! byte-identically (the Turtle is raw text the generator writes, not a graph
//! serialization, so it too is byte-pinned — the parity test also confirms
//! `scores.ttl` is graph-isomorphic to the committed file as a belt-and-braces
//! check). The network half (`gmeow evals run`) is gated and not part of the
//! build, so it is not ported here.

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_errors::abox::{AboxObject, X_GMEOW_ENGLISH, abox_annotations};
use gmeow_errors::render::nq_escape;
use purrdf::slice::rdf_query::{Dataset, Object, Subject};
use serde_json::Value;

use crate::node::{Stage, StageInput, StageOutput, StageProduct};

/// Logical path of the generated leaderboard.
pub const LEADERBOARD_PATH: &str = "generated/evals/leaderboard.md";
/// Logical path of the generated meta-claim scores.
pub const SCORES_PATH: &str = "generated/evals/scores.ttl";

const GM: &str = "https://blackcatinformatics.ca/gmeow/";
/// The `ev:` prefix expansion this generator's Turtle declares
/// (`@prefix ev: <...> .` in [`render_scores_ttl`]).
const EV: &str = "https://blackcatinformatics.ca/gmeow/evals/";

/// Criterion local names, in leaderboard column order (`_CRITERIA`).
const CRITERIA: &[&str] = &[
    "schema-validity",
    "grounding-precision",
    "grounding-recall",
    "hallucination-resistance",
    "abstention-quality",
    "calibration",
];

/// Map a criterion key to its `ev:crit-*` local name (the Turtle index).
fn crit_local(criterion: &str) -> &'static str {
    match criterion {
        "schema-validity" => "crit-schema-validity",
        "grounding-precision" => "crit-grounding-precision",
        "grounding-recall" => "crit-grounding-recall",
        "hallucination-resistance" => "crit-hallucination",
        "abstention-quality" => "crit-abstention",
        "calibration" => "crit-calibration",
        other => unreachable!("unknown criterion {other}"),
    }
}

// ── Model id slug (`_slug`) ────────────────────────────────────────────────────

/// Path- and IRI-safe key for a model id (`_slug`): an already-safe id is its
/// own slug; otherwise a lossy sanitization gains a blake2s content-hash suffix.
fn slug(model: &str) -> String {
    let sanitized: String = {
        let mut s = String::new();
        let mut prev_dash = false;
        for ch in model.chars() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                s.push(ch);
                prev_dash = false;
            } else if !prev_dash {
                s.push('-');
                prev_dash = true;
            }
        }
        s.trim_matches('-').to_string()
    };
    if sanitized == model && !model.is_empty() {
        return model.to_string();
    }
    let base = if sanitized.is_empty() {
        "model".to_string()
    } else {
        sanitized.to_lowercase()
    };
    // _slug fallback: lossy sanitization + a blake2s content-hash of the raw id.
    let hash = blake2s::hex(model.as_bytes(), 4);
    format!("{base}-{hash}")
}

mod blake2s {
    //! Self-contained BLAKE2s (RFC 7693) — only the unkeyed, single-shot digest
    //! the `_slug` fallback needs. Kept local to avoid a new crypto dependency.
    const IV: [u32; 8] = [
        0x6A09_E667,
        0xBB67_AE85,
        0x3C6E_F372,
        0xA54F_F53A,
        0x510E_527F,
        0x9B05_688C,
        0x1F83_D9AB,
        0x5BE0_CD19,
    ];
    const SIGMA: [[usize; 16]; 10] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
        [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
        [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
        [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
        [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
        [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
        [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
        [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
        [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
    ];

    fn g(v: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize, x: u32, y: u32) {
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
        v[d] = (v[d] ^ v[a]).rotate_right(16);
        v[c] = v[c].wrapping_add(v[d]);
        v[b] = (v[b] ^ v[c]).rotate_right(12);
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
        v[d] = (v[d] ^ v[a]).rotate_right(8);
        v[c] = v[c].wrapping_add(v[d]);
        v[b] = (v[b] ^ v[c]).rotate_right(7);
    }

    fn compress(h: &mut [u32; 8], block: &[u8; 64], t: u64, last: bool) {
        let mut m = [0u32; 16];
        for (i, word) in m.iter_mut().enumerate() {
            *word = u32::from_le_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        let mut v = [0u32; 16];
        v[..8].copy_from_slice(&h[..]);
        v[8..].copy_from_slice(&IV);
        v[12] ^= (t & 0xFFFF_FFFF) as u32;
        v[13] ^= (t >> 32) as u32;
        if last {
            v[14] = !v[14];
        }
        for round in &SIGMA {
            g(&mut v, 0, 4, 8, 12, m[round[0]], m[round[1]]);
            g(&mut v, 1, 5, 9, 13, m[round[2]], m[round[3]]);
            g(&mut v, 2, 6, 10, 14, m[round[4]], m[round[5]]);
            g(&mut v, 3, 7, 11, 15, m[round[6]], m[round[7]]);
            g(&mut v, 0, 5, 10, 15, m[round[8]], m[round[9]]);
            g(&mut v, 1, 6, 11, 12, m[round[10]], m[round[11]]);
            g(&mut v, 2, 7, 8, 13, m[round[12]], m[round[13]]);
            g(&mut v, 3, 4, 9, 14, m[round[14]], m[round[15]]);
        }
        for i in 0..8 {
            h[i] ^= v[i] ^ v[i + 8];
        }
    }

    /// Unkeyed BLAKE2s digest of `data` truncated to `out_len` bytes, hex-encoded.
    pub fn hex(data: &[u8], out_len: usize) -> String {
        let mut h = IV;
        h[0] ^= 0x0101_0000 ^ (out_len as u32);
        let mut t: u64 = 0;
        let mut i = 0;
        while data.len() - i > 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[i..i + 64]);
            t = t.wrapping_add(64);
            compress(&mut h, &block, t, false);
            i += 64;
        }
        let rem = &data[i..];
        let mut block = [0u8; 64];
        block[..rem.len()].copy_from_slice(rem);
        t = t.wrapping_add(rem.len() as u64);
        compress(&mut h, &block, t, true);
        let mut out = String::new();
        for word in &h {
            for byte in word.to_le_bytes() {
                out.push_str(&format!("{byte:02x}"));
            }
        }
        out.truncate(out_len * 2);
        out
    }
}

// ── Scorecard ──────────────────────────────────────────────────────────────────

/// One model's mechanical scores, all in `[0, 1]` (`Scorecard`).
struct Scorecard {
    model: String,
    emitted: usize,
    valid: usize,
    /// Criterion → score, insertion-ordered exactly like the Python dict
    /// (schema-validity is set first, then the others in scoring order).
    scores: Vec<(String, f64)>,
    notes: Vec<String>,
}

impl Scorecard {
    fn score(&self, criterion: &str) -> f64 {
        self.scores
            .iter()
            .find(|(k, _)| k == criterion)
            .map(|(_, v)| *v)
            .unwrap_or(0.0)
    }

    /// Unweighted mean across the rubric criteria (`overall`).
    fn overall(&self) -> f64 {
        if self.scores.is_empty() {
            return 0.0;
        }
        self.scores.iter().map(|(_, v)| *v).sum::<f64>() / self.scores.len() as f64
    }
}

/// Python `round(x, 4)` — round-half-to-even at 4 decimal places.
fn round4(x: f64) -> f64 {
    round_py(x, 4)
}

/// CPython `round(x, ndigits)`: correctly-rounded decimal rounding, ties-to-even.
fn round_py(x: f64, ndigits: i32) -> f64 {
    if !x.is_finite() {
        return x;
    }
    // Match CPython's _Py_dg_dtoa-based rounding via a formatted decimal: format
    // to ndigits with round-half-to-even, then parse back. Rust's formatter
    // rounds half-to-even, matching CPython's behaviour for these values.
    let s = format!("{x:.*}", ndigits as usize);
    s.parse::<f64>().unwrap_or(x)
}

/// Python `repr(float)` for a non-integral-looking value: shortest round-trip
/// representation, always with a fractional part (`1.0`, `0.6`, `0.8333`).
/// Rust's `{:?}` (Debug) for f64 matches CPython `repr` for the values here.
fn py_float(x: f64) -> String {
    format!("{x:?}")
}

// ── Corpus + digest ────────────────────────────────────────────────────────────

/// `_corpus_texts`: sourceLocation → (text, ONE declared digest) from the corpus
/// manifest. rdflib's `graph.value` returns an arbitrary single `contentDigest`;
/// we collect every declared digest and treat the source as digest-current iff
/// the freshly-computed blake3 equals ANY declared one (an md5/sha256 declared
/// digest can never equal the blake3 we compute, so this faithfully reproduces
/// the Python outcome regardless of which single digest `value` happened to pick).
struct CorpusDoc {
    text: String,
    declared_digests: Vec<String>,
}

/// Render an object term the way oxigraph's `Term::to_string()` did for a
/// non-literal location object (`<iri>` / `_:label`) — literals surface their
/// lexical value directly. Faithfully reproduces the prior `other.to_string()`.
fn object_location(object: &Object) -> String {
    match object {
        Object::Literal { value, .. } => value.clone(),
        Object::Named(iri) => format!("<{iri}>"),
        Object::Blank(label) => format!("_:{label}"),
        Object::Triple(_) => String::new(),
    }
}

fn parse_corpus(
    path: &Path,
) -> Result<BTreeMap<String, (String, Vec<String>)>, gmeow_errors::Diag> {
    let bytes = std::fs::read(path)?;
    let dataset = Dataset::parse_turtle(&bytes, None, "corpus").map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: e.to_string(),
        })
    })?;
    let source_location = format!("{GM}sourceLocation");
    let content_digest = format!("{GM}contentDigest");

    // subjects with a sourceLocation → location string. Collect the (subject, object)
    // pairs first (any subject kind, named or blank) — the native query surfaces them
    // through `for_each_quad`, the dataset-order scan oxigraph's pattern scan provided.
    let mut rows: Vec<(Subject, Object)> = Vec::new();
    dataset.for_each_quad(|subject, predicate, object, _graph| {
        if predicate == source_location {
            rows.push((subject, object));
        }
    });

    let mut out: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    for (subject, object) in rows {
        let location = object_location(&object);
        // All declared digests for this subject (literal objects only).
        let mut digests: Vec<String> = Vec::new();
        for dq in dataset
            .objects_of_subject(&subject, &content_digest)
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: e.to_string(),
                })
            })?
        {
            if let Object::Literal { value, .. } = dq {
                digests.push(value);
            }
        }
        out.insert(location, (String::new(), digests));
    }
    Ok(out)
}

/// Resolve corpus texts: read each source file under `root` (`_corpus_texts`).
fn corpus_texts(root: &Path) -> Result<BTreeMap<String, CorpusDoc>, gmeow_errors::Diag> {
    let manifest = parse_corpus(&root.join("evals").join("corpus.ttl"))?;
    let mut out: BTreeMap<String, CorpusDoc> = BTreeMap::new();
    for (location, (_t, declared_digests)) in manifest {
        let text = std::fs::read_to_string(root.join(&location))?;
        out.insert(
            location,
            CorpusDoc {
                text,
                declared_digests,
            },
        );
    }
    Ok(out)
}

/// `_current_digest`: `"blake3:" + blake3(text).hexdigest()`.
fn current_digest(text: &str) -> String {
    let hash = blake3::hash(text.as_bytes());
    format!("blake3:{}", hash.to_hex())
}

/// `_span_verified`: quote re-anchors; when the digest is current, offsets bind.
fn span_verified(span: &Value, text: &str, digest_current: bool) -> bool {
    let quote = span.get("quote").and_then(Value::as_str).unwrap_or("");
    if !text.contains(quote) {
        return false;
    }
    if digest_current {
        // Python uses int(str(...)) — the schema constrains these to integers.
        let start = span.get("start").and_then(Value::as_i64).unwrap_or(-1);
        let end = span.get("end").and_then(Value::as_i64).unwrap_or(-1);
        if !(0 <= start && start < end) {
            return false;
        }
        // Python offsets are Unicode code-point offsets into the source text.
        let chars: Vec<char> = text.chars().collect();
        if end as usize > chars.len() {
            return false;
        }
        let sliced: String = chars[start as usize..end as usize].iter().collect();
        if sliced != quote {
            return false;
        }
    }
    true
}

// ── Schema validation (Draft 2020-12, the committed claim-emission schema) ──────

/// The subset of JSON-Schema Draft 2020-12 the emission contract uses, with
/// jsonschema-compatible error messages and `best_match`-compatible first-error
/// selection. Returns `Ok(())` for a valid instance or `Err(message)` with the
/// SAME first-line wording jsonschema's `validate()` raises (so the scorecard
/// `notes` are byte-identical).
struct Schema {
    raw: Value,
}

impl Schema {
    fn load(path: &Path) -> Result<Self, gmeow_errors::Diag> {
        let bytes = std::fs::read(path)?;
        let raw: Value = serde_json::from_slice(&bytes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("schema parse: {e}"),
            })
        })?;
        Ok(Self { raw })
    }

    /// Validate `instance`, returning the first error message (jsonschema order).
    /// On failure the diagnostic carries the jsonschema message VERBATIM (the
    /// [`crate::error::EvalSchema`] kind's `Display` is the bare message), so the
    /// scorecard `notes` stay byte-identical to the reference validator.
    fn validate(&self, instance: &Value) -> gmeow_errors::Result<()> {
        match validate_node(&self.raw, instance) {
            None => Ok(()),
            Some(err) => Err(gmeow_errors::Diag::of_kind(crate::error::EvalSchema {
                message: err,
            })),
        }
    }
}

/// jsonschema `best_match` prefers the error from the deepest/most-specific
/// schema location. We mirror the observed precedence at one node:
///   1. `type` mismatch (the instance is the wrong shape entirely),
///   2. `required` (a property is missing) — reported before keyword failures
///      on present properties,
///   3. `additionalProperties` (an unexpected property),
///   4. per-property keyword failures (recursing, deepest error wins),
///   5. array `items` failures (recursing).
///
/// This reproduces the committed corpus and the documented jsonschema ordering.
fn validate_node(schema: &Value, instance: &Value) -> Option<String> {
    let obj = schema.as_object()?;

    // 1. type
    if let Some(ty) = obj.get("type").and_then(Value::as_str)
        && !type_matches(ty, instance)
    {
        return Some(format!(
            "{} is not of type {}",
            json_repr(instance),
            quote_single(ty)
        ));
    }

    // const / enum (scalars)
    if let Some(expected) = obj.get("const")
        && instance != expected
    {
        return Some(format!("{} was expected", json_repr(expected)));
    }
    if let Some(Value::Array(choices)) = obj.get("enum")
        && !choices.iter().any(|c| c == instance)
    {
        let rendered: Vec<String> = choices.iter().map(json_repr).collect();
        return Some(format!(
            "{} is not one of [{}]",
            json_repr(instance),
            rendered.join(", ")
        ));
    }

    // number bounds
    if let Some(n) = instance.as_f64() {
        if let Some(min) = obj.get("minimum").and_then(Value::as_f64)
            && n < min
        {
            return Some(format!(
                "{} is less than the minimum of {}",
                json_repr(instance),
                py_num(min)
            ));
        }
        if let Some(max) = obj.get("maximum").and_then(Value::as_f64)
            && n > max
        {
            return Some(format!(
                "{} is greater than the maximum of {}",
                json_repr(instance),
                py_num(max)
            ));
        }
    }

    // string length
    if let Some(s) = instance.as_str()
        && let Some(min_len) = obj.get("minLength").and_then(Value::as_u64)
        && (s.chars().count() as u64) < min_len
    {
        return Some(format!("{} is too short", json_repr(instance)));
    }

    if let Some(map) = instance.as_object() {
        // 2. required (declaration order)
        if let Some(Value::Array(required)) = obj.get("required") {
            for r in required {
                if let Some(name) = r.as_str()
                    && !map.contains_key(name)
                {
                    return Some(format!("{} is a required property", quote_single(name)));
                }
            }
        }
        // 3. additionalProperties: false
        if obj.get("additionalProperties") == Some(&Value::Bool(false)) {
            let declared = obj.get("properties").and_then(Value::as_object);
            for key in map.keys() {
                let known = declared.map(|d| d.contains_key(key)).unwrap_or(false);
                if !known {
                    return Some(format!(
                        "Additional properties are not allowed ({} was unexpected)",
                        quote_single(key)
                    ));
                }
            }
        }
        // 4. per-property recursion
        if let Some(props) = obj.get("properties").and_then(Value::as_object) {
            for (key, subschema) in props {
                if let Some(value) = map.get(key)
                    && let Some(err) = validate_node(subschema, value)
                {
                    return Some(err);
                }
            }
        }
    }

    // 5. array items recursion
    if let Some(arr) = instance.as_array()
        && let Some(items) = obj.get("items")
    {
        for value in arr {
            if let Some(err) = validate_node(items, value) {
                return Some(err);
            }
        }
    }

    None
}

fn type_matches(ty: &str, instance: &Value) -> bool {
    match ty {
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "string" => instance.is_string(),
        "number" => instance.is_number(),
        "integer" => instance.is_i64() || instance.is_u64() || is_integral_float(instance),
        "boolean" => instance.is_boolean(),
        "null" => instance.is_null(),
        _ => true,
    }
}

fn is_integral_float(instance: &Value) -> bool {
    instance
        .as_f64()
        .map(|f| f.fract() == 0.0 && f.is_finite())
        .unwrap_or(false)
}

/// jsonschema renders a missing/expected key with single quotes (Python repr).
fn quote_single(s: &str) -> String {
    format!("'{s}'")
}

/// jsonschema renders an instance value with Python `repr` semantics: strings
/// single-quoted, numbers/bools/null bare.
fn json_repr(value: &Value) -> String {
    match value {
        Value::String(s) => format!("'{s}'"),
        Value::Bool(b) => {
            if *b {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Value::Null => "None".to_string(),
        Value::Number(n) => py_num(n.as_f64().unwrap_or(0.0)),
        other => other.to_string(),
    }
}

/// Render a number the way Python `repr` does inside a jsonschema message
/// (`1.5`, `1`, `0`). Integral values print without a decimal point.
fn py_num(n: f64) -> String {
    if n.fract() == 0.0 && n.is_finite() && n.abs() < 1e16 {
        format!("{}", n as i64)
    } else {
        format!("{n:?}")
    }
}

// ── Scoring (`score_emissions`) ─────────────────────────────────────────────────

fn score_emissions(
    model: &str,
    jsonl: &str,
    schema: &Schema,
    corpus: &BTreeMap<String, CorpusDoc>,
    expectations: &Value,
) -> Scorecard {
    let mut emitted = 0usize;
    let mut valid = 0usize;
    let mut notes: Vec<String> = Vec::new();
    let mut claims: Vec<Value> = Vec::new();

    for line in jsonl.split('\n') {
        if line.trim().is_empty() {
            continue;
        }
        emitted += 1;
        let obj: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                notes.push(invalid_note(&e.to_string()));
                continue;
            }
        };
        if let Err(diag) = schema.validate(&obj) {
            notes.push(invalid_note(&diag.to_string()));
            continue;
        }
        valid += 1;
        claims.push(obj);
    }

    let mut scores: Vec<(String, f64)> = Vec::new();
    let schema_validity = if emitted > 0 {
        valid as f64 / emitted as f64
    } else {
        0.0
    };
    scores.push(("schema-validity".to_string(), schema_validity));

    let mut grounded_flags: Vec<bool> = Vec::new();
    let mut verified_quotes: Vec<String> = Vec::new();
    for claim in &claims {
        let source = claim.get("source").and_then(Value::as_str).unwrap_or("");
        let (text, digest_current) = match corpus.get(source) {
            Some(doc) => {
                let dc = !doc.text.is_empty()
                    && doc
                        .declared_digests
                        .iter()
                        .any(|d| *d == current_digest(&doc.text));
                (doc.text.as_str(), dc)
            }
            None => ("", false),
        };
        let spans = claim
            .get("evidence")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut supporting_ok = false;
        for span in &spans {
            if span_verified(span, text, digest_current) {
                let quote = span.get("quote").and_then(Value::as_str).unwrap_or("");
                verified_quotes.push(quote.to_string());
                if span.get("polarity").and_then(Value::as_str) == Some("supports") {
                    supporting_ok = true;
                }
            }
        }
        grounded_flags.push(supporting_ok);
    }

    let n = claims.len();
    let grounded = grounded_flags.iter().filter(|b| **b).count();
    let grounding_precision = if n > 0 {
        grounded as f64 / n as f64
    } else {
        0.0
    };
    scores.push(("grounding-precision".to_string(), grounding_precision));
    let hallucination = if n > 0 {
        1.0 - (n - grounded) as f64 / n as f64
    } else {
        0.0
    };
    scores.push(("hallucination-resistance".to_string(), hallucination));

    let expected = expectations
        .get("expected")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let recovered = expected
        .iter()
        .filter(|item| {
            let must = item.get("must_quote").and_then(Value::as_str).unwrap_or("");
            verified_quotes.iter().any(|q| q.contains(must))
        })
        .count();
    let grounding_recall = if !expected.is_empty() {
        recovered as f64 / expected.len() as f64
    } else {
        1.0
    };
    scores.push(("grounding-recall".to_string(), grounding_recall));

    let bait = expectations
        .get("bait")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut taken = 0usize;
    for item in &bait {
        let keywords: Vec<String> = item
            .get("keywords")
            .and_then(Value::as_array)
            .map(|ks| {
                ks.iter()
                    .filter_map(Value::as_str)
                    .map(|k| k.to_lowercase())
                    .collect()
            })
            .unwrap_or_default();
        for claim in &claims {
            let text_lower = claim
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase();
            if keywords.iter().all(|k| text_lower.contains(k)) {
                taken += 1;
                break;
            }
        }
    }
    let abstention = if !bait.is_empty() {
        1.0 - (taken as f64 / bait.len() as f64)
    } else {
        1.0
    };
    scores.push(("abstention-quality".to_string(), abstention));

    let errors: Vec<f64> = claims
        .iter()
        .zip(grounded_flags.iter())
        .map(|(claim, ok)| {
            let conf = claim
                .get("confidence")
                .and_then(Value::as_f64)
                .unwrap_or(0.5);
            (conf - if *ok { 1.0 } else { 0.0 }).abs()
        })
        .collect();
    let calibration = if !errors.is_empty() {
        1.0 - errors.iter().sum::<f64>() / errors.len() as f64
    } else {
        1.0
    };
    scores.push(("calibration".to_string(), calibration));

    // round(score, 4) for every criterion.
    for (_, v) in scores.iter_mut() {
        *v = round4(*v);
    }

    Scorecard {
        model: model.to_string(),
        emitted,
        valid,
        scores,
        notes,
    }
}

/// `f"invalid line: {str(exc).splitlines()[0][:100]}"` — first line, ≤100 chars.
fn invalid_note(message: &str) -> String {
    let first = message.split('\n').next().unwrap_or(message);
    let truncated: String = first.chars().take(100).collect();
    format!("invalid line: {truncated}")
}

// ── Render (`all_scorecards`, `_render_*`) ──────────────────────────────────────

/// Score every committed emission, sorted by overall score descending then model
/// id ascending (`all_scorecards`).
fn all_scorecards(root: &Path) -> Result<Vec<Scorecard>, gmeow_errors::Diag> {
    let schema = Schema::load(&root.join("evals").join("claim-emission.schema.json"))?;
    let corpus = corpus_texts(root)?;
    let expectations_bytes = std::fs::read(root.join("evals").join("expectations.json"))?;
    let expectations: Value = serde_json::from_slice(&expectations_bytes).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("expectations parse: {e}"),
        })
    })?;

    // sorted(_OUTPUTS_DIR.glob("*/claims.jsonl")) → models in path order.
    let outputs_dir = root.join("evals").join("outputs");
    let mut model_dirs: Vec<String> = Vec::new();
    if outputs_dir.is_dir() {
        for entry in std::fs::read_dir(&outputs_dir)? {
            let entry = entry?;
            if entry.path().join("claims.jsonl").is_file() {
                model_dirs.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    model_dirs.sort();

    let mut cards: Vec<Scorecard> = Vec::new();
    for model in &model_dirs {
        let jsonl = std::fs::read_to_string(outputs_dir.join(model).join("claims.jsonl"))?;
        cards.push(score_emissions(
            model,
            &jsonl,
            &schema,
            &corpus,
            &expectations,
        ));
    }

    // sorted(cards, key=lambda c: (-c.overall, c.model)) — stable.
    cards.sort_by(|a, b| {
        b.overall()
            .partial_cmp(&a.overall())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.model.cmp(&b.model))
    });
    Ok(cards)
}

fn render_leaderboard(cards: &[Scorecard]) -> String {
    let mut lines: Vec<String> = vec![
        "<!-- GENERATED by `gmeow-dev sync --mode update --outputs generated` (evals) — DO NOT EDIT. -->".to_string(),
        String::new(),
        "# gmeow-evals leaderboard: which models emit valid GMEOW claims?".to_string(),
        String::new(),
        "Mechanical scores (0 to 1) against the published contract: the".to_string(),
        "[extraction prompt](../../docs/prompts/claim-extraction-v1.md), the".to_string(),
        "[emission schema](../../evals/claim-emission.schema.json), and the".to_string(),
        "[audit gates](../../docs/hallucination-resistant-kg.md), under the".to_string(),
        "published [rubric](../../evals/rubric.ttl). Scores are themselves".to_string(),
        "GMEOW claims — see `scores.ttl` (vantage-indexed Assessments).".to_string(),
        String::new(),
        format!("| model | overall | {} | claims |", CRITERIA.join(" | ")),
        format!("|---|---|{}---|", "---|".repeat(CRITERIA.len())),
    ];
    for card in cards {
        let cells: Vec<String> = CRITERIA
            .iter()
            .map(|c| format!("{:.2}", card.score(c)))
            .collect();
        lines.push(format!(
            "| {} | {:.2} | {} | {}/{} |",
            card.model,
            card.overall(),
            cells.join(" | "),
            card.valid,
            card.emitted
        ));
    }
    lines.push(String::new());
    lines.push(
        "Run `gmeow evals run --endpoint …` (network) to add a model; \
`gmeow evals score` re-scores committed emissions offline."
            .to_string(),
    );
    lines.join("\n") + "\n"
}

/// Push the four canonical A-Box structural annotations (`rdfs:label` /
/// `skos:definition` / `rdfs:isDefinedBy` / `gmeow:graphBoxRole`) for
/// `subject_iri` onto `out`, one Turtle triple line per annotation (full
/// `<iri>` subject/predicate — canonicalization downstream does not care that
/// these lines don't use the file's `ev:`/`gmeow:` prefixes). Routed through
/// the single [`gmeow_errors::abox::abox_annotations`] contract every
/// generated A-Box individual satisfies identically, rather than a second
/// hand-rolled copy of the four-triple shape.
fn annotate_abox_turtle(
    out: &mut Vec<String>,
    subject_iri: &str,
    label: &str,
    definition: &str,
    graph_iri: &str,
) {
    for (predicate, object) in abox_annotations(subject_iri, label, definition, graph_iri) {
        let object_text = match object {
            AboxObject::Iri(iri) => format!("<{iri}>"),
            AboxObject::CarrierLiteral(value) => {
                format!("\"{}\"@{X_GMEOW_ENGLISH}", nq_escape(&value))
            }
        };
        out.push(format!("<{subject_iri}> <{predicate}> {object_text} ."));
    }
}

fn render_scores_ttl(cards: &[Scorecard]) -> String {
    // The isDefinedBy target: `generated/evals/scores.ttl`'s RDF-fanout named
    // graph, derived through the SAME canonical `rdf_fanout_graph_iri` the
    // superset/fold gate uses to reconstruct the bundle from this file — never
    // a second hand-copied literal.
    let graph_iri = crate::stages::superset::rdf_fanout_graph_iri(SCORES_PATH)
        .expect("SCORES_PATH resolves an rdf-fanout graph IRI");
    let mut lines: Vec<String> = vec![
        "# GENERATED by `gmeow-dev sync --mode update --outputs generated` (evals) — DO NOT EDIT."
            .to_string(),
        "# Evaluation is meta-claims: each score is a vantage-indexed".to_string(),
        "# Assessment by the harness against the published rubric — attributed,".to_string(),
        "# contestable, never a detached dashboard number.".to_string(),
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .".to_string(),
        "@prefix ev:    <https://blackcatinformatics.ca/gmeow/evals/> .".to_string(),
        String::new(),
    ];
    for card in cards {
        let safe = slug(&card.model);
        let model_iri = format!("{EV}model-{safe}");
        lines.push(format!("ev:model-{safe} a gmeow:SoftwareAgent ."));
        annotate_abox_turtle(
            &mut lines,
            &model_iri,
            &card.model,
            &format!(
                "Software agent scored under the gmeow eval harness: {}.",
                card.model
            ),
            &graph_iri,
        );
        for criterion in CRITERIA {
            let crit = crit_local(criterion);
            lines.push(format!(
                "ev:assessment-{safe}-{criterion} a gmeow:Assessment ;\n    \
gmeow:vantage ev:harness ;\n    \
gmeow:assessmentTarget ev:model-{safe} ;\n    \
gmeow:assessmentCriterion ev:{crit} ;\n    \
gmeow:assessmentRubric ev:rubric ;\n    \
gmeow:assessmentScoreValue {} .",
                py_float(card.score(criterion))
            ));
            let assessment_iri = format!("{EV}assessment-{safe}-{criterion}");
            annotate_abox_turtle(
                &mut lines,
                &assessment_iri,
                &format!("{} / {criterion}", card.model),
                &format!(
                    "{criterion} assessment of {} against the eval rubric.",
                    card.model
                ),
                &graph_iri,
            );
        }
        lines.push(String::new());
    }
    lines.join("\n")
}

/// Render one model's scorecard JSON (`json.dumps(payload, indent=2, sort_keys=True) + "\n"`).
fn render_scorecard_json(card: &Scorecard) -> String {
    let mut out = String::from("{\n");
    // sort_keys=True ⇒ emitted, model, notes, overall, scores, valid.
    out.push_str(&format!("  \"emitted\": {},\n", card.emitted));
    out.push_str(&format!("  \"model\": {},\n", json_str(&card.model)));
    // notes
    if card.notes.is_empty() {
        out.push_str("  \"notes\": [],\n");
    } else {
        out.push_str("  \"notes\": [\n");
        let rendered: Vec<String> = card
            .notes
            .iter()
            .map(|note| format!("    {}", json_str(note)))
            .collect();
        out.push_str(&rendered.join(",\n"));
        out.push_str("\n  ],\n");
    }
    out.push_str(&format!(
        "  \"overall\": {},\n",
        py_float(round4(card.overall()))
    ));
    // scores (sorted keys)
    let mut score_keys: Vec<&(String, f64)> = card.scores.iter().collect();
    score_keys.sort_by(|a, b| a.0.cmp(&b.0));
    out.push_str("  \"scores\": {\n");
    let rendered: Vec<String> = score_keys
        .iter()
        .map(|(k, v)| format!("    {}: {}", json_str(k), py_float(*v)))
        .collect();
    out.push_str(&rendered.join(",\n"));
    out.push_str("\n  },\n");
    out.push_str(&format!("  \"valid\": {}\n", card.valid));
    out.push_str("}\n");
    out
}

/// `json.dumps(s)` of a string with the default `ensure_ascii=True`.
fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if (c as u32) < 0x7f => out.push(c),
            c => {
                let cp = c as u32;
                if cp <= 0xFFFF {
                    out.push_str(&format!("\\u{cp:04x}"));
                } else {
                    let v = cp - 0x10000;
                    let hi = 0xD800 + (v >> 10);
                    let lo = 0xDC00 + (v & 0x3FF);
                    out.push_str(&format!("\\u{hi:04x}\\u{lo:04x}"));
                }
            }
        }
    }
    out.push('"');
    out
}

/// Build every committed eval artifact under `root` keyed by logical path.
pub fn render_evals(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let cards = all_scorecards(root)?;
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    out.insert(
        LEADERBOARD_PATH.to_string(),
        render_leaderboard(&cards).into_bytes(),
    );
    // scores.ttl rides as an RDF-fanout named graph: emit EXACTLY the canonical fold
    // (shared prefix authority, no banner) so the superset gate reconstructs it.
    out.insert(
        SCORES_PATH.to_string(),
        purrdf::turtle_normalize::canonical_turtle(
            render_scores_ttl(&cards).as_bytes(),
            &crate::stages::superset::rdf_prefixes(),
        )
        .map(String::into_bytes)
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-export-evals".to_string(),
                message: format!("canonicalize scores.ttl: {e}"),
            })
        })?,
    );
    for card in &cards {
        out.insert(
            format!("generated/evals/{}.scorecard.json", card.model),
            render_scorecard_json(card).into_bytes(),
        );
    }
    Ok(out)
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `stage-export-evals` export-leaf stage.
pub struct EvalsStage;

impl Stage for EvalsStage {
    fn id(&self) -> &str {
        "stage-export-evals"
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        "evals.v1"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // Pure source read of NON-fold inputs: the emission schema, the corpus
        // manifest + every source document it references, the ground-truth
        // expectations, and every recorded model emission. Declare them ALL so a
        // re-scored corpus / new model / changed expectation busts the cache.
        // `consumes() == []`.
        let evals = root.join("evals");
        let mut files: Vec<std::path::PathBuf> = vec![
            evals.join("claim-emission.schema.json"),
            evals.join("corpus.ttl"),
            evals.join("expectations.json"),
        ];
        // Every corpus source document (sourceLocation rows in corpus.ttl).
        for location in parse_corpus(&evals.join("corpus.ttl"))?.keys() {
            files.push(root.join(location));
        }
        // Every recorded model emission: evals/outputs/<model>/claims.jsonl.
        let outputs = evals.join("outputs");
        if let Ok(entries) = std::fs::read_dir(&outputs) {
            for entry in entries.flatten() {
                let claims = entry.path().join("claims.jsonl");
                if claims.is_file() {
                    files.push(claims);
                }
            }
        }
        Ok(files)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            render_evals(input.root)?,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evals_blake2s_matches_hashlib() {
        // hashlib.blake2s(data, digest_size=4).hexdigest() — canonical Python.
        assert_eq!(blake2s::hex(b"", 4), "36e9d246");
        assert_eq!(blake2s::hex(b"abc", 4), "df7101d3");
        assert_eq!(blake2s::hex(b"openai/gpt-4.1", 4), "7558b2f5");
    }

    #[test]
    fn evals_slug_is_path_safe() {
        assert_eq!(slug("reference-baseline"), "reference-baseline");
        // lossy ids gain a content-hash suffix
        assert!(slug("openai/gpt-4.1").starts_with("openai-gpt-4-1-"));
    }

    /// Shift-left: drive the SAME native structural lint `make validate`/`make
    /// check` run (`gmeow_validate::lint::structural_lint_dataset`) over this
    /// generator's real output for a small synthetic scorecard set, so a
    /// missing/incorrect A-Box annotation on a minted `ev:model-*` /
    /// `ev:assessment-*` individual reds HERE — a fast `cargo nextest -p
    /// gmeow-pipeline` — rather than only surfacing at the next expensive
    /// whole-bundle SHACL validation (`make validate` / the pipeline
    /// stage-validate).
    #[test]
    fn minted_individuals_satisfy_the_assertional_abox_contract() {
        use gmeow_validate::lint::{LintConfig, structural_lint_dataset};

        let cards = vec![Scorecard {
            model: "acme/test-model-1".to_string(),
            emitted: 4,
            valid: 4,
            scores: vec![
                ("schema-validity".to_string(), 1.0),
                ("grounding-precision".to_string(), 0.5),
                ("grounding-recall".to_string(), 0.5),
                ("hallucination-resistance".to_string(), 0.5),
                ("abstention-quality".to_string(), 1.0),
                ("calibration".to_string(), 0.8),
            ],
            notes: Vec::new(),
        }];

        let ttl = render_scores_ttl(&cards);
        // The real bundle supplies `gmeow:boxABox a gmeow:GraphBoxRole` from the
        // kernel slice; add it here (same pattern as `provenance_graph.rs`'s
        // `minted_individuals_satisfy_the_assertional_abox_contract` and
        // `release.rs`'s `minted_attestations_satisfy_the_assertional_contract`)
        // so the graphBoxRole-typing check has its declaration to resolve
        // against. `gmeow:` is already `@prefix`-declared by `render_scores_ttl`.
        let doc = format!("{ttl}\ngmeow:boxABox a gmeow:GraphBoxRole .\n");
        let ds = purrdf::parse_dataset(doc.as_bytes(), "text/turtle", None)
            .expect("parse the synthetic scores.ttl fragment");

        let cfg = LintConfig {
            namespace: GM.to_string(),
            ontology_iri: GM.trim_end_matches('/').to_string(),
            selector_tokens: Default::default(),
            core_slice_iris: Default::default(),
            annotation_predicates: Default::default(),
        };
        let report = structural_lint_dataset(&ds, &cfg);
        let errors = report.errors();
        let evals_errors: Vec<&String> = errors.iter().filter(|e| e.contains(EV)).collect();
        assert!(
            evals_errors.is_empty(),
            "every minted evals individual must satisfy the A-Box annotation \
             contract (rdfs:label / skos:definition / rdfs:isDefinedBy / \
             gmeow:graphBoxRole): {evals_errors:?}"
        );
    }
}
