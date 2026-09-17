// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Append observations reuse the shared physical program and committed proofs.
//! Input admission authenticates prefix extension and unchanged past contextual
//! evidence before any cached judgment can be consumed.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;

use purrdf::RdfDataset;
use purrdf::sparql::StopSignal;

use super::{
    ContextualAssessment, PreparedRequest, RdfFrame, diagnostic, finish_assessment, iri, malformed,
    prepare_request, request_sources,
};
use crate::modal::composite::{JudgmentCache, Program};
use crate::modal::journal::{FiniteJournal, JournalBoundary};

/// A compiled observation monitor for one explicit request over a finite journal.
///
/// Callers supply complete RDF snapshots containing already observed journal
/// entries. The monitor never executes effects or constructs entries. A new
/// snapshot may append entries or finalize its existing head; it must preserve
/// the compiled formula and every previously admitted non-temporal context fact.
/// Changing past observations requires a new monitor or a one-shot assessment.
///
/// The cache retains at most the selected number of committed judgments, with
/// their reachable proof DAG. Eviction causes recomputation. Temporal suffixes
/// and their ancestors are recomputed against each observed prefix; unchanged
/// non-temporal subprograms reuse their exact proofs. This is incremental
/// judgment maintenance, not a constant-time temporal automaton. Input parsing
/// and prefix admission remain separate from the new-judgment step budget.
pub struct ContextualMonitor {
    request: String,
    formula: String,
    selected: String,
    source_graph: Option<String>,
    program: Program,
    journal: FiniteJournal,
    context_basis: BTreeMap<String, String>,
    cache: JudgmentCache,
    capacity: NonZeroUsize,
}

impl ContextualMonitor {
    /// Compile an admitted finite request and bind its initial journal evidence.
    /// No formula judgment or external effect is performed by compilation.
    ///
    /// # Errors
    /// Rejects absent or ambiguous requests, unsupported formulas, malformed
    /// context records, and unauthenticated or unowned journal prefixes.
    pub fn compile(
        dataset: &RdfDataset,
        request: &str,
        capacity: NonZeroUsize,
    ) -> gmeow_errors::Result<Self> {
        let (selected_request, source_graph) = select(dataset, request)?;
        let frame = RdfFrame::load_in_graph(dataset, &[], source_graph).map_err(diagnostic)?;
        let prepared = prepare_request(&frame, selected_request, None)?;
        let program = Program::lower(&prepared.source, &prepared.selected).map_err(diagnostic)?;
        let journal = frame
            .selected_journal(&prepared.selected)
            .map_err(diagnostic)?
            .clone();
        Ok(Self {
            request: request.into(),
            formula: prepared.formula,
            selected: prepared.selected,
            source_graph: graph_identity(&frame)?,
            program,
            journal,
            context_basis: frame.monitor_context_basis(),
            cache: JudgmentCache::default(),
            capacity,
        })
    }

    /// Assess the supplied observation, preserving valid committed work between
    /// appends. `max_steps` budgets newly evaluated judgments, including work
    /// after cache eviction; a cached judgment does not consume another step.
    /// Cancellation and deadline signals still apply to cache hits.
    ///
    /// # Errors
    /// Rejects a changed request/program/source graph, altered past evidence,
    /// journal retraction or replacement, and reopening a finalized journal.
    /// Rejection does not advance the monitor's admitted prefix or cache.
    pub fn observe(
        &mut self,
        dataset: &RdfDataset,
        max_steps: u64,
        stop: Option<&dyn StopSignal>,
    ) -> gmeow_errors::Result<ContextualAssessment> {
        let (request, source_graph) = select(dataset, &self.request)?;
        let frame = RdfFrame::load_in_graph(dataset, &[], source_graph).map_err(diagnostic)?;
        let PreparedRequest {
            formula,
            selected,
            source,
            provenance,
        } = prepare_request(&frame, request, Some(max_steps))?;
        if formula != self.formula
            || selected != self.selected
            || source.content_key() != self.program.formula_key
            || graph_identity(&frame)? != self.source_graph
        {
            return Err(diagnostic(malformed(
                "monitor observation changes its compiled request or source graph",
            )));
        }
        let journal = frame.selected_journal(&selected).map_err(diagnostic)?;
        if journal.identity() != self.journal.identity()
            || journal.enactment() != self.journal.enactment()
            || journal.initial_head() != self.journal.initial_head()
            || !journal.entries().starts_with(self.journal.entries())
            || (self.journal.boundary() == JournalBoundary::Finalized
                && journal.prefix_identity() != self.journal.prefix_identity())
        {
            return Err(diagnostic(malformed(
                "monitor observation is not an append or finalization of its exact journal prefix",
            )));
        }
        let context_basis = frame.monitor_context_basis();
        if self
            .context_basis
            .iter()
            .any(|(identity, basis)| context_basis.get(identity) != Some(basis))
        {
            return Err(diagnostic(malformed(
                "monitor observation changes previously admitted non-temporal context evidence",
            )));
        }
        let (evaluation, cache) = self
            .program
            .evaluate_with_cache(
                &frame,
                &selected,
                Some(max_steps),
                stop,
                &self.cache,
                self.capacity.get(),
            )
            .map_err(|error| diagnostic(error).with_focus(&self.request))?;
        let assessment = finish_assessment(
            &frame,
            self.request.clone(),
            formula,
            provenance,
            Some(max_steps),
            evaluation,
        )?;
        self.journal = journal.clone();
        self.context_basis = context_basis;
        self.cache = cache;
        Ok(assessment)
    }
}

fn select(
    dataset: &RdfDataset,
    request: &str,
) -> gmeow_errors::Result<(purrdf::TermId, Option<purrdf::TermId>)> {
    request_sources(dataset, Some(request))
        .map_err(diagnostic)?
        .remove(request)
        .ok_or_else(|| diagnostic(malformed("the selected monitor request is absent")))
}

fn graph_identity(frame: &RdfFrame<'_>) -> gmeow_errors::Result<Option<String>> {
    frame
        .source_graph
        .map(|graph| iri(frame.dataset, graph))
        .transpose()
        .map_err(diagnostic)
}

impl RdfFrame<'_> {
    /// A new context may arrive on append, but a prior context's admitted
    /// coordinates, selected claims and non-temporal transitions are immutable.
    /// Resolve dataset-local term IDs before comparing separate snapshots.
    fn monitor_context_basis(&self) -> BTreeMap<String, String> {
        let mut records: BTreeMap<_, BTreeSet<Vec<u8>>> = self
            .contexts
            .iter()
            .map(|(identity, context)| {
                (
                    identity.clone(),
                    BTreeSet::from([serde_json::to_vec(&(
                        "context",
                        context,
                        self.evidence_closed[identity],
                    ))
                    .expect("admitted context serializes")]),
                )
            })
            .collect();
        for ((context, subject, predicate, object), evidence) in &self.claims {
            records
                .get_mut(context)
                .expect("admitted claim context")
                .insert(
                    serde_json::to_vec(&(
                        "claim",
                        self.dataset.term_value(*subject).to_canonical_bytes(),
                        self.dataset.term_value(*predicate).to_canonical_bytes(),
                        self.dataset.term_value(*object).to_canonical_bytes(),
                        &evidence.support,
                        &evidence.opposition,
                        evidence.complete,
                    ))
                    .expect("resolved claim serializes"),
                );
        }
        for ((context, axis), successors) in &self.edges {
            if axis.iri() == crate::modal::TYPED_ACCESSIBILITY[3] {
                continue;
            }
            let members = successors
                .transitions
                .iter()
                .map(|transition| (&transition.destination, &transition.witness))
                .collect::<BTreeSet<_>>();
            records
                .get_mut(context)
                .expect("admitted successor context")
                .insert(
                    serde_json::to_vec(&(
                        "successors",
                        axis.iri(),
                        members,
                        &successors.closure_witness,
                    ))
                    .expect("resolved successors serialize"),
                );
        }
        records
            .into_iter()
            .map(|(context, rows)| {
                let mut hash = blake3::Hasher::new();
                hash.update(b"gmeow-monitor-context-basis-v1\0");
                for row in rows {
                    crate::runtime::frame(&mut hash, b"record", &row);
                }
                (context, hash.finalize().to_hex().to_string())
            })
            .collect()
    }
}
