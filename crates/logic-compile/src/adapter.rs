// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! OWL/gUFO adapter: normalize legacy `owl:*` / `gufo:` source into the IR.
//!
//! The OWL/gUFO adapter; the Python duplicate (`logic_adapter.py`) was
//! retired.  It accepts legacy
//! RDF that uses `owl:*` structural vocabulary and/or `gufo:` stereotypes and
//! normalizes it into the same [`LogicProgram`] IR the `logic:` front-end
//! produces, enabling the **round-trip isomorphism gate** ([`assert_ir_isomorphic`]):
//! a construct authored in `logic:` and an equivalent construct in
//! `owl:*`/`gufo:` form must normalize to identical IR.
//!
//! Class-expression restrictions (`owl:Restriction` + `owl:onProperty` +
//! value/cardinality constraints) are lifted via the shared [`super::restriction`]
//! skolemizer into deterministic, content-addressed `logic:restriction/<hash>` nodes —
//! the SAME routine the `logic:` front-end runs — so an OWL-authored restriction and
//! its `logic:`-authored twin normalize to identical IR.
//!
//! # Adapter contract
//!
//! * **Fail-soft** on unrecognised constructs (malformed restrictions missing
//!   `owl:onProperty`, anonymous blank-node objects, unmapped `owl:` predicates) —
//!   emit a [`Diagnostic`] and skip; nothing is silently lost.
//! * **Raise** ([`LogicParseError`]) on empty/unreadable input.
//!
//! The `gufo: class → logic: term` *coverage* correspondence (the `gmeow:logic ⊇ gUFO floor`
//! coverage floor, with its `SUPERSEDED` sentinel) is **not** part of
//! the compiler runtime — only the 11-stereotype runtime sort map lives in this
//! module. The coverage floor is enforced natively by the integration test
//! `crates/logic/tests/gufo_superset.rs` (which retired the Python fixture
//! `tests/test_logic_gufo_superset.py`).

use std::collections::{BTreeSet, HashSet};
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

use purrdf::{RdfDataset, parse_dataset};

use super::frontend::{Diagnostic, LogicParseError, Severity};
use super::graphutil::{
    Node, RDF_TYPE, default_graph_quads, iri_of, is_empty, nn, subject_is_blank, subject_of,
    subject_str, subjects_with, term_is_blank, term_is_literal, term_str,
};
use super::ir::{
    ANNOTATION_LIFT_PREDS, ContextualScope, Formula, LOGIC_NAMESPACE, LogicAxiom, LogicProgram,
    LogicRule, NodeKind, ReasoningContract, X_GMEOW_ENGLISH_TAG, annotation_pred_is_load_bearing,
    subject_is_gmeow_authored,
};
use super::restriction::{
    LiftedTriple, RestrictionVocab, datarange_node_labels, enumeration_node_labels,
    restriction_node_labels, skolemize_dataranges, skolemize_enumerations, skolemize_restrictions,
};

const GUFO_NS: &str = "http://purl.org/nemo/gufo#";
const OWL_NS: &str = "http://www.w3.org/2002/07/owl#";
const RDFS_NS: &str = "http://www.w3.org/2000/01/rdf-schema#";

fn gufo(local: &str) -> String {
    format!("{GUFO_NS}{local}")
}
fn owl(local: &str) -> String {
    format!("{OWL_NS}{local}")
}
fn rdfs(local: &str) -> String {
    format!("{RDFS_NS}{local}")
}
fn logic(local: &str) -> String {
    format!("{LOGIC_NAMESPACE}{local}")
}

// --------------------------------------------------------------------------- //
// Mapping tables (the authoritative owl/gufo → logic: maps)
// --------------------------------------------------------------------------- //

/// gUFO stereotype local name → `logic:` sort local name (`rdf:type` assertions).
const GUFO_TO_LOGIC_SORT: &[(&str, &str)] = &[
    ("Kind", "Kind"),
    ("SubKind", "SubKind"),
    ("Phase", "Phase"),
    ("Role", "Role"),
    ("Category", "Category"),
    ("Mixin", "Mixin"),
    ("RoleMixin", "RoleMixin"),
    ("PhaseMixin", "PhaseMixin"),
    ("Relator", "Relator"),
    ("EventType", "Event"),
    ("SituationType", "Situation"),
];

/// OWL/RDFS structural predicate → `logic:` predicate local name.
/// The tuple is `(namespace, owl_local, logic_local)`.
const OWL_PRED_TO_LOGIC: &[(&str, &str, &str)] = &[
    (RDFS_NS, "subClassOf", "subClassOf"),
    (OWL_NS, "equivalentClass", "equivalentClass"),
    (OWL_NS, "disjointWith", "disjointWith"),
    (RDFS_NS, "subPropertyOf", "subPropertyOf"),
    (OWL_NS, "equivalentProperty", "equivalentProperty"),
    (OWL_NS, "inverseOf", "inverseOf"),
    (RDFS_NS, "domain", "domain"),
    (RDFS_NS, "range", "range"),
];

/// OWL property-characteristic class local name → `logic:` characteristic local
/// name (used as `rdf:type` objects).
const OWL_CHARACTERISTIC_TO_LOGIC: &[(&str, &str)] = &[
    ("TransitiveProperty", "transitiveProperty"),
    ("SymmetricProperty", "symmetricProperty"),
    ("FunctionalProperty", "functionalProperty"),
    ("InverseFunctionalProperty", "inverseFunctionalProperty"),
    ("ReflexiveProperty", "reflexiveProperty"),
    ("AsymmetricProperty", "asymmetricProperty"),
    ("IrreflexiveProperty", "irreflexiveProperty"),
];

/// OWL/RDFS meta predicates that carry no structural logic payload AND are not lifted as
/// first-class annotations. `rdfs:label`/`rdfs:comment` are DELIBERATELY absent — they are now
/// lifted into `NodeKind::Annotation` axioms (`ANNOTATION_LIFT_PREDS`) exactly as the frontend
/// twin lifts them, so an owl:/rdfs:-authored construct and its logic:-authored twin normalize
/// to identical annotation axioms (the IR-isomorphism gate). `seeAlso`/`isDefinedBy` and the
/// `owl:` versioning/imports predicates carry no annotation payload and stay skipped.
fn rdfs_skip_preds() -> HashSet<String> {
    [
        rdfs("seeAlso"),
        rdfs("isDefinedBy"),
        owl("versionIRI"),
        owl("versionInfo"),
        owl("imports"),
        owl("deprecated"),
    ]
    .into_iter()
    .collect()
}

// --------------------------------------------------------------------------- //
// IR isomorphism gate
// --------------------------------------------------------------------------- //

const SEP: char = '\u{0}';

/// Raised by [`assert_ir_isomorphic`] when two programs differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IRIsomorphismError(pub String);

impl std::fmt::Display for IRIsomorphismError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for IRIsomorphismError {}

fn py_bool(b: bool) -> &'static str {
    if b { "True" } else { "False" }
}

/// Stable diff key for an axiom (mirrors Python `_axiom_key`: subject, predicate,
/// obj, obj_is_literal — scope and negation are intentionally excluded).
fn axiom_key(a: &LogicAxiom) -> String {
    format!(
        "{}{SEP}{}{SEP}{}{SEP}{}",
        a.subject,
        a.predicate,
        a.obj,
        py_bool(a.obj_is_literal)
    )
}

/// Stable diff key for a rule (mirrors Python `_rule_key`).
fn rule_key(r: &LogicRule) -> String {
    let head = &r.head;
    let head_key = format!("{}{SEP}{}{SEP}{}", head.subject, head.predicate, head.obj);
    let mut body: Vec<String> = r
        .body
        .iter()
        .map(|b| format!("{}{SEP}{}{SEP}{}", b.subject, b.predicate, b.obj))
        .collect();
    body.sort();
    let mut base = format!("{head_key}{SEP}{}", body.join("|"));
    if !r.distinct_pairs.is_empty() {
        let distinct = r
            .distinct_pairs
            .iter()
            .map(|(a, b)| format!("{a}{SEP}{b}"))
            .collect::<Vec<_>>()
            .join("|");
        base.push(SEP);
        base.push_str(&distinct);
    }
    base
}

/// Stable diff key for a reasoning contract (previously `profile_key`).
fn contract_key(c: &ReasoningContract) -> String {
    c.sort_key()
}

/// Assert that two [`LogicProgram`]s are canonically equal, raising
/// [`IRIsomorphismError`] with a directional diff on mismatch (mirrors the Python
/// `assert_ir_isomorphic`).
pub fn assert_ir_isomorphic(
    prog_a: &LogicProgram,
    prog_b: &LogicProgram,
) -> Result<(), IRIsomorphismError> {
    if prog_a == prog_b {
        return Ok(());
    }

    let axioms_a: HashSet<String> = prog_a.axioms.iter().map(axiom_key).collect();
    let axioms_b: HashSet<String> = prog_b.axioms.iter().map(axiom_key).collect();
    let rules_a: HashSet<String> = prog_a.rules.iter().map(rule_key).collect();
    let rules_b: HashSet<String> = prog_b.rules.iter().map(rule_key).collect();
    let contracts_a: HashSet<String> = prog_a.contracts.iter().map(contract_key).collect();
    let contracts_b: HashSet<String> = prog_b.contracts.iter().map(contract_key).collect();
    let formulas_a: HashSet<String> = prog_a.formulas.iter().map(Formula::content_key).collect();
    let formulas_b: HashSet<String> = prog_b.formulas.iter().map(Formula::content_key).collect();

    let diff = |from: &HashSet<String>, to: &HashSet<String>| -> Vec<String> {
        let mut v: Vec<String> = from.difference(to).cloned().collect();
        v.sort();
        v
    };

    let mut lines: Vec<String> = Vec::new();
    for item in diff(&axioms_a, &axioms_b) {
        lines.push(format!("A has, B lacks (axiom):  {item}"));
    }
    for item in diff(&axioms_b, &axioms_a) {
        lines.push(format!("B has, A lacks (axiom):  {item}"));
    }
    for item in diff(&rules_a, &rules_b) {
        lines.push(format!("A has, B lacks (rule):   {item}"));
    }
    for item in diff(&rules_b, &rules_a) {
        lines.push(format!("B has, A lacks (rule):   {item}"));
    }
    for item in diff(&contracts_a, &contracts_b) {
        lines.push(format!("A has, B lacks (contract): {item}"));
    }
    for item in diff(&contracts_b, &contracts_a) {
        lines.push(format!("B has, A lacks (contract): {item}"));
    }
    for item in diff(&formulas_a, &formulas_b) {
        lines.push(format!("A has, B lacks (formula): {item}"));
    }
    for item in diff(&formulas_b, &formulas_a) {
        lines.push(format!("B has, A lacks (formula): {item}"));
    }

    if lines.is_empty() {
        if prog_a.source_iri != prog_b.source_iri {
            lines.push(format!(
                "source_iri differs: A={:?}  B={:?}",
                prog_a.source_iri, prog_b.source_iri
            ));
        } else {
            lines.push("programs differ (canonical mismatch — check nested scope)".to_owned());
        }
    }

    Err(IRIsomorphismError(format!(
        "IR isomorphism gate FAILED — programs do not normalize identically:\n  {}",
        lines.join("\n  ")
    )))
}

// --------------------------------------------------------------------------- //
// Axiom extraction from OWL/gUFO source
// --------------------------------------------------------------------------- //

/// An internal carrier before converting to a [`LogicAxiom`].
struct MappedAxiom {
    subject: String,
    predicate: String,
    obj: String,
    obj_is_literal: bool,
}

impl From<LiftedTriple> for MappedAxiom {
    fn from(t: LiftedTriple) -> Self {
        Self {
            subject: t.subject,
            predicate: t.predicate,
            obj: t.obj,
            obj_is_literal: t.obj_is_literal,
        }
    }
}

fn extract_gufo_sort_axioms(
    store: &RdfDataset,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<MappedAxiom> {
    let mut result = Vec::new();
    for (gufo_local, logic_local) in GUFO_TO_LOGIC_SORT {
        let gufo_class = Node::iri(gufo(gufo_local));
        let logic_type_iri = logic(logic_local);
        for subject in subjects_with(store, &nn(RDF_TYPE), &gufo_class) {
            if subject_is_blank(&subject) {
                diagnostics.push(warn(
                    "BLANK_NODE_GUFO_SORT",
                    format!(
                        "Blank-node subject typed {} cannot be normalized to a logic: sort; \
                         skipped",
                        gufo(gufo_local)
                    ),
                    None,
                ));
                continue;
            }
            result.push(MappedAxiom {
                subject: subject_str(&subject),
                predicate: RDF_TYPE.to_owned(),
                obj: logic_type_iri.clone(),
                obj_is_literal: false,
            });
        }
    }
    result
}

fn extract_owl_char_axioms(
    store: &RdfDataset,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<MappedAxiom> {
    let mut result = Vec::new();
    for (owl_local, logic_local) in OWL_CHARACTERISTIC_TO_LOGIC {
        let owl_char = Node::iri(owl(owl_local));
        let logic_type = logic(logic_local);
        for subject in subjects_with(store, &nn(RDF_TYPE), &owl_char) {
            if subject_is_blank(&subject) {
                diagnostics.push(warn(
                    "BLANK_NODE_OWL_CHAR",
                    format!(
                        "Blank-node subject typed {} cannot be normalized; skipped",
                        owl(owl_local)
                    ),
                    None,
                ));
                continue;
            }
            result.push(MappedAxiom {
                subject: subject_str(&subject),
                predicate: RDF_TYPE.to_owned(),
                obj: logic_type.clone(),
                obj_is_literal: false,
            });
        }
    }
    result
}

fn extract_owl_structural_axioms(
    store: &RdfDataset,
    rnodes: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<MappedAxiom> {
    let quads = default_graph_quads(store);
    let mut result = Vec::new();
    for (ns, owl_local, logic_local) in OWL_PRED_TO_LOGIC {
        let owl_pred_iri = format!("{ns}{owl_local}");
        let logic_pred = logic(logic_local);
        for quad in &quads {
            if quad.predicate.as_str() != owl_pred_iri {
                continue;
            }
            // Restriction anchor / internal edges are owned by the skolemizer: a
            // `C rdfs:subClassOf <restriction>` edge is re-emitted redirected to the
            // skolem node, and a restriction node never contributes a flat structural
            // axiom of its own.  Skip both here so nothing double-emits.
            if rnodes.contains(&term_str(&quad.object))
                || rnodes.contains(&subject_str(&quad.subject))
            {
                continue;
            }
            if subject_is_blank(&quad.subject) {
                // Anonymous subject — skip silently (blank reification helper).
                continue;
            }
            if term_is_blank(&quad.object) {
                let s_str = subject_str(&quad.subject);
                let pred_str = format!("{ns}{owl_local}");
                diagnostics.push(warn(
                    "UNMAPPED_OWL_CONSTRUCT",
                    format!(
                        "{s_str:?} {pred_str:?} [blank node]: anonymous blank-node object \
                         cannot be normalized; skipped"
                    ),
                    Some(s_str),
                ));
                continue;
            }
            result.push(MappedAxiom {
                subject: subject_str(&quad.subject),
                predicate: logic_pred.clone(),
                obj: term_str(&quad.object),
                obj_is_literal: term_is_literal(&quad.object),
            });
        }
    }
    result
}

fn extract_unmapped_owl_triples(
    store: &RdfDataset,
    rnodes: &BTreeSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let skip = rdfs_skip_preds();
    let mapped_owl: HashSet<String> = OWL_PRED_TO_LOGIC
        .iter()
        .map(|(ns, local, _)| format!("{ns}{local}"))
        .collect();
    for q in store.quads().filter(|q| q.g.is_none()) {
        let predicate = iri_of(store, q.p);
        let p_str = predicate.as_str();
        if !p_str.starts_with(OWL_NS) {
            continue;
        }
        if mapped_owl.contains(p_str) || skip.contains(p_str) {
            continue;
        }
        if p_str == RDF_TYPE {
            continue;
        }
        let subject = subject_of(store, q.s);
        // Restriction internals (`<r> owl:onProperty/owl:someValuesFrom/…`) are lifted
        // by the skolemizer, not dropped.
        if rnodes.contains(&subject_str(&subject)) {
            continue;
        }
        if subject_is_blank(&subject) {
            continue;
        }
        let s_str = subject_str(&subject);
        diagnostics.push(warn(
            "UNMAPPED_OWL_CONSTRUCT",
            format!("OWL predicate {p_str:?} on {s_str:?} has no logic: equivalent; skipped"),
            Some(s_str),
        ));
    }
}

fn warn(code: &str, message: String, subject: Option<String>) -> Diagnostic {
    Diagnostic {
        severity: Severity::Warning,
        code: code.to_owned(),
        message,
        subject,
    }
}

fn err(code: &str, message: String, subject: Option<String>) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        code: code.to_owned(),
        message,
        subject,
    }
}

/// Lift the RDFS/SKOS annotation surface into first-class [`NodeKind::Annotation`] axioms —
/// the owl/rdfs-authored TWIN of `frontend::extract_annotation_axioms`. Both sites lift the
/// SAME `ANNOTATION_LIFT_PREDS` the SAME way (carrier tag fail-closed, load-bearing bit) so an
/// owl:/rdfs:-authored construct and its logic:-authored twin normalize to identical annotation
/// axioms and `assert_ir_isomorphic` stays green. Returns fully-built axioms (node_kind +
/// load_bearing set) rather than `MappedAxiom`s, since those two fields must survive to the
/// program for the full-program equality the isomorphism gate's fast path checks.
fn extract_annotation_axioms(
    store: &RdfDataset,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<LogicAxiom> {
    let mut axioms: Vec<LogicAxiom> = Vec::new();
    for quad in default_graph_quads(store) {
        let p_str = quad.predicate.as_str();
        if !ANNOTATION_LIFT_PREDS.contains(&p_str) {
            continue;
        }
        if subject_is_blank(&quad.subject) {
            continue;
        }
        // Only GMEOW-authored subjects are lifted (see the frontend twin): a foreign subject's
        // external-vocabulary label is not GMEOW's annotation surface.
        if !subject_is_gmeow_authored(&subject_str(&quad.subject)) {
            continue;
        }
        let Node::Lit { lexical, lang, .. } = &quad.object else {
            continue;
        };
        match lang.as_deref() {
            Some(X_GMEOW_ENGLISH_TAG) => {}
            Some(other) => {
                diagnostics.push(err(
                    "NON_CARRIER_ANNOTATION_LANG",
                    format!(
                        "annotation literal on <{}> {p_str} carries language tag @{other}, not \
                         the internal @{X_GMEOW_ENGLISH_TAG} carrier tag",
                        subject_str(&quad.subject),
                    ),
                    Some(subject_str(&quad.subject)),
                ));
                continue;
            }
            None => continue,
        }
        match LogicAxiom::new(
            subject_str(&quad.subject),
            p_str,
            lexical.clone(),
            true,
            false,
            ContextualScope::default(),
        ) {
            Ok(ax) => axioms.push(
                ax.with_node_kind(NodeKind::Annotation)
                    .with_load_bearing(annotation_pred_is_load_bearing(p_str)),
            ),
            Err(exc) => diagnostics.push(warn(
                "MALFORMED_ANNOTATION",
                exc.message().to_owned(),
                Some(subject_str(&quad.subject)),
            )),
        }
    }
    axioms
}

// --------------------------------------------------------------------------- //
// Public API
// --------------------------------------------------------------------------- //

/// Normalize legacy `owl:*` / `gufo:` RDF already loaded into a wasm-clean
/// [`RdfDataset`] (default graph) into a [`LogicProgram`] + diagnostics.
pub fn adapt_legacy_dataset(
    store: &RdfDataset,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), LogicParseError> {
    if is_empty(store) {
        return Err(LogicParseError(
            "Source graph is empty — nothing to adapt.  Pass a non-empty graph or a \
             Turtle file with owl:* / gufo: triples."
                .to_owned(),
        ));
    }

    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut mapped: Vec<MappedAxiom> = Vec::new();

    // Lift OWL class-expression restrictions (owl:Restriction + onProperty +
    // value/cardinality constraints), owl:oneOf enumerations, and owl:withRestrictions
    // datatype restrictions (dataranges) into deterministic skolem-keyed logic: axioms.
    // The combined node set drives the generic extractors' skip filter so the anchor edges
    // and internals are owned solely by the skolemizers.
    let owl_vocab = RestrictionVocab::owl();
    let mut handled = restriction_node_labels(store, &owl_vocab);
    handled.extend(enumeration_node_labels(store, &owl_vocab));
    handled.extend(datarange_node_labels(store, &owl_vocab));
    for lifted in skolemize_restrictions(store, &owl_vocab, &mut diagnostics) {
        mapped.push(lifted.into());
    }
    for lifted in skolemize_enumerations(store, &owl_vocab, &mut diagnostics) {
        mapped.push(lifted.into());
    }
    for lifted in skolemize_dataranges(store, &owl_vocab, &mut diagnostics) {
        mapped.push(lifted.into());
    }

    mapped.extend(extract_gufo_sort_axioms(store, &mut diagnostics));
    mapped.extend(extract_owl_char_axioms(store, &mut diagnostics));
    mapped.extend(extract_owl_structural_axioms(
        store,
        &handled,
        &mut diagnostics,
    ));
    extract_unmapped_owl_triples(store, &handled, &mut diagnostics);

    // Build LogicAxiom instances, dedup by content (the Python `set(...)`).
    let mut seen: HashSet<String> = HashSet::new();
    let mut axioms: Vec<LogicAxiom> = Vec::new();
    for m in mapped {
        match LogicAxiom::new(
            m.subject.clone(),
            m.predicate,
            m.obj,
            m.obj_is_literal,
            false,
            ContextualScope::default(),
        ) {
            Ok(ax) => {
                if seen.insert(axiom_key(&ax)) {
                    axioms.push(ax);
                }
            }
            Err(exc) => diagnostics.push(warn(
                "MALFORMED_ADAPTED_AXIOM",
                exc.message().to_owned(),
                Some(m.subject),
            )),
        }
    }

    // Lift the RDFS/SKOS annotation surface as first-class NodeKind::Annotation axioms — the
    // twin of the frontend lift, so an owl:/rdfs:-authored annotation and its logic: twin
    // normalize identically (the isomorphism gate). Deduped through the same content key.
    for ax in extract_annotation_axioms(store, &mut diagnostics) {
        if seen.insert(axiom_key(&ax)) {
            axioms.push(ax);
        }
    }

    // OWL/gUFO has no rule or profile surface (logic:-only).
    let program = LogicProgram::new(axioms, Vec::<LogicRule>::new(), Vec::new(), source_iri);
    Ok((program, diagnostics))
}

/// Normalize legacy `owl:*` / `gufo:` Turtle text into a [`LogicProgram`].
pub fn adapt_legacy_str(
    turtle: &str,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), LogicParseError> {
    // Native codec parse → frozen wasm-clean IR dataset, straight into the adapter
    // (no oxigraph Store hop).
    let dataset = parse_dataset(turtle.as_bytes(), "text/turtle", None)
        .map_err(|e| LogicParseError(format!("Failed to parse Turtle source: {e}")))?;
    adapt_legacy_dataset(dataset.as_ref(), source_iri)
}

/// Normalize a legacy `owl:*` / `gufo:` Turtle file into a [`LogicProgram`].
///
/// Native-only: `wasm32` has no filesystem, so the wasm-able compiler exposes only
/// the in-memory `adapt_legacy_str` / `adapt_legacy_dataset` entry points.
#[cfg(not(target_arch = "wasm32"))]
pub fn adapt_legacy_path(
    path: &Path,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), LogicParseError> {
    if !path.exists() {
        return Err(LogicParseError(format!(
            "Source file does not exist: {}",
            path.display()
        )));
    }
    let turtle = std::fs::read_to_string(path)
        .map_err(|e| LogicParseError(format!("Failed to read {}: {e}", path.display())))?;
    adapt_legacy_str(&turtle, source_iri)
}

#[cfg(test)]
mod tests;
