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
//! # Scaffold note (issue #499 Task 4)
//!
//! `materialize` is the v0-scaffold entry point.  It does real quad +
//! derivation-metadata round-tripping through the oxigraph named-graph store
//! but does NOT yet invoke the Nemo chase.  The chase wire-up arrives in
//! issue #501.  Every input quad is tagged with asserted-fact provenance
//! (derivation_id, rule_iri, source_quad_ids, profile, budget_status) so the
//! full metadata pipeline is exercised end-to-end.

use pyo3::prelude::*;
use pyo3::types::PyDict;

use oxigraph::io::RdfFormat;
use oxigraph::model::{GraphName, NamedNode};
use oxigraph::store::Store;

use crate::seam::{BudgetStatus, DerivedQuad, DerivationId};

// ── Public profile IRI ─────────────────────────────────────────────────────────────────────────

/// The IRI used for input (asserted) facts in the scaffold materialize.
///
/// Real profiles are IRI-identified named individuals in the ontology; for
/// asserted base facts we use this constant as a sentinel so callers can
/// distinguish "came in as input" from "derived by rule X".
const ASSERTED_PROFILE: &str =
    "http://logic.gmeow.example/profile/MonotonicDatalogProfile";

/// The rule IRI used for asserted (non-derived) input quads in the scaffold.
const ASSERTED_RULE: &str = "http://logic.gmeow.example/rule/asserted";

// ── Helpers ───────────────────────────────────────────────────────────────────────────────────

/// Build a stable derivation-step IRI for an input quad identified by index.
///
/// The IRI is deterministic: `…/derivation/input/{n}`.  When Nemo is wired
/// in (#501) this will be replaced by Nemo's own derivation IDs.
fn input_derivation_id(n: usize) -> DerivationId {
    DerivationId(format!(
        "http://logic.gmeow.example/derivation/input/{n}"
    ))
}

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

// ── materialize ───────────────────────────────────────────────────────────────────────────────

/// Scaffold materializer: load `input` quads into the world store, attach
/// derivation metadata, and return the annotated quads.
///
/// # Arguments
///
/// - `rules`  — rule set string (ignored at v0; Nemo chase is issue #501).
/// - `input`  — N-Quads string.  Each quad is loaded into the named graph
///              that matches its graph component (the "world" in gmeow-logic).
///
/// # Returns
///
/// A list of Python dicts, one per input quad, each carrying the full seam
/// metadata (graph, subject, predicate, object, graph_component, derivation_id,
/// rule_iri, source_quad_ids, profile, budget_status).  The round-trip
/// preserves the named-graph of each quad: a quad in world W comes back with
/// `graph == W`.
///
/// An empty `input` string returns an empty list.
///
/// # Errors
///
/// Returns a Python `ValueError` if the N-Quads input cannot be parsed.
#[pyfunction]
fn materialize(py: Python<'_>, _rules: &str, input: &str) -> PyResult<Vec<PyObject>> {
    // ── Load into an in-memory oxigraph store ─────────────────────────────
    let store = Store::new().map_err(|e| {
        pyo3::exceptions::PyRuntimeError::new_err(format!("store creation failed: {e}"))
    })?;

    if !input.trim().is_empty() {
        store
            .load_from_reader(RdfFormat::NQuads, input.as_bytes())
            .map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "N-Quads parse error: {e}"
                ))
            })?;
    }

    // ── Collect quads with derivation metadata ────────────────────────────
    let mut derived_quads: Vec<DerivedQuad> = Vec::new();

    for (idx, result) in store.iter().enumerate() {
        let quad = result.map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("store iteration error: {e}"))
        })?;

        // Graph component — we only expose named-graph quads (default graph is
        // not a "world" in gmeow-logic semantics).
        let graph_nn: NamedNode = match &quad.graph_name {
            GraphName::NamedNode(n) => n.clone(),
            // Default-graph quads are stored under a synthetic world IRI so
            // the metadata contract still holds (every quad has a world).
            GraphName::DefaultGraph => NamedNode::new(
                "http://logic.gmeow.example/world/default",
            )
            .expect("static IRI is valid"),
            GraphName::BlankNode(b) => NamedNode::new(format!(
                "http://logic.gmeow.example/world/bnode/{}",
                b.as_str()
            ))
            .expect("blank-node world IRI is valid"),
        };

        // subject / predicate / object as oxigraph Terms
        use oxigraph::model::Term;
        let subject_term: Term = quad.subject.into();
        let object_term: Term = quad.object;
        let predicate_nn: NamedNode = quad.predicate;

        let derivation_id = input_derivation_id(idx);

        let dq = DerivedQuad {
            graph: graph_nn.clone(),
            subject: subject_term,
            predicate: predicate_nn,
            object: object_term,
            graph_component: graph_nn,
            derivation_id,
            rule_iri: ASSERTED_RULE.to_owned(),
            source_quad_ids: vec![],
            profile: ASSERTED_PROFILE.to_owned(),
            budget_status: BudgetStatus::Ok,
        };

        derived_quads.push(dq);
    }

    // ── Serialize to Python dicts ─────────────────────────────────────────
    derived_quads
        .iter()
        .map(|dq| derived_quad_to_dict(py, dq))
        .collect()
}

// ── Module registration ───────────────────────────────────────────────────────────────────────

/// Python extension module `gmeow_logic`.
///
/// Exposes the `materialize(rules, input)` function.
#[pymodule]
fn gmeow_logic(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(materialize, m)?)?;
    Ok(())
}
