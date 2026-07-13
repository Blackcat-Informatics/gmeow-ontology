// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native typed-closure adapters and neutral row vocabulary.
//!
//! A *reasoner* is a partial decision procedure over a fragment of the logic.
//! The native physical core (`crate::physical`) is the sole production authority.
//! This module carries the neutral typed rows used by the structured native
//! materialization adapters.
//! The independent backward resolver and the `purrdf::entail` cross-check remain
//! comparison surfaces only; neither is a production fallback.
//!
//! # Neutral vocabulary
//!
//! The closure vocabulary ([`TypedRow`], [`TypedProvenance`], [`TypedChaseResult`])
//! lives here rather than inside a parser or evaluator, so consumers share one
//! engine-neutral result shape.
//!
//! # Provenance as a capability
//!
//! Native Record-mode evaluation populates provenance. Comparison adapters that do
//! not carry a proof-height annotation use `None`; consumers must never fabricate an
//! attribution or silently reinterpret absence as zero.

use purrdf::TermValue;
use purrdf::provenance::Attribution;

/// Wrap a reasoning-oracle condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn oracle_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Oracle { detail })
}

// ── Neutral closure vocabulary ────────────────────────────────────────────────

/// A single materialized row with decoded, native-term arguments.
///
/// The predicate stays a relation-name `String` (it is a name, not a term — see
/// [`crate::facts::TypedFact`]); every argument is a decoded [`TermValue`].
/// Arity-generic: callers coerce positions (e.g. subject/object/world for a
/// ternary reasoning row).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TypedRow {
    /// The relation name (a full predicate IRI, un-bracketed, or a bare
    /// program-local predicate symbol).
    pub predicate: String,
    /// One decoded native term per column in the row.
    pub args: Vec<TermValue>,
}

/// Provenance metadata for a typed row.
///
/// Every populated field carries real native trace data. An adapter that lacks an
/// annotation leaves the optional field empty; fabricated attribution is a hard
/// error, never a silent default.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TypedProvenance {
    /// Whether this fact is an EDB (asserted input) fact.
    pub is_edb: bool,
    /// Name of the rule that derived this fact, as set via `#[name("...")]`.
    pub rule_name: Option<String>,
    /// Immediate antecedent facts (premises) that the rule consumed, decoded.
    pub antecedents: Vec<TypedRow>,
    /// Selected minimal proof-tree height from the native Record lane (`0` for
    /// asserted input). Oracles/evaluators that do not carry the annotation report
    /// `None`; absence is never fabricated as zero.
    pub proof_height: Option<crate::provenance::ProofHeight>,
    /// Structured slice attributions (§9 / S5) — carried through unchanged.
    /// Populated at the validation boundary when slice context is available;
    /// no in-crate consumer reads it yet.
    #[allow(dead_code)]
    pub attributions: Vec<Attribution>,
}

/// The full result of a typed forward materialization: every derived row with
/// its provenance.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TypedChaseResult {
    /// All materialized rows, each paired with its provenance.
    pub rows: Vec<(TypedRow, TypedProvenance)>,
}

// Structured native materialization.
pub(crate) fn native_forward_eval_rules_with_frontier(
    facts: &crate::facts::TypedFactSet,
    eval_rules: Vec<crate::rule_ir::EvalRule>,
) -> gmeow_errors::Result<(TypedChaseResult, crate::query_ir::CompletionFrontier)> {
    let interner = facts.interner();
    let store = crate::store::WorldStore::new();
    for fact in facts.facts() {
        if fact.args.len() != 3 {
            return Err(oracle_err(format!(
                "native structured-rule EDB fact for predicate {:?} has arity {} (expected 3)",
                fact.predicate,
                fact.args.len()
            )));
        }
        let subject = interner.resolve(fact.args[0]).clone();
        let object = interner.resolve(fact.args[1]).clone();
        let world = world_lexical(interner.resolve(fact.args[2]))?;
        store
            .insert_quad_terms(&world, subject, TermValue::iri(&fact.predicate), object)
            .map_err(|e| oracle_err(format!("native structured-rule store seed failed: {e}")))?;
    }

    let lookup = crate::physical::compile_cached(crate::reason::native_contract_hash(), eval_rules);
    let Some(executable) = lookup.executable else {
        return Err(oracle_err(
            "native structured-rule chase does not decide a non-stratifiable program".to_owned(),
        ));
    };
    let (rows, frontier) =
        match crate::physical::materialize_native(&store, executable.as_ref(), None)? {
            crate::physical::NativeOutcome::Decided(budgeted) => {
                let frontier = budgeted.frontier();
                (budgeted.rows, frontier)
            }
            crate::physical::NativeOutcome::Unsupported(kind) => {
                return Err(oracle_err(format!(
                    "native structured-rule chase returned Unsupported({kind:?})"
                )));
            }
        };

    let typed = rows
        .into_iter()
        .map(|row| {
            let is_edb = row.rule_iri == crate::provenance::ASSERT_RULE_IRI;
            let proof_height = row.proof_height;
            let rule_name = if is_edb { None } else { Some(row.rule_iri) };
            let antecedents = row
                .antecedents
                .into_iter()
                .map(|ante| TypedRow {
                    predicate: ante.predicate,
                    args: vec![
                        ante.subject,
                        ante.object,
                        TermValue::simple_literal(&row.graph),
                    ],
                })
                .collect();
            (
                TypedRow {
                    predicate: row.predicate,
                    args: vec![
                        row.subject,
                        row.object,
                        TermValue::simple_literal(&row.graph),
                    ],
                },
                TypedProvenance {
                    is_edb,
                    rule_name,
                    antecedents,
                    proof_height: Some(proof_height),
                    attributions: Vec::new(),
                },
            )
        })
        .collect();
    Ok((TypedChaseResult { rows: typed }, frontier))
}

/// Decode the named world carried by a typed fact.
fn world_lexical(term: &TermValue) -> gmeow_errors::Result<String> {
    match term {
        TermValue::Literal {
            lexical_form,
            datatype,
            language: None,
            ..
        } if datatype == "http://www.w3.org/2001/XMLSchema#string" => Ok(lexical_form.clone()),
        other => Err(oracle_err(format!(
            "native structured-rule EDB world term must be a plain string literal, got {other:?}"
        ))),
    }
}
