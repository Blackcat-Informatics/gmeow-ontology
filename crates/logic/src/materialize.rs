// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Pure-Rust materialization core — the engine pipeline behind the
//! `gmeow_logic.materialize` PyO3 binding, extracted so it is unit-testable
//! natively (no Python, no FFI).
//!
//! The PyO3 wrapper in [`crate::py`] keeps only the marshalling shell: the empty
//! short-circuit, the non-stratifiable native routing (issue #651, which returns
//! Python rows), and the `DerivedQuad → PyDict` serialization. Everything between
//! — parse N-Quads → encode Nemo facts → run the chase → decode rows to
//! [`DerivedQuad`]s with real provenance → apply the post-hoc budget governor —
//! lives here and is exercised by both the FFI and native `#[test]`s.
//!
//! The split is behaviour-preserving: with all budget parameters `None` (the
//! default), [`materialize_core`] produces the exact same `DerivedQuad` sequence
//! the inlined FFI path did, preserving the oracle≡engine parity guarantee.

use oxigraph::model::{GraphName, NamedNode, Term};

use std::time::Instant;

use gmeow_rdf::oxigraph::{store_from_dataset, GraphPolicy};
use gmeow_rdf::parse_dataset;

use crate::encode::{
    decode_iri_term, decode_nemo_term, decode_string_constant, encode_quad_to_nemo_fact,
};
use crate::nemo_engine::{run_chase, ChaseRow, ChaseRowWithProvenance};
use crate::provenance::{mint_derivation_id, mint_reifier, ASSERT_RULE_IRI, LOGIC_NAMESPACE};
use crate::result::PreservationClaim;
use crate::seam::{BudgetStatus, DerivationId, DerivedQuad};

// ── Constants ──────────────────────────────────────────────────────────────────

/// The IRI used for the semantic/decidability profile of asserted/materialized quads.
pub(crate) const ASSERTED_PROFILE: &str =
    "https://blackcatinformatics.ca/logic/PositiveHornProfile";

// ── Error ──────────────────────────────────────────────────────────────────────

/// Failure modes of [`materialize_core`].
///
/// The variants mirror the two Python-visible exception types the FFI wrapper
/// raises: [`MaterializeError::Parse`] maps to `ValueError` (a malformed N-Quads
/// input the caller can fix), [`MaterializeError::Chase`] maps to `RuntimeError`
/// (an internal chase/decode failure). Keeping them distinct preserves the
/// pre-extraction exception contract byte-for-byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterializeError {
    /// The input N-Quads failed to parse — surfaced to Python as `ValueError`.
    Parse(String),
    /// The chase, a term decode, or provenance computation failed — surfaced to
    /// Python as `RuntimeError`.
    Chase(String),
}

impl std::fmt::Display for MaterializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaterializeError::Parse(m) | MaterializeError::Chase(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for MaterializeError {}

// ── Provenance helpers ────────────────────────────────────────────────────────

/// Compute the reifier IRI for a decoded quad's (S, P, O) triple.
///
/// Uses [`crate::provenance::mint_reifier`] on the already-decoded oxigraph
/// terms so the result is byte-identical to the Python oracle.
///
/// # Errors
///
/// Returns an error if subject or object is an RDF-star quoted triple.
pub(crate) fn reifier_for_quad(
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
pub(crate) fn reifier_for_antecedent_row(row: &ChaseRow) -> Result<String, String> {
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

// ── Core pipeline ───────────────────────────────────────────────────────────────

/// Materialize `input` N-Quads under `rules`, returning derived quads with real
/// provenance and (optional) budget bookkeeping.
///
/// This is the pure engine pipeline — no PyO3, no GIL, no Python oracle. The FFI
/// wrapper in [`crate::py`] handles the empty short-circuit, non-stratifiable
/// native routing, and the dict marshalling; everything else is here.
///
/// With all of `max_rule_firings`/`max_answers`/`time_ms` set to `None` (the
/// default), the post-hoc budget governor is skipped entirely and the output is
/// byte-identical to the pre-#502 chase order with every quad `budget_status = Ok`.
///
/// # Errors
///
/// Returns [`MaterializeError::Parse`] for malformed N-Quads input and
/// [`MaterializeError::Chase`] for chase, decode, or provenance failures.
pub fn materialize_core(
    rules: &str,
    input: &str,
    max_rule_firings: Option<u64>,
    max_answers: Option<u64>,
    time_ms: Option<u64>,
) -> Result<Vec<DerivedQuad>, MaterializeError> {
    // Start the post-fixpoint wall-clock the instant we enter (the chase itself is
    // not interruptible; `time_ms` bounds the post-chase decode/bookkeeping).
    let budget_active = max_rule_firings.is_some() || max_answers.is_some() || time_ms.is_some();
    let start = Instant::now();

    // ── Short-circuit: nothing to do ──────────────────────────────────────────
    if input.trim().is_empty() {
        return Ok(vec![]);
    }

    // ── 1. Parse input N-Quads through the native codec, then fold into an
    //       oxigraph Store (text-free IR → Store hop, #909) ───────────────────
    let dataset = parse_dataset(input.as_bytes(), "application/n-quads", None)
        .map_err(|e| MaterializeError::Parse(format!("N-Quads parse error: {e}")))?;
    let store = store_from_dataset(dataset.as_ref(), GraphPolicy::PreserveNamedGraphs)
        .map_err(|e| MaterializeError::Chase(format!("store materialization failed: {e}")))?;

    // ── 2. Encode each quad as a Nemo ground-fact line ───────────────────────
    let mut fact_lines: Vec<String> = Vec::new();
    for result in store.iter() {
        let quad =
            result.map_err(|e| MaterializeError::Chase(format!("store iteration error: {e}")))?;

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

    // ── 4. Run the Nemo chase ────────────────────────────────────────────────
    let rows_with_prov: Vec<ChaseRowWithProvenance> =
        run_chase(rls).map_err(|e| MaterializeError::Chase(format!("chase error: {e}")))?;

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
            MaterializeError::Chase(format!("invalid predicate IRI {predicate_iri:?}: {e}"))
        })?;

        // subject: must be an IRI term
        let subject_iri = decode_iri_term(&row.values[0])
            .map_err(|e| MaterializeError::Chase(format!("row[{idx}] subject: {e}")))?;
        let subject_nn = NamedNode::new(&subject_iri).map_err(|e| {
            MaterializeError::Chase(format!("row[{idx}] subject IRI {subject_iri:?}: {e}"))
        })?;
        let subject_term = Term::NamedNode(subject_nn);

        // object: IRI, typed literal, language literal, or plain literal
        let object_term = decode_nemo_term(&row.values[1])
            .map_err(|e| MaterializeError::Chase(format!("row[{idx}] object: {e}")))?;

        // context (world): Nemo string constant → strip outer double-quotes
        let world_str = decode_string_constant(&row.values[2])
            .map_err(|e| MaterializeError::Chase(format!("row[{idx}] world: {e}")))?;
        let graph_nn = NamedNode::new(&world_str).map_err(|e| {
            MaterializeError::Chase(format!("row[{idx}] world IRI {world_str:?}: {e}"))
        })?;

        // ── Real provenance computation ───────────────────────────────────────
        let self_reifier = reifier_for_quad(&subject_term, &predicate_nn, &object_term)
            .map_err(|e| MaterializeError::Chase(format!("row[{idx}] reifier error: {e}")))?;

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
                    MaterializeError::Chase(format!("row[{idx}] antecedent decode error: {e}"))
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

    Ok(final_quads)
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
/// Enforcement rules (applied post-fixpoint; the retired Python oracle
/// `gmeow_tools.logic_materialize` used the same contract):
/// - **Asserted EDB facts are GIVEN and are NEVER truncated by a derivation
///   budget.** They are always kept in full; only **derived (IDB)** quads are
///   bounded. This is the sound-partial contract: a budget bounds derivation
///   work, not the input.
/// - `max_rule_firings` and `max_answers` each bound the count of **derived**
///   quads; the effective derived cap is the minimum of the declared ceilings.
///   The kept derived set is the canonical-sort PREFIX (by [`budget_sort_key`])
///   so a truncation is a reproducible, sound subset keyed on
///   `(graph, subject, predicate, obj)`.
/// - `time_ms` bounds the post-fixpoint wall-clock; when exceeded the result is
///   marked exhausted but never truncated below the count ceilings (we keep the
///   sound subset computed so far; we never fabricate).
///
/// When a ceiling trips, **every kept quad** (EDB and derived alike) is stamped
/// `BudgetStatus::Exhausted`, so the kept set is unambiguously a sound subset of
/// the full fixpoint, not the complete answer.
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

// ── Profile-routed materialization (issue #785) ──────────────────────────────────
//
// The native conformance harness (`crates/conformance`) drives the engine
// directly, with no PyO3 boundary. It therefore needs ONE public entry point that
// reproduces the routing the `gmeow_logic.materialize` PyO3 wrapper performs in
// [`crate::py`]: empty-input short-circuit, the non-stratifiable native routing
// (well-founded / cautious-stable / declared-StratifiedNAF echo, issue #651), and
// otherwise the Nemo [`materialize_core`] chase. The wrapper's routing logic and
// this function are the SAME native evaluators (`crate::wellfounded`,
// `crate::stablemodel`, `crate::rule_ir`), so the produced quads are identical by
// construction — the harness is not a second engine, only a second caller.

/// Convert a non-stratifiable [`crate::rule_ir::DerivedRow`] to a [`DerivedQuad`].
///
/// Mirrors `py::derived_rows_to_dicts`: the native non-stratifiable paths run to a
/// polynomial fixpoint with no budget ceiling, so every quad is stamped
/// [`ASSERTED_PROFILE`] / [`BudgetStatus::Ok`], and the quad is self-contained
/// (`graph_component == graph`).
fn derived_row_to_quad(row: crate::rule_ir::DerivedRow) -> Result<DerivedQuad, MaterializeError> {
    let graph = NamedNode::new(&row.graph)
        .map_err(|e| MaterializeError::Chase(format!("invalid world IRI {:?}: {e}", row.graph)))?;
    Ok(DerivedQuad {
        graph: graph.clone(),
        subject: row.subject,
        predicate: row.predicate,
        object: row.object,
        graph_component: graph,
        derivation_id: DerivationId(row.derivation_id),
        rule_iri: row.rule_iri,
        source_quad_ids: row.source_quad_ids,
        profile: ASSERTED_PROFILE.to_owned(),
        budget_status: BudgetStatus::Ok,
    })
}

/// A profile-routed materialization: the derived quads plus the preservation
/// judgment disclosing any derivation rules the routing could not evaluate
/// (downstream disclosure). The faithful evaluators — the Nemo chase, the
/// well-founded alternating fixpoint, and the cautious-stable materializer — carry
/// `{exact}`; the non-stratifiable EDB-echo path, which materializes only the
/// asserted facts because the declared engine cannot evaluate the rules, carries
/// `{sound-under}` naming the dropped rule IRIs, so the loss is disclosed rather
/// than silently swallowed.
#[derive(Debug, Clone)]
pub struct Materialization {
    /// The derived quads (asserted EDB + any derived IDB).
    pub quads: Vec<DerivedQuad>,
    /// The preservation judgment for this materialization.
    pub preservation: PreservationClaim,
}

/// The rule IRIs of a non-stratifiable rule set — the derivation rules the EDB-echo
/// path could not evaluate (downstream disclosure). The IRIs come from each rule's
/// `#[name(...)]`, or `logic:rule/anonymous` for unnamed rules. Best-effort: if the
/// text does not re-parse as eval rules, a single generic marker is returned so the
/// loss is named, never silent.
fn dropped_rule_constructs(rules: &str) -> Vec<String> {
    match crate::rule_ir::parse_eval_rules(rules) {
        Ok(parsed) => parsed.into_iter().map(|r| r.rule_iri).collect(),
        Err(_) => vec![format!("{LOGIC_NAMESPACE}rule/non-stratifiable")],
    }
}

/// The preservation judgment for a `(rules, profile)` pair — a property of the
/// program and engine, independent of the input EDB. The faithful evaluators
/// (well-founded, cautious-stable, the Nemo positive-Horn chase, and empty or
/// genuinely stratifiable rule sets) carry `{exact}`; a declared StratifiedNAF set
/// that FAILS stratification is echoed asserted-only, dropping — and disclosing —
/// its derivation rules as `{sound-under}`. Computed before quad generation so the
/// empty-input fast path discloses the SAME set a populated EDB would: an empty
/// world must not erase the unsupported-rule disclosure (the legalization floor).
fn routed_preservation(
    rules: &str,
    profile: Option<&str>,
) -> Result<PreservationClaim, MaterializeError> {
    let faithful = matches!(
        profile,
        Some("WellFoundedProfile") | Some("StableModelProfile")
    ) || rules.trim().is_empty()
        || crate::certify::is_stratifiable(rules).map_err(MaterializeError::Parse)?;
    Ok(if faithful {
        PreservationClaim::exact()
    } else {
        PreservationClaim::for_unsupported(dropped_rule_constructs(rules))
    })
}

/// Echo only the asserted EDB facts per world (the honest minimal materialization
/// for a declared `StratifiedNAFProfile` set that fails stratification). Mirrors
/// `py::echo_edb_only`.
fn echo_edb_only(input: &str) -> Result<Vec<crate::rule_ir::DerivedRow>, MaterializeError> {
    let store = crate::store::WorldStore::new();
    store.load_nquads(input).map_err(MaterializeError::Parse)?;
    let mut worlds = store.worlds();
    worlds.sort();
    let mut rows = Vec::new();
    for world in &worlds {
        let edb =
            crate::rule_ir::world_edb_facts(&store, world).map_err(MaterializeError::Chase)?;
        rows.extend(crate::rule_ir::echo_asserted(world, &edb).map_err(MaterializeError::Chase)?);
    }
    Ok(rows)
}

/// Materialize the forward chase, routing by declared semantic `profile`.
///
/// This is the public, PyO3-free entry point the conformance harness (#785) calls.
/// It reproduces the routing of the `gmeow_logic.materialize` wrapper exactly:
///
/// * empty `input` ⇒ empty result;
/// * `Some("WellFoundedProfile")` ⇒ the native alternating-fixpoint evaluator;
/// * `Some("StableModelProfile")` ⇒ the native cautious (skeptical) materializer;
/// * any other profile (or `None`) ⇒ the Nemo [`materialize_core`] chase, EXCEPT a
///   non-empty rule set that fails stratification, which is echoed asserted-only.
///
/// Returns the derived quads (asserted EDB + derived IDB) with full provenance.
///
/// # Errors
/// [`MaterializeError::Parse`] for malformed N-Quads / `.rls`; [`MaterializeError::Chase`]
/// for an evaluation or provenance failure.
pub fn materialize_routed(
    rules: &str,
    input: &str,
    max_rule_firings: Option<u64>,
    max_answers: Option<u64>,
    time_ms: Option<u64>,
    profile: Option<&str>,
) -> Result<Materialization, MaterializeError> {
    // Preservation is a property of `(rules, profile)`, independent of the EDB — derive
    // it ONCE (One-Path) so every return path below (empty input, native routing, the
    // Nemo chase) discloses the SAME judgment. An empty world must never erase the
    // unsupported-rule disclosure a populated one would carry.
    let preservation = routed_preservation(rules, profile)?;

    if input.trim().is_empty() {
        return Ok(Materialization {
            quads: vec![],
            preservation,
        });
    }

    // Non-stratifiable native routing. `None` ⇒ fall through to Nemo. The well-founded
    // and cautious-stable evaluators run their native fixpoints; any other profile with
    // a declared set that fails stratification (⇔ `preservation` discloses dropped
    // rules) is echoed asserted-only.
    let routed: Option<Vec<crate::rule_ir::DerivedRow>> = match profile {
        Some("WellFoundedProfile") => {
            let store = crate::store::WorldStore::new();
            store.load_nquads(input).map_err(MaterializeError::Parse)?;
            let eval_rules =
                crate::rule_ir::parse_eval_rules(rules).map_err(MaterializeError::Parse)?;
            Some(
                crate::wellfounded::materialize(&store, &eval_rules)
                    .map_err(MaterializeError::Chase)?,
            )
        }
        Some("StableModelProfile") => {
            let store = crate::store::WorldStore::new();
            store.load_nquads(input).map_err(MaterializeError::Parse)?;
            let eval_rules =
                crate::rule_ir::parse_eval_rules(rules).map_err(MaterializeError::Parse)?;
            Some(
                crate::stablemodel::cautious_materialize(&store, &eval_rules)
                    .map_err(MaterializeError::Chase)?,
            )
        }
        _ => {
            // PositiveHorn / declared StratifiedNAF / Probabilistic / Procedural / None.
            // A declared set that FAILS stratification has a non-empty unsupported set and
            // is echoed asserted-only. Otherwise the stratifiable Datalog± fragment is the
            // native physical core's competence: TRY it first (native is authoritative for
            // what it decides) and fall through to the Nemo fallback ONLY for a declared
            // native gap (`NativeOutcome::Unsupported`). `preservation` is exact for this
            // arm, so the native and Nemo paths disclose the same judgment.
            if !preservation.unsupported_constructs.is_empty() {
                Some(echo_edb_only(input)?)
            } else if max_rule_firings.is_some() || max_answers.is_some() || time_ms.is_some() {
                // Budget-constrained materialization stays on the Nemo fallback: the
                // native core decides the WHOLE stratifiable least model and does not yet
                // reproduce the oracle's post-hoc budget governor (the truncation cut and
                // the exhausted / incomplete disclosure it stamps). An unbudgeted call is
                // native's competence; a budgeted one is a declared native gap → Nemo.
                None
            } else {
                let store = crate::store::WorldStore::new();
                store.load_nquads(input).map_err(MaterializeError::Parse)?;
                let eval_rules =
                    crate::rule_ir::parse_eval_rules(rules).map_err(MaterializeError::Parse)?;
                match crate::physical::materialize_native(&store, &eval_rules)
                    .map_err(MaterializeError::Chase)?
                {
                    crate::physical::NativeOutcome::Decided(rows) => Some(rows),
                    // A declared native gap (e.g. non-stratifiable after parse) falls
                    // through to the demoted Nemo fallback / conformance oracle.
                    crate::physical::NativeOutcome::Unsupported(_) => None,
                }
            }
        }
    };

    if let Some(rows) = routed {
        let quads = rows
            .into_iter()
            .map(derived_row_to_quad)
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Materialization {
            quads,
            preservation,
        });
    }

    // Stratifiable / projection-only ⇒ the Nemo chase (with the post-hoc governor).
    // The chase is faithful for the positive-Horn fragment, so the claim is exact.
    let quads = materialize_core(rules, input, max_rule_firings, max_answers, time_ms)?;
    Ok(Materialization {
        quads,
        preservation,
    })
}

#[cfg(test)]
mod tests {
    //! Native coverage of the materialize engine pipeline (issue #786 / T5).
    //!
    //! These are the Rust ports of the engine assertions that previously lived in
    //! `tests/test_logic_engine.py` and drove `gmeow_logic.materialize` through
    //! PyO3. They exercise the pure [`materialize_core`] directly — no Python, no
    //! FFI — so the chase, world mapping, real provenance, and the assert-sentinel
    //! contract are pinned natively. The PyO3 binding retains only a thin
    //! marshalling smoke (`tests/test_logic_engine.py`).

    use super::*;

    // ── Fixtures ────────────────────────────────────────────────────────────────

    /// N-Quads covering two distinct named-graph worlds.
    const TWO_WORLD_NQUADS: &str = concat!(
        "<http://example.org/s/1> <http://example.org/p/type> <http://example.org/o/Thing> <http://world/Alpha> .\n",
        "<http://example.org/s/2> <http://example.org/p/name> <http://example.org/o/Foo> <http://world/Alpha> .\n",
        "<http://example.org/s/3> <http://example.org/p/type> <http://example.org/o/Bar> <http://world/Beta> .\n",
    );

    /// A subClassOf chain in one world: Dog ⊑ Mammal, Mammal ⊑ Animal (Alpha).
    const CHAIN_NQUADS: &str = concat!(
        "<http://example.org/Dog> <https://blackcatinformatics.ca/logic/subClassOf> <http://example.org/Mammal> <http://world/Alpha> .\n",
        "<http://example.org/Mammal> <https://blackcatinformatics.ca/logic/subClassOf> <http://example.org/Animal> <http://world/Alpha> .\n",
    );

    /// Transitivity rule in Nemo IRI-predicate syntax.
    const TRANSITIVITY_RULES: &str = concat!(
        "<https://blackcatinformatics.ca/logic/subClassOf>(?X, ?Z, ?C0) :-\n",
        "    <https://blackcatinformatics.ca/logic/subClassOf>(?X, ?Y, ?C0),\n",
        "    <https://blackcatinformatics.ca/logic/subClassOf>(?Y, ?Z, ?C1) .\n",
    );

    /// Named-rule variant: `#[name("...")]` makes the rule IRI flow through.
    const NAMED_RULE_IRI: &str =
        "https://blackcatinformatics.ca/logic/rules/subClassOf-transitivity";
    const NAMED_TRANSITIVITY_RULES: &str = concat!(
        "#[name(\"https://blackcatinformatics.ca/logic/rules/subClassOf-transitivity\")]\n",
        "<https://blackcatinformatics.ca/logic/subClassOf>(?X, ?Z, ?C0) :-\n",
        "    <https://blackcatinformatics.ca/logic/subClassOf>(?X, ?Y, ?C0),\n",
        "    <https://blackcatinformatics.ca/logic/subClassOf>(?Y, ?Z, ?C1) .\n",
    );

    /// The bare subClassOf predicate IRI (as it appears in `predicate.as_str()`).
    const SUBCLASS_PRED: &str = "https://blackcatinformatics.ca/logic/subClassOf";

    // ── Helpers ─────────────────────────────────────────────────────────────────

    /// Materialize with the default (no-budget) parameters and unwrap.
    fn run(rules: &str, input: &str) -> Vec<DerivedQuad> {
        materialize_core(rules, input, None, None, None).expect("materialize_core must not fail")
    }

    /// Collect the `(subject, object)` display pairs for the subClassOf predicate.
    fn sco_pairs(quads: &[DerivedQuad]) -> std::collections::HashSet<(String, String)> {
        quads
            .iter()
            .filter(|q| q.predicate.as_str() == SUBCLASS_PRED)
            .map(|q| (q.subject.to_string(), q.object.to_string()))
            .collect()
    }

    // ── AC#2: round-trip with derivation metadata ────────────────────────────────

    #[test]
    fn materialize_core_returns_all_input_quads() {
        let result = run("", TWO_WORLD_NQUADS);
        assert_eq!(result.len(), 3, "expected 3 quads back");
        let subjects: std::collections::HashSet<String> =
            result.iter().map(|q| q.subject.to_string()).collect();
        let expected: std::collections::HashSet<String> = [
            "<http://example.org/s/1>",
            "<http://example.org/s/2>",
            "<http://example.org/s/3>",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(subjects, expected, "subject set mismatch");
    }

    #[test]
    fn materialize_core_graph_is_correct_world() {
        let result = run("", TWO_WORLD_NQUADS);
        let worlds: std::collections::HashSet<&str> =
            result.iter().map(|q| q.graph.as_str()).collect();
        let expected: std::collections::HashSet<&str> = ["http://world/Alpha", "http://world/Beta"]
            .into_iter()
            .collect();
        assert_eq!(worlds, expected, "world set mismatch");
    }

    #[test]
    fn materialize_core_graph_equals_graph_component() {
        for q in run("", TWO_WORLD_NQUADS) {
            assert_eq!(
                q.graph.as_str(),
                q.graph_component.as_str(),
                "graph and graph_component must be identical"
            );
        }
    }

    #[test]
    fn materialize_core_asserted_budget_status_is_ok() {
        for q in run("", TWO_WORLD_NQUADS) {
            assert_eq!(
                q.budget_status,
                BudgetStatus::Ok,
                "asserted quad must be Ok"
            );
        }
    }

    #[test]
    fn materialize_core_derivation_id_is_nonempty() {
        for q in run("", TWO_WORLD_NQUADS) {
            assert!(
                !q.derivation_id.as_str().is_empty(),
                "derivation_id must be non-empty"
            );
        }
    }

    #[test]
    fn materialize_core_rule_iri_is_nonempty() {
        for q in run("", TWO_WORLD_NQUADS) {
            assert!(!q.rule_iri.is_empty(), "rule_iri must be non-empty");
        }
    }

    #[test]
    fn materialize_core_profile_is_the_asserted_profile() {
        for q in run("", TWO_WORLD_NQUADS) {
            assert_eq!(
                q.profile, ASSERTED_PROFILE,
                "profile must be the asserted profile"
            );
        }
    }

    #[test]
    fn materialize_core_required_fields_are_welltyped() {
        // Every materialized quad must carry non-degenerate values across all the
        // surface fields the FFI marshals into the Python dict.
        for q in run("", TWO_WORLD_NQUADS) {
            assert!(!q.graph.as_str().is_empty());
            assert!(!q.subject.to_string().is_empty());
            assert!(!q.predicate.as_str().is_empty());
            assert!(!q.object.to_string().is_empty());
            assert!(!q.derivation_id.as_str().is_empty());
            assert!(!q.rule_iri.is_empty());
            assert!(!q.profile.is_empty());
            // Asserted quads carry exactly their own self-reifier as the source.
            assert_eq!(
                q.source_quad_ids.len(),
                1,
                "asserted quad has one source (self)"
            );
            assert!(q.source_quad_ids.iter().all(|s| !s.is_empty()));
        }
    }

    #[test]
    fn materialize_core_world_isolation() {
        let result = run("", TWO_WORLD_NQUADS);
        let alpha: Vec<&DerivedQuad> = result
            .iter()
            .filter(|q| q.graph.as_str() == "http://world/Alpha")
            .collect();
        let beta: Vec<&DerivedQuad> = result
            .iter()
            .filter(|q| q.graph.as_str() == "http://world/Beta")
            .collect();
        assert_eq!(alpha.len(), 2, "expected 2 Alpha quads");
        assert_eq!(beta.len(), 1, "expected 1 Beta quad");

        let alpha_subjects: std::collections::HashSet<String> =
            alpha.iter().map(|q| q.subject.to_string()).collect();
        let beta_subjects: std::collections::HashSet<String> =
            beta.iter().map(|q| q.subject.to_string()).collect();
        assert!(
            alpha_subjects.is_disjoint(&beta_subjects),
            "cross-world subject leak detected"
        );
    }

    #[test]
    fn materialize_core_asserted_quads_carry_assert_sentinel() {
        for q in run("", TWO_WORLD_NQUADS) {
            assert_eq!(
                q.rule_iri, ASSERT_RULE_IRI,
                "asserted quad must carry the logic:assert sentinel"
            );
        }
    }

    // ── AC#4: empty-case ─────────────────────────────────────────────────────────

    #[test]
    fn materialize_core_empty_input_returns_empty() {
        assert!(run("", "").is_empty(), "empty input must return no quads");
    }

    #[test]
    fn materialize_core_whitespace_input_returns_empty() {
        assert!(
            run("", "   \n  \t  ").is_empty(),
            "whitespace input must return no quads"
        );
    }

    // ── AC#5: real inference with transitivity ───────────────────────────────────

    #[test]
    fn materialize_core_inference_derives_transitive_quad() {
        let result = run(TRANSITIVITY_RULES, CHAIN_NQUADS);
        let pairs = sco_pairs(&result);
        assert!(
            pairs.contains(&(
                "<http://example.org/Dog>".to_string(),
                "<http://example.org/Animal>".to_string()
            )),
            "transitive closure (Dog, Animal) not derived; pairs: {pairs:?}"
        );
    }

    #[test]
    fn materialize_core_inference_world_isolation() {
        let result = run(TRANSITIVITY_RULES, CHAIN_NQUADS);
        assert!(!result.is_empty(), "expected derived quads");
        let worlds: std::collections::HashSet<&str> =
            result.iter().map(|q| q.graph.as_str()).collect();
        assert_eq!(
            worlds,
            ["http://world/Alpha"].into_iter().collect(),
            "derivation must stay in world Alpha"
        );
    }

    #[test]
    fn materialize_core_inference_input_quads_still_present() {
        let pairs = sco_pairs(&run(TRANSITIVITY_RULES, CHAIN_NQUADS));
        assert!(
            pairs.contains(&(
                "<http://example.org/Dog>".to_string(),
                "<http://example.org/Mammal>".to_string()
            )),
            "input quad Dog->Mammal missing"
        );
        assert!(
            pairs.contains(&(
                "<http://example.org/Mammal>".to_string(),
                "<http://example.org/Animal>".to_string()
            )),
            "input quad Mammal->Animal missing"
        );
    }

    // ── AC#6: real provenance on derived quads ───────────────────────────────────

    /// Find the single derived Dog->Animal transitive quad.
    fn dog_animal(quads: &[DerivedQuad]) -> DerivedQuad {
        let derived: Vec<&DerivedQuad> = quads
            .iter()
            .filter(|q| {
                q.predicate.as_str() == SUBCLASS_PRED
                    && q.subject.to_string() == "<http://example.org/Dog>"
                    && q.object.to_string() == "<http://example.org/Animal>"
            })
            .collect();
        assert_eq!(
            derived.len(),
            1,
            "expected exactly one Dog->Animal derived quad"
        );
        derived[0].clone()
    }

    #[test]
    fn materialize_core_derived_quad_has_nonempty_source_quad_ids() {
        let result = run(TRANSITIVITY_RULES, CHAIN_NQUADS);
        let da = dog_animal(&result);
        assert!(
            !da.source_quad_ids.is_empty(),
            "derived quad must carry real antecedents"
        );
        assert!(
            da.source_quad_ids.iter().all(|s| !s.is_empty()),
            "every source_quad_id must be a non-empty reifier IRI"
        );
    }

    #[test]
    fn materialize_core_derived_rule_iri_is_not_assert_sentinel() {
        let result = run(TRANSITIVITY_RULES, CHAIN_NQUADS);
        let da = dog_animal(&result);
        assert_ne!(
            da.rule_iri, ASSERT_RULE_IRI,
            "a derived quad must NOT carry the assert sentinel"
        );
    }

    #[test]
    fn materialize_core_named_rule_iri_flows_through() {
        let result = run(NAMED_TRANSITIVITY_RULES, CHAIN_NQUADS);
        let da = dog_animal(&result);
        assert_eq!(
            da.rule_iri, NAMED_RULE_IRI,
            "named-rule IRI must flow through to the derived quad's rule_iri"
        );
    }

    // ── downstream disclosure of unsupported (dropped) rules ───────────────────

    use gmeow_logic_compile::ir::PreservationKind;

    /// A win/lose negation cycle: `lose ⊃ win`, `win ⊃ move ∧ ¬lose`. Negation in a
    /// cycle ⇒ not stratifiable, so a declared `StratifiedNAFProfile` set echoes the
    /// asserted EDB only and DROPS these rules.
    const NON_STRAT_RULES: &str = concat!(
        "#[name(\"https://example.org/ns/ruleLose\")]\n",
        "<https://example.org/ns/lose>(?X, ?X, ?W) :-\n",
        "    <https://example.org/ns/win>(?X, ?X, ?W) .\n",
        "#[name(\"https://example.org/ns/ruleWin\")]\n",
        "<https://example.org/ns/win>(?X, ?X, ?W) :-\n",
        "    <https://example.org/ns/move>(?X, ?Y, ?W),\n",
        "    ~<https://example.org/ns/lose>(?Y, ?Y, ?W) .\n",
    );

    /// One asserted `move` fact in one world — the EDB the echo path returns.
    const GAME_NQUADS: &str = concat!(
        "<https://example.org/ns/p1> <https://example.org/ns/move> ",
        "<https://example.org/ns/p2> <https://example.org/world/game> .\n",
    );

    /// Disclosure: the non-stratifiable EDB-echo path MUST disclose the dropped
    /// derivation rules as a sound under-approximation — never silently return a bare
    /// `{exact}`. This is the adversarial guard against a regression to the optimistic
    /// `PreservationClaim::exact()` default.
    #[test]
    fn materialize_routed_non_stratifiable_discloses_dropped_rules() {
        let m = materialize_routed(
            NON_STRAT_RULES,
            GAME_NQUADS,
            None,
            None,
            None,
            Some("StratifiedNAFProfile"),
        )
        .expect("routed materialize must not fail");

        // Sound-under, not exact: the dropped rules are named, not swallowed.
        assert!(
            m.preservation
                .polarities
                .contains(&PreservationKind::SoundUnder),
            "non-stratifiable echo must be SoundUnder, got {:?}",
            m.preservation.polarities
        );
        assert!(
            !m.preservation.polarities.contains(&PreservationKind::Exact),
            "a lossy echo must not also claim Exact"
        );
        assert_eq!(
            m.preservation.unsupported_constructs,
            [
                "https://example.org/ns/ruleLose",
                "https://example.org/ns/ruleWin"
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            "the two dropped rule IRIs must be disclosed verbatim"
        );
        // The asserted EDB is still materialized (the `move` fact survives).
        assert!(
            !m.quads.is_empty(),
            "the asserted EDB must still be echoed under the non-stratifiable path"
        );
    }

    /// The legalization floor holds on the empty-input fast path too: an empty world
    /// with a non-stratifiable rule set must STILL disclose the dropped rules — the
    /// unsupported set is a property of the program, not of the EDB, so a degenerate
    /// (empty) input must not erase the disclosure into a bare `{exact}`.
    #[test]
    fn materialize_routed_empty_input_still_discloses_non_stratifiable_rules() {
        let m = materialize_routed(
            NON_STRAT_RULES,
            "",
            None,
            None,
            None,
            Some("StratifiedNAFProfile"),
        )
        .expect("routed materialize must not fail on empty input");

        // No facts ⇒ no quads, but the unsupported-rule disclosure must survive.
        assert!(m.quads.is_empty(), "empty input yields no quads");
        assert!(
            m.preservation
                .polarities
                .contains(&PreservationKind::SoundUnder),
            "empty input with non-stratifiable rules must still be SoundUnder, got {:?}",
            m.preservation.polarities
        );
        assert!(
            !m.preservation.polarities.contains(&PreservationKind::Exact),
            "a dropped-rule disclosure must not also claim Exact"
        );
        assert_eq!(
            m.preservation.unsupported_constructs,
            [
                "https://example.org/ns/ruleLose",
                "https://example.org/ns/ruleWin"
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            "the dropped rule IRIs must be disclosed even with an empty EDB"
        );
    }

    /// Disclosure is uniform: a faithful stratifiable chase carries `{exact}`
    /// with an empty unsupported set — the disclosure surface is present even when
    /// nothing was dropped, so a consumer never has to assume.
    #[test]
    fn materialize_routed_stratifiable_is_exact() {
        let m = materialize_routed(
            TRANSITIVITY_RULES,
            CHAIN_NQUADS,
            None,
            None,
            None,
            Some("PositiveHornProfile"),
        )
        .expect("routed materialize must not fail");

        assert!(
            m.preservation.unsupported_constructs.is_empty(),
            "a faithful chase drops nothing: {:?}",
            m.preservation.unsupported_constructs
        );
        assert!(
            m.preservation.polarities.contains(&PreservationKind::Exact),
            "a faithful chase is exact, got {:?}",
            m.preservation.polarities
        );
    }

    /// Native-first forward wiring: a stratifiable Datalog± program
    /// under the default `_ =>` routing arm is materialized by the native physical core
    /// (`crate::physical::materialize_native`), NOT Nemo. The native engine is
    /// authoritative for the closure, so the derived transitive `subClassOf` edge
    /// `Dog ⊑ Animal` (from `Dog ⊑ Mammal`, `Mammal ⊑ Animal`) must be present in the
    /// result. This pins that the native path is taken and produces the closure.
    #[test]
    fn materialize_routed_forward_uses_native_closure() {
        let m = materialize_routed(
            TRANSITIVITY_RULES,
            CHAIN_NQUADS,
            None,
            None,
            None,
            Some("PositiveHornProfile"),
        )
        .expect("routed materialize must not fail");

        let sub_class_of = "https://blackcatinformatics.ca/logic/subClassOf";
        let want_subj = "http://example.org/Dog";
        let want_obj = "http://example.org/Animal";
        let derived: Vec<(String, String, String)> = m
            .quads
            .iter()
            .map(|q| {
                (
                    q.subject.to_string(),
                    q.predicate.as_str().to_owned(),
                    q.object.to_string(),
                )
            })
            .collect();
        assert!(
            derived.iter().any(|(s, p, o)| p == sub_class_of
                && s == &format!("<{want_subj}>")
                && o == &format!("<{want_obj}>")),
            "native closure must derive Dog ⊑ Animal: {derived:?}"
        );
        // The two asserted EDB edges plus the one derived edge: closure is non-trivial.
        let sco_edges: Vec<_> = derived
            .iter()
            .filter(|(_, p, _)| p == sub_class_of)
            .collect();
        assert_eq!(
            sco_edges.len(),
            3,
            "expected 2 asserted + 1 derived subClassOf edges: {sco_edges:?}"
        );
    }

    /// Budget-constrained materialization must route to the Nemo fallback, not the native
    /// core: native decides the WHOLE least model and cannot reproduce the oracle's
    /// post-hoc truncation. With a deliberately unsatisfiable rule-firing budget the
    /// routed result must still come back (handled by the Nemo governor), and the
    /// asserted-EDB rows are echoed regardless — confirming the budgeted call did not take
    /// the native arm (which the `_ =>` routing now guards out when any budget is set).
    /// The exact truncation/exhausted-disclosure semantics are pinned by the
    /// `external/szs-mini/unknown-budget` conformance case.
    #[test]
    fn materialize_routed_budget_uses_nemo_fallback() {
        let m = materialize_routed(
            TRANSITIVITY_RULES,
            CHAIN_NQUADS,
            Some(0), // a zero rule-firing budget the native core has no governor for
            None,
            None,
            Some("PositiveHornProfile"),
        )
        .expect("budgeted routed materialize must not fail (Nemo governor handles it)");
        // The two asserted EDB edges are always echoed; the budgeted path produced a
        // well-formed result via the fallback rather than erroring or running native.
        assert!(
            m.quads.iter().any(|q| q.rule_iri == ASSERT_RULE_IRI),
            "asserted-EDB echo must be present from the Nemo fallback path"
        );
    }
}
