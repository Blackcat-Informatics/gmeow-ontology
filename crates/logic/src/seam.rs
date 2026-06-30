// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Nemo–Prolog seam data contract.
//!
//! # Asymmetric blackboard
//!
//! The two reasoning engines — **Nemo** (forward, existential-rule materialization) and **Scryer
//! Prolog** (backward, SLD resolution) — **never call each other directly**. They communicate
//! exclusively through the oxigraph quad store (see [`crate::store::WorldStore`]): Nemo writes
//! derived quads into named graphs; Scryer reads them through the three fixed foreign predicates
//! declared in [`ScryerForeign`]. The named-graph IRI *is* the world; everything on this module
//! is scoped to that abstraction.
//!
//! # Materialize-back policy
//!
//! Prolog-derived answers are **not** written back into oxigraph by default. Phase 2
//! (Scryer resolution) is a read-only query layer; its derivations are *virtual* —
//! cited in explanations as virtual derivation steps keyed by `derivation_id`, never as stored
//! quads.
//!
//! **Two explicit exceptions are deferred to issue #501** and are intentionally absent from this
//! module:
//! 1. Stratum-C constructed worlds *are* materialized (into a transient named graph).
//! 2. A query may opt into IDB memoization, writing a clearly-marked derived graph.
//!
//! In all cases the invariant holds: **no Prolog answer is silently promoted to an asserted base
//! fact**, and an explanation must be able to cite every step, virtual or materialized.

use gmeow_rdf::TermValue;

// ── Newtype wrappers ────────────────────────────────────────────────────────────────────────────

/// A stable, opaque identifier for a single derivation step.
///
/// Stored as an IRI string. Derivation IDs are assigned by the Nemo layer during
/// materialization and carried through as provenance anchors for explanation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DerivationId(pub String);

impl DerivationId {
    /// Return the IRI string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DerivationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ── BudgetStatus ───────────────────────────────────────────────────────────────────────────────

/// Execution-budget status for a derivation step.
///
/// Serializes to the canonical lowercase strings required by the conformance corpus:
/// `ok`, `partial`, or `exhausted`.
///
/// - `Ok`        — derivation completed within budget.
/// - `Partial`   — derivation was cut short; result may be incomplete.
/// - `Exhausted` — budget was fully consumed; result may be unsound or incomplete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BudgetStatus {
    /// Derivation completed within all declared budget limits.
    Ok,
    /// Derivation was interrupted before fixpoint; result is a partial closure.
    Partial,
    /// All budget was consumed; derivation did not reach fixpoint.
    Exhausted,
}

impl BudgetStatus {
    /// Return the canonical lowercase string for this status.
    ///
    /// These strings are the normative serialization used in the conformance corpus
    /// and in any JSON/text projection of [`DerivedQuad`].
    pub fn as_str(self) -> &'static str {
        match self {
            BudgetStatus::Ok => "ok",
            BudgetStatus::Partial => "partial",
            BudgetStatus::Exhausted => "exhausted",
        }
    }
}

impl std::fmt::Display for BudgetStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── DerivedQuad ────────────────────────────────────────────────────────────────────────────────

/// A derived quad as it crosses the Nemo → oxigraph blackboard boundary.
///
/// Every quad materialized by Nemo into a world's named graph carries its full derivation
/// metadata so that Scryer (and the explanation surface) can trace provenance without
/// consulting any secondary index.
///
/// Field names and semantics match the design contract in
/// `slices/core/logic/design/LOGIC-RUNTIME.md §"The seam data contract"` verbatim:
///
/// ```text
/// Nemo output (per derived quad written to oxigraph):
///   graph:          IRI            # the world the quad belongs to
///   quad:           (S, P, O, G)   # the quad itself (G == graph)
///   derivation_id:  IRI            # stable id for this derivation step
///   rule_iri:       IRI            # the rule that fired
///   source_quad_ids: [IRI]         # the antecedent quads consumed
///   profile:        IRI            # the semantic/decidability profile in force
///   budget_status:  enum           # ok | partial | exhausted
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct DerivedQuad {
    /// The world (named-graph) IRI this quad belongs to. Identical to the `G` component.
    pub graph: String,

    /// Subject of the derived triple.
    ///
    /// RDF 1.2 subjects may be IRIs, blank nodes, or triple terms; gmeow-logic carries
    /// them as a native [`TermValue`] for forward-compatibility with RDF 1.2 triple terms
    /// across the whole quad.
    pub subject: TermValue,

    /// Predicate of the derived triple. Always an IRI string.
    pub predicate: String,

    /// Object of the derived triple. May be an IRI, blank node, literal, or (in RDF 1.2)
    /// a triple term.
    pub object: TermValue,

    /// Graph component of the quad. Must equal [`Self::graph`]; carried separately so the
    /// quad is self-contained when projected to a `(S, P, O, G)` tuple.
    pub graph_component: String,

    /// Stable IRI identifying this derivation step. Used as a provenance anchor in
    /// explanations and virtual derivation traces.
    pub derivation_id: DerivationId,

    /// IRI of the rule that fired to produce this quad.
    pub rule_iri: String,

    /// IRIs of the antecedent quads (reifier IRIs in the statement layer) consumed by the
    /// rule that fired.
    pub source_quad_ids: Vec<String>,

    /// IRI of the semantic / decidability profile that was in force when this quad was
    /// derived (e.g. `logic:MonotonicDatalogProfile`).
    pub profile: String,

    /// Budget status at the point this quad was derived.
    pub budget_status: BudgetStatus,
}

// ── ScryerForeign trait ────────────────────────────────────────────────────────────────────────

/// Typed signatures for the three Scryer foreign predicates that read the blackboard.
///
/// These are **contract stubs only** — no Prolog embedding is present in this task.
/// Embedding Scryer and wiring these to a live Prolog session is deferred to a later rung
/// (see issue #499 task notes and LOGIC-RUNTIME.md §"Backward chaining").
///
/// Each method maps directly to its Prolog mode annotation:
///
/// ```prolog
/// in_world(+W, ?S, ?P, ?O)
/// derived_by(?QuadId, ?Rule, ?Sources)
/// contradiction_witness(+W, ?WitnessGraph)
/// ```
pub trait ScryerForeign {
    /// `in_world(+W, ?S, ?P, ?O)` — world-indexed quad lookup.
    ///
    /// Mode: `W` is ground (the world IRI is always known at call time); `S`, `P`, `O` may be
    /// unbound variables that unify against the store, or ground terms that act as filters.
    ///
    /// Backed by an oxigraph named-graph pattern query. The non-recursive, non-unification-heavy
    /// case is served directly by SPARQL (fast path); recursive or unification-heavy calls go to
    /// Scryer's own resolution loop.
    ///
    /// Returns the set of quads (as [`DerivedQuad`] references) in world `world` that unify
    /// with the (possibly partial) `(subject, predicate, object)` pattern.
    fn in_world<'a>(
        &'a self,
        world: &str,
        subject: Option<&TermValue>,
        predicate: Option<&str>,
        object: Option<&TermValue>,
    ) -> Box<dyn Iterator<Item = &'a DerivedQuad> + 'a>;

    /// `derived_by(?QuadId, ?Rule, ?Sources)` — provenance leg for explanations.
    ///
    /// Mode: any argument may be unbound; all are output if unbound. When `quad_id` is ground
    /// this is a direct provenance lookup; when unbound it enumerates all derivations.
    ///
    /// Returns an iterator of `(derivation_id, rule_iri, source_quad_ids)` triples for
    /// derivations that match the (possibly partial) pattern.
    fn derived_by<'a>(
        &'a self,
        quad_id: Option<&DerivationId>,
        rule: Option<&str>,
        sources: Option<&[String]>,
    ) -> Box<dyn Iterator<Item = (&'a DerivationId, &'a str, &'a [String])> + 'a>;

    /// `contradiction_witness(+W, ?WitnessGraph)` — within-world inconsistency, as a statement
    /// graph.
    ///
    /// Mode: `W` is ground (the world to inspect is always specified); `WitnessGraph` is an
    /// output — the IRI of a GMEOW statement graph representing the minimal conflict set
    /// (paraconsistent witness) if one exists, or nothing if the world is consistent.
    ///
    /// Contradictions are never bare failures; a witness graph is always emitted (see
    /// LOGIC-RUNTIME.md §"Contradiction witnesses").
    fn contradiction_witness<'a>(&'a self, world: &str) -> Box<dyn Iterator<Item = String> + 'a>;
}

// ── WorldStoreForeign ──────────────────────────────────────────────────────────────────────────

/// A concrete [`ScryerForeign`] implementer that owns a snapshot of asserted base facts
/// drawn from a [`crate::store::WorldStore`] world.
///
/// `WorldStoreForeign` is populated by [`WorldStoreForeign::from_world`], which takes a
/// synchronous snapshot of all quads in a named-graph world and wraps each as a
/// [`DerivedQuad`] carrying the `logic:assert` rule IRI and a content-addressed
/// [`DerivationId`].  The snapshot is immutable after construction.
///
/// This is the "asserted facts as DB" fast path for Scryer's `in_world/4` predicate:
/// no Nemo chase is needed when the query is over base EDB facts only.
pub struct WorldStoreForeign {
    quads: Vec<DerivedQuad>,
}

impl WorldStoreForeign {
    /// Build a `WorldStoreForeign` by snapshotting all quads in `world` from `store`.
    ///
    /// Each oxigraph quad is converted to a [`DerivedQuad`] representing an asserted
    /// base fact:
    /// - `rule_iri` = [`crate::provenance::ASSERT_RULE_IRI`]
    /// - `reifier` = `mint_reifier(subject, predicate, object)`
    /// - `derivation_id` = `mint_derivation_id(ASSERT_RULE_IRI, [reifier])`
    /// - `source_quad_ids` = `[reifier]`
    /// - `budget_status` = [`BudgetStatus::Ok`]
    ///
    /// Quads whose predicate is not a `NamedNode` (i.e. blank-node predicates — which
    /// RDF 1.2 does not permit) are skipped.  Quads where `mint_reifier` fails (e.g.
    /// RDF-star triple-term subjects or objects) are skipped with the error propagated
    /// as a warning string in the `Err` variant — callers should treat this as a
    /// programming error, not a recoverable condition.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` if `store.quads_for_pattern_in_world` itself panics
    /// (it does not in practice), or if reifier minting fails for any quad.
    pub fn from_world(
        store: &crate::store::WorldStore,
        world: &str,
        profile: &str,
    ) -> Result<Self, String> {
        let raw_quads = store.quads_for_pattern_in_world(world, None, None, None);

        let mut derived: Vec<DerivedQuad> = Vec::with_capacity(raw_quads.len());

        for quad in raw_quads {
            // quad.p is always an IRI (RDF invariant); a non-IRI predicate is skipped.
            let Some(predicate) = quad.p.as_iri().map(str::to_owned) else {
                continue;
            };
            let subject = quad.s.clone();
            let object = quad.o.clone();

            let reifier = crate::provenance::mint_reifier(&subject, &predicate, &object)
                .map_err(|e| format!("WorldStoreForeign: mint_reifier failed: {e}"))?;

            let derivation_id = DerivationId(crate::provenance::mint_derivation_id(
                crate::provenance::ASSERT_RULE_IRI,
                &[reifier.as_str()],
            ));

            derived.push(DerivedQuad {
                graph: world.to_owned(),
                subject,
                predicate,
                object,
                graph_component: world.to_owned(),
                derivation_id,
                rule_iri: crate::provenance::ASSERT_RULE_IRI.to_owned(),
                source_quad_ids: vec![reifier],
                profile: profile.to_owned(),
                budget_status: BudgetStatus::Ok,
            });
        }

        Ok(Self { quads: derived })
    }
}

impl ScryerForeign for WorldStoreForeign {
    /// `in_world(+W, ?S, ?P, ?O)` — return quads in `world` matching the optional pattern.
    ///
    /// Filters `self.quads` by world equality and each provided optional term filter.
    fn in_world<'a>(
        &'a self,
        world: &str,
        subject: Option<&TermValue>,
        predicate: Option<&str>,
        object: Option<&TermValue>,
    ) -> Box<dyn Iterator<Item = &'a DerivedQuad> + 'a> {
        let world = world.to_owned();
        let subject = subject.cloned();
        let predicate = predicate.map(str::to_owned);
        let object = object.cloned();

        Box::new(self.quads.iter().filter(move |dq| {
            if dq.graph != world {
                return false;
            }
            if let Some(ref s) = subject {
                if &dq.subject != s {
                    return false;
                }
            }
            if let Some(ref p) = predicate {
                if &dq.predicate != p {
                    return false;
                }
            }
            if let Some(ref o) = object {
                if &dq.object != o {
                    return false;
                }
            }
            true
        }))
    }

    /// `derived_by(?QuadId, ?Rule, ?Sources)` — provenance enumeration.
    ///
    /// Enumerates `self.quads` as `(derivation_id, rule_iri, source_quad_ids)` triples.
    /// Filters by `quad_id` (when `Some`, match `derivation_id`) and `rule` (when `Some`,
    /// match `rule_iri`).
    ///
    /// **`sources` is ignored as an input filter.** In this monotonic-fragment
    /// implementation, `sources` is OUTPUT-ONLY (provenance enumeration, not input
    /// filtering).  Callers that need to filter by source must do so on the returned
    /// iterator.
    fn derived_by<'a>(
        &'a self,
        quad_id: Option<&DerivationId>,
        rule: Option<&str>,
        _sources: Option<&[String]>,
    ) -> Box<dyn Iterator<Item = (&'a DerivationId, &'a str, &'a [String])> + 'a> {
        let quad_id = quad_id.cloned();
        let rule = rule.map(|r| r.to_owned());

        Box::new(self.quads.iter().filter_map(move |dq| {
            if let Some(ref qid) = quad_id {
                if &dq.derivation_id != qid {
                    return None;
                }
            }
            if let Some(ref r) = rule {
                if dq.rule_iri.as_str() != r.as_str() {
                    return None;
                }
            }
            Some((
                &dq.derivation_id,
                dq.rule_iri.as_str(),
                dq.source_quad_ids.as_slice(),
            ))
        }))
    }

    /// `contradiction_witness(+W, ?WitnessGraph)` — always empty in this implementation.
    ///
    /// Monotonic-vacuous in v4: the monotonic fragment has no within-world contradictions;
    /// real paraconsistent witnesses arrive with #503. This empty result is vacuously-correct,
    /// NOT a silent stub.
    fn contradiction_witness<'a>(&'a self, _world: &str) -> Box<dyn Iterator<Item = String> + 'a> {
        Box::new(std::iter::empty())
    }
}

// ── Unit tests ─────────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::WorldStore;

    // ── BudgetStatus canonical spellings ──────────────────────────────────────────────────────

    #[test]
    fn budget_status_ok_spells_ok() {
        assert_eq!(BudgetStatus::Ok.as_str(), "ok");
        assert_eq!(BudgetStatus::Ok.to_string(), "ok");
    }

    #[test]
    fn budget_status_partial_spells_partial() {
        assert_eq!(BudgetStatus::Partial.as_str(), "partial");
        assert_eq!(BudgetStatus::Partial.to_string(), "partial");
    }

    #[test]
    fn budget_status_exhausted_spells_exhausted() {
        assert_eq!(BudgetStatus::Exhausted.as_str(), "exhausted");
        assert_eq!(BudgetStatus::Exhausted.to_string(), "exhausted");
    }

    // ── DerivedQuad construction and field access ─────────────────────────────────────────────

    fn make_derived_quad() -> DerivedQuad {
        let world = "http://logic.gmeow.example/world/alpha".to_owned();
        DerivedQuad {
            graph: world.clone(),
            subject: TermValue::iri("http://example.org/subject/1"),
            predicate: "http://example.org/predicate/type".to_owned(),
            object: TermValue::iri("http://example.org/object/Thing"),
            graph_component: world.clone(),
            derivation_id: DerivationId("http://logic.gmeow.example/derivation/d001".to_owned()),
            rule_iri: "http://logic.gmeow.example/rule/r001".to_owned(),
            source_quad_ids: vec![
                "http://logic.gmeow.example/quad/q001".to_owned(),
                "http://logic.gmeow.example/quad/q002".to_owned(),
            ],
            profile: "http://logic.gmeow.example/profile/MonotonicDatalog".to_owned(),
            budget_status: BudgetStatus::Ok,
        }
    }

    #[test]
    fn derived_quad_graph_field_accessible() {
        let dq = make_derived_quad();
        assert_eq!(dq.graph, "http://logic.gmeow.example/world/alpha");
    }

    #[test]
    fn derived_quad_graph_equals_graph_component() {
        let dq = make_derived_quad();
        assert_eq!(
            dq.graph, dq.graph_component,
            "graph and graph_component must be equal"
        );
    }

    #[test]
    fn derived_quad_derivation_id_round_trips() {
        let dq = make_derived_quad();
        assert_eq!(
            dq.derivation_id.as_str(),
            "http://logic.gmeow.example/derivation/d001"
        );
        assert_eq!(
            dq.derivation_id.to_string(),
            "http://logic.gmeow.example/derivation/d001"
        );
    }

    #[test]
    fn derived_quad_rule_iri_round_trips() {
        let dq = make_derived_quad();
        assert_eq!(dq.rule_iri, "http://logic.gmeow.example/rule/r001");
    }

    #[test]
    fn derived_quad_source_quad_ids_populated() {
        let dq = make_derived_quad();
        assert_eq!(dq.source_quad_ids.len(), 2);
        assert_eq!(
            dq.source_quad_ids[0],
            "http://logic.gmeow.example/quad/q001"
        );
        assert_eq!(
            dq.source_quad_ids[1],
            "http://logic.gmeow.example/quad/q002"
        );
    }

    #[test]
    fn derived_quad_profile_round_trips() {
        let dq = make_derived_quad();
        assert_eq!(
            dq.profile,
            "http://logic.gmeow.example/profile/MonotonicDatalog"
        );
    }

    #[test]
    fn derived_quad_budget_status_ok() {
        let dq = make_derived_quad();
        assert_eq!(dq.budget_status, BudgetStatus::Ok);
        assert_eq!(dq.budget_status.as_str(), "ok");
    }

    #[test]
    fn derived_quad_clone_is_equal() {
        let dq = make_derived_quad();
        let cloned = dq.clone();
        assert_eq!(dq, cloned);
    }

    // ── DerivationId display ──────────────────────────────────────────────────────────────────

    #[test]
    fn derivation_id_display_matches_as_str() {
        let id = DerivationId("http://example.org/d/42".to_owned());
        assert_eq!(id.as_str(), id.to_string().as_str());
    }

    // ── WorldStoreForeign ─────────────────────────────────────────────────────────────────────

    const TEST_WORLD: &str = "http://world/TestForeign";
    const TEST_PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
    const S1: &str = "http://example.org/s1";
    const P1: &str = "http://example.org/p1";
    const O1: &str = "http://example.org/o1";
    const S2: &str = "http://example.org/s2";
    const P2: &str = "http://example.org/p2";
    const O2: &str = "http://example.org/o2";

    fn small_store() -> WorldStore {
        let store = WorldStore::new();
        store.insert_quad(TEST_WORLD, S1, P1, O1);
        store.insert_quad(TEST_WORLD, S2, P2, O2);
        store
    }

    fn small_foreign() -> WorldStoreForeign {
        let store = small_store();
        WorldStoreForeign::from_world(&store, TEST_WORLD, TEST_PROFILE)
            .expect("from_world on a valid store must succeed")
    }

    #[test]
    fn foreign_in_world_all_none_returns_all_asserted_quads() {
        let foreign = small_foreign();
        let quads: Vec<_> = foreign.in_world(TEST_WORLD, None, None, None).collect();
        assert_eq!(quads.len(), 2, "should return both asserted quads");
    }

    #[test]
    fn foreign_in_world_predicate_filter() {
        let foreign = small_foreign();
        let quads: Vec<_> = foreign.in_world(TEST_WORLD, None, Some(P1), None).collect();
        assert_eq!(quads.len(), 1, "P1 filter should return exactly 1 quad");
        assert_eq!(quads[0].predicate, P1);
    }

    #[test]
    fn foreign_in_world_subject_filter() {
        let foreign = small_foreign();
        let subj_term = TermValue::iri(S2);
        let quads: Vec<_> = foreign
            .in_world(TEST_WORLD, Some(&subj_term), None, None)
            .collect();
        assert_eq!(quads.len(), 1, "S2 filter should return exactly 1 quad");
        assert_eq!(quads[0].subject, subj_term);
    }

    #[test]
    fn foreign_in_world_wrong_world_returns_empty() {
        let foreign = small_foreign();
        let quads: Vec<_> = foreign
            .in_world("http://world/Other", None, None, None)
            .collect();
        assert!(quads.is_empty(), "wrong world must return no quads");
    }

    #[test]
    fn foreign_derived_by_enumerates_with_assert_rule() {
        let foreign = small_foreign();
        let triples: Vec<_> = foreign.derived_by(None, None, None).collect();
        assert_eq!(triples.len(), 2, "should enumerate 2 asserted derivations");
        for (_, rule, _) in &triples {
            assert_eq!(
                *rule,
                crate::provenance::ASSERT_RULE_IRI,
                "rule_iri must be ASSERT_RULE_IRI for asserted facts"
            );
        }
    }

    #[test]
    fn foreign_derived_by_rule_filter() {
        let foreign = small_foreign();
        // Filter by ASSERT_RULE_IRI — should return both.
        let triples: Vec<_> = foreign
            .derived_by(None, Some(crate::provenance::ASSERT_RULE_IRI), None)
            .collect();
        assert_eq!(triples.len(), 2);

        // Filter by a different rule IRI — should return none.
        let triples_none: Vec<_> = foreign
            .derived_by(None, Some("http://example.org/someOtherRule"), None)
            .collect();
        assert!(triples_none.is_empty());
    }

    #[test]
    fn foreign_derived_by_derivation_id_filter() {
        let foreign = small_foreign();
        // Get the derivation_id of the first quad.
        let first_id = foreign.quads[0].derivation_id.clone();
        let triples: Vec<_> = foreign.derived_by(Some(&first_id), None, None).collect();
        assert_eq!(
            triples.len(),
            1,
            "derivation_id filter must return exactly 1"
        );
        assert_eq!(triples[0].0, &first_id);
    }

    #[test]
    fn foreign_contradiction_witness_is_empty() {
        let foreign = small_foreign();
        let witnesses: Vec<_> = foreign.contradiction_witness(TEST_WORLD).collect();
        assert!(
            witnesses.is_empty(),
            "monotonic fragment: contradiction_witness must always be empty"
        );
    }

    #[test]
    fn foreign_derivation_ids_are_well_formed_iris() {
        let foreign = small_foreign();
        for dq in &foreign.quads {
            let id = dq.derivation_id.as_str();
            assert!(
                id.starts_with("https://blackcatinformatics.ca/gmeow/derivation/"),
                "derivation_id must use derivation prefix: {id:?}"
            );
        }
    }
}
