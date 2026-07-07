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
//!   a rule program.  The PRODUCTION implementer is the native stratified core
//!   ([`NativeForwardOracle`], returned by [`forward_oracle`]); the Nemo bridge
//!   ([`NemoForwardOracle`], reached only via [`nemo_forward_oracle`]) is
//!   retained solely as the bootstrap/parity oracle off the primary path.
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

use purrdf::TermValue;
use purrdf::provenance::Attribution;

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
///
/// Off the primary path after the native flip: constructed only via
/// [`nemo_forward_oracle`] (the parity gates + the scheduled cross-check lane
/// `crate::reason::crosscheck_native_vs_nemo`), never from `reason_all`.
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
///
/// This is the PRODUCTION materialization engine: the native stratified
/// semi-naive core ([`NativeForwardOracle`]).  Every production reasoning entry
/// (`reason_all` / `run_reasoning`, the RL fragment, and `materialize`) resolves
/// its forward chase through here, so the whole-bundle closure runs native.
///
/// Nemo is NO LONGER on the primary path.  It is retained ONLY as the bootstrap
/// oracle for the scheduled cross-check lane and the native↔Nemo parity gates —
/// reached exclusively via [`nemo_forward_oracle`], never from `reason_all`.
pub(crate) fn forward_oracle() -> impl ForwardOracle {
    NativeForwardOracle
}

/// The Nemo forward oracle — the ONLY remaining Nemo materialization entry point.
///
/// After the flip of [`forward_oracle`] onto the native core, Nemo leaves the
/// primary reasoning path entirely.  This provider is the single seam through
/// which the retained Nemo engine is reached: the native↔Nemo parity gates and
/// the scheduled cross-check lane consult it, and nothing on the `reason_all`
/// production path does.  Keeping it a named provider (rather than constructing
/// [`NemoForwardOracle`] ad hoc) preserves the "single naming site" discipline
/// for the bootstrap oracle too.
///
/// Its production consumer is the scheduled differential cross-check
/// [`crate::reason::crosscheck_native_vs_nemo`] (the `reason-nemo-crosscheck` /
/// `make maint-nemo-crosscheck` lane); the `#[cfg(test)]` parity gates also reach
/// Nemo through here.
pub(crate) fn nemo_forward_oracle() -> NemoForwardOracle {
    NemoForwardOracle
}

/// A facts-only Nemo forward adapter for the **existential** fragment.
///
/// The value-inventing chase mints labeled nulls that Nemo's provenance trace cannot
/// follow (`run_chase_typed` hard-errors "no trace tree"), so this oracle uses the
/// facts-only path ([`crate::nemo_engine::run_chase_typed_facts_only`]) and reports
/// `provides_provenance() == false`.  It is the parity oracle for the native existential
/// chase, where the gate compares FACTS null-blind (provenance is exempt).
///
/// Consumed by the existential-chase parity gate and by `materialize_routed`, which
/// demotes an uncertified existential program to this facts-only path (the provenance
/// oracle would hard-error on its invented nulls).
pub(crate) struct NemoFactsOracle;

impl ForwardOracle for NemoFactsOracle {
    fn name(&self) -> &'static str {
        "nemo"
    }

    fn materialize(
        &self,
        facts: &crate::facts::TypedFactSet,
        rules: &str,
        budget: &ForwardBudget,
    ) -> Result<TypedChaseResult, String> {
        if budget.is_bounded() {
            return Err(format!(
                "NemoFactsOracle cannot honor a forward budget inline ({budget:?}); \
                 forward-budget governance is a router/native-governor concern above \
                 the oracle boundary"
            ));
        }
        let rows = crate::nemo_engine::run_chase_typed_facts_only(facts, rules)?;
        // `provides_provenance() == false`, so every row carries EMPTY provenance
        // (never a fabricated attribution — the no-optionality doctrine).
        Ok(TypedChaseResult {
            rows: rows
                .into_iter()
                .map(|row| {
                    (
                        row,
                        TypedProvenance {
                            is_edb: false,
                            rule_name: None,
                            antecedents: Vec::new(),
                            attributions: Vec::new(),
                        },
                    )
                })
                .collect(),
        })
    }

    fn provides_provenance(&self) -> bool {
        false
    }
}

/// The native forward adapter — gmeow's own stratified semi-naive core.
///
/// Wraps [`crate::physical::materialize_native`] (the native chase
/// `crate::materialize::materialize_routed` already runs) behind the
/// [`ForwardOracle`] seam, so the fixed OWL-profile rule texts can be routed onto
/// the native engine instead of Nemo.  This is the substitution point the oracle
/// boundary was built for.
///
/// The native chase is a pure Horn/stratified evaluator: it takes the ternary
/// `predicate(subject, object, world)` EDB, reconstructs the world-indexed
/// [`WorldStore`](crate::store::WorldStore) it materializes over, runs the fixed
/// least-fixpoint closure, and re-exposes each [`DerivedRow`](crate::rule_ir::DerivedRow)
/// as a ternary [`TypedRow`].  It reports `provides_provenance() == true`: each
/// row carries its firing-rule identity (the assert sentinel for echoed EDB, else
/// the rule IRI) AND its immediate antecedents, re-exposed from the native
/// `DerivedRow.antecedents` (the matched body facts) as ternary
/// `predicate(subject, object, world)` rows — the production provenance the
/// reason/explain/materialize consumers require (they cannot invert a reifier
/// hash).  The FACT set is what the parity ledger compares — provenance is exempt
/// (`compare_materialization` compares only fact keys) — but the antecedents must
/// be real here, since this is now the primary reasoning path.
///
/// This is the PRODUCTION materialization path: [`forward_oracle`] returns it, so
/// every production reasoning entry (`reason_all` / `run_reasoning`, the RL
/// fragment, and `materialize`) runs its forward chase through this adapter.  The
/// native↔Nemo parity gates additionally construct it directly to check gap-zero.
pub(crate) struct NativeForwardOracle;

impl ForwardOracle for NativeForwardOracle {
    fn name(&self) -> &'static str {
        "native"
    }

    fn materialize(
        &self,
        facts: &crate::facts::TypedFactSet,
        rules: &str,
        budget: &ForwardBudget,
    ) -> Result<TypedChaseResult, String> {
        // Forward-budget governance is a router/native-governor concern layered
        // ABOVE the oracle boundary (the 5-field incomplete-never-wrong result is
        // minted there): this seam stays UNBOUNDED, exactly like Nemo's.  A
        // non-default budget is a hard error, never a silently-unbudgeted chase.
        if budget.is_bounded() {
            return Err(format!(
                "NativeForwardOracle cannot honor a forward budget inline ({budget:?}); \
                 forward-budget governance is a router/native-governor concern above \
                 the oracle boundary"
            ));
        }

        // ── Dispatch: binary (named-ternary EL/DL) vs generic (n-ary RL/RDF, or
        //    an arity-3 program that carries a non-ternary HELPER relation) ──
        //
        // The binary `seminaive` core keys every relation by a CONSTANT predicate
        // NAME and models exactly the ternary `<predicate>(subject, object, world)`
        // shape: `crate::rule_ir::lower_nemo_atom` reads only `terms[0]`/`terms[1]`
        // and DROPS the rest, so it is faithful ONLY when EVERY atom it sees is
        // arity-3.  Feed it an arity-4 `triple(?s,?p,?o,?w)` atom or an arity-2
        // `helper(?x,?y)` atom and it does not error — it silently mis-slots the
        // terms and produces a WRONG closure.  So the binary path is admissible
        // ONLY for a program whose EDB *and* whose whole rule set are purely
        // arity-3 named-ternary.  Everything else routes to the arity-generic
        // evaluator `crate::physical::generic`, which keeps EVERY term (predicate
        // position included) and is a strict generalization — it evaluates the
        // arity-3 named-ternary relations too (a constant predicate name is just
        // another relation), so the mixed case (arity-3 program rules + a
        // non-ternary helper, the shape `crate::materialize` accepts from a user
        // program) runs correctly on it.
        //
        // The signal is the ARITY of the encoding, read from BOTH surfaces:
        //
        // * EL/DL: every quad → ternary `<predicate>(subject, object, world)` (EDB
        //   arity 3) and every rule atom is arity-3 named-ternary → BINARY core.
        // * OWL 2 RL/RDF (`crate::reason::rl`): every quad → the 4-ary
        //   `triple(?s, ?p, ?o, ?w)` relation (predicate as a DATA term) and the
        //   meta-rules carry arity-4 / arity-3 generic atoms → GENERIC evaluator.
        // * A user materialize program (`crate::materialize`): arity-3 EDB but a
        //   rule set that declares a HELPER predicate of another arity → GENERIC
        //   evaluator (the binary core cannot represent the helper atom).
        //
        // A program is binary-eligible IFF its EDB is all-ternary AND its rule set
        // is all-ternary (`rules_are_pure_ternary`).  A non-ternary rule set that
        // ALSO carries negation is genuinely un-runnable by either native path
        // (binary needs ternary, generic is positive-only): it routes to generic,
        // where `parse_generic_rules` HARD-FAILS on the negated atom — never a
        // guess, never a silent approximation.  An empty EDB with an all-ternary
        // rule set stays binary (the closure is vacuously empty either way).
        let edb_all_ternary = facts.facts().all(|f| f.args.len() == 3);
        let binary_eligible = edb_all_ternary && rules_are_pure_ternary(rules)?;
        if !binary_eligible {
            // Generic (n-ary) path: parse the rule text KEEPING all terms and run
            // the arity-generic positive-Datalog least-fixpoint.  The binary core
            // below is left UNTOUCHED — EL/DL never reach this branch.
            let generic_rules = crate::physical::parse_generic_rules(rules)?;
            return crate::physical::materialize_generic(facts, &generic_rules);
        }

        // Reconstruct the world-indexed store the native chase materializes over
        // from the ternary typed EDB.  Every reasoning fact is
        // `predicate(subject, object, world)`; a non-ternary fact is a rule-text /
        // EDB-construction bug (the fixed reasoning rule texts declare only ternary
        // relations), so it is a hard error — mirroring `run_reasoning`'s ternary
        // contract, not a silent skip.
        let interner = facts.interner();
        let store = crate::store::WorldStore::new();
        for fact in facts.facts() {
            if fact.args.len() != 3 {
                return Err(format!(
                    "NativeForwardOracle EDB fact for predicate {:?} has arity {} \
                     (expected 3): the ternary reasoning encoding is \
                     predicate(subject, object, world)",
                    fact.predicate,
                    fact.args.len()
                ));
            }
            let subject = interner.resolve(fact.args[0]).clone();
            let object = interner.resolve(fact.args[1]).clone();
            let world = world_lexical(interner.resolve(fact.args[2]))?;
            store
                .insert_quad_terms(&world, subject, TermValue::iri(&fact.predicate), object)
                .map_err(|e| format!("NativeForwardOracle store seed failed: {e}"))?;
        }

        // Parse the rule text into the Horn/stratified IR and run the native
        // least-fixpoint closure UNBOUNDED (`None` max_steps — the governor never
        // cuts, so the full least model is produced).
        let eval_rules = crate::rule_ir::parse_eval_rules(rules)?;
        let rows = match crate::physical::materialize_native(&store, &eval_rules, None)? {
            crate::physical::NativeOutcome::Decided(budgeted) => budgeted.rows,
            // A non-stratifiable program is a DECLARED gap the native engine does
            // not decide — never approximate it into a fabricated closure.
            crate::physical::NativeOutcome::Unsupported(kind) => {
                return Err(format!(
                    "NativeForwardOracle: the native chase does not decide this rule set \
                     (Unsupported({kind:?})); it must not approximate an undecided fragment"
                ));
            }
        };

        // Re-expose each native `DerivedRow` as a ternary `TypedRow`
        // `predicate(subject, object, world)` — the shape `run_reasoning`'s decoder
        // and `typed_row_fact_key` both coerce back to a `(subject, predicate,
        // object)` fact key.  `is_edb` keys off the assert sentinel; `rule_name`
        // is the firing rule IRI for a derived fact, `None` for an echoed EDB fact.
        //
        // The antecedents ARE populated below from `DerivedRow.antecedents` (the
        // matched body facts): `crate::materialize::materialize` mints reifiers from
        // `TypedProvenance::antecedents`, and `chase_rows_to_inferred` decodes them
        // into `InferredAxiom.premises`, so the primary reasoning path now carries
        // real native provenance end-to-end.
        let typed = rows
            .into_iter()
            .map(|row| {
                let is_edb = row.rule_iri == crate::provenance::ASSERT_RULE_IRI;
                let rule_name = if is_edb { None } else { Some(row.rule_iri) };
                // Re-expose each matched body fact as a ternary antecedent
                // `predicate(subject, object, world)` — the exact shape
                // `chase_rows_to_inferred`'s `decode_premise` and
                // `materialize`'s `reifier_for_antecedent_row` consume.  Every
                // antecedent of a within-world derivation shares the derived
                // row's world (`row.graph`), so the world column is that graph
                // literal (the same `simple_literal(&row.graph)` the derived row
                // itself carries).  An EDB echo has no antecedents (empty), so the
                // premise list is correctly empty for asserted rows.
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
                        attributions: Vec::new(),
                    },
                )
            })
            .collect();

        Ok(TypedChaseResult { rows: typed })
    }

    fn provides_provenance(&self) -> bool {
        true
    }
}

/// Whether EVERY atom of EVERY rule in `rules` is arity-3 — the binary
/// [`crate::physical::seminaive`] core's admissibility test.
///
/// The binary core keys a relation by its constant predicate name and models the
/// ternary `predicate(subject, object, world)` shape only; `lower_nemo_atom`
/// silently drops all terms past `terms[1]`, so an atom of any other arity (a
/// 4-ary `triple(?s,?p,?o,?w)`, a 2-ary `helper(?x,?y)`) would be mis-slotted into
/// a WRONG closure rather than rejected.  This inspection lets the dispatch route
/// such a program to the arity-generic evaluator BEFORE that silent corruption.
///
/// It uses the SAME translation-only Nemo front-end (`parse_unvalidated`) the two
/// rule lowerings use, so the atom surface it counts is byte-identical to what the
/// engines see.  It inspects the head plus every positive AND negated body atom;
/// unlike [`crate::physical::parse_generic_rules`] it does NOT reject a negated
/// atom — a non-ternary negated rule set is left for the generic path to hard-fail
/// on (positive-only), and a ternary negated rule set is a legal binary program
/// (stratified NAF).  A pure syntax error propagates.
///
/// Called on the production dispatch path by [`NativeForwardOracle::materialize`]
/// (now that [`forward_oracle`] returns the native adapter).
pub(crate) fn rules_are_pure_ternary(rules: &str) -> Result<bool, String> {
    use crate::nemo_engine::NemoParsedRules;
    use nemo::rule_model::programs::ProgramRead;

    let program = NemoParsedRules::parse_unvalidated(rules)?.into_program();
    for rule in program.rules() {
        let atom_is_ternary =
            |atom: &nemo::rule_model::components::atom::Atom| atom.terms().count() == 3;
        let head_ok = rule.head().iter().all(atom_is_ternary);
        let body_ok = rule
            .body_positive()
            .chain(rule.body_negative())
            .all(atom_is_ternary);
        if !head_ok || !body_ok {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The raw world string of a ternary EDB fact's world term.
///
/// The world position is always a plain `xsd:string` literal (the Nemo
/// string-constant treatment `TypedFactSet::push_quad` applies); any other shape
/// is a hard error — mirroring `crate::reason::world_string`.
///
/// Called on the production dispatch path by [`NativeForwardOracle::materialize`]
/// (now that [`forward_oracle`] returns the native adapter).
fn world_lexical(term: &TermValue) -> Result<String, String> {
    match term {
        TermValue::Literal {
            lexical_form,
            datatype,
            language: None,
            ..
        } if datatype == "http://www.w3.org/2001/XMLSchema#string" => Ok(lexical_form.clone()),
        other => Err(format!(
            "NativeForwardOracle EDB world term must be a plain string literal, got {other:?}"
        )),
    }
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
        // Nemo is off the primary path now; reach the retained bootstrap oracle
        // through its single naming site.
        let oracle = nemo_forward_oracle();
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
        let oracle = nemo_forward_oracle();
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

    // ── Dispatch predicate: `rules_are_pure_ternary` (Task 5) ──────────────────
    //
    // These exercise the rule-arity signal that decides binary vs generic WITHOUT
    // driving any chase engine (pure Nemo-parse), so they carry no engine-group
    // token: the fixed EL/DL calculi are pure arity-3 (→ binary), while a rule set
    // that carries a NON-ternary helper atom or the arity-4 `triple` relation is
    // not (→ generic).

    /// The fixed EL and DL calculi are pure arity-3 named-ternary, so they are
    /// binary-eligible — the promotion of EL/DL onto the binary core (Tasks 2/4)
    /// depends on this staying true.
    #[test]
    fn rules_are_pure_ternary_accepts_the_fixed_el_and_dl_calculi() {
        assert!(
            rules_are_pure_ternary(crate::reason::el::EL_RULES)
                .expect("EL rules parse for arity inspection"),
            "EL_RULES must be pure arity-3 (binary-eligible)"
        );
        assert!(
            rules_are_pure_ternary(&crate::reason::dl::dl_rules())
                .expect("dl_rules() parse for arity inspection"),
            "dl_rules() must be pure arity-3 (binary-eligible)"
        );
    }

    /// A user materialize program whose rule set declares a BINARY helper predicate
    /// is NOT binary-eligible: the binary core would silently mis-slot the helper's
    /// two terms, so the dispatch must route the whole program to the generic
    /// evaluator. The fixed DL calculus combined with such a helper stays
    /// non-eligible (one non-ternary atom taints the set).
    #[test]
    fn rules_are_pure_ternary_rejects_a_non_ternary_helper() {
        let helper = "helperEdge(?x, ?y) :- \
             <http://www.w3.org/2000/01/rdf-schema#subClassOf>(?x, ?y, ?w) .\n";
        assert!(
            !rules_are_pure_ternary(helper).expect("helper rule parses"),
            "a binary helperEdge head is arity-2 → NOT binary-eligible"
        );
        let combined = format!("{}\n{helper}", crate::reason::dl::dl_rules());
        assert!(
            !rules_are_pure_ternary(&combined).expect("combined rules parse"),
            "dl_rules() + a binary helper is NOT binary-eligible (one non-ternary atom taints it)"
        );
    }

    /// The OWL 2 RL/RDF meta-rules use the arity-4 `triple(?s,?p,?o,?w)` relation
    /// (predicate-as-data), which is not arity-3, so they are NOT binary-eligible —
    /// the generic evaluator is the only faithful native path for them (Task 3).
    #[test]
    fn rules_are_pure_ternary_rejects_the_arity_four_rl_triple_relation() {
        assert!(
            !rules_are_pure_ternary(crate::reason::rl::RL_RULES)
                .expect("RL rules parse for arity inspection"),
            "RL_RULES carry the arity-4 `triple` relation → NOT binary-eligible"
        );
    }

    // ── Antecedent threading on BOTH native dispatch branches (gap G3) ─────────
    //
    // The production `NativeForwardOracle` reports `provides_provenance() == true`;
    // the escaped bug had it emit EMPTY `antecedents` on derived rows (masked
    // because the native↔Nemo parity ledger compares only fact keys, never
    // provenance). The fix threads matched body facts as antecedents through BOTH
    // dispatch branches. These two guards drive `forward_oracle()` end-to-end
    // through each branch — proving WHICH branch via the `rules_are_pure_ternary`
    // dispatch signal — and assert a DERIVED row (`is_edb == false`) carries a
    // NON-EMPTY `antecedents` list. Falsifiable: revert either branch's
    // antecedent threading to `Vec::new()` and the matching guard goes red.

    /// BINARY seminaive branch: the fixed DL calculus is pure arity-3, so the
    /// production oracle routes A⊑B, B⊑C through the binary `seminaive` core and
    /// derives the transitive A⊑C. The derived edge must cite its two body facts —
    /// i.e. carry NON-EMPTY `antecedents` — which the empty-antecedents bug broke.
    #[test]
    fn native_oracle_threads_antecedents_on_binary_seminaive_path() {
        let oracle = forward_oracle();
        assert_eq!(oracle.name(), "native");

        let subclass = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
        let world = "http://gmeow.example/w";
        let a = TermValue::Iri("http://gmeow.example/A".into());
        let b = TermValue::Iri("http://gmeow.example/B".into());
        let c = TermValue::Iri("http://gmeow.example/C".into());
        let mut edb = TypedFactSet::new();
        edb.push_quad(&a, subclass, &b, world);
        edb.push_quad(&b, subclass, &c, world);

        let rules = crate::reason::dl::dl_rules();
        // Prove dispatch took the BINARY branch: EDB + rule set are all arity-3.
        assert!(
            rules_are_pure_ternary(&rules).expect("dl_rules parse for arity inspection"),
            "dl_rules() must be pure arity-3 so this drives the binary seminaive branch"
        );

        let result = oracle
            .materialize(&edb, &rules, &ForwardBudget::UNBOUNDED)
            .expect("unbudgeted native chase must succeed");

        // The transitive subClassOf(A, C) is DERIVED (not an EDB echo); assert it
        // cites its matched body facts through the `TypedProvenance::antecedents`
        // field — the exact provenance the escaped bug left empty.
        let derived_transitive = result.rows.iter().find(|(row, prov)| {
            !prov.is_edb
                && row.predicate == subclass
                && row.args.len() == 3
                && row.args[0] == a
                && row.args[1] == c
        });
        let (_, prov) = derived_transitive.unwrap_or_else(|| {
            panic!(
                "transitive subClassOf(A, C) must be derived on the binary path; got {:?}",
                result.rows
            )
        });
        assert!(
            !prov.antecedents.is_empty(),
            "derived subClassOf(A, C) must cite NON-EMPTY antecedents (the empty-antecedents \
             bug fails here); got {prov:?}"
        );
    }

    /// GENERIC n-ary branch: the OWL 2 RL/RDF rules carry the arity-4
    /// `triple(?s,?p,?o,?w)` relation, so they are NOT pure-ternary and the
    /// production oracle routes to the arity-generic evaluator. A 4-ary EDB
    /// (A⊑B, B⊑C) makes `rl:scm-sco` derive A⊑C; the derived row must carry
    /// NON-EMPTY `antecedents` on this branch too.
    #[test]
    fn native_oracle_threads_antecedents_on_generic_nary_path() {
        let oracle = forward_oracle();
        assert_eq!(oracle.name(), "native");

        let subclass = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
        let world = "http://gmeow.example/w";

        // Build the arity-4 `triple(?s, ?p, ?o, ?w)` EDB directly (predicate as a
        // DATA term), the shape the generic evaluator consumes.
        let mut edb = TypedFactSet::new();
        let a = edb.intern(&TermValue::iri("http://gmeow.example/A"));
        let b = edb.intern(&TermValue::iri("http://gmeow.example/B"));
        let c = edb.intern(&TermValue::iri("http://gmeow.example/C"));
        let sc = edb.intern(&TermValue::iri(subclass));
        let w = edb.intern(&TermValue::simple_literal(world));
        edb.push_fact("triple", vec![a, sc, b, w]);
        edb.push_fact("triple", vec![b, sc, c, w]);

        // Prove dispatch took the GENERIC branch: the rule set is NOT pure arity-3.
        assert!(
            !rules_are_pure_ternary(crate::reason::rl::RL_RULES)
                .expect("RL rules parse for arity inspection"),
            "RL_RULES carry the arity-4 `triple` relation, so this drives the generic branch"
        );

        let result = oracle
            .materialize(&edb, crate::reason::rl::RL_RULES, &ForwardBudget::UNBOUNDED)
            .expect("unbudgeted native chase must succeed");

        // At least one DERIVED row (e.g. the transitive A⊑C from `rl:scm-sco`) must
        // cite its matched body facts through `TypedProvenance::antecedents`.
        let derived_with_antecedents = result
            .rows
            .iter()
            .any(|(_row, prov)| !prov.is_edb && !prov.antecedents.is_empty());
        assert!(
            derived_with_antecedents,
            "at least one derived row on the generic path must cite NON-EMPTY antecedents \
             (the empty-antecedents bug fails here); got {:?}",
            result.rows
        );
    }
}
