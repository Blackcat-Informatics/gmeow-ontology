// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit state-path admission over the same attributed evidence and calculus.

use super::*;
use crate::modal::composite::TemporalPoint;

#[cfg(test)]
mod tests;

/// The complete selection for one finite observation. The caller supplies every
/// coordinate and both closure decisions explicitly. A path is not a journal:
/// these states carry no fabricated transition entries or commit hashes.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PathSelection {
    /// Authored path identity.
    pub path: String,
    /// Named RDF world containing the order and attributed situation claims.
    pub world: String,
    /// Standpoint owning the selected positive and opposing claims.
    pub standpoint: String,
    /// Explicit, nonempty membership in earliest-to-latest order. Each adjacent
    /// pair must be witnessed by `temporallySucceeds(later, earlier)` in `world`.
    pub states: Vec<String>,
    /// Whether this observation admits no further state beyond its last member.
    pub finalized: bool,
    /// Whether the selected standpoint's evidence inventory is complete.
    /// Closing this inventory never manufactures opposition to an absent claim.
    pub evidence_closed: bool,
}

/// A canonical formula already lowered from its source syntax. Formula identity
/// remains separate from the request identity and its selected observation.
pub struct PathQuery<'a> {
    /// Identity of the caller's explicit evaluation request.
    pub request: &'a str,
    /// Caller-supplied identity of this already-lowered formula or goal expression.
    /// This selector does not independently authenticate an RDF source graph;
    /// source-graph and occurrence admission belongs to the owning compiler.
    pub formula_identity: &'a str,
    /// Explicit state at which this query begins, within the admitted path.
    /// Atomic or guard queries can select the current state; temporal queries
    /// retain the same full observation and select their own suffix start.
    pub anchor: &'a str,
    /// The shared FOL translation; no separate temporal formula representation.
    pub formula: &'a Formula,
}

/// Evaluate a selected state path using the shared contextual physical program.
/// Only attributed RDF 1.2 claims in the selected world and standpoint supply
/// atom evidence. The source digest binds their resolved terms, the exact order,
/// membership and closure decisions; changing any of them changes the receipt.
///
/// # Errors
/// Refuses empty, duplicate, oversized or incorrectly ordered memberships,
/// invalid or unselected anchors, malformed attribution, unsupported formulas
/// and unavailable modal axes.
pub fn evaluate_path(
    dataset: &RdfDataset,
    selection: &PathSelection,
    query: PathQuery<'_>,
    max_steps: Option<u64>,
    stop: Option<&dyn StopSignal>,
) -> gmeow_errors::Result<ContextualAssessment> {
    PreparedPath::admit(dataset, selection)?
        .prepare(query)?
        .evaluate(max_steps, stop)
}

/// An immutable path observation with its world-scoped evidence admitted once.
/// Multiple goal formulas share the same native dataset, attributed claim index,
/// order witnesses and observation digest. The borrow binds this analysis to the
/// exact selected input; changing its evidence or membership needs new admission.
pub struct PreparedPath<'a> {
    frame: PathFrame<'a>,
}

impl<'a> PreparedPath<'a> {
    /// Admit explicit path membership, order, standpoint and attributed evidence.
    /// This performs no formula evaluation and constructs no corpus or journal.
    ///
    /// # Errors
    /// Rejects malformed membership, ordering, world identity or attribution.
    pub fn admit(
        dataset: &'a RdfDataset,
        selection: &'a PathSelection,
    ) -> gmeow_errors::Result<Self> {
        Ok(Self {
            frame: PathFrame::admit(dataset, selection).map_err(diagnostic)?,
        })
    }

    /// Lower one shared FOL formula against this admitted observation. Repeated
    /// evaluation reuses its physical program without reparsing or relowering.
    ///
    /// # Errors
    /// Rejects invalid request/formula/anchor IRIs, an anchor outside the
    /// admitted membership, and unsupported formula structure.
    pub fn prepare(&self, query: PathQuery<'_>) -> gmeow_errors::Result<PreparedPathQuery<'_, 'a>> {
        for identity in [query.request, query.formula_identity, query.anchor] {
            Term::iri(identity)?;
        }
        if !self.frame.positions.contains_key(query.anchor) {
            return Err(diagnostic(malformed(
                "the evaluation anchor is outside the admitted path membership",
            )));
        }
        let program = Program::lower(query.formula, query.anchor).map_err(diagnostic)?;
        let basis = serde_json::to_vec(&(
            "gmeow-finite-prepared-path-query-v1",
            crate::runtime::EngineContract::current().descriptor_hash,
            query.request,
            query.formula_identity,
            query.anchor,
            program.formula_key.to_string(),
            &self.frame.observation,
        ))
        .expect("admitted prepared path query serializes");
        Ok(PreparedPathQuery {
            path: self,
            request: query.request.into(),
            formula_identity: query.formula_identity.into(),
            anchor: query.anchor.into(),
            identity: blake3::hash(&basis).to_hex().to_string(),
            program,
        })
    }
}

/// One compiled formula borrowing an exact admitted observation. It retains no
/// verdict or mutable execution state between calls; budgets and cancellation
/// are checked by each fresh run of the same shared physical program.
pub struct PreparedPathQuery<'path, 'input> {
    path: &'path PreparedPath<'input>,
    request: String,
    formula_identity: String,
    anchor: String,
    identity: String,
    program: Program,
}

impl PreparedPathQuery<'_, '_> {
    /// Identity of this prepared program and its supplied formula/request selectors,
    /// anchor, observation and engine contract. It identifies reusable preparation,
    /// not a verdict: each execution also binds its own resource allowance.
    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// Evaluate without rebuilding the path evidence index or physical program.
    /// Proofs, opposing proofs and pending temporal evidence remain native results.
    ///
    /// # Errors
    /// Refuses unavailable modal axes and malformed evidence reached by execution.
    pub fn evaluate(
        &self,
        max_steps: Option<u64>,
        stop: Option<&dyn StopSignal>,
    ) -> gmeow_errors::Result<ContextualAssessment> {
        let frame = &self.path.frame;
        let selection = frame.selection;
        let selected = &self.anchor;
        let basis = serde_json::to_vec(&(
            "gmeow-finite-path-evaluation-v2",
            self.identity(),
            max_steps,
        ))
        .expect("admitted path query serializes");
        let mut provenance =
            ResultProvenance::native(blake3::hash(&basis).to_hex().to_string(), &selection.world);
        provenance.query = self.formula_identity.clone();
        provenance.conclusion = self.formula_identity.clone();
        provenance.context.standpoint = Some(selection.standpoint.clone());
        provenance.context.attributed = Some(selected.clone());
        provenance.context.path = Some(selection.path.clone());
        let mut evaluation = self
            .program
            .evaluate(frame, selected, max_steps, stop)
            .map_err(diagnostic)?;
        // Atomic assessments bind the same observation as temporal operators.
        if evaluation.temporal_prefixes.is_empty() {
            evaluation
                .temporal_prefixes
                .push(TemporalBasis::Path(frame.observation.clone()));
        }
        finish_assessment(
            &frame.rdf,
            self.request.clone(),
            self.formula_identity.clone(),
            provenance,
            max_steps,
            evaluation,
        )
    }
}

struct PathFrame<'a> {
    rdf: RdfFrame<'a>,
    selection: &'a PathSelection,
    positions: BTreeMap<&'a str, usize>,
    observation: PathObservation,
    order_witnesses: Vec<String>,
}

impl<'a> PathFrame<'a> {
    fn admit(
        dataset: &'a RdfDataset,
        selection: &'a PathSelection,
    ) -> Result<Self, AdmissionError> {
        if selection.states.is_empty() || selection.states.len() > 65_536 {
            return Err(malformed(
                "a finite path requires between 1 and 65,536 explicit states",
            ));
        }
        for identity in [&selection.path, &selection.world, &selection.standpoint]
            .into_iter()
            .chain(selection.states.iter())
        {
            Term::iri(identity).map_err(|error| malformed(error.message()))?;
        }
        let positions: BTreeMap<_, _> = selection
            .states
            .iter()
            .enumerate()
            .map(|(position, state)| (state.as_str(), position))
            .collect();
        if positions.len() != selection.states.len() {
            return Err(malformed("a finite path cannot repeat a state identity"));
        }
        let world = iri_id(dataset, &selection.world)
            .ok_or_else(|| malformed("the selected path world is absent"))?;
        let metadata = Metadata {
            dataset,
            graph: GraphMatch::Named(world),
        };
        let order = format!("{LOGIC_NAMESPACE}temporallySucceeds");
        let mut order_witnesses = Vec::new();
        for pair in selection.states.windows(2) {
            let (earlier, later) = (&pair[0], &pair[1]);
            let later_id =
                iri_id(dataset, later).ok_or_else(|| malformed("path state is absent"))?;
            let predecessors = objects(&metadata, later_id, &order)
                .into_iter()
                .map(|value| iri(dataset, value))
                .collect::<Result<BTreeSet<_>, _>>()?;
            if !predecessors.contains(earlier)
                || predecessors.iter().any(|state| {
                    positions
                        .get(state.as_str())
                        .is_some_and(|index| *index >= positions[later.as_str()])
                })
            {
                return Err(malformed(
                    "path order must witness its adjacent predecessor and exclude backward edges",
                ));
            }
            order_witnesses.push(crate::provenance::reifier_from_strings(
                later,
                &order,
                &format!("<{earlier}>"),
            ));
        }
        let first = iri_id(dataset, &selection.states[0])
            .ok_or_else(|| malformed("the first selected state is absent"))?;
        for predecessor in objects(&metadata, first, &order) {
            if positions.contains_key(iri(dataset, predecessor)?.as_str()) {
                return Err(malformed(
                    "the first path state has a predecessor inside the selection",
                ));
            }
        }
        // One shared evidence index suffices for every state. The bound state
        // resolves in the atom's subject; duplicating all claims per position
        // would multiply memory by the path length.
        let mut rdf = RdfFrame {
            dataset,
            source_graph: Some(world),
            contexts: BTreeMap::from([(
                selection.path.clone(),
                Context {
                    world: selection.world.clone(),
                    standpoint: selection.standpoint.clone(),
                    enactment: None,
                    journal_position: None,
                    norm_scope: None,
                    protocol_scope: None,
                },
            )]),
            evidence_closed: BTreeMap::from([(selection.path.clone(), selection.evidence_closed)]),
            claims: BTreeMap::new(),
            edges: BTreeMap::new(),
            attribution_inferences: BTreeMap::new(),
            native_evidence: BTreeMap::new(),
            journals: BTreeMap::new(),
        };
        rdf.index_claims(&[])?;
        let basis = serde_json::to_vec(&(selection, rdf.basis_digest(), &order_witnesses))
            .expect("admitted path basis serializes");
        let source_digest = blake3::hash(&basis).to_hex().to_string();
        let observation = PathObservation {
            identity: format!("urn:gmeow:path-observation:{source_digest}"),
            path: selection.path.clone(),
            world: selection.world.clone(),
            standpoint: selection.standpoint.clone(),
            source_digest,
            finalized: selection.finalized,
        };
        Ok(Self {
            rdf,
            selection,
            positions,
            observation,
            order_witnesses,
        })
    }
}

impl Frame for PathFrame<'_> {
    fn context(&self, identity: &str) -> Result<&Context, AdmissionError> {
        if !self.positions.contains_key(identity) {
            return Err(malformed(
                "a path formula selected a state outside its membership",
            ));
        }
        self.rdf.context(&self.selection.path)
    }

    fn atom(
        &self,
        context: &str,
        relation: &str,
        subject: &Term,
        object: &Term,
    ) -> Result<Evidence, AdmissionError> {
        self.context(context)?;
        self.rdf
            .atom(&self.selection.path, relation, subject, object)
    }

    fn successors(&self, _: &str, _: Accessibility) -> Result<Successors, AdmissionError> {
        Err(AdmissionError::OutsideFragment(
            "a selected state path admits finite temporal operators only".into(),
        ))
    }

    fn temporal(&self, context: &str) -> Result<TemporalTrace, AdmissionError> {
        self.context(context)?;
        let position = self.positions[context];
        let points = self
            .selection
            .states
            .iter()
            .enumerate()
            .skip(position)
            .take(2)
            .map(|(index, state)| {
                let mut witnesses = vec![self.observation.identity.clone()];
                if index > 0 {
                    witnesses.push(self.order_witnesses[index - 1].clone());
                }
                TemporalPoint {
                    context: state.clone(),
                    witnesses,
                }
            })
            .collect();
        Ok(TemporalTrace {
            points,
            prefix: TemporalBasis::Path(self.observation.clone()),
        })
    }
}
