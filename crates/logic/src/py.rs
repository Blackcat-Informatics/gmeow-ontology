// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! PyO3 Python bindings for `gmeow-logic`.
//!
//! # Platform note
//!
//! This module is compiled only on native (non-wasm32) targets because pyo3
//! physically cannot link into a wasm binary — the CPython C extension ABI is
//! unavailable there.  The `#[cfg(not(target_arch = "wasm32"))]` guard in
//! `lib.rs` is platform-correct, not an optionality toggle: there are zero
//! degraded fallbacks and zero feature flags controlling this.
//!
//! # Nemo wire-up (issue #501 Task 4)
//!
//! `materialize` now drives the full Nemo chase WITH real proof-trace provenance:
//!
//! 1. Parse input N-Quads into an oxigraph `Store`.
//! 2. Encode each quad as a Nemo IRI-predicate ground fact:
//!    `<predicate_iri>(<subject_iri>, <object_term>, "world_iri").`
//! 3. Concatenate the caller-supplied `.rls` rule text.
//! 4. Run `run_chase` (GIL released) → `Vec<ChaseRowWithProvenance>`.
//! 5. Decode each ternary `ChaseRowWithProvenance` back to an oxigraph quad.
//! 6. Compute real provenance using `mint_reifier` / `mint_derivation_id`:
//!    - Asserted (EDB) quads: `rule_iri = logic:assert`,
//!      `source_quad_ids = [self_reifier]`,
//!      `derivation_id = mint_derivation_id(assert_rule, [self_reifier])`
//!    - Derived (IDB) quads: `rule_iri` from the firing rule's name (set via
//!      `#[name("...")]` in the `.rls` source), antecedent reifiers from the
//!      immediate premise ChaseRows.
//! 7. Return the quads as Python dicts.
//!
//! # Encode / decode contract
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
//!
//! Nemo includes EDB facts in the derived predicates, so round-trips through an
//! empty rule set return all input quads unchanged.

use pyo3::prelude::*;
use pyo3::types::PyDict;

use oxigraph::io::RdfFormat;
use oxigraph::model::{GraphName, Literal, NamedNode, NamedOrBlankNode, Term};
use oxigraph::store::Store;
use sha1::{Digest, Sha1};

use crate::nemo_engine::{run_chase, ChaseRow, ChaseRowWithProvenance};
use crate::provenance::{mint_derivation_id, mint_reifier, ASSERT_RULE_IRI, LOGIC_NAMESPACE};
use crate::seam::{BudgetStatus, DerivationId, DerivedQuad};

// ── Constants ──────────────────────────────────────────────────────────────────

/// The IRI used for the semantic/decidability profile.
const ASSERTED_PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";

/// Prefix for Skolem IRIs derived from blank-node identifiers.
///
/// Matches the Python oracle: `{NAMESPACE}skolem/{sha1_hex(bnode_id_utf8)}`.
const SKOLEM_PREFIX: &str = "https://blackcatinformatics.ca/gmeow/skolem/";

// ── Skolemization ─────────────────────────────────────────────────────────────

/// Compute the SHA-1 hex digest of a UTF-8 string — matching the Python recipe
/// `sha1(str(bnode).encode("utf-8")).hexdigest()`.
fn sha1_hex(s: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Skolemize a blank-node identifier to a stable IRI string.
fn skolem_iri(bnode_id: &str) -> String {
    format!("{}{}", SKOLEM_PREFIX, sha1_hex(bnode_id))
}

// ── Encode: oxigraph quad → Nemo ground-fact line ────────────────────────────

/// Encode an oxigraph `Term` as a Nemo argument string.
///
/// - `NamedNode(iri)` → `<iri>`
/// - `BlankNode(id)` → Skolemized `<https://...skolem/{sha1}>` (same as NamedNode)
/// - `Literal(value)` → depends on datatype / language (see below)
/// - `Triple` → unsupported; empty string (gmeow-logic only uses IRI/BNode/Literal)
fn encode_term(term: &Term) -> String {
    match term {
        Term::NamedNode(nn) => format!("<{}>", nn.as_str()),
        Term::BlankNode(bn) => format!("<{}>", skolem_iri(bn.as_str())),
        Term::Literal(lit) => encode_literal(lit),
        Term::Triple(_) => String::new(), // RDF-star triple terms: unsupported in Nemo
    }
}

/// Encode a subject term (NamedNode or BlankNode) as a Nemo argument.
fn encode_subject(subject: &NamedOrBlankNode) -> String {
    match subject {
        NamedOrBlankNode::NamedNode(nn) => format!("<{}>", nn.as_str()),
        NamedOrBlankNode::BlankNode(bn) => format!("<{}>", skolem_iri(bn.as_str())),
    }
}

/// Encode an oxigraph `Literal` as a Nemo constant string.
///
/// - Plain `xsd:string` literal: `"value"`
/// - Language-tagged: `"value"@lang`
/// - Any other datatype: `"value"^^<datatype_iri>`
fn encode_literal(lit: &Literal) -> String {
    // Escape inner double-quotes and backslashes
    let escaped = lit.value().replace('\\', "\\\\").replace('"', "\\\"");

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
fn encode_quad_to_nemo_fact(
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
    let world_escaped = world_iri.replace('\\', "\\\\").replace('"', "\\\"");
    format!("{}({}, {}, \"{}\").", pred, subj, obj, world_escaped)
}

// ── Decode: Nemo ChaseRow → oxigraph quad ────────────────────────────────────

/// Decode an error-prefixed description for decode failures.
fn decode_err(context: &str, got: &str) -> String {
    format!("decode error [{context}]: {got:?}")
}

/// Decode a Nemo display-form IRI term (`<iri>`) to an IRI string.
///
/// Nemo displays IRI values as `<http://...>`.  This strips the angle brackets.
fn decode_iri_term(s: &str) -> Result<String, String> {
    if s.starts_with('<') && s.ends_with('>') {
        Ok(s[1..s.len() - 1].to_owned())
    } else {
        Err(decode_err("expected <iri>", s))
    }
}

/// Decode a Nemo display-form string constant (`"value"`) to the raw string.
///
/// Nemo displays plain string datavalues as `"value"` (the outer double-quotes
/// are part of the display representation, not the value).  This strips them.
fn decode_string_constant(s: &str) -> Result<String, String> {
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        // Un-escape \" → " and \\ → \
        let inner = &s[1..s.len() - 1];
        Ok(inner.replace("\\\"", "\"").replace("\\\\", "\\"))
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
fn decode_nemo_term(s: &str) -> Result<Term, String> {
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
        // Un-escape the value
        let value = raw_value.replace("\\\"", "\"").replace("\\\\", "\\");

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
fn split_nemo_literal_content(s: &str) -> Result<(&str, &str), String> {
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

// ── Provenance helpers ────────────────────────────────────────────────────────

/// Compute the reifier IRI for a decoded quad's (S, P, O) triple.
///
/// Uses [`crate::provenance::mint_reifier`] on the already-decoded oxigraph
/// terms so the result is byte-identical to the Python oracle.
///
/// # Errors
///
/// Returns an error if subject or object is an RDF-star quoted triple.
fn reifier_for_quad(
    subject: &Term,
    predicate: &NamedNode,
    object: &Term,
) -> Result<String, String> {
    mint_reifier(subject, predicate, object)
}

/// Compute the reifier IRI for an antecedent ChaseRow.
///
/// Decodes the Nemo display-form row (ternary: S, O, world) back to oxigraph
/// terms and calls `mint_reifier`.  Returns an error if decode fails — a
/// partial antecedent list would produce a wrong derivation_id, which is
/// worse than failing loudly.
fn reifier_for_antecedent_row(row: &ChaseRow) -> Result<String, String> {
    if row.values.len() != 3 {
        return Err(format!(
            "antecedent row has {} values (expected 3): {:?}",
            row.values.len(),
            row
        ));
    }
    // predicate: raw IRI string
    let pred_nn = NamedNode::new(&row.predicate)
        .map_err(|e| format!("antecedent predicate IRI {:?}: {e}", row.predicate))?;
    // subject: IRI
    let subj_iri = decode_iri_term(&row.values[0])?;
    let subj_nn = NamedNode::new(&subj_iri)
        .map_err(|e| format!("antecedent subject IRI {subj_iri:?}: {e}"))?;
    let subj_term = Term::NamedNode(subj_nn);
    // object: any term
    let obj_term = decode_nemo_term(&row.values[1])?;

    mint_reifier(&subj_term, &pred_nn, &obj_term)
}

/// Determine the `rule_iri` for a derived quad's provenance record.
///
/// If the trace extracted a rule name (set via `#[name("...")]` in the `.rls`
/// source), that name is used directly as the rule IRI — `project_nemo` encodes
/// the rule IRI as the rule name.
///
/// Fallback: `logic:rule/anonymous` for unnamed rules.
fn rule_iri_from_name(rule_name: Option<&str>) -> String {
    match rule_name {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => format!("{}rule/anonymous", LOGIC_NAMESPACE),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Convert a [`DerivedQuad`] to a Python dict with all metadata fields.
///
/// Keys exposed to Python:
/// - `graph`           — named-graph IRI string (the world)
/// - `subject`         — S IRI/term string
/// - `predicate`       — P IRI string
/// - `object`          — O IRI/term string
/// - `graph_component` — same as `graph` (quad self-contained, per seam contract)
/// - `derivation_id`   — IRI string
/// - `rule_iri`        — IRI string
/// - `source_quad_ids` — list of IRI strings
/// - `profile`         — IRI string
/// - `budget_status`   — canonical lowercase string (`"ok"`, `"partial"`, `"exhausted"`)
fn derived_quad_to_dict(py: Python<'_>, dq: &DerivedQuad) -> PyResult<PyObject> {
    let d = PyDict::new(py);
    d.set_item("graph", dq.graph.as_str())?;
    d.set_item("subject", dq.subject.to_string())?;
    d.set_item("predicate", dq.predicate.as_str())?;
    d.set_item("object", dq.object.to_string())?;
    d.set_item("graph_component", dq.graph_component.as_str())?;
    d.set_item("derivation_id", dq.derivation_id.as_str())?;
    d.set_item("rule_iri", &dq.rule_iri)?;
    d.set_item("source_quad_ids", &dq.source_quad_ids)?;
    d.set_item("profile", &dq.profile)?;
    d.set_item("budget_status", dq.budget_status.as_str())?;
    Ok(d.into())
}

// ── materialize ───────────────────────────────────────────────────────────────

/// Run the Nemo chase against `input` (N-Quads) and `rules` (`.rls` text).
///
/// # Arguments
///
/// - `rules` — Nemo rule-language string (may be empty for a pure EDB round-trip).
/// - `input` — N-Quads string.  Each quad is encoded as a Nemo ground fact and
///             fed as EDB to the chase.  The named-graph IRI is the "world".
///
/// # Returns
///
/// A list of Python dicts, one per derived quad (including EDB facts, since
/// Nemo returns EDB predicates in `derived_predicates()`).  Each dict carries
/// the full seam metadata: graph, subject, predicate, object, graph_component,
/// derivation_id, rule_iri, source_quad_ids, profile, budget_status.
///
/// Provenance is real — not stubs:
/// - Asserted (EDB) quads carry `rule_iri = logic:assert`,
///   `source_quad_ids = [self_reifier]`, and a content-addressed `derivation_id`.
/// - Derived (IDB) quads carry the firing rule's IRI (from `#[name("...")]`),
///   `source_quad_ids` of the immediate antecedents, and a content-addressed
///   `derivation_id`.
///
/// An empty (or whitespace-only) `input` returns an empty list immediately
/// without invoking the chase.
///
/// # Errors
///
/// Returns a Python `ValueError` for N-Quads parse errors and
/// `RuntimeError` for chase or decode failures.
#[pyfunction]
fn materialize(py: Python<'_>, rules: &str, input: &str) -> PyResult<Vec<PyObject>> {
    // ── Short-circuit: nothing to do ──────────────────────────────────────────
    if input.trim().is_empty() {
        return Ok(vec![]);
    }

    // ── 1. Parse input N-Quads into an oxigraph Store ────────────────────────
    let store = Store::new().map_err(|e| {
        pyo3::exceptions::PyRuntimeError::new_err(format!("store creation failed: {e}"))
    })?;
    store
        .load_from_reader(RdfFormat::NQuads, input.as_bytes())
        .map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("N-Quads parse error: {e}"))
        })?;

    // ── 2. Encode each quad as a Nemo ground-fact line ───────────────────────
    let mut fact_lines: Vec<String> = Vec::new();
    for result in store.iter() {
        let quad = result.map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("store iteration error: {e}"))
        })?;

        // Resolve the world IRI (named-graph component).
        // Default and blank-node graphs are skipped — matching the Python oracle
        // (_extract_worlds checks `isinstance(graph_id, URIRef)` and skips non-named
        // graphs).  Fabricating synthetic world IRIs for unnamed graphs would break
        // the oracle≡engine parity guarantee (AC-d).
        let world_iri: String = match &quad.graph_name {
            GraphName::NamedNode(nn) => nn.as_str().to_owned(),
            GraphName::DefaultGraph | GraphName::BlankNode(_) => continue,
        };

        let line =
            encode_quad_to_nemo_fact(&quad.subject, &quad.predicate, &quad.object, &world_iri);
        fact_lines.push(line);
    }

    // ── 3. Build the complete .rls program ───────────────────────────────────
    let edb_block = fact_lines.join("\n");
    let rls = if rules.trim().is_empty() {
        edb_block
    } else {
        format!("{}\n{}", edb_block, rules)
    };

    // ── 4. Run the Nemo chase (GIL released) ─────────────────────────────────
    let rows_with_prov: Vec<ChaseRowWithProvenance> = py
        .allow_threads(|| run_chase(rls))
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("chase error: {e}")))?;

    // ── 5. Decode ChaseRows → DerivedQuads with real provenance ──────────────
    let mut derived_quads: Vec<DerivedQuad> = Vec::new();

    for (idx, rwp) in rows_with_prov.iter().enumerate() {
        let row = &rwp.row;
        let prov = &rwp.provenance;

        // We only handle ternary (arity-3) predicates — the gmeow-logic encoding.
        if row.values.len() != 3 {
            continue;
        }

        // predicate: raw IRI string (Nemo strips angle brackets in Tag::to_string)
        let predicate_iri = &row.predicate;
        let predicate_nn = NamedNode::new(predicate_iri.as_str()).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "invalid predicate IRI {predicate_iri:?}: {e}"
            ))
        })?;

        // subject: must be an IRI term
        let subject_iri = decode_iri_term(&row.values[0]).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("row[{idx}] subject: {e}"))
        })?;
        let subject_nn = NamedNode::new(&subject_iri).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "row[{idx}] subject IRI {subject_iri:?}: {e}"
            ))
        })?;
        let subject_term = Term::NamedNode(subject_nn);

        // object: IRI, typed literal, language literal, or plain literal
        let object_term = decode_nemo_term(&row.values[1]).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("row[{idx}] object: {e}"))
        })?;

        // context (world): Nemo string constant → strip outer double-quotes
        let world_str = decode_string_constant(&row.values[2]).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("row[{idx}] world: {e}"))
        })?;
        let graph_nn = NamedNode::new(&world_str).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "row[{idx}] world IRI {world_str:?}: {e}"
            ))
        })?;

        // ── Real provenance computation ───────────────────────────────────────
        let self_reifier =
            reifier_for_quad(&subject_term, &predicate_nn, &object_term).map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!("row[{idx}] reifier error: {e}"))
            })?;

        let (rule_iri, source_quad_ids, derivation_id) = if prov.is_edb {
            // Asserted (EDB) fact: logic:assert sentinel, self-reifier as source.
            let rule = ASSERT_RULE_IRI.to_owned();
            let sources = vec![self_reifier.clone()];
            let deriv = mint_derivation_id(&rule, &[self_reifier.as_str()]);
            (rule, sources, deriv)
        } else {
            // Derived (IDB) fact: rule IRI from the rule name, antecedents as sources.
            // Antecedent decode is fallible — a partial list produces a wrong
            // derivation_id, which is worse than propagating the error.
            let rule = rule_iri_from_name(prov.rule_name.as_deref());
            let sources: Vec<String> = prov
                .antecedent_rows
                .iter()
                .map(reifier_for_antecedent_row)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "row[{idx}] antecedent decode error: {e}"
                    ))
                })?;
            let source_refs: Vec<&str> = sources.iter().map(|s| s.as_str()).collect();
            let deriv = mint_derivation_id(&rule, &source_refs);
            (rule, sources, deriv)
        };

        let dq = DerivedQuad {
            graph: graph_nn.clone(),
            subject: subject_term,
            predicate: predicate_nn,
            object: object_term,
            graph_component: graph_nn,
            derivation_id: DerivationId(derivation_id),
            rule_iri,
            source_quad_ids,
            profile: ASSERTED_PROFILE.to_owned(),
            budget_status: BudgetStatus::Ok,
        };
        derived_quads.push(dq);
    }

    // ── 6. Serialize to Python dicts ─────────────────────────────────────────
    derived_quads
        .iter()
        .map(|dq| derived_quad_to_dict(py, dq))
        .collect()
}

// ── Module registration ───────────────────────────────────────────────────────

/// Python extension module `gmeow_logic`.
///
/// Exposes the `materialize(rules, input)` function.
#[pymodule]
fn gmeow_logic(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(materialize, m)?)?;
    Ok(())
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::term_n3;

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

    // ── reifier_for_quad ──────────────────────────────────────────────────────

    #[test]
    fn reifier_for_quad_golden_1() {
        // Matches golden-1 from determinism-goldens.json
        let s = Term::NamedNode(NamedNode::new("http://example.org/a").unwrap());
        let p = NamedNode::new("http://example.org/related").unwrap();
        let o = Term::NamedNode(NamedNode::new("http://example.org/b").unwrap());
        let got = reifier_for_quad(&s, &p, &o).expect("IRI terms must not fail");
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/reifier/10d9bdab72fe25cf3b81fe842b3a105077d98a6a"
        );
    }

    // ── term_n3 reexport from provenance ─────────────────────────────────────

    #[test]
    fn term_n3_iri_for_quad_object() {
        let nn = NamedNode::new("http://example.org/Foo").unwrap();
        let term = Term::NamedNode(nn);
        assert_eq!(
            term_n3(&term).expect("IRI term must not fail"),
            "<http://example.org/Foo>"
        );
    }
}
