// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The reasoning-oracle boundary.
//!
//! A *reasoner* is a partial decision procedure over a fragment of the logic.
//! The native physical core (`crate::physical`) is the production authority;
//! independent reference evaluators are test-only comparison adapters.
//!
//! Two dual traits mirror the forward/backward duality of Datalog±:
//! materialization (least-fixpoint `T_P` closure) and reference goal resolution.
//!
//! - [`ForwardOracle`] — materialize the deductive closure of a typed EDB under
//!   a rule program. The production implementation is the native stratified core,
//!   returned by [`forward_oracle`].
//! - [`BackwardOracle`] — test-only seam for the declarative SLD reference
//!   resolver (`ReferenceBackwardOracle`), used to check the native demand-
//!   transformed engine against an independent evaluation strategy.
//!
//! # Neutral vocabulary
//!
//! The closure vocabulary ([`TypedRow`], [`TypedProvenance`], [`TypedChaseResult`])
//! lives here, not inside any adapter, so the trait does not depend on the
//! engine that happens to produce it.
//!
//! # Provenance as a capability
//!
//! Provenance is a *queried capability*
//! ([`ForwardOracle::provides_provenance`]), never a mandatory method — an
//! oracle that cannot attribute derivations reports `false` and its consumers
//! hard-fail rather than fabricate attribution.
//!
//! [`forward_oracle`] is the production materialization provider.

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
/// An oracle that reports [`ForwardOracle::provides_provenance`] `== false` must
/// never emit a populated `TypedProvenance` (fabricated attribution is a hard
/// error, not a silent default) — the field carries real trace data or nothing.
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

/// Benchmark-corpus adapter for the repo-owned named-ternary fixture language.
pub(crate) fn native_forward_with_frontier(
    facts: &crate::facts::TypedFactSet,
    source: &str,
) -> gmeow_errors::Result<(TypedChaseResult, crate::query_ir::CompletionFrontier)> {
    let rules = crate::rule_ir::parse_benchmark_rules(source)?;
    native_forward_eval_rules_with_frontier(facts, rules)
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
            "NativeForwardOracle EDB world term must be a plain string literal, got {other:?}"
        ))),
    }
}
