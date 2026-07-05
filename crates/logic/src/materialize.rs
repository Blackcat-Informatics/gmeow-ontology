// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Pure-Rust materialization core — the engine pipeline behind the
//! `gmeow_logic.materialize` PyO3 binding, extracted so it is unit-testable
//! natively (no Python, no FFI).
//!
//! The PyO3 wrapper in [`crate::py`] keeps only the marshalling shell: the empty
//! short-circuit, the non-stratifiable native routing (which returns
//! Python rows), and the `DerivedQuad → PyDict` serialization. Everything between
//! — parse N-Quads → build the typed EDB ([`TypedFactSet`]) → run the typed
//! chase through the forward oracle → coerce typed rows to [`DerivedQuad`]s with real
//! provenance → apply the post-hoc budget governor — lives here and is exercised
//! by both the FFI and native `#[test]`s.  Fact-string encoding and decoding are
//! confined to the Nemo adapter ([`crate::nemo_engine`]); this module only ever
//! sees native [`TermValue`]s.
//!
//! The split is behaviour-preserving: with all budget parameters `None` (the
//! default), [`materialize_core`] produces the exact same `DerivedQuad` sequence
//! the inlined FFI path did, preserving the oracle≡engine parity guarantee.

use std::time::Instant;

use purrdf::{parse_dataset, TermValue};

use crate::facts::TypedFactSet;
use crate::nemo_engine::TypedRow;
use crate::oracle::ForwardOracle;
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
/// Uses [`crate::provenance::mint_reifier`] on the already-decoded native
/// terms so the result is byte-identical to the Python oracle.
///
/// # Errors
///
/// Returns an error if subject or object is an RDF-star quoted triple.
pub(crate) fn reifier_for_quad(
    subject: &TermValue,
    predicate: &str,
    object: &TermValue,
) -> Result<String, String> {
    mint_reifier(subject, predicate, object)
}

/// Compute the reifier IRI for a typed antecedent row.
///
/// The row must be ternary (S, O, world) with an IRI — or Skolem-IRI — subject
/// term; the typed terms feed [`mint_reifier`] directly.  Returns an error for
/// any other shape — a partial antecedent list would produce a wrong
/// derivation_id, which is worse than failing loudly.
pub(crate) fn reifier_for_antecedent_row(row: &TypedRow) -> Result<String, String> {
    if row.args.len() != 3 {
        return Err(format!(
            "antecedent row has {} values (expected 3): {:?}",
            row.args.len(),
            row
        ));
    }
    // subject: must be an IRI (or Skolem IRI) term — a world-scoped quad never
    // carries a literal subject, so anything else is a malformed antecedent.
    let subject = &row.args[0];
    if !matches!(subject, TermValue::Iri(_)) {
        return Err(format!(
            "antecedent subject must be an IRI (or Skolem IRI) term, got {subject:?}"
        ));
    }
    // predicate: raw IRI string; object: any term.
    mint_reifier(subject, &row.predicate, &row.args[1])
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

/// A materialized chase row that is not a world-scoped quad.
///
/// The seam contract renders exactly one fact shape as a quad:
/// `predicate(subject, object, world)` — arity 3.  A rule program may
/// legitimately declare helper predicates of any other arity (legal Nemo, e.g.
/// a binary `helper(?x, ?y)` join relation); such a row cannot be a
/// world-scoped [`DerivedQuad`], so it is surfaced here verbatim instead of
/// being dropped.
/// A coerced world-scoped quad paired with its EDB/IDB flag, so the budget governor can
/// bound derived (IDB) firings without re-deriving provenance.
type QuadWithEdbFlag = (DerivedQuad, bool);

#[derive(Debug, Clone, PartialEq)]
pub struct NonQuadRow {
    /// The relation name (a full predicate IRI, un-bracketed, or a bare
    /// program-local predicate symbol).
    pub predicate: String,
    /// The decoded native terms, one per column (arity ≠ 3).
    pub args: Vec<TermValue>,
    /// Whether the row was asserted (EDB) rather than derived by a rule.
    pub is_edb: bool,
}

/// The result of [`materialize_core`]: the world-scoped derived quads plus
/// every chase row that is not a quad.
///
/// Nothing the chase materializes is lost silently: an arity-3 row becomes a
/// [`DerivedQuad`]; every other row — a typed helper-predicate row is legal
/// Nemo but cannot be a world-scoped quad under the seam contract — lands in
/// [`MaterializeOutcome::non_quad_rows`] explicitly.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterializeOutcome {
    /// The world-scoped quads (asserted EDB + derived IDB), with provenance.
    pub quads: Vec<DerivedQuad>,
    /// The non-ternary rows the chase materialized (helper predicates).
    pub non_quad_rows: Vec<NonQuadRow>,
}

/// Parse `input` N-Quads and build the typed EDB — one arity-3 fact per
/// named-graph quad.
///
/// The single canonical N-Quads → [`TypedFactSet`] path, shared by
/// [`materialize_core`] and the forward parity gate so the EDB-build logic is
/// never forked.  Default and blank-node graphs are skipped (a non-named graph
/// has no world IRI); fabricating synthetic world IRIs would break the
/// oracle≡engine parity guarantee.  An empty input yields an empty EDB.
///
/// # Errors
///
/// Returns [`MaterializeError::Parse`] for malformed N-Quads.
pub(crate) fn edb_from_nquads(input: &str) -> Result<TypedFactSet, MaterializeError> {
    let mut edb = TypedFactSet::new();
    if input.trim().is_empty() {
        return Ok(edb);
    }

    let dataset = parse_dataset(input.as_bytes(), "application/n-quads", None)
        .map_err(|e| MaterializeError::Parse(format!("N-Quads parse error: {e}")))?;

    for quad in dataset.quads() {
        // Resolve the world IRI (named-graph component); skip non-named graphs.
        let world_iri: String = match quad.g.map(|g| dataset.term_value(g)) {
            Some(TermValue::Iri(iri)) => iri,
            _ => continue,
        };

        let subject = dataset.term_value(quad.s);
        let predicate = dataset.term_value(quad.p);
        let object = dataset.term_value(quad.o);
        // The predicate of an RDF quad is always an IRI; it is the relation name.
        let predicate_iri = match &predicate {
            TermValue::Iri(iri) => iri.as_str(),
            // Defensive: a non-IRI predicate is invalid RDF and cannot be a fact.
            _ => continue,
        };

        // Blank subjects/objects are Skolemized inside `push_quad`; the world
        // travels as a plain string literal (the Nemo string-constant treatment).
        edb.push_quad(&subject, predicate_iri, &object, &world_iri);
    }

    Ok(edb)
}

/// Materialize `input` N-Quads under `rules`, returning derived quads with real
/// provenance and (optional) budget bookkeeping, plus any non-quad helper rows.
///
/// This is the pure engine pipeline — no PyO3, no GIL, no Python oracle. The FFI
/// wrapper in [`crate::py`] handles the empty short-circuit, non-stratifiable
/// native routing, and the dict marshalling; everything else is here.
///
/// With all of `max_rule_firings`/`max_answers`/`time_ms` set to `None` (the
/// default), the post-hoc budget governor is skipped entirely and the quad
/// output is byte-identical to the chase order with every quad
/// `budget_status = Ok`.
///
/// # Errors
///
/// Returns [`MaterializeError::Parse`] for malformed N-Quads input and
/// [`MaterializeError::Chase`] for chase, row-coercion, or provenance failures.
pub fn materialize_core(
    rules: &str,
    input: &str,
    max_rule_firings: Option<u64>,
    max_answers: Option<u64>,
    time_ms: Option<u64>,
) -> Result<MaterializeOutcome, MaterializeError> {
    // Start the post-fixpoint wall-clock the instant we enter (the chase itself is
    // not interruptible; `time_ms` bounds the post-chase decode/bookkeeping).
    let budget_active = max_rule_firings.is_some() || max_answers.is_some() || time_ms.is_some();
    let start = Instant::now();

    // ── Short-circuit: nothing to do ──────────────────────────────────────────
    if input.trim().is_empty() {
        return Ok(MaterializeOutcome {
            quads: vec![],
            non_quad_rows: vec![],
        });
    }

    // ── 1–2. Parse input N-Quads and build the typed EDB ─────────────────────
    let edb = edb_from_nquads(input)?;

    // ── 3. Run the typed forward chase through the oracle boundary ───────────
    // The oracle is the sole fact-stringifier; the unbudgeted closure is
    // materialized here and any budget is applied post-fixpoint below.  This
    // path consumes per-row provenance (EDB/IDB flag, rule, antecedents), so an
    // oracle that cannot attribute derivations cannot drive it — hard-fail
    // rather than fabricate provenance.
    let oracle = crate::oracle::forward_oracle();
    if !oracle.provides_provenance() {
        return Err(MaterializeError::Chase(format!(
            "forward oracle '{}' provides no provenance, which materialize requires",
            oracle.name()
        )));
    }
    let chase = oracle
        .materialize(&edb, rules, &crate::oracle::ForwardBudget::UNBOUNDED)
        .map_err(|e| MaterializeError::Chase(format!("chase error: {e}")))?;

    // ── 4. Coerce typed rows → DerivedQuads with real provenance ─────────────
    // Carry the EDB/IDB flag alongside each quad so the budget governor can bound
    // IDB firings (`max_rule_firings`) without re-deriving provenance.
    let (derived_quads, non_quad_rows) = coerce_typed_rows(&chase.rows)?;

    // ── 5. Post-hoc budget governor ─────────────────────────────
    // With no budget params (the default), this whole block is skipped, so the
    // output is byte-identical to: chase order, every quad "ok".  The budget
    // bounds derived QUADS only; non-quad helper rows are surfaced in full.
    let final_quads: Vec<DerivedQuad> = if budget_active {
        apply_budget(derived_quads, max_rule_firings, max_answers, time_ms, start)
    } else {
        derived_quads.into_iter().map(|(dq, _edb)| dq).collect()
    };

    Ok(MaterializeOutcome {
        quads: final_quads,
        non_quad_rows,
    })
}

/// Coerce typed chase rows → `(DerivedQuad, is_edb)` pairs plus the non-quad helper
/// rows, computing real provenance from each row's [`crate::oracle::TypedProvenance`].
///
/// The single authored row→quad coercion, shared by [`materialize_core`] (full
/// provenance from the Nemo trace) and the existential facts-only demotion in
/// [`materialize_routed`]. When the driving oracle attributes nothing (the facts-only
/// path — `is_edb == false`, no rule name, no antecedents), the coercion mints the
/// honest self-derivation of an unnamed rule with EMPTY sources; it never fabricates
/// attribution. An arity ≠ 3 row is a helper predicate surfaced in the non-quad bucket,
/// never dropped; a malformed ternary row (non-IRI subject, non-string world, or an
/// unreifiable antecedent) is a hard [`MaterializeError::Chase`].
fn coerce_typed_rows(
    rows: &[(TypedRow, crate::oracle::TypedProvenance)],
) -> Result<(Vec<QuadWithEdbFlag>, Vec<NonQuadRow>), MaterializeError> {
    let mut derived_quads: Vec<QuadWithEdbFlag> = Vec::new();
    let mut non_quad_rows: Vec<NonQuadRow> = Vec::new();

    for (idx, (row, prov)) in rows.iter().enumerate() {
        // Only a ternary (arity-3) row is a world-scoped quad under the seam
        // contract; any other arity is a helper-predicate row — legal Nemo, but
        // not a quad — surfaced explicitly, never dropped.
        if row.args.len() != 3 {
            non_quad_rows.push(NonQuadRow {
                predicate: row.predicate.clone(),
                args: row.args.clone(),
                is_edb: prov.is_edb,
            });
            continue;
        }

        // predicate: raw IRI string (Nemo strips angle brackets in Tag::to_string)
        let predicate_iri = row.predicate.clone();

        // subject: must be an IRI (or Skolem-IRI) term — a world-scoped quad
        // never carries a literal subject.
        let subject_term = match &row.args[0] {
            iri @ TermValue::Iri(_) => iri.clone(),
            other => {
                return Err(MaterializeError::Chase(format!(
                    "row[{idx}] subject: a world-scoped quad subject must be an \
                     IRI (or Skolem IRI) term, got {other:?}"
                )));
            }
        };

        // object: IRI, typed literal, language literal, or plain literal
        let object_term = row.args[1].clone();

        // context (world): must be a plain string literal (the Nemo
        // string-constant treatment of the world position).
        let world_str = match &row.args[2] {
            TermValue::Literal {
                lexical_form,
                datatype,
                language: None,
                ..
            } if datatype == "http://www.w3.org/2001/XMLSchema#string" => lexical_form.clone(),
            other => {
                return Err(MaterializeError::Chase(format!(
                    "row[{idx}] world: the world position of a ternary row must \
                     be a plain string literal, got {other:?}"
                )));
            }
        };

        // ── Real provenance computation ───────────────────────────────────────
        let self_reifier = reifier_for_quad(&subject_term, &predicate_iri, &object_term)
            .map_err(|e| MaterializeError::Chase(format!("row[{idx}] reifier error: {e}")))?;

        let (rule_iri, source_quad_ids, derivation_id) = if prov.is_edb {
            // Asserted (EDB) fact: logic:assert sentinel, self-reifier as source.
            let rule = ASSERT_RULE_IRI.to_owned();
            let sources = vec![self_reifier.clone()];
            let deriv = mint_derivation_id(&rule, &[self_reifier.as_str()]);
            (rule, sources, deriv)
        } else {
            // Derived (IDB) fact: rule IRI from the rule name, antecedents as sources.
            // Antecedent coercion is fallible — a partial list produces a wrong
            // derivation_id, which is worse than propagating the error.  A
            // non-ternary antecedent of a ternary row is a hard error: the
            // consumed premise is not a world-scoped quad and has no reifier.
            // (The facts-only demotion carries no antecedents, so `sources` is
            // empty here — honest, since that oracle attributes nothing.)
            let rule = rule_iri_from_name(prov.rule_name.as_deref());
            let sources: Vec<String> = prov
                .antecedents
                .iter()
                .map(reifier_for_antecedent_row)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| {
                    MaterializeError::Chase(format!("row[{idx}] antecedent error: {e}"))
                })?;
            let source_refs: Vec<&str> = sources.iter().map(|s: &String| s.as_str()).collect();
            let deriv = mint_derivation_id(&rule, &source_refs);
            (rule, sources, deriv)
        };

        let is_edb = prov.is_edb;
        let dq = DerivedQuad {
            graph: world_str.clone(),
            subject: subject_term,
            predicate: predicate_iri,
            object: object_term,
            graph_component: world_str,
            derivation_id: DerivationId(derivation_id),
            rule_iri,
            source_quad_ids,
            profile: ASSERTED_PROFILE.to_owned(),
            // Default-path budget status (overwritten by the governor when a ceiling trips).
            budget_status: BudgetStatus::Ok,
        };
        derived_quads.push((dq, is_edb));
    }

    Ok((derived_quads, non_quad_rows))
}

/// Canonical sort key for a derived quad: `(graph, subject, predicate, object)`.
///
/// This is the deterministic order the budget governor truncates to, so a kept
/// subset is always a sound *prefix* of a stable ordering — never a fabricated or
/// reordered result. The key uses the same string surfaces the seam already
/// projects (`graph`/`subject`/`predicate`/`object` display forms).
fn budget_sort_key(dq: &DerivedQuad) -> (String, String, String, String) {
    (
        dq.graph.clone(),
        crate::provenance::term_display(&dq.subject),
        dq.predicate.clone(),
        crate::provenance::term_display(&dq.object),
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

// ── Profile-routed materialization ──────────────────────────────────
//
// The native conformance harness (`crates/conformance`) drives the engine
// directly, with no PyO3 boundary. It therefore needs ONE public entry point that
// reproduces the routing the `gmeow_logic.materialize` PyO3 wrapper performs in
// [`crate::py`]: empty-input short-circuit, the non-stratifiable native routing
// (well-founded / cautious-stable / declared-StratifiedNAF echo), and
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
    Ok(DerivedQuad {
        graph: row.graph.clone(),
        subject: row.subject,
        predicate: row.predicate,
        object: row.object,
        graph_component: row.graph,
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
    /// Non-quad helper rows from the Nemo chase (see
    /// [`MaterializeOutcome::non_quad_rows`]).  Empty on the native routes,
    /// whose rule IR derives world-scoped quads only.
    pub non_quad_rows: Vec<NonQuadRow>,
    /// The preservation judgment for this materialization.
    pub preservation: PreservationClaim,
    /// The completion frontier of the native forward governor: which strata / predicates
    /// are settled and how many derivations were committed.  Empty
    /// ([`crate::query_ir::CompletionFrontier::empty`]) on the ungoverned routes (empty
    /// input, well-founded / cautious-stable / echo, the Nemo fallback), so the field is
    /// always present — a consumer never has to assume "no frontier ⇒ complete".
    pub frontier: crate::query_ir::CompletionFrontier,
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
/// This is the public, PyO3-free entry point the conformance harness calls.
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
            non_quad_rows: vec![],
            preservation,
            frontier: crate::query_ir::CompletionFrontier::empty(),
        });
    }

    // Non-stratifiable native routing. `None` ⇒ fall through to Nemo. The well-founded
    // and cautious-stable evaluators run their native fixpoints; any other profile with
    // a declared set that fails stratification (⇔ `preservation` discloses dropped
    // rules) is echoed asserted-only.
    // Each native route yields its derived rows plus the budget status the native
    // governor stamped (`Ok` for the ungoverned well-founded / cautious-stable /
    // echo paths; the semi-naive governor's `Ok`/`Exhausted` for the PositiveHorn arm).
    // Each `Some` carries the derived rows, the governor's status, and the completion
    // frontier.  The ungoverned routes (well-founded / cautious-stable / echo, and the
    // Nemo fallback below) run to their natural fixpoint outside the semi-naive governor,
    // so they carry the empty frontier; only the native PositiveHorn arm reports a real
    // one.
    type RoutedRows = (
        Vec<crate::rule_ir::DerivedRow>,
        BudgetStatus,
        crate::query_ir::CompletionFrontier,
    );
    let routed: Option<RoutedRows> = match profile {
        Some("WellFoundedProfile") => {
            let store = crate::store::WorldStore::new();
            store.load_nquads(input).map_err(MaterializeError::Parse)?;
            let eval_rules =
                crate::rule_ir::parse_eval_rules(rules).map_err(MaterializeError::Parse)?;
            Some((
                crate::wellfounded::materialize(&store, &eval_rules)
                    .map_err(MaterializeError::Chase)?,
                BudgetStatus::Ok,
                crate::query_ir::CompletionFrontier::empty(),
            ))
        }
        Some("StableModelProfile") => {
            let store = crate::store::WorldStore::new();
            store.load_nquads(input).map_err(MaterializeError::Parse)?;
            let eval_rules =
                crate::rule_ir::parse_eval_rules(rules).map_err(MaterializeError::Parse)?;
            Some((
                crate::stablemodel::cautious_materialize(&store, &eval_rules)
                    .map_err(MaterializeError::Chase)?,
                BudgetStatus::Ok,
                crate::query_ir::CompletionFrontier::empty(),
            ))
        }
        _ => {
            // PositiveHorn / declared StratifiedNAF / Probabilistic / Procedural / None.
            // A declared set that FAILS stratification has a non-empty unsupported set and
            // is echoed asserted-only. Otherwise the stratifiable Datalog± fragment is the
            // native physical core's competence: TRY it first (native is authoritative for
            // what it decides) and fall through to the Nemo fallback ONLY for a declared
            // native gap (`NativeOutcome::Unsupported`). `preservation` is exact for this
            // arm, so the native and Nemo paths disclose the same judgment.
            // A value-inventing (existential-rule) program is the native chase's
            // competence — routed HERE, before the Datalog arm, because
            // `parse_eval_rules`/`materialize_native` cannot represent an existential head
            // variable (`ground_head` hard-errors on one).  Certify termination and run
            // the restricted chase; an uncertified program with no budget is a declared
            // native gap that demotes to the Nemo FACTS-ONLY oracle (below), exactly like a
            // non-stratifiable Datalog program demotes to the provenance-carrying oracle.
            let existential_rules =
                crate::physical::parse_existential_rules(rules).map_err(MaterializeError::Parse)?;
            if existential_rules.iter().any(|r| r.is_existential()) {
                // A wall-clock budget cannot be honored for a value-inventing program: the
                // native chase governs by derivation STEPS (not elapsed time), the
                // facts-only Nemo oracle rejects an inline budget, and the provenance
                // oracle hard-errors on the invented nulls. Silently ignoring `time_ms` (or
                // routing to the provenance oracle, which would error) both violate
                // no-silent-degradation — so refuse the combination and name the supported
                // budget instead.
                if time_ms.is_some() {
                    return Err(MaterializeError::Chase(
                        "wall-clock budget (time_ms) is not supported for value-inventing \
                         existential programs: the native chase governs by derivation steps \
                         and no oracle honors a wall-clock bound on invented nulls — use a \
                         step budget (max_rule_firings / max_answers)"
                            .to_owned(),
                    ));
                }
                let store = crate::store::WorldStore::new();
                store.load_nquads(input).map_err(MaterializeError::Parse)?;
                let max_steps = match (max_rule_firings, max_answers) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                };
                let (admission, chase_outcome) =
                    crate::physical::chase_materialize(&store, &existential_rules, max_steps)
                        .map_err(MaterializeError::Chase)?;
                match chase_outcome {
                    crate::physical::NativeOutcome::Decided(budgeted) => {
                        let frontier = budgeted.frontier();
                        Some((budgeted.rows, budgeted.status, frontier))
                    }
                    // Uncertified with no budget ⇒ a declared native gap: demote to the
                    // Nemo FACTS-ONLY oracle for a full (unbudgeted) closure. The native
                    // provenance-carrying oracle (`materialize_core`) would hard-error on
                    // this program's invented labeled nulls ("no trace tree"); the
                    // facts-only path materializes the closure as FACTS with EMPTY
                    // provenance (honest — it attributes nothing, never a fabricated
                    // trace). This returns the arm's whole result directly rather than
                    // falling through to the Datalog `materialize_core` fallback.
                    crate::physical::NativeOutcome::Unsupported(_) => {
                        // The refusal is a native capability-gap, not a silent drop: the
                        // certificate's weak-acyclicity violations are the counted
                        // `reason::ledger` DlGap surface (`capability_gap_rows`), scoped
                        // out of the DL/EL crosscheck by their `existential-chase` category.
                        // Production emits no existential rules today, so the audit sink is
                        // the parity/oracle harness; here we assert the invariant and demote.
                        debug_assert!(
                            !admission.capability_gap_rows().is_empty(),
                            "a refused existential program must surface ≥1 counted capability-gap row"
                        );
                        return demote_existential_to_facts_only(rules, input, preservation);
                    }
                }
            } else if !preservation.unsupported_constructs.is_empty() {
                Some((
                    echo_edb_only(input)?,
                    BudgetStatus::Ok,
                    crate::query_ir::CompletionFrontier::empty(),
                ))
            } else if time_ms.is_some() {
                // A wall-clock budget is a genuine native gap: the semi-naive governor
                // counts committed derivations, not elapsed time, so a `time_ms` request
                // demotes to the Nemo post-hoc governor. (A step/derivation budget —
                // `max_rule_firings` / `max_answers` — is now native's competence, below.)
                None
            } else {
                let store = crate::store::WorldStore::new();
                store.load_nquads(input).map_err(MaterializeError::Parse)?;
                let eval_rules =
                    crate::rule_ir::parse_eval_rules(rules).map_err(MaterializeError::Parse)?;
                // The forward step/derivation budget: a rule firing IS a committed
                // derivation, so `max_rule_firings` maps to the native governor's
                // `max_steps`; `max_answers` is the same derivation cap under another
                // name, so the ceiling is their `min` (matching the Nemo `derived_cap`).
                // Exhaustion stamps `Exhausted` on every emitted quad — incomplete, never
                // wrong.
                let max_steps = match (max_rule_firings, max_answers) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                };
                match crate::physical::materialize_native(&store, &eval_rules, max_steps)
                    .map_err(MaterializeError::Chase)?
                {
                    crate::physical::NativeOutcome::Decided(budgeted) => {
                        // Surface the forward governor's completion frontier instead of
                        // dropping it: an `Exhausted` chase is incomplete, and the caller
                        // reads `completed < total` to tell which strata are settled.
                        let frontier = budgeted.frontier();
                        Some((budgeted.rows, budgeted.status, frontier))
                    }
                    // A declared native gap (e.g. non-stratifiable after parse) falls
                    // through to the demoted Nemo fallback / conformance oracle.
                    crate::physical::NativeOutcome::Unsupported(_) => None,
                }
            }
        }
    };

    if let Some((rows, status, frontier)) = routed {
        let quads = rows
            .into_iter()
            .map(derived_row_to_quad)
            .map(|dq| {
                dq.map(|mut q| {
                    // Frontier-aware per-quad budget stamp. The overall `status` is the
                    // whole-run verdict; a single quad can be MORE settled than the run.
                    //
                    // - `Ok` (natural fixpoint) ⇒ every quad stays `Ok` (unchanged).
                    // - `Exhausted` (a step cut) ⇒ a quad whose PREDICATE reached its
                    //   stratum's natural fixpoint has a FINAL least-model extension
                    //   (`frontier.saturated_preds`, matched on the bare-IRI predicate name
                    //   `seminaive` inserts — the head/EDB `predicate.as_str()`), so it is
                    //   conclusive / complete-for-fragment and keeps `Ok`; only a quad from
                    //   the cut or unreached strata is genuinely incomplete → `Exhausted`.
                    //   This is the difference between a blanket "undetermined" and the
                    //   sound per-stratum verdict the frontier records.
                    // - `Partial` (a `max_answers` answer-cap) is owned by the backward leg
                    //   and never reaches this forward native path; the catch-all preserves
                    //   it verbatim regardless, so the frontier never overrides an answer-cap.
                    q.budget_status = match status {
                        BudgetStatus::Exhausted
                            if frontier.saturated_preds.contains(&q.predicate) =>
                        {
                            BudgetStatus::Ok
                        }
                        other => other,
                    };
                    q
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Materialization {
            quads,
            non_quad_rows: vec![],
            preservation,
            frontier,
        });
    }

    // Stratifiable / projection-only ⇒ the Nemo chase (with the post-hoc governor).
    // The chase is faithful for the positive-Horn fragment, so the claim is exact.
    let outcome = materialize_core(rules, input, max_rule_firings, max_answers, time_ms)?;
    Ok(Materialization {
        quads: outcome.quads,
        non_quad_rows: outcome.non_quad_rows,
        preservation,
        // The Nemo chase runs its own post-hoc governor, not the native semi-naive one,
        // so it exposes no stratum frontier.
        frontier: crate::query_ir::CompletionFrontier::empty(),
    })
}

/// Materialize an uncertified, no-budget existential program via the Nemo FACTS-ONLY
/// oracle — the honest demotion for the value-inventing fragment the native restricted
/// chase refuses (`NativeOutcome::Unsupported`) rather than looping.
///
/// The provenance-carrying oracle ([`materialize_core`]) cannot drive this program: Nemo's
/// trace cannot follow the invented labeled nulls ("no trace tree"). [`NemoFactsOracle`]
/// materializes the full (unbudgeted) closure as FACTS with EMPTY provenance — it attributes
/// nothing, so the coercion mints the honest unnamed self-derivation and never fabricates a
/// trace. The rows flow through the SAME [`coerce_typed_rows`] coercion `materialize_core`
/// uses, so the [`DerivedQuad`] shape is identical; every quad is stamped
/// [`BudgetStatus::Ok`] (an unbudgeted full closure), matching the `Decided`-path assembly
/// under an empty frontier. `preservation` is the judgment already derived for `(rules,
/// profile)`, carried through unchanged.
///
/// # Errors
///
/// [`MaterializeError::Parse`] for a malformed EDB; [`MaterializeError::Chase`] for a
/// facts-only chase or row-coercion failure.
fn demote_existential_to_facts_only(
    rules: &str,
    input: &str,
    preservation: PreservationClaim,
) -> Result<Materialization, MaterializeError> {
    use crate::oracle::ForwardOracle;

    let edb = edb_from_nquads(input)?;
    let chase = crate::oracle::NemoFactsOracle
        .materialize(&edb, rules, &crate::oracle::ForwardBudget::UNBOUNDED)
        .map_err(|e| MaterializeError::Chase(format!("facts-only chase error: {e}")))?;
    let (derived_quads, non_quad_rows) = coerce_typed_rows(&chase.rows)?;
    // The facts-only closure is unbudgeted: every coerced quad is already `Ok`, so the
    // per-quad frontier stamp the `Decided` path applies is a no-op here — emit them directly.
    let quads = derived_quads.into_iter().map(|(dq, _edb)| dq).collect();
    Ok(Materialization {
        quads,
        non_quad_rows,
        preservation,
        // Nemo runs to its natural fixpoint outside the native semi-naive governor, so it
        // exposes no stratum frontier — same as the Datalog Nemo fallback.
        frontier: crate::query_ir::CompletionFrontier::empty(),
    })
}

#[cfg(test)]
mod tests {
    //! Native coverage of the materialize engine pipeline (T5).
    //!
    //! These are the Rust ports of the engine assertions that previously lived in
    //! `tests/test_logic_engine.py` and drove `gmeow_logic.materialize` through
    //! PyO3. They exercise the pure [`materialize_core`] directly — no Python, no
    //! FFI — so the chase, world mapping, real provenance, and the assert-sentinel
    //! contract are pinned natively. The PyO3 binding retains only a thin
    //! marshalling smoke (`tests/test_logic_engine.py`).

    use super::*;

    #[test]
    fn materialize_routed_refuses_time_ms_on_an_existential_program() {
        // A wall-clock budget cannot be honored for a value-inventing program: the native
        // chase governs by derivation STEPS, the facts-only oracle rejects an inline
        // budget, and the provenance oracle errors on the invented nulls. Silently ignoring
        // `time_ms` would violate no-silent-degradation, so the router must REFUSE.
        let world = "http://world/W";
        let ty = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        let input = format!("<http://ex/a> <{ty}> <http://ex/C> <{world}> .\n");
        let rules = format!(
            "<http://ex/p>(?x, !y, ?w), <{ty}>(!y, <http://ex/D>, ?w) :- \
             <{ty}>(?x, <http://ex/C>, ?w) ."
        );
        let err = materialize_routed(
            &rules,
            &input,
            None,
            None,
            Some(1000),
            Some("PositiveHornProfile"),
        )
        .expect_err("time_ms on an existential program must be refused, not silently ignored");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("time_ms") && msg.contains("existential"),
            "the refusal must name the unsupported wall-clock budget: {msg}"
        );
    }

    #[test]
    fn materialize_routed_runs_the_native_chase_on_an_existential_program() {
        // A value-inventing program (`C ⊑ ∃p.D`) is now routed to the native restricted
        // chase THROUGH materialize_routed — the wiring that makes the chase live, not a
        // hard-fail on the unbound head var.
        let world = "http://world/W";
        let ty = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        let input = format!("<http://ex/a> <{ty}> <http://ex/C> <{world}> .\n");
        let rules = format!(
            "<http://ex/p>(?x, !y, ?w), <{ty}>(!y, <http://ex/D>, ?w) :- <{ty}>(?x, <http://ex/C>, ?w) ."
        );

        let m = materialize_routed(
            &rules,
            &input,
            None,
            None,
            None,
            Some("PositiveHornProfile"),
        )
        .expect("existential materialize should route to the native chase");

        // The chase invented a `p`-edge from `a` to a fresh witness…
        let p_edges: Vec<_> = m
            .quads
            .iter()
            .filter(|q| q.predicate == "http://ex/p")
            .collect();
        assert_eq!(
            p_edges.len(),
            1,
            "one invented p-edge; quads: {:#?}",
            m.quads
        );
        let witness = crate::provenance::term_display(&p_edges[0].object);
        assert!(
            witness.contains("/skolem/"),
            "the p-edge target is an invented null: {witness}"
        );
        // …and typed that witness `D`.
        assert!(
            m.quads.iter().any(|q| q.predicate == ty
                && crate::provenance::term_display(&q.subject) == witness
                && crate::provenance::term_display(&q.object) == "<http://ex/D>"),
            "the witness is typed D; quads: {:#?}",
            m.quads
        );
    }

    #[test]
    fn materialize_routed_demotes_an_uncertified_existential_to_nemo_facts_only() {
        // An UNCERTIFIED existential program with NO budget: `chase_materialize` returns
        // `Unsupported(NonTerminatingExistential)` (the certifier's constant-refined weak
        // acyclicity, over-approximated by wildcard subsumption, sees the existential
        // p-position in a cycle back through the `p(<a>, ?z, ?w)` reader). Rather than
        // hard-erroring in the provenance oracle ("no trace tree" on the invented nulls),
        // `materialize_routed` must DEMOTE to the Nemo FACTS-ONLY oracle and materialize the
        // (terminating) closure's FACTS — invented witnesses present, no error.
        let world = "http://world/W";
        let ty = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        // rule A (existential): C(x) ⊑ ∃p.  rule B (Datalog): p(a, z) → C(z).
        // The B-guard `<http://ex/a>` in SUBJECT position stops the restricted chase after
        // one witness generation, so Nemo terminates while the native certifier still refuses
        // the whole program (the wildcard-subsumed position cycle is only an over-approximation).
        let rules = format!(
            "<http://ex/p>(?x, !y, ?w) :- <{ty}>(?x, <http://ex/C>, ?w) .\n\
             <{ty}>(?z, <http://ex/C>, ?w) :- <http://ex/p>(<http://ex/a>, ?z, ?w) .\n"
        );
        let input = format!("<http://ex/a> <{ty}> <http://ex/C> <{world}> .\n");

        let m = materialize_routed(
            &rules,
            &input,
            None,
            None,
            None,
            Some("PositiveHornProfile"),
        )
        .expect("uncertified existential must demote gracefully, not hard-error");

        // The facts-only Nemo chase invented a p-edge from `a` to a fresh labeled null…
        let p_edges: Vec<_> = m
            .quads
            .iter()
            .filter(|q| {
                q.predicate == "http://ex/p"
                    && crate::provenance::term_display(&q.subject) == "<http://ex/a>"
            })
            .collect();
        assert_eq!(
            p_edges.len(),
            1,
            "exactly one invented p-edge from a; quads: {:#?}",
            m.quads
        );
        let witness = crate::provenance::term_display(&p_edges[0].object);
        assert!(
            witness.contains("nemo-null"),
            "the p-edge target is a Nemo-invented null (proves the facts-only demotion ran), \
             got: {witness}"
        );
        // …with EMPTY provenance — the facts-only oracle attributes nothing, never a
        // fabricated trace.
        assert!(
            p_edges[0].source_quad_ids.is_empty(),
            "facts-only demotion attributes nothing; source_quad_ids: {:?}",
            p_edges[0].source_quad_ids
        );
    }

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

    /// Materialize with the default (no-budget) parameters and unwrap the quads.
    fn run(rules: &str, input: &str) -> Vec<DerivedQuad> {
        materialize_core(rules, input, None, None, None)
            .expect("materialize_core must not fail")
            .quads
    }

    /// Collect the `(subject, object)` display pairs for the subClassOf predicate.
    fn sco_pairs(quads: &[DerivedQuad]) -> std::collections::HashSet<(String, String)> {
        quads
            .iter()
            .filter(|q| q.predicate.as_str() == SUBCLASS_PRED)
            .map(|q| {
                (
                    crate::provenance::term_display(&q.subject),
                    crate::provenance::term_display(&q.object),
                )
            })
            .collect()
    }

    // ── AC#2: round-trip with derivation metadata ────────────────────────────────

    #[test]
    fn materialize_core_returns_all_input_quads() {
        let result = run("", TWO_WORLD_NQUADS);
        assert_eq!(result.len(), 3, "expected 3 quads back");
        let subjects: std::collections::HashSet<String> = result
            .iter()
            .map(|q| crate::provenance::term_display(&q.subject))
            .collect();
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
            assert!(!q.graph.is_empty());
            assert!(!crate::provenance::term_display(&q.subject).is_empty());
            assert!(!q.predicate.is_empty());
            assert!(!crate::provenance::term_display(&q.object).is_empty());
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

        let alpha_subjects: std::collections::HashSet<String> = alpha
            .iter()
            .map(|q| crate::provenance::term_display(&q.subject))
            .collect();
        let beta_subjects: std::collections::HashSet<String> = beta
            .iter()
            .map(|q| crate::provenance::term_display(&q.subject))
            .collect();
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
                    && crate::provenance::term_display(&q.subject) == "<http://example.org/Dog>"
                    && crate::provenance::term_display(&q.object) == "<http://example.org/Animal>"
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

    // ── non-quad helper-predicate rows: the explicit partition ──────────────────

    /// A binary helper predicate derived FROM the ternary facts, alongside the
    /// ternary transitivity closure.  The helper rows are legal Nemo but cannot
    /// be world-scoped quads (arity ≠ 3).
    const HELPER_RULES: &str = concat!(
        "helperEdge(?X, ?Y) :- <https://blackcatinformatics.ca/logic/subClassOf>(?X, ?Y, ?W) .\n",
        "<https://blackcatinformatics.ca/logic/subClassOf>(?X, ?Z, ?C0) :-\n",
        "    <https://blackcatinformatics.ca/logic/subClassOf>(?X, ?Y, ?C0),\n",
        "    <https://blackcatinformatics.ca/logic/subClassOf>(?Y, ?Z, ?C1) .\n",
    );

    /// A ternary rule that consumes the binary helper directly: the derived
    /// ternary row's immediate antecedent is then a non-ternary row, which has
    /// no reifier and must hard-error (never fabricate a partial derivation).
    const HELPER_CONSUMING_RULES: &str = concat!(
        "helperEdge(?X, ?Y) :- <https://blackcatinformatics.ca/logic/subClassOf>(?X, ?Y, ?W) .\n",
        "<https://blackcatinformatics.ca/logic/subClassOf>(?X, ?Z, ?W) :-\n",
        "    helperEdge(?X, ?Y),\n",
        "    <https://blackcatinformatics.ca/logic/subClassOf>(?Y, ?Z, ?W) .\n",
    );

    /// Pin the explicit non-quad partition: a program with a binary helper
    /// predicate (a) still derives its ternary facts correctly, (b) surfaces
    /// every helper row in the explicit bucket, and (c) loses nothing silently
    /// — the quads plus the bucket account for every materialized row.
    #[test]
    fn materialize_core_helper_rows_surface_in_non_quad_bucket() {
        let outcome = materialize_core(HELPER_RULES, CHAIN_NQUADS, None, None, None)
            .expect("materialize_core must not fail");

        // (a) the ternary closure is derived exactly as without the helper.
        let pairs = sco_pairs(&outcome.quads);
        let expected_pairs: std::collections::HashSet<(String, String)> = [
            ("<http://example.org/Dog>", "<http://example.org/Mammal>"),
            ("<http://example.org/Mammal>", "<http://example.org/Animal>"),
            ("<http://example.org/Dog>", "<http://example.org/Animal>"),
        ]
        .into_iter()
        .map(|(s, o)| (s.to_owned(), o.to_owned()))
        .collect();
        assert_eq!(pairs, expected_pairs, "ternary closure mismatch");

        // (b) every helper row lands in the bucket, typed and derived (IDB):
        // the helper fires over all three subClassOf facts (incl. the closure).
        assert_eq!(
            outcome.non_quad_rows.len(),
            3,
            "expected 3 helperEdge rows in the non-quad bucket: {:?}",
            outcome.non_quad_rows
        );
        for row in &outcome.non_quad_rows {
            assert_eq!(row.predicate, "helperEdge");
            assert_eq!(row.args.len(), 2, "helperEdge is binary: {row:?}");
            assert!(!row.is_edb, "helperEdge rows are rule-derived: {row:?}");
        }
        let helper_pairs: std::collections::HashSet<(String, String)> = outcome
            .non_quad_rows
            .iter()
            .map(|r| {
                (
                    crate::provenance::term_display(&r.args[0]),
                    crate::provenance::term_display(&r.args[1]),
                )
            })
            .collect();
        assert_eq!(
            helper_pairs, expected_pairs,
            "helper rows must mirror the subClassOf pairs"
        );

        // (c) nothing silent: 3 quads + 3 helper rows = every materialized row.
        assert_eq!(outcome.quads.len(), 3);
        assert_eq!(outcome.quads.len() + outcome.non_quad_rows.len(), 6);
    }

    /// A non-ternary ANTECEDENT of a ternary row stays a hard error: the
    /// consumed premise is not a world-scoped quad, has no reifier, and a
    /// partial source list would mint a wrong derivation_id.
    #[test]
    fn materialize_core_non_ternary_antecedent_is_hard_error() {
        let err = materialize_core(HELPER_CONSUMING_RULES, CHAIN_NQUADS, None, None, None)
            .expect_err("a ternary row derived from a binary antecedent must hard-error");
        match &err {
            MaterializeError::Chase(msg) => assert!(
                msg.contains("antecedent") && msg.contains("has 2 values (expected 3)"),
                "error must name the non-ternary antecedent: {msg}"
            ),
            other => panic!("expected MaterializeError::Chase, got {other:?}"),
        }
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
                    crate::provenance::term_display(&q.subject),
                    q.predicate.as_str().to_owned(),
                    crate::provenance::term_display(&q.object),
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

    /// A step/derivation budget (`max_rule_firings`) now runs on the NATIVE core, not the
    /// Nemo fallback: the semi-naive governor honours the ceiling and stamps `Exhausted`.
    /// A zero-firing budget derives NO IDB (only the asserted EDB echo) and marks every
    /// emitted quad `Exhausted` — incomplete, never wrong.
    #[test]
    fn materialize_routed_forward_budget_runs_native() {
        let m = materialize_routed(
            TRANSITIVITY_RULES,
            CHAIN_NQUADS,
            Some(0), // zero rule firings — the native governor stops before any derivation
            None,
            None,
            Some("PositiveHornProfile"),
        )
        .expect("budgeted routed materialize must not fail (native governor handles it)");

        // The two asserted EDB edges are echoed; no derived subClassOf edge is produced.
        assert_eq!(
            m.quads.len(),
            2,
            "0-firing budget ⇒ only the 2 EDB echoes: {:?}",
            m.quads
        );
        let sub_class_of = "https://blackcatinformatics.ca/logic/subClassOf";
        let derived_present = m.quads.iter().any(|q| {
            crate::provenance::term_display(&q.subject) == "<http://example.org/Dog>"
                && q.predicate.as_str() == sub_class_of
                && crate::provenance::term_display(&q.object) == "<http://example.org/Animal>"
        });
        assert!(
            !derived_present,
            "Dog ⊑ Animal must NOT be derived under a 0-firing budget"
        );
        assert!(
            m.quads
                .iter()
                .all(|q| q.budget_status == BudgetStatus::Exhausted),
            "every emitted quad must be stamped Exhausted under an exceeded budget"
        );
        // GAP A: the completion frontier crosses the PUBLIC `Materialization` boundary.
        // A 0-firing cut leaves the single subClassOf stratum unsaturated (0 of 1); only
        // the EDB predicate is settled, and no derivation was committed.
        assert_eq!(
            m.frontier.completed, 0,
            "the cut stratum is not saturated: {:?}",
            m.frontier
        );
        assert_eq!(
            m.frontier.total, 1,
            "one stratum in the transitivity program"
        );
        assert_eq!(
            m.frontier.consumed_steps, 0,
            "a 0-firing budget commits no derivation"
        );
        // `subClassOf` is SELF-RECURSIVE — both the asserted EDB and the recursive rule
        // head — so its full least-model extension includes the (undrawn) transitive
        // closure. A 0-firing cut leaves that stratum unsaturated, so the predicate is NOT
        // settled: the frontier under-claims rather than over-claiming a complete
        // extension. Consequently every emitted quad (the EDB echoes) is correctly stamped
        // `Exhausted` above — the frontier-aware stamp does not spuriously promote them.
        assert!(
            !m.frontier.saturated_preds.contains(sub_class_of),
            "a cut self-recursive head is NOT settled from the EDB seed alone: {:?}",
            m.frontier.saturated_preds
        );
    }

    /// A budget LARGER than the completion cost completes on native with `Ok`: the full
    /// closure is derived and no quad is stamped `Exhausted`.
    #[test]
    fn materialize_routed_forward_budget_completes_ok() {
        // The chain closure needs exactly one derivation (Dog ⊑ Animal); a budget of 1
        // (or more) reaches the fixpoint.
        let m = materialize_routed(
            TRANSITIVITY_RULES,
            CHAIN_NQUADS,
            Some(8),
            None,
            None,
            Some("PositiveHornProfile"),
        )
        .expect("budgeted routed materialize must not fail");
        assert_eq!(m.quads.len(), 3, "2 EDB + 1 derived closure edge");
        assert!(
            m.quads.iter().all(|q| q.budget_status == BudgetStatus::Ok),
            "an ample budget completes ⇒ every quad Ok"
        );
        // GAP A: an ample budget saturates every stratum, so the public frontier reports
        // `completed == total` — a complete run, distinct from the cut one above.
        assert_eq!(
            m.frontier.completed, m.frontier.total,
            "an ample budget saturates the whole program: {:?}",
            m.frontier
        );
        assert_eq!(
            m.frontier.total, 1,
            "one stratum in the transitivity program"
        );
        assert!(
            m.frontier.consumed_steps >= 1,
            "the closure edge is one committed derivation: {:?}",
            m.frontier
        );
    }

    // ── Frontier-aware per-quad stamping under a MID-stratum cut ──────────────────
    //
    // A two-stratum stratified-negation program: `reachable` (stratum 0, seed + edge
    // step) then `unreachable` (stratum 1, `~reachable`). Over nodes {a, b, c, d} with
    // `reachableSeed(a)` and `edge(a, b)`: `reachable = {a, b}` (2 derivations) and
    // `unreachable = {c, d}` (2 derivations). A 3-firing budget saturates stratum 0 (2
    // derivations) then commits ONE `unreachable` derivation before the cut — so the two
    // strata are observably differently settled in the SAME run.

    /// Two-stratum reach/unreach program in the 3-ary (world-column) rule syntax.
    const REACH_RULES: &str = concat!(
        "#[name(\"http://example.org/rules/reachSeed\")]\n",
        "<http://example.org/reachable>(?X, ?X, ?C) :-\n",
        "    <http://example.org/reachableSeed>(?X, ?X, ?C) .\n",
        "#[name(\"http://example.org/rules/reachStep\")]\n",
        "<http://example.org/reachable>(?Y, ?Y, ?C) :-\n",
        "    <http://example.org/reachable>(?X, ?X, ?C),\n",
        "    <http://example.org/edge>(?X, ?Y, ?C) .\n",
        "#[name(\"http://example.org/rules/unreach\")]\n",
        "<http://example.org/unreachable>(?X, ?X, ?C) :-\n",
        "    <http://example.org/node>(?X, ?X, ?C),\n",
        "    ~<http://example.org/reachable>(?X, ?X, ?C) .\n",
    );

    /// EDB for [`REACH_RULES`]: four self-loop `node` facts, one `reachableSeed`, one
    /// `edge` a→b, all in world `W`.
    const REACH_NQUADS: &str = concat!(
        "<http://example.org/a> <http://example.org/node> <http://example.org/a> <http://world/W> .\n",
        "<http://example.org/b> <http://example.org/node> <http://example.org/b> <http://world/W> .\n",
        "<http://example.org/c> <http://example.org/node> <http://example.org/c> <http://world/W> .\n",
        "<http://example.org/d> <http://example.org/node> <http://example.org/d> <http://world/W> .\n",
        "<http://example.org/a> <http://example.org/reachableSeed> <http://example.org/a> <http://world/W> .\n",
        "<http://example.org/a> <http://example.org/edge> <http://example.org/b> <http://world/W> .\n",
    );

    const REACHABLE_PRED: &str = "http://example.org/reachable";
    const UNREACHABLE_PRED: &str = "http://example.org/unreachable";

    /// GAP B (forward). Under an `Exhausted` mid-stratum cut, a quad whose predicate's
    /// stratum SATURATED carries `Ok` (its extension is final — complete-for-fragment),
    /// while a quad from the CUT stratum carries `Exhausted`. The old blanket stamp
    /// over-claimed `Exhausted` on the settled stratum too; this is exactly the verdict
    /// the completion frontier makes observable.
    #[test]
    fn materialize_routed_forward_frontier_aware_per_quad_status() {
        let m = materialize_routed(
            REACH_RULES,
            REACH_NQUADS,
            Some(3), // saturate `reachable` (2), commit 1 `unreachable`, then cut
            None,
            None,
            Some("StratifiedNAFProfile"),
        )
        .expect("budgeted routed materialize must not fail (native governor handles it)");

        // The run is incomplete overall: stratum 1 (`unreachable`) was cut mid-fixpoint.
        assert_eq!(
            m.frontier.completed, 1,
            "only stratum 0 (reachable) saturated: {:?}",
            m.frontier
        );
        assert_eq!(m.frontier.total, 2, "reachable + unreachable strata");
        assert_eq!(
            m.frontier.consumed_steps, 3,
            "2 reachable + 1 unreachable derivation before the cut: {:?}",
            m.frontier
        );
        assert!(
            m.frontier.saturated_preds.contains(REACHABLE_PRED),
            "reachable's stratum completed ⇒ settled: {:?}",
            m.frontier.saturated_preds
        );
        assert!(
            !m.frontier.saturated_preds.contains(UNREACHABLE_PRED),
            "unreachable's stratum was cut ⇒ NOT settled: {:?}",
            m.frontier.saturated_preds
        );

        // Every `reachable` (and EDB `node`/`edge`/`reachableSeed`) quad is conclusive:
        // stamped `Ok` even though the RUN exhausted. This also proves the predicate-name
        // membership test actually matches (a silent no-op would leave these `Exhausted`).
        let reachable_quads: Vec<_> = m
            .quads
            .iter()
            .filter(|q| q.predicate.as_str() == REACHABLE_PRED)
            .collect();
        assert_eq!(
            reachable_quads.len(),
            2,
            "reachable = {{a, b}}: {:?}",
            m.quads
        );
        assert!(
            reachable_quads
                .iter()
                .all(|q| q.budget_status == BudgetStatus::Ok),
            "saturated-stratum quads are conclusive (Ok) under an exhausted run"
        );
        assert!(
            m.quads
                .iter()
                .filter(|q| {
                    let p = q.predicate.as_str();
                    p != REACHABLE_PRED && p != UNREACHABLE_PRED
                })
                .all(|q| q.budget_status == BudgetStatus::Ok),
            "EDB predicates are settled from the seed ⇒ their echoes are Ok"
        );

        // The committed `unreachable` quad is from the CUT stratum: genuinely incomplete,
        // stamped `Exhausted`. Exactly one was committed before the budget tripped.
        let unreachable_quads: Vec<_> = m
            .quads
            .iter()
            .filter(|q| q.predicate.as_str() == UNREACHABLE_PRED)
            .collect();
        assert_eq!(
            unreachable_quads.len(),
            1,
            "budget 3 commits one unreachable derivation before the cut: {:?}",
            m.quads
        );
        assert!(
            unreachable_quads
                .iter()
                .all(|q| q.budget_status == BudgetStatus::Exhausted),
            "cut-stratum quads are incomplete (Exhausted)"
        );
    }

    /// GAP B determinism: the frontier-aware stamping is a pure function of the inputs, so
    /// a re-run is byte-identical (same quads, same per-quad statuses, same frontier).
    #[test]
    fn materialize_routed_forward_frontier_aware_is_deterministic() {
        let run = || {
            materialize_routed(
                REACH_RULES,
                REACH_NQUADS,
                Some(3),
                None,
                None,
                Some("StratifiedNAFProfile"),
            )
            .expect("materialize must not fail")
        };
        let a = run();
        let b = run();
        let key = |m: &Materialization| {
            let mut rows: Vec<(String, String, String, String)> = m
                .quads
                .iter()
                .map(|q| {
                    (
                        crate::provenance::term_display(&q.subject),
                        q.predicate.clone(),
                        crate::provenance::term_display(&q.object),
                        format!("{:?}", q.budget_status),
                    )
                })
                .collect();
            rows.sort();
            rows
        };
        assert_eq!(key(&a), key(&b), "re-run must be identical");
        assert_eq!(
            a.frontier, b.frontier,
            "the frontier is deterministic across runs"
        );
    }

    /// A wall-clock budget (`time_ms`) remains a native gap and demotes to the Nemo
    /// post-hoc governor: the native semi-naive engine counts committed derivations, not
    /// elapsed time. The routed result must equal `materialize_core` (the Nemo path)
    /// exactly, proving the demotion.
    #[test]
    fn materialize_routed_time_budget_uses_nemo_fallback() {
        let m = materialize_routed(
            TRANSITIVITY_RULES,
            CHAIN_NQUADS,
            None,
            None,
            Some(0), // a zero wall-clock budget the native core has no governor for
            Some("PositiveHornProfile"),
        )
        .expect("time-budgeted routed materialize must not fail (Nemo governor handles it)");
        let oracle = materialize_core(TRANSITIVITY_RULES, CHAIN_NQUADS, None, None, Some(0))
            .expect("time-budgeted Nemo materialize_core must succeed");
        assert_eq!(
            m.quads, oracle.quads,
            "a time_ms budget must demote to the Nemo fallback exactly"
        );
    }
}
