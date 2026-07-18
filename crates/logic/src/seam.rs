// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! World-scoped fact and provenance access contract.
//!
//! [`WorldFactSource`] is the read-only input boundary shared by the native
//! demand-transformed evaluator and the retained reference resolver. A named-graph
//! IRI identifies the world, while [`DerivedQuad`] carries the fact and its provenance.
//! Query answers never mutate the source snapshot.

use std::cell::Cell;

use purrdf::{DatasetView, GraphMatch, QuadIds, TermRef, TermValue};

const RDF_REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
const SNAPSHOT_SOURCE_CONTRACT: &str =
    "https://blackcatinformatics.ca/gmeow/contract/world-fact-snapshot-v2";

// ── Newtype wrappers ────────────────────────────────────────────────────────────────────────────

/// A stable, opaque identifier for a single derivation step.
///
/// Stored as an IRI string and carried through as a provenance anchor.
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

/// One owned provenance-lookup result.
pub type DerivationRecord = (DerivationId, String, Vec<String>);

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

/// A world-scoped quad with derivation metadata.
///
/// Every materialized quad carries enough metadata for the explanation surface to
/// trace provenance without consulting a secondary index.
///
/// Field names and semantics match the design contract in
/// `slices/grounding/logic/design/LOGIC-RUNTIME.md §"The seam data contract"` verbatim:
///
/// ```text
/// Native carrier (per derived quad):
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

/// Stable identity of the RDF source consulted by one query operation.
///
/// `generation` identifies an immutable source snapshot. `source_contract`
/// identifies the caller/provider contract under which that snapshot is exposed.
/// Both are explicit: GMEOW never guesses a durable generation from an address or
/// silently treats a changed provider as the same world.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorldSourceIdentity {
    /// Caller/provider identity for the immutable RDF snapshot.
    pub generation: String,
    /// Stable identity of the source/provider contract.
    pub source_contract: String,
}

impl WorldSourceIdentity {
    /// Construct an explicit source identity.
    #[must_use]
    pub fn new(generation: impl Into<String>, source_contract: impl Into<String>) -> Self {
        Self {
            generation: generation.into(),
            source_contract: source_contract.into(),
        }
    }
}

/// One source-side RDF pattern, always scoped by the separately supplied world.
///
/// Owned terms make extraction plans independent of a particular dataset's local
/// term ids. A [`RdfViewFactSource`] resolves them into the selected view's id space
/// immediately before the indexed probe.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorldFactPattern {
    /// Optional subject constraint.
    pub subject: Option<TermValue>,
    /// Optional predicate-IRI constraint.
    pub predicate: Option<String>,
    /// Optional object constraint.
    pub object: Option<TermValue>,
}

impl WorldFactPattern {
    /// An unconstrained scan of one named world.
    pub const ANY: Self = Self {
        subject: None,
        predicate: None,
        object: None,
    };

    /// Construct a pattern from optional owned RDF values.
    #[must_use]
    pub fn new(
        subject: Option<TermValue>,
        predicate: Option<String>,
        object: Option<TermValue>,
    ) -> Self {
        Self {
            subject,
            predicate,
            object,
        }
    }

    /// Whether this pattern includes every row another pattern can match.
    #[must_use]
    pub fn subsumes(&self, other: &Self) -> bool {
        self.subject
            .as_ref()
            .is_none_or(|value| other.subject.as_ref() == Some(value))
            && self
                .predicate
                .as_ref()
                .is_none_or(|value| other.predicate.as_ref() == Some(value))
            && self
                .object
                .as_ref()
                .is_none_or(|value| other.object.as_ref() == Some(value))
    }
}

/// Deterministic structural evidence about source access performed by one dispatch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorldSourceMetrics {
    /// Number of world-pattern probes issued by the engine.
    pub pattern_probes: u64,
    /// Number of cardinality estimates pushed into the backing RDF view.
    pub cardinality_probes: u64,
    /// Sum of the source's upper-bound cardinality estimates for planned probes.
    ///
    /// This is structural planning evidence, not an exact count of delivered rows.
    pub estimated_primary_quads: u64,
    /// Primary RDF quads delivered to the engine.
    pub primary_quads: u64,
    /// RDF 1.2 `rdf:reifies` virtual quads delivered to the engine.
    pub reifier_quads: u64,
    /// RDF 1.2 annotation virtual quads delivered to the engine.
    pub annotation_quads: u64,
}

impl WorldSourceMetrics {
    /// Total delivered RDF rows across the primary and RDF 1.2 virtual tables.
    #[must_use]
    pub const fn delivered_quads(self) -> u64 {
        self.primary_quads + self.reifier_quads + self.annotation_quads
    }
}

// ── WorldFactSource trait ────────────────────────────────────────────────────────────────────────

/// Read-only access to the facts, provenance, and contradiction witnesses of a world.
///
/// The hot read operation is visitor-shaped so a view-backed source can resolve a
/// borrowed [`DatasetView`] row, present a temporary [`DerivedQuad`], and release it
/// without collecting a world-sized vector. This object-safe seam is the actual
/// external/provider boundary; the underlying RDF scan remains statically dispatched
/// through [`DatasetView`].
pub trait WorldFactSource {
    /// Stable identity of the immutable source snapshot and provider contract.
    fn identity(&self) -> &WorldSourceIdentity;

    /// World-indexed quad lookup.
    ///
    /// Mode: `W` is ground (the world IRI is always known at call time); `S`, `P`, `O` may be
    /// unbound variables that unify against the store, or ground terms that act as filters.
    ///
    /// Calls `visitor` for every quad in `world` that unifies with `pattern`.
    /// Implementations must push the pattern into their indexed backing view; they
    /// must not satisfy a selective request by first materializing the whole world.
    fn visit_world(
        &self,
        world: &str,
        pattern: &WorldFactPattern,
        visitor: &mut dyn FnMut(&DerivedQuad) -> gmeow_errors::Result<()>,
    ) -> gmeow_errors::Result<()>;

    /// Estimate the primary RDF rows a world-pattern probe can admit.
    ///
    /// `None` means the source has no cardinality contract. Implementations with a
    /// backing RDF index should push the complete `(S,P,O,G)` pattern into that
    /// index's estimate operation; callers use the value only to order independent
    /// source probes, never as a semantic count or an absence test.
    fn estimate_world(
        &self,
        _world: &str,
        _pattern: &WorldFactPattern,
    ) -> gmeow_errors::Result<Option<usize>> {
        Ok(None)
    }

    /// Collect a pattern result into owned, dataset-independent values.
    ///
    /// This compatibility helper is intentionally not used by the physical EDB
    /// loader. Consumers that can process rows incrementally should call
    /// [`visit_world`](Self::visit_world).
    fn in_world(
        &self,
        world: &str,
        subject: Option<&TermValue>,
        predicate: Option<&str>,
        object: Option<&TermValue>,
    ) -> gmeow_errors::Result<Vec<DerivedQuad>> {
        let pattern = WorldFactPattern::new(
            subject.cloned(),
            predicate.map(str::to_owned),
            object.cloned(),
        );
        let mut quads = Vec::new();
        self.visit_world(world, &pattern, &mut |quad| {
            quads.push(quad.clone());
            Ok(())
        })?;
        Ok(quads)
    }

    /// Structural source-access evidence accumulated so far.
    fn metrics(&self) -> WorldSourceMetrics {
        WorldSourceMetrics::default()
    }

    /// Provenance lookup for explanations.
    ///
    /// Mode: any argument may be unbound; all are output if unbound. When `quad_id` is ground
    /// this is a direct provenance lookup; when unbound it enumerates all derivations.
    ///
    /// Returns owned `(derivation_id, rule_iri, source_quad_ids)` triples for
    /// derivations that match the (possibly partial) pattern. Ownership lets a
    /// paged/provider-backed source release each borrowed row before returning;
    /// failures are explicit and cannot be confused with semantic absence.
    fn derived_by(
        &self,
        quad_id: Option<&DerivationId>,
        rule: Option<&str>,
        sources: Option<&[String]>,
    ) -> gmeow_errors::Result<Vec<DerivationRecord>>;

    /// Within-world inconsistency as a statement graph.
    ///
    /// Mode: `W` is ground (the world to inspect is always specified); `WitnessGraph` is an
    /// output — the IRI of a GMEOW statement graph representing the minimal conflict set
    /// (paraconsistent witness) if one exists, or nothing if the world is consistent.
    ///
    /// Contradictions are never bare failures; a witness graph is always emitted (see
    /// LOGIC-RUNTIME.md §"Contradiction witnesses").
    fn contradiction_witness<'a>(&'a self, _world: &str) -> Box<dyn Iterator<Item = String> + 'a> {
        Box::new(std::iter::empty())
    }
}

/// A zero-copy world source over any PurRDF resident, paged, or succinct-pack view.
///
/// The source never owns or freezes the dataset. It resolves caller-owned RDF values
/// into the selected view's local id space, pushes the complete `(S,P,O,G)` pattern
/// through [`DatasetView::quads_for_pattern`], and owns only each row that actually
/// crosses into the logic engine. RDF 1.2 reifier and annotation side tables are
/// exposed as their virtual quads when present.
pub struct RdfViewFactSource<'view, V: DatasetView> {
    view: &'view V,
    profile: String,
    identity: WorldSourceIdentity,
    metrics: Cell<WorldSourceMetrics>,
}

impl<'view, V: DatasetView> RdfViewFactSource<'view, V> {
    /// Bind an explicit source identity and semantic profile to a borrowed RDF view.
    #[must_use]
    pub fn new(view: &'view V, profile: impl Into<String>, identity: WorldSourceIdentity) -> Self {
        Self {
            view,
            profile: profile.into(),
            identity,
            metrics: Cell::new(WorldSourceMetrics::default()),
        }
    }

    fn deliver(
        &self,
        world: &str,
        quad: QuadIds<V::Id>,
        kind: VirtualQuadKind,
        visitor: &mut dyn FnMut(&DerivedQuad) -> gmeow_errors::Result<()>,
    ) -> gmeow_errors::Result<()> {
        let predicate = match self.view.resolve(quad.p) {
            TermRef::Iri(iri) => iri.to_owned(),
            _ => {
                return Err(source_error(
                    "RDF view yielded a non-IRI predicate; the source contract is invalid",
                ));
            }
        };
        let subject = term_value(self.view, quad.s)?;
        let object = term_value(self.view, quad.o)?;
        let derived = asserted_quad(world, subject, predicate, object, &self.profile)?;
        visitor(&derived)?;

        let mut metrics = self.metrics.get();
        match kind {
            VirtualQuadKind::Primary => metrics.primary_quads += 1,
            VirtualQuadKind::Reifier => metrics.reifier_quads += 1,
            VirtualQuadKind::Annotation => metrics.annotation_quads += 1,
        }
        self.metrics.set(metrics);
        Ok(())
    }

    fn provenance_row(&self, quad: QuadIds<V::Id>) -> gmeow_errors::Result<Option<DerivedQuad>> {
        let Some(graph) = quad.g else {
            return Ok(None);
        };
        let world = match self.view.resolve(graph) {
            TermRef::Iri(iri) => iri.to_owned(),
            _ => {
                return Err(source_error(
                    "RDF view yielded a non-IRI named graph; the world source contract is invalid",
                ));
            }
        };
        let predicate = match self.view.resolve(quad.p) {
            TermRef::Iri(iri) => iri.to_owned(),
            _ => {
                return Err(source_error(
                    "RDF view yielded a non-IRI predicate; the source contract is invalid",
                ));
            }
        };
        Ok(Some(asserted_quad(
            &world,
            term_value(self.view, quad.s)?,
            predicate,
            term_value(self.view, quad.o)?,
            &self.profile,
        )?))
    }
}

impl<V: DatasetView> WorldFactSource for RdfViewFactSource<'_, V> {
    fn identity(&self) -> &WorldSourceIdentity {
        &self.identity
    }

    fn visit_world(
        &self,
        world: &str,
        pattern: &WorldFactPattern,
        visitor: &mut dyn FnMut(&DerivedQuad) -> gmeow_errors::Result<()>,
    ) -> gmeow_errors::Result<()> {
        let mut metrics = self.metrics.get();
        metrics.pattern_probes += 1;
        self.metrics.set(metrics);

        let Some(graph) = self.view.term_id_by_value(&TermValue::iri(world)) else {
            return Ok(());
        };
        let subject = match &pattern.subject {
            Some(value) => match self.view.term_id_by_value(value) {
                Some(id) => Some(id),
                None => return Ok(()),
            },
            None => None,
        };
        let predicate = match &pattern.predicate {
            Some(value) => match self.view.term_id_by_value(&TermValue::iri(value)) {
                Some(id) => Some(id),
                None => return Ok(()),
            },
            None => None,
        };
        let object = match &pattern.object {
            Some(value) => match self.view.term_id_by_value(value) {
                Some(id) => Some(id),
                None => return Ok(()),
            },
            None => None,
        };

        for quad in
            self.view
                .quads_for_pattern(subject, predicate, object, GraphMatch::Named(graph))
        {
            self.deliver(world, quad, VirtualQuadKind::Primary, visitor)?;
        }

        let matches = |quad: &QuadIds<V::Id>| {
            quad.g == Some(graph)
                && subject.is_none_or(|id| quad.s == id)
                && predicate.is_none_or(|id| quad.p == id)
                && object.is_none_or(|id| quad.o == id)
        };
        let object_can_be_triple = pattern
            .object
            .as_ref()
            .is_none_or(|value| matches!(value, TermValue::Triple { .. }));
        if object_can_be_triple
            && pattern
                .predicate
                .as_deref()
                .is_none_or(|value| value == RDF_REIFIES)
        {
            for quad in self.view.reifier_quads().filter(|quad| matches(quad)) {
                self.deliver(world, quad, VirtualQuadKind::Reifier, visitor)?;
            }
        }
        if self.view.capabilities().annotations {
            if let Some(reifier) = subject {
                for (annotation_predicate, annotation_object, annotation_graph) in
                    self.view.annotations_of_with_graph(reifier)
                {
                    let quad = QuadIds {
                        s: reifier,
                        p: annotation_predicate,
                        o: annotation_object,
                        g: annotation_graph,
                    };
                    if matches(&quad) {
                        self.deliver(world, quad, VirtualQuadKind::Annotation, visitor)?;
                    }
                }
            } else {
                for quad in self.view.annotation_quads().filter(|quad| matches(quad)) {
                    self.deliver(world, quad, VirtualQuadKind::Annotation, visitor)?;
                }
            }
        }
        Ok(())
    }

    fn estimate_world(
        &self,
        world: &str,
        pattern: &WorldFactPattern,
    ) -> gmeow_errors::Result<Option<usize>> {
        let Some(graph) = self.view.term_id_by_value(&TermValue::iri(world)) else {
            return Ok(Some(0));
        };
        let subject = match &pattern.subject {
            Some(value) => match self.view.term_id_by_value(value) {
                Some(id) => Some(id),
                None => return Ok(Some(0)),
            },
            None => None,
        };
        let predicate = match &pattern.predicate {
            Some(value) => match self.view.term_id_by_value(&TermValue::iri(value)) {
                Some(id) => Some(id),
                None => return Ok(Some(0)),
            },
            None => None,
        };
        let object = match &pattern.object {
            Some(value) => match self.view.term_id_by_value(value) {
                Some(id) => Some(id),
                None => return Ok(Some(0)),
            },
            None => None,
        };
        let estimate =
            self.view
                .cardinality_estimate(subject, predicate, object, GraphMatch::Named(graph));
        let mut metrics = self.metrics.get();
        metrics.cardinality_probes += 1;
        metrics.estimated_primary_quads = metrics
            .estimated_primary_quads
            .saturating_add(u64::try_from(estimate).unwrap_or(u64::MAX));
        self.metrics.set(metrics);
        Ok(Some(estimate))
    }

    fn metrics(&self) -> WorldSourceMetrics {
        self.metrics.get()
    }

    fn derived_by(
        &self,
        quad_id: Option<&DerivationId>,
        rule: Option<&str>,
        sources: Option<&[String]>,
    ) -> gmeow_errors::Result<Vec<DerivationRecord>> {
        let mut rows = Vec::new();
        let mut admit = |quad| -> gmeow_errors::Result<()> {
            let Some(derived) = self.provenance_row(quad)? else {
                return Ok(());
            };
            if quad_id.is_some_and(|candidate| candidate != &derived.derivation_id)
                || rule.is_some_and(|candidate| candidate != derived.rule_iri)
                || sources.is_some_and(|candidate| candidate != derived.source_quad_ids)
            {
                return Ok(());
            }
            rows.push((
                derived.derivation_id,
                derived.rule_iri,
                derived.source_quad_ids,
            ));
            Ok(())
        };
        for quad in self.view.quads() {
            admit(quad)?;
        }
        for quad in self.view.reifier_quads() {
            admit(quad)?;
        }
        if self.view.capabilities().annotations {
            for quad in self.view.annotation_quads() {
                admit(quad)?;
            }
        }
        Ok(rows)
    }
}

#[derive(Clone, Copy)]
enum VirtualQuadKind {
    Primary,
    Reifier,
    Annotation,
}

fn source_error(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason {
        detail: detail.into(),
    })
}

fn term_value<V: DatasetView>(view: &V, id: V::Id) -> gmeow_errors::Result<TermValue> {
    match view.resolve(id) {
        TermRef::Iri(iri) => Ok(TermValue::iri(iri)),
        TermRef::Blank { label, scope } => Ok(TermValue::Blank {
            label: label.to_owned(),
            scope,
        }),
        TermRef::Literal {
            lexical,
            datatype,
            language,
            direction,
        } => {
            let TermRef::Iri(datatype) = view.resolve(datatype) else {
                return Err(source_error(
                    "RDF view yielded a literal whose datatype is not an IRI",
                ));
            };
            Ok(TermValue::Literal {
                lexical_form: lexical.to_owned(),
                datatype: datatype.to_owned(),
                language: language.map(str::to_owned),
                direction,
            })
        }
        TermRef::Triple { s, p, o } => Ok(TermValue::Triple {
            s: Box::new(term_value(view, s)?),
            p: Box::new(term_value(view, p)?),
            o: Box::new(term_value(view, o)?),
        }),
    }
}

fn asserted_quad(
    world: &str,
    subject: TermValue,
    predicate: String,
    object: TermValue,
    profile: &str,
) -> gmeow_errors::Result<DerivedQuad> {
    let reifier = crate::provenance::mint_reifier(&subject, &predicate, &object)
        .map_err(|error| source_error(format!("mint_reifier failed: {error}")))?;
    let derivation_id = DerivationId(crate::provenance::mint_derivation_id(
        crate::provenance::ASSERT_RULE_IRI,
        &[reifier.as_str()],
    ));
    Ok(DerivedQuad {
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
    })
}

// ── WorldFactSnapshot ──────────────────────────────────────────────────────────────────────────

/// A concrete [`WorldFactSource`] implementer that owns a snapshot of asserted base facts
/// drawn from a [`crate::store::WorldStore`] world.
///
/// `WorldFactSnapshot` is populated by [`WorldFactSnapshot::from_world`], which takes a
/// synchronous snapshot of all quads in a named-graph world and wraps each as a
/// [`DerivedQuad`] carrying the `logic:assert` rule IRI and a content-addressed
/// [`DerivationId`].  The snapshot is immutable after construction.
///
/// This snapshot is the native evaluator's asserted-fact input.
pub struct WorldFactSnapshot {
    quads: Vec<DerivedQuad>,
    identity: WorldSourceIdentity,
    metrics: Cell<WorldSourceMetrics>,
}

impl WorldFactSnapshot {
    /// Build a `WorldFactSnapshot` by snapshotting all quads in `world` from `store`.
    ///
    /// Each oxigraph quad is converted to a [`DerivedQuad`] representing an asserted
    /// base fact:
    /// - `rule_iri` = [`crate::provenance::ASSERT_RULE_IRI`]
    /// - `reifier` = `mint_reifier(subject, predicate, object)`
    /// - `derivation_id` = `mint_derivation_id(ASSERT_RULE_IRI, [reifier])`
    /// - `source_quad_ids` = `[reifier]`
    /// - `budget_status` = [`BudgetStatus::Ok`]
    ///
    /// Quads whose predicate is not an IRI (which RDF 1.2 does not permit) are
    /// skipped. The first `mint_reifier` failure aborts the entire snapshot and
    /// returns `Err`; no partial snapshot is exposed to a caller.
    ///
    /// # Errors
    ///
    /// Returns `Err` if reifier minting fails for any quad.
    pub fn from_world(
        store: &crate::store::WorldStore,
        world: &str,
        profile: &str,
    ) -> gmeow_errors::Result<Self> {
        let raw_quads = store.quads_for_pattern_in_world(world, None, None, None);

        let mut derived: Vec<DerivedQuad> = Vec::with_capacity(raw_quads.len());

        for quad in raw_quads {
            // quad.p is always an IRI (RDF invariant); a non-IRI predicate is skipped.
            let Some(predicate) = quad.p.as_iri().map(str::to_owned) else {
                continue;
            };
            let subject = quad.s.clone();
            let object = quad.o.clone();

            derived.push(asserted_quad(world, subject, predicate, object, profile)?);
        }

        let identity = snapshot_identity(&derived)?;
        Ok(Self {
            quads: derived,
            identity,
            metrics: Cell::new(WorldSourceMetrics::default()),
        })
    }
}

impl WorldFactSource for WorldFactSnapshot {
    fn identity(&self) -> &WorldSourceIdentity {
        &self.identity
    }

    /// `in_world(+W, ?S, ?P, ?O)` — return quads in `world` matching the optional pattern.
    ///
    /// Filters `self.quads` by world equality and each provided optional term filter.
    fn visit_world(
        &self,
        world: &str,
        pattern: &WorldFactPattern,
        visitor: &mut dyn FnMut(&DerivedQuad) -> gmeow_errors::Result<()>,
    ) -> gmeow_errors::Result<()> {
        let mut metrics = self.metrics.get();
        metrics.pattern_probes += 1;
        let mut delivered = 0_u64;
        for quad in &self.quads {
            if quad.graph != world
                || pattern
                    .subject
                    .as_ref()
                    .is_some_and(|subject| &quad.subject != subject)
                || pattern
                    .predicate
                    .as_ref()
                    .is_some_and(|predicate| &quad.predicate != predicate)
                || pattern
                    .object
                    .as_ref()
                    .is_some_and(|object| &quad.object != object)
            {
                continue;
            }
            if let Err(error) = visitor(quad) {
                metrics.primary_quads = metrics.primary_quads.saturating_add(delivered);
                self.metrics.set(metrics);
                return Err(error);
            }
            delivered += 1;
        }
        metrics.primary_quads = metrics.primary_quads.saturating_add(delivered);
        self.metrics.set(metrics);
        Ok(())
    }

    fn metrics(&self) -> WorldSourceMetrics {
        self.metrics.get()
    }

    fn estimate_world(
        &self,
        world: &str,
        pattern: &WorldFactPattern,
    ) -> gmeow_errors::Result<Option<usize>> {
        let estimate = self
            .quads
            .iter()
            .filter(|quad| {
                quad.graph == world
                    && pattern
                        .subject
                        .as_ref()
                        .is_none_or(|subject| &quad.subject == subject)
                    && pattern
                        .predicate
                        .as_ref()
                        .is_none_or(|predicate| &quad.predicate == predicate)
                    && pattern
                        .object
                        .as_ref()
                        .is_none_or(|object| &quad.object == object)
            })
            .count();
        let mut metrics = self.metrics.get();
        metrics.cardinality_probes += 1;
        metrics.estimated_primary_quads = metrics
            .estimated_primary_quads
            .saturating_add(u64::try_from(estimate).unwrap_or(u64::MAX));
        self.metrics.set(metrics);
        Ok(Some(estimate))
    }

    /// `derived_by(?QuadId, ?Rule, ?Sources)` — provenance enumeration.
    ///
    /// Enumerates `self.quads` as `(derivation_id, rule_iri, source_quad_ids)` triples.
    /// Filters every bound component and returns owned provenance rows.
    fn derived_by(
        &self,
        quad_id: Option<&DerivationId>,
        rule: Option<&str>,
        sources: Option<&[String]>,
    ) -> gmeow_errors::Result<Vec<DerivationRecord>> {
        Ok(self
            .quads
            .iter()
            .filter(|dq| {
                quad_id.is_none_or(|candidate| candidate == &dq.derivation_id)
                    && rule.is_none_or(|candidate| candidate == dq.rule_iri)
                    && sources.is_none_or(|candidate| candidate == dq.source_quad_ids)
            })
            .map(|dq| {
                (
                    dq.derivation_id.clone(),
                    dq.rule_iri.clone(),
                    dq.source_quad_ids.clone(),
                )
            })
            .collect())
    }

    /// `contradiction_witness(+W, ?WitnessGraph)` — always empty in this implementation.
    ///
    /// Monotonic-vacuous in v4: the monotonic fragment has no within-world contradictions;
    /// real paraconsistent witnesses arrive later. This empty result is vacuously-correct,
    /// NOT a silent stub.
    fn contradiction_witness<'a>(&'a self, _world: &str) -> Box<dyn Iterator<Item = String> + 'a> {
        Box::new(std::iter::empty())
    }
}

fn snapshot_identity(quads: &[DerivedQuad]) -> gmeow_errors::Result<WorldSourceIdentity> {
    let mut rows = Vec::with_capacity(quads.len());
    for quad in quads {
        rows.push([
            quad.graph.clone(),
            crate::provenance::term_n3(&quad.subject)?,
            quad.predicate.clone(),
            crate::provenance::term_n3(&quad.object)?,
            quad.profile.clone(),
        ]);
    }
    rows.sort();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"gmeow-world-fact-snapshot-generation-v2");
    for row in rows {
        for field in row {
            hasher.update(&(field.len() as u64).to_le_bytes());
            hasher.update(field.as_bytes());
        }
    }
    Ok(WorldSourceIdentity::new(
        format!("urn:blake3:{}", hasher.finalize().to_hex()),
        SNAPSHOT_SOURCE_CONTRACT,
    ))
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

    // ── WorldFactSnapshot ─────────────────────────────────────────────────────────────────────

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

    fn small_foreign() -> WorldFactSnapshot {
        let store = small_store();
        WorldFactSnapshot::from_world(&store, TEST_WORLD, TEST_PROFILE)
            .expect("from_world on a valid store must succeed")
    }

    #[test]
    fn foreign_in_world_all_none_returns_all_asserted_quads() {
        let foreign = small_foreign();
        let quads = foreign
            .in_world(TEST_WORLD, None, None, None)
            .expect("snapshot scan");
        assert_eq!(quads.len(), 2, "should return both asserted quads");
    }

    #[test]
    fn foreign_in_world_predicate_filter() {
        let foreign = small_foreign();
        let quads = foreign
            .in_world(TEST_WORLD, None, Some(P1), None)
            .expect("snapshot scan");
        assert_eq!(quads.len(), 1, "P1 filter should return exactly 1 quad");
        assert_eq!(quads[0].predicate, P1);
    }

    #[test]
    fn foreign_in_world_subject_filter() {
        let foreign = small_foreign();
        let subj_term = TermValue::iri(S2);
        let quads = foreign
            .in_world(TEST_WORLD, Some(&subj_term), None, None)
            .expect("snapshot scan");
        assert_eq!(quads.len(), 1, "S2 filter should return exactly 1 quad");
        assert_eq!(quads[0].subject, subj_term);
    }

    #[test]
    fn foreign_in_world_wrong_world_returns_empty() {
        let foreign = small_foreign();
        let quads = foreign
            .in_world("http://world/Other", None, None, None)
            .expect("snapshot scan");
        assert!(quads.is_empty(), "wrong world must return no quads");
    }

    #[test]
    fn foreign_derived_by_enumerates_with_assert_rule() {
        let foreign = small_foreign();
        let triples = foreign
            .derived_by(None, None, None)
            .expect("provenance scan");
        assert_eq!(triples.len(), 2, "should enumerate 2 asserted derivations");
        for (_, rule, _) in &triples {
            assert_eq!(
                rule,
                crate::provenance::ASSERT_RULE_IRI,
                "rule_iri must be ASSERT_RULE_IRI for asserted facts"
            );
        }
    }

    #[test]
    fn foreign_derived_by_rule_filter() {
        let foreign = small_foreign();
        // Filter by ASSERT_RULE_IRI — should return both.
        let triples = foreign
            .derived_by(None, Some(crate::provenance::ASSERT_RULE_IRI), None)
            .expect("provenance scan");
        assert_eq!(triples.len(), 2);

        // Filter by a different rule IRI — should return none.
        let triples_none = foreign
            .derived_by(None, Some("http://example.org/someOtherRule"), None)
            .expect("provenance scan");
        assert!(triples_none.is_empty());
    }

    #[test]
    fn foreign_derived_by_derivation_id_filter() {
        let foreign = small_foreign();
        // Get the derivation_id of the first quad.
        let first_id = foreign.quads[0].derivation_id.clone();
        let triples = foreign
            .derived_by(Some(&first_id), None, None)
            .expect("provenance scan");
        assert_eq!(
            triples.len(),
            1,
            "derivation_id filter must return exactly 1"
        );
        assert_eq!(triples[0].0, first_id);
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
