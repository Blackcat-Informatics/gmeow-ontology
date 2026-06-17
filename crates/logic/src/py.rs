// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3 Python bindings for `gmeow-logic`.
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
//! Encode/decode helpers (oxigraph term ⇄ Nemo fact string) live in
//! [`crate::encode`]; this module handles the PyO3 surface and chase wiring.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use oxigraph::io::RdfFormat;
use oxigraph::model::{GraphName, NamedNode, Term};
use oxigraph::store::Store;

use std::time::Instant;

use crate::certify::certify as certify_rules;
use crate::dispatch::dispatch_query;
use crate::encode::{
    decode_iri_term, decode_nemo_term, decode_string_constant, encode_quad_to_nemo_fact,
};
use crate::nemo_engine::{run_chase, ChaseRow, ChaseRowWithProvenance};
use crate::provenance::{mint_derivation_id, mint_reifier, ASSERT_RULE_IRI, LOGIC_NAMESPACE};
use crate::query_ir::{parse_query_program, Budget};
use crate::seam::{BudgetStatus, DerivationId, DerivedQuad, WorldStoreForeign};
use crate::store::WorldStore;

// ── Constants ──────────────────────────────────────────────────────────────────

/// The IRI used for the semantic/decidability profile.
const ASSERTED_PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";

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
fn derived_quad_to_dict(py: Python<'_>, dq: &DerivedQuad) -> PyResult<Py<PyAny>> {
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
    Ok(d.into_any().unbind())
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
/// # Budget governor (issue #502)
///
/// The optional `max_rule_firings`, `max_answers`, and `time_ms` parameters bound
/// the run. Asserted **EDB input facts are always kept in full** — a budget never
/// drops a given input quad; only **derived (IDB)** quads are bounded.
///
/// **The count ceilings (`max_rule_firings`, `max_answers`) are engine-independent
/// and deterministic.** Nemo's `reason()` runs to full fixpoint with no native
/// budget hook, and the Python oracle likewise runs to fixpoint under these
/// ceilings; both then truncate the derived set *post-hoc* to the canonical-sort
/// prefix of the **complete** derivation and stamp the kept quads
/// `BudgetStatus::Exhausted`. Because both engines compute the same fixpoint and
/// keep the same canonical prefix, the count ceilings yield identical verdicts.
///
/// **Only `time_ms` is engine-dependent.** The Python oracle can cut the chase
/// mid-flight on the wall clock; on the Rust side `time_ms` bounds only the
/// *post-fixpoint* work (decode + bookkeeping), not the chase itself. A genuinely
/// non-terminating rule set is the static certifier's job to reject up front (see
/// [`crate::certify`]), not the governor's to interrupt.
///
/// The `time_ms` divergence is **named, not glossed** (honesty invariant): under
/// the count ceilings the verdict and budget strings match the oracle exactly;
/// only on a wall-clock cut do the behaviours legitimately differ, and that
/// difference is documented here, in `certify.rs`, and in `crates/logic/README.md`.
///
/// When a ceiling trips, kept rows are a **sound subset** of the full fixpoint —
/// a prefix of the canonical (graph, S, P, O) sort — never fabricated. With all
/// three parameters `None` (the default), the output is **byte-identical to
/// pre-#502**: every quad keeps `budget_status = "ok"` and the chase-order output
/// is preserved unchanged.
///
/// # Errors
///
/// Returns a Python `ValueError` for N-Quads parse errors and
/// `RuntimeError` for chase or decode failures.
#[pyfunction]
#[pyo3(signature = (rules, input, max_rule_firings=None, max_answers=None, time_ms=None))]
fn materialize(
    py: Python<'_>,
    rules: &str,
    input: &str,
    max_rule_firings: Option<u64>,
    max_answers: Option<u64>,
    time_ms: Option<u64>,
) -> PyResult<Vec<Py<PyAny>>> {
    // Start the post-fixpoint wall-clock the instant we enter (the chase itself is
    // not interruptible; `time_ms` bounds the post-chase decode/bookkeeping — see
    // the budget-governor docs above and the honesty paragraph in README.md).
    let budget_active = max_rule_firings.is_some() || max_answers.is_some() || time_ms.is_some();
    let start = Instant::now();
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
        .detach(|| run_chase(rls))
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("chase error: {e}")))?;

    // ── 5. Decode ChaseRows → DerivedQuads with real provenance ──────────────
    // Carry the EDB/IDB flag alongside each quad so the budget governor can bound
    // IDB firings (`max_rule_firings`) without re-deriving provenance.
    let mut derived_quads: Vec<(DerivedQuad, bool)> = Vec::new();

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
            let source_refs: Vec<&str> = sources.iter().map(|s: &String| s.as_str()).collect();
            let deriv = mint_derivation_id(&rule, &source_refs);
            (rule, sources, deriv)
        };

        let is_edb = prov.is_edb;
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
            // Default-path budget status (overwritten below when a ceiling trips).
            budget_status: BudgetStatus::Ok,
        };
        derived_quads.push((dq, is_edb));
    }

    // ── 6. Post-hoc budget governor (issue #502) ─────────────────────────────
    // With no budget params (the default), this whole block is skipped, so the
    // output is byte-identical to pre-#502: chase order, every quad "ok".
    let final_quads: Vec<DerivedQuad> = if budget_active {
        apply_budget(derived_quads, max_rule_firings, max_answers, time_ms, start)
    } else {
        derived_quads.into_iter().map(|(dq, _edb)| dq).collect()
    };

    // ── 7. Serialize to Python dicts ─────────────────────────────────────────
    final_quads
        .iter()
        .map(|dq| derived_quad_to_dict(py, dq))
        .collect()
}

/// Canonical sort key for a derived quad: `(graph, subject, predicate, object)`.
///
/// This is the deterministic order the budget governor truncates to, so a kept
/// subset is always a sound *prefix* of a stable ordering — never a fabricated or
/// reordered result. The key uses the same string surfaces the seam already
/// projects (`graph`/`subject`/`predicate`/`object` display forms).
fn budget_sort_key(dq: &DerivedQuad) -> (String, String, String, String) {
    (
        dq.graph.as_str().to_owned(),
        dq.subject.to_string(),
        dq.predicate.as_str().to_owned(),
        dq.object.to_string(),
    )
}

/// Apply the post-hoc budget ceilings to the materialized quads.
///
/// Enforcement (mirrors the Python `materialize_program` ceilings, applied
/// post-fixpoint — see `gmeow_tools.logic_materialize`):
/// - **Asserted EDB facts are GIVEN and are NEVER truncated by a derivation
///   budget.** They are always kept in full; only **derived (IDB)** quads are
///   bounded. This is the sound-partial contract: a budget bounds derivation
///   work, not the input. (The Python oracle keeps EDB in a separate list that
///   the truncation never touches; this mirrors that.)
/// - `max_rule_firings` and `max_answers` each bound the count of **derived**
///   quads; the effective derived cap is the minimum of the declared ceilings.
///   The kept derived set is the canonical-sort PREFIX (by [`budget_sort_key`])
///   so a truncation is a reproducible, sound subset, identical to the Python
///   oracle's `(graph, subject, predicate, obj)` prefix.
/// - `time_ms` bounds the post-fixpoint wall-clock; when exceeded the result is
///   marked exhausted but never truncated below the count ceilings (we keep the
///   sound subset computed so far; we never fabricate).
///
/// When a ceiling trips, **every kept quad** (EDB and derived alike) is stamped
/// `BudgetStatus::Exhausted`, matching the Python oracle, which stamps every quad
/// of an exhausted run so the kept set is unambiguously a sound subset of the
/// full fixpoint, not the complete answer.
fn apply_budget(
    quads: Vec<(DerivedQuad, bool)>,
    max_rule_firings: Option<u64>,
    max_answers: Option<u64>,
    time_ms: Option<u64>,
    start: Instant,
) -> Vec<DerivedQuad> {
    // Split EDB (asserted, always kept) from IDB (derived, bounded by budget).
    let mut edb: Vec<DerivedQuad> = Vec::new();
    let mut idb: Vec<DerivedQuad> = Vec::new();
    for (dq, is_edb) in quads {
        if is_edb {
            edb.push(dq);
        } else {
            idb.push(dq);
        }
    }

    // Deterministic canonical order over the DERIVED quads so a truncation is a
    // sound prefix identical to the Python oracle's.
    idb.sort_by_key(budget_sort_key);

    // Effective derived cap = min of the declared count ceilings (each bounds
    // derived quads). EDB is never counted against either ceiling.
    let derived_cap: Option<usize> = match (max_rule_firings, max_answers) {
        (Some(a), Some(b)) => Some((a.min(b)) as usize),
        (Some(a), None) => Some(a as usize),
        (None, Some(b)) => Some(b as usize),
        (None, None) => None,
    };

    let mut exhausted = false;
    if let Some(cap) = derived_cap {
        if idb.len() > cap {
            idb.truncate(cap);
            exhausted = true;
        }
    }

    // Time ceiling bounds the post-fixpoint work. If exceeded, mark exhausted but
    // keep whatever sound subset the count ceilings allowed (never fabricate).
    if let Some(limit) = time_ms {
        let elapsed_ms = start.elapsed().as_millis() as u64;
        if elapsed_ms >= limit {
            exhausted = true;
        }
    }

    let status = if exhausted {
        BudgetStatus::Exhausted
    } else {
        BudgetStatus::Ok
    };

    // Emit EDB (full) + bounded IDB, all stamped with the run status. The kept
    // set ordering is not contractual (the diff compares quad SETS, not order),
    // but EDB-then-IDB keeps the output readable.
    edb.into_iter()
        .chain(idb)
        .map(|mut dq| {
            dq.budget_status = status;
            dq
        })
        .collect()
}

// ── certify ─────────────────────────────────────────────────────────────────

/// Statically certify a Nemo `.rls` rule set against a declared semantic profile.
///
/// This is the Rust mirror of the Python oracle
/// (`gmeow_tools.logic_certify.certify_program`). The returned dict has the SAME
/// shape, keys, and values as Python `CertificationVerdict.to_json()`:
///
/// ```python
/// {
///   "certified": bool,
///   "decidability_class": str,
///   "profile_id": str,
///   "violations": [str, …]   # sorted, byte-identical to the oracle
/// }
/// ```
///
/// `profile` matches the Python profile-id strings, e.g. `"PositiveHornProfile"`,
/// `"StratifiedNAFProfile"`, `"StableModelProfile"`. Certification uses
/// *sufficient* conditions and is *necessarily incomplete* (termination is
/// undecidable): a clean verdict proves membership in the declared
/// decidable/terminating fragment; a violation only proves the cheap structural
/// condition does not hold.
///
/// # Errors
///
/// Returns a Python `ValueError` if `rules` is not parseable Nemo `.rls`.
#[pyfunction]
fn certify(py: Python<'_>, rules: &str, profile: &str) -> PyResult<Py<PyAny>> {
    let verdict = certify_rules(rules, profile).map_err(|e| {
        pyo3::exceptions::PyValueError::new_err(format!("certify parse error: {e}"))
    })?;
    let (certified, decidability_class, profile_id, violations) = verdict.to_json_pairs();

    let d = PyDict::new(py);
    // Insert in the same sorted-key order Python's `to_json()` literal uses.
    d.set_item("certified", certified)?;
    d.set_item("decidability_class", decidability_class)?;
    d.set_item("profile_id", profile_id)?;
    d.set_item("violations", violations)?;
    Ok(d.into_any().unbind())
}

// ── query ───────────────────────────────────────────────────────────────────

/// Resolve a `.logic` backward goal over a materialized world (issue #504, v4).
///
/// This is the AC-1/AC-2 engine surface: it loads the materialized EDB, parses the
/// `.logic` query program (rules + goal), enforces the cut/profile gate, and routes
/// the goal through the dispatcher (oxigraph SPARQL fast path for non-recursive
/// pattern goals; embedded Scryer Prolog with tabling for recursive/unification-heavy
/// goals). Answers are **virtual** — nothing is written back into the store.
///
/// # Arguments
///
/// - `world_nquads` — the materialized world(s) as N-Quads (named graphs = worlds).
/// - `query_program` — the `.logic` source (prefixes, Horn rules, optional `!` cut,
///   exactly one `?- goal.`).
/// - `profile` — the semantic profile in force (bare name or IRI). Cut is permitted
///   ONLY under `ProceduralPrologProfile`; otherwise this raises `ValueError`.
/// - `world_iri` — which world to resolve against. If `None`, the store must contain
///   exactly one named graph (auto-selected); otherwise this is an error.
/// - `max_answers` — output cap (→ `status="partial"`).
/// - `max_steps` — inference-count ceiling (→ `status="exhausted"`).
///
/// # Returns
///
/// A dict `{"bindings": [ {var: canonical_str, …}, … ], "status": "ok"|"partial"|"exhausted"}`
/// where each canonical value is the oracle/engine `Const` form (`<iri>` for IRIs).
/// The binding list is canonically sorted for determinism.
///
/// # Errors
///
/// Raises Python `ValueError` on malformed N-Quads/query, a missing/ambiguous world,
/// a cut outside `ProceduralPrologProfile`, or a Scryer/resolution error.
#[pyfunction]
#[pyo3(signature = (world_nquads, query_program, profile, world_iri=None, max_answers=None, max_steps=None))]
fn query(
    py: Python<'_>,
    world_nquads: &str,
    query_program: &str,
    profile: &str,
    world_iri: Option<&str>,
    max_answers: Option<usize>,
    max_steps: Option<u64>,
) -> PyResult<Py<PyAny>> {
    let value_err = |msg: String| pyo3::exceptions::PyValueError::new_err(msg);

    // 1. Load the materialized EDB into a world-indexed store.
    let store = WorldStore::new();
    store.load_nquads(world_nquads).map_err(value_err)?;

    // 2. Resolve the target world (explicit, or the single named graph).
    let world = match world_iri {
        Some(w) => w.to_owned(),
        None => {
            let worlds = store.worlds();
            if worlds.len() != 1 {
                return Err(value_err(format!(
                    "world_iri not given and the store has {} named graphs \
                     (need exactly 1): {worlds:?}",
                    worlds.len()
                )));
            }
            worlds.into_iter().next().expect("len == 1")
        }
    };
    let world_nn = NamedNode::new(&world)
        .map_err(|e| value_err(format!("invalid world IRI {world:?}: {e}")))?;

    // 3. Parse the query program (rules + goal).
    let program = parse_query_program(query_program).map_err(value_err)?;

    // 4. Build the read-only EDB accessor for this world.
    let foreign = WorldStoreForeign::from_world(&store, &world, profile).map_err(value_err)?;

    // 5. Dispatch. A Stratum-C counterfactual program (#505) routes through
    //    transient world construction; a plain v4 backward goal runs against the
    //    materialized world via the cut/profile-gated dispatcher.
    let budget = Budget {
        max_answers,
        max_steps,
    };
    // The counterfactual path returns a CfAnswer (status may be "unknown" or
    // "incomplete"); the plain path returns an AnswerSet. Normalize both to a
    // binding list plus a canonical status string.
    let (answer_bindings, status_str): (Vec<crate::query_ir::Binding>, String) =
        if crate::counterfactual::is_counterfactual(&program) {
            // Honor a program-declared `depth_budget(N)`; otherwise the engine default.
            let depth = program
                .counterfactual
                .as_ref()
                .and_then(|c| c.depth_budget)
                .unwrap_or(crate::counterfactual::DEFAULT_DEPTH_BUDGET);
            let cf = crate::counterfactual::construct_and_resolve(
                &store, &program, profile, &budget, depth,
            )
            .map_err(value_err)?;
            (cf.bindings, cf.status.as_str().to_owned())
        } else {
            let answer = dispatch_query(&foreign, &store, &world_nn, &program, profile, &budget)
                .map_err(value_err)?;
            (answer.bindings, answer.status.as_str().to_owned())
        };

    // 6. Build the Python result dict: {"bindings": [...], "status": "..."}.
    let bindings = PyList::empty(py);
    for binding in &answer_bindings {
        let row = PyDict::new(py);
        for (var, val) in binding {
            row.set_item(var, val)?;
        }
        bindings.append(row)?;
    }
    let result = PyDict::new(py);
    result.set_item("bindings", bindings)?;
    result.set_item("status", status_str)?;
    Ok(result.into_any().unbind())
}

// ── Module registration ───────────────────────────────────────────────────────

/// Python extension module `gmeow_logic`.
///
/// Exposes:
/// - `materialize(rules, input, max_rule_firings=None, max_answers=None, time_ms=None)`
/// - `certify(rules, profile) -> dict`
/// - `query(world_nquads, query_program, profile, world_iri=None, max_answers=None, max_steps=None) -> dict`
#[pymodule]
fn gmeow_logic(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(materialize, m)?)?;
    m.add_function(wrap_pyfunction!(certify, m)?)?;
    m.add_function(wrap_pyfunction!(query, m)?)?;
    Ok(())
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::decode_nemo_term;
    use crate::provenance::term_n3;

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

    // ── decode_nemo_term imported from encode ─────────────────────────────────

    #[test]
    fn py_decode_nemo_term_iri_smoke() {
        // Verify that py.rs can use decode_nemo_term from crate::encode.
        let term = decode_nemo_term("<http://example.org/Smoke>").unwrap();
        match term {
            Term::NamedNode(nn) => assert_eq!(nn.as_str(), "http://example.org/Smoke"),
            other => panic!("expected NamedNode, got {other:?}"),
        }
    }
}
