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

use oxigraph::model::{NamedNode, Term};

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
    pub graph: NamedNode,

    /// Subject of the derived triple.
    ///
    /// RDF 1.2 subjects may be IRIs, blank nodes, or triple terms (in the object position in
    /// RDF 1.2 — but gmeow-logic uses [`Term`] here for forward-compatibility with RDF 1.2
    /// triple terms across the whole quad).
    pub subject: Term,

    /// Predicate of the derived triple. Always a [`NamedNode`] (IRI).
    pub predicate: NamedNode,

    /// Object of the derived triple. May be a named node, blank node, literal, or (in RDF 1.2)
    /// a triple term.
    pub object: Term,

    /// Graph component of the quad. Must equal [`Self::graph`]; carried separately so the
    /// quad is self-contained when projected to a `(S, P, O, G)` tuple.
    pub graph_component: NamedNode,

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
        world: &NamedNode,
        subject: Option<&Term>,
        predicate: Option<&NamedNode>,
        object: Option<&Term>,
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
    fn contradiction_witness<'a>(
        &'a self,
        world: &NamedNode,
    ) -> Box<dyn Iterator<Item = NamedNode> + 'a>;
}

// ── Unit tests ─────────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
        let world = NamedNode::new("http://logic.gmeow.example/world/alpha").expect("valid IRI");
        DerivedQuad {
            graph: world.clone(),
            subject: Term::NamedNode(
                NamedNode::new("http://example.org/subject/1").expect("valid IRI"),
            ),
            predicate: NamedNode::new("http://example.org/predicate/type").expect("valid IRI"),
            object: Term::NamedNode(
                NamedNode::new("http://example.org/object/Thing").expect("valid IRI"),
            ),
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
        assert_eq!(dq.graph.as_str(), "http://logic.gmeow.example/world/alpha");
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
}
