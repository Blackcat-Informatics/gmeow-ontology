// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The reasoning-oracle boundary.
//!
//! A *reasoner* is a partial decision procedure over a fragment of the logic:
//! the native physical core (`crate::physical`) is the primary path, and an
//! external engine is consulted only for the fragment the native core does not
//! yet decide.  This module makes that boundary a typed seam so the external
//! engines are **swappable adapters** rather than concretely-named call targets.
//!
//! Two dual traits mirror the forward/backward duality of Datalog±:
//! materialization (least-fixpoint `T_P` closure) and goal resolution (SLD).
//!
//! - [`ForwardOracle`] — materialize the deductive closure of a typed EDB under
//!   a rule program.  The Nemo bridge is the sole implementer today
//!   ([`NemoForwardOracle`]).
//! - [`BackwardOracle`] — resolve a goal against a world's facts, returning an
//!   answer set.  Implemented by the Scryer engine ([`ScryerBackwardOracle`])
//!   and by the declarative SLD reference resolver (`ReferenceBackwardOracle`,
//!   the parity oracle the native magic-sets engine is checked against).
//!
//! # Neutral vocabulary
//!
//! The closure vocabulary ([`TypedRow`], [`TypedProvenance`], [`TypedChaseResult`])
//! lives here, not inside any adapter, so the trait does not depend on the
//! engine that happens to produce it — this is what lets an engine's *solver
//! adapter* be deleted.  For Scryer that solver adapter is the whole engine
//! (retiring it is removing its adapter + its Cargo line).  Nemo also carries a
//! separate rule/term codec (`NemoParsedRules` / `decode_nemo_term`), a
//! wire-format concern distinct from solver invocation, so fully retiring Nemo
//! additionally requires neutralizing that codec (see *Single naming site*).
//!
//! # Provenance as a capability
//!
//! Nemo attributes each derived fact via `engine.trace()`; the native core's
//! provenance has a different shape.  So provenance is a *queried capability*
//! ([`ForwardOracle::provides_provenance`]), never a mandatory method — an
//! oracle that cannot attribute derivations reports `false` and its consumers
//! hard-fail rather than fabricate attribution.
//!
//! # Single naming site
//!
//! [`forward_oracle`] and [`backward_oracle`] are the *only* places a solver is
//! invoked.  Every call site depends on the trait via these providers, so
//! swapping the backing solver (or deleting the Scryer adapter outright) is a
//! one-line change here.  Nemo's rule/term *codec* (`NemoParsedRules` /
//! `decode_nemo_term`) is a distinct wire-format subsystem — the neutral rule-IR
//! carrier — named outside this seam in production code, so retiring Nemo
//! *entirely* additionally requires neutralizing that codec; it is not covered
//! by this solver boundary.

use purrdf::provenance::Attribution;
use purrdf::TermValue;

use crate::query_ir::{AnswerSet, Budget, QProgram};
use crate::seam::ScryerForeign;

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

// ── Forward budget ────────────────────────────────────────────────────────────

/// A declared bound on a forward materialization.
///
/// Distinct from the backward [`Budget`]: a forward run is bounded by rule
/// firings, derived-answer count, and post-fixpoint wall-clock — not by SLD
/// inference steps.  The default is unbounded.
///
/// The Nemo chase is not interruptible, so a [`ForwardOracle`] backed by it
/// cannot honor a non-default `ForwardBudget` *inline*; enforcement is a
/// governor concern layered above the oracle.  A non-default budget handed to
/// such an oracle is therefore a hard error (see [`NemoForwardOracle::materialize`]),
/// never silently dropped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ForwardBudget {
    /// Maximum IDB rule firings.
    pub max_rule_firings: Option<u64>,
    /// Maximum derived answers.
    pub max_answers: Option<u64>,
    /// Post-fixpoint wall-clock ceiling, in milliseconds.
    pub time_ms: Option<u64>,
}

impl ForwardBudget {
    /// The unbounded budget (no field set) — the value every current forward
    /// call site passes, since inline forward-budget governance is a
    /// native-governor concern above the oracle boundary, not an oracle
    /// capability.
    pub const UNBOUNDED: ForwardBudget = ForwardBudget {
        max_rule_firings: None,
        max_answers: None,
        time_ms: None,
    };

    /// Whether any bound is set.
    pub fn is_bounded(&self) -> bool {
        self.max_rule_firings.is_some() || self.max_answers.is_some() || self.time_ms.is_some()
    }
}

// ── Forward oracle ────────────────────────────────────────────────────────────

/// A forward reasoner: materialize the deductive closure of a typed EDB under a
/// rule program.
pub(crate) trait ForwardOracle {
    /// Stable label for ledgers and diagnostics (e.g. `"nemo"`).
    fn name(&self) -> &'static str;

    /// Materialize the closure of `facts` under `rules`.
    ///
    /// `rules` is the engine's rule text (the existential-rule superset carrier).
    /// `budget` is a declared bound; an oracle that cannot honor a non-default
    /// bound inline must return `Err` rather than silently ignore it.
    fn materialize(
        &self,
        facts: &crate::facts::TypedFactSet,
        rules: &str,
        budget: &ForwardBudget,
    ) -> Result<TypedChaseResult, String>;

    /// Whether [`materialize`](Self::materialize) populates per-row provenance.
    fn provides_provenance(&self) -> bool;
}

/// The Nemo forward adapter.  Wraps `nemo_engine::run_chase_typed` verbatim; the
/// process-global `CHASE_LOCK` stays inside that call.
pub(crate) struct NemoForwardOracle;

impl ForwardOracle for NemoForwardOracle {
    fn name(&self) -> &'static str {
        "nemo"
    }

    fn materialize(
        &self,
        facts: &crate::facts::TypedFactSet,
        rules: &str,
        budget: &ForwardBudget,
    ) -> Result<TypedChaseResult, String> {
        // The Nemo chase is not interruptible and applies no budget inline; a
        // non-default budget cannot be honored here.  Hard-fail rather than run
        // an unbounded chase and pretend the bound was respected (no seam lie).
        if budget.is_bounded() {
            return Err(format!(
                "NemoForwardOracle cannot honor a forward budget inline \
                 ({budget:?}); forward-budget governance is a router/native-governor \
                 concern above the oracle boundary"
            ));
        }
        crate::nemo_engine::run_chase_typed(facts, rules)
    }

    fn provides_provenance(&self) -> bool {
        true
    }
}

/// The default forward oracle — the sole engine-naming site for materialization.
pub(crate) fn forward_oracle() -> impl ForwardOracle {
    NemoForwardOracle
}

// ── Backward oracle ───────────────────────────────────────────────────────────

/// A backward reasoner: resolve `program`'s goal against `world`'s facts.
pub(crate) trait BackwardOracle {
    /// Stable label for ledgers and diagnostics (e.g. `"scryer"`).
    fn name(&self) -> &'static str;

    /// Resolve the goal, returning a canonical answer set plus budget status.
    ///
    /// `tabling` lists IDB predicate IRIs to memoize (cyclic predicates).  It is
    /// **advisory**: an oracle that ignores it must still return the same answer
    /// set — tabling affects termination/performance, never the answers — so a
    /// resolver with no memo table (e.g. `ReferenceBackwardOracle`) honoring
    /// the contract while dropping `tabling` is not an LSP violation.
    fn solve(
        &self,
        foreign: &dyn ScryerForeign,
        world: &str,
        program: &QProgram,
        tabling: &[String],
        budget: &Budget,
    ) -> Result<AnswerSet, String>;
}

/// The Scryer backward adapter.  Wraps `scryer_engine::run_scryer` verbatim; the
/// process-global `SCRYER_LOCK` stays inside that call.
pub(crate) struct ScryerBackwardOracle;

impl BackwardOracle for ScryerBackwardOracle {
    fn name(&self) -> &'static str {
        "scryer"
    }

    fn solve(
        &self,
        foreign: &dyn ScryerForeign,
        world: &str,
        program: &QProgram,
        tabling: &[String],
        budget: &Budget,
    ) -> Result<AnswerSet, String> {
        crate::scryer_engine::run_scryer(foreign, world, program, tabling, budget)
    }
}

/// The declarative SLD reference resolver as a backward oracle — the parity
/// oracle the native magic-sets engine is checked against.  SLD needs no memo
/// table, so `tabling` is ignored (answer-preserving, per the trait contract).
///
/// This is a conformance/parity oracle, not a production engine (the production
/// backward oracle is [`ScryerBackwardOracle`]); it exists solely so the parity
/// gate can be generic over [`BackwardOracle`], hence `#[cfg(test)]`.
#[cfg(test)]
pub(crate) struct ReferenceBackwardOracle;

#[cfg(test)]
impl BackwardOracle for ReferenceBackwardOracle {
    fn name(&self) -> &'static str {
        "reference-sld"
    }

    fn solve(
        &self,
        foreign: &dyn ScryerForeign,
        world: &str,
        program: &QProgram,
        _tabling: &[String],
        budget: &Budget,
    ) -> Result<AnswerSet, String> {
        crate::reference_resolver::resolve(foreign, world, program, budget)
    }
}

/// The default backward oracle — the sole engine-naming site for resolution.
pub(crate) fn backward_oracle() -> impl BackwardOracle {
    ScryerBackwardOracle
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::TypedFactSet;

    /// A trivial transitive-closure chase materializes the derived edge and the
    /// Nemo adapter reports it provides provenance.
    #[test]
    fn nemo_forward_oracle_materializes_and_provides_provenance() {
        let oracle = forward_oracle();
        assert_eq!(oracle.name(), "nemo");
        assert!(oracle.provides_provenance());

        // EDB: edge(a, b), edge(b, c). Rule: path is edge, transitively closed.
        // Full IRIs (predicate contains `://` so it renders angle-bracketed) and the
        // `#[name(...)]` directive Nemo expects, mirroring the parity corpus conventions.
        let edge = "http://example.org/edge";
        let path = "http://example.org/path";
        let world = "http://example.org/world";
        let mut edb = TypedFactSet::new();
        let a = TermValue::Iri("http://example.org/a".into());
        let b = TermValue::Iri("http://example.org/b".into());
        let c = TermValue::Iri("http://example.org/c".into());
        edb.push_quad(&a, edge, &b, world);
        edb.push_quad(&b, edge, &c, world);

        let rules = format!(
            "#[name(\"http://example.org/rules/edge-is-path\")]\n\
             <{path}>(?s, ?o, ?w) :- <{edge}>(?s, ?o, ?w) .\n\
             #[name(\"http://example.org/rules/path-trans\")]\n\
             <{path}>(?s, ?o, ?w) :- <{path}>(?s, ?m, ?w), <{edge}>(?m, ?o, ?w) .\n"
        );

        let result = oracle
            .materialize(&edb, &rules, &ForwardBudget::UNBOUNDED)
            .expect("unbudgeted chase must succeed");

        // path(a, c, w) is derived by transitivity.
        let derived_path_a_c = result.rows.iter().any(|(row, _prov)| {
            row.predicate.contains("path")
                && row.args.len() == 3
                && row.args[0] == a
                && row.args[1] == c
        });
        assert!(
            derived_path_a_c,
            "transitive path(a,c) must be materialized; got {:?}",
            result.rows
        );
    }

    /// A non-default forward budget is a hard error, never a silently-unbudgeted
    /// full chase (no seam lie).
    #[test]
    fn nemo_forward_oracle_rejects_a_budget_it_cannot_honor() {
        let oracle = forward_oracle();
        let edb = TypedFactSet::new();
        let budget = ForwardBudget {
            max_rule_firings: Some(10),
            ..ForwardBudget::default()
        };
        let err = oracle
            .materialize(&edb, "", &budget)
            .expect_err("a bounded budget must be rejected, not silently ignored");
        assert!(err.contains("cannot honor a forward budget"), "got: {err}");
    }

    /// The default backward oracle is the Scryer adapter.
    #[test]
    fn backward_oracle_default_is_scryer() {
        assert_eq!(backward_oracle().name(), "scryer");
        assert_eq!(ReferenceBackwardOracle.name(), "reference-sld");
    }
}
