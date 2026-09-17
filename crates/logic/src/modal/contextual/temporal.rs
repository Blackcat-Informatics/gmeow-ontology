// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Graph-scoped journal admission and selection of attributed state observations.

use super::{
    Accessibility, AdmissionError, Frame, LOGIC_NAMESPACE, Metadata, RdfFrame, TemporalBasis,
    TemporalPrefix, TemporalTrace, iri, iri_id, malformed, objects, optional_iri, require_type,
    required, required_iri,
};
use crate::modal::composite::TemporalPoint;
use crate::modal::journal::{
    FiniteJournal, JournalBoundary, JournalError, JournalObservation, MAX_JOURNAL_ENTRIES,
    ObservedEntry,
};
use crate::runtime::{OutcomeTag, TransitionEntry};
use purrdf::{DatasetView, GraphMatch, TermRef};
use std::collections::BTreeSet;

fn digest<D: DatasetView + ?Sized>(
    metadata: &Metadata<'_, D>,
    subject: D::Id,
    property: &str,
) -> Result<String, AdmissionError> {
    let term = required(metadata, subject, property)?;
    match metadata.resolve(term) {
        TermRef::Literal {
            lexical,
            datatype,
            language: None,
            direction: None,
        } if matches!(
            metadata.resolve(datatype),
            TermRef::Iri("http://www.w3.org/2001/XMLSchema#string")
        ) =>
        {
            lexical
                .strip_prefix("blake3:")
                .map(str::to_owned)
                .ok_or_else(|| malformed(format!("logic:{property} requires a blake3: digest")))
        }
        _ => Err(malformed(format!(
            "logic:{property} requires an algorithm-prefixed string digest"
        ))),
    }
}

fn outcome<D: DatasetView + ?Sized>(
    metadata: &Metadata<'_, D>,
    subject: D::Id,
) -> Result<OutcomeTag, AdmissionError> {
    match required_iri(metadata, subject, "journalOutcomeTag")?.strip_prefix(LOGIC_NAMESPACE) {
        Some("OutcomeApplied") => Ok(OutcomeTag::Applied),
        Some("OutcomeRequiresFullRebuild") => Ok(OutcomeTag::RequiresFullRebuild),
        Some("OutcomeUnsupportedFragment") => Ok(OutcomeTag::UnsupportedFragment),
        Some("OutcomeIncomplete") => Ok(OutcomeTag::Incomplete),
        Some("OutcomeInvalid") => Ok(OutcomeTag::Invalid),
        Some("OutcomeEngineFailure") => Ok(OutcomeTag::EngineFailure),
        _ => Err(malformed(
            "journalOutcomeTag requires an existing typed runtime outcome",
        )),
    }
}

fn admit<D: DatasetView + ?Sized>(
    metadata: &Metadata<'_, D>,
    identity: &str,
    enactment: &str,
) -> Result<FiniteJournal, AdmissionError> {
    let subject =
        iri_id(metadata.dataset, identity).ok_or_else(|| malformed("missing selected journal"))?;
    require_type(metadata, subject, "TransitionJournal")?;
    let owner = iri_id(metadata.dataset, enactment)
        .ok_or_else(|| malformed("missing journal enactment"))?;
    require_type(metadata, owner, "Enactment")?;
    if required_iri(metadata, owner, "enactmentJournal")? != identity {
        return Err(malformed(
            "selected journal is not the enactment's declared journal",
        ));
    }
    let ownership = iri_id(
        metadata.dataset,
        &format!("{LOGIC_NAMESPACE}enactmentJournal"),
    )
    .expect("required ownership binding exists");
    let owners = gmeow_logic_compile::frontend::selected_source_statements(
        metadata.dataset,
        None,
        Some(ownership),
        Some(subject),
        metadata.graph,
    )
    .map(|quad| quad.s)
    .collect::<BTreeSet<_>>();
    if owners != BTreeSet::from([owner]) {
        return Err(malformed(
            "selected journal has ambiguous enactment ownership",
        ));
    }
    let boundary =
        match required_iri(metadata, subject, "journalBoundary")?.strip_prefix(LOGIC_NAMESPACE) {
            Some("OpenJournalBoundary") => JournalBoundary::Open,
            Some("FinalizedJournalBoundary") => JournalBoundary::Finalized,
            _ => {
                return Err(malformed(
                    "journalBoundary requires an explicit open or finalized boundary",
                ));
            }
        };
    let inventory = objects(metadata, subject, &format!("{LOGIC_NAMESPACE}journalEntry"));
    if inventory.len() > MAX_JOURNAL_ENTRIES {
        return Err(AdmissionError::JournalLimit {
            limit: MAX_JOURNAL_ENTRIES,
        });
    }
    let entries = inventory
        .into_iter()
        .map(|entry| {
            require_type(metadata, entry, "JournalEntry")?;
            Ok(ObservedEntry {
                identity: iri(metadata.dataset, entry)?,
                predecessor: optional_iri(metadata, entry, "journalPredecessor")?,
                transition: TransitionEntry {
                    prev_state_hash: digest(metadata, entry, "journalPrevHead")?,
                    delta_identity: digest(metadata, entry, "journalDeltaIdentity")?,
                    new_state_hash: digest(metadata, entry, "journalNewHead")?,
                    outcome_tag: outcome(metadata, entry)?,
                },
            })
        })
        .collect::<Result<Vec<_>, AdmissionError>>()?;
    FiniteJournal::admit(JournalObservation {
        identity: identity.into(),
        enactment: enactment.into(),
        initial_head: digest(metadata, subject, "journalInitialHead")?,
        head_entry: required_iri(metadata, subject, "journalHead")?,
        entries,
        boundary,
    })
    .map_err(|error| match error {
        JournalError::Invalid(detail) => malformed(detail),
        JournalError::AdmissionLimit { limit } => AdmissionError::JournalLimit { limit },
    })
}

impl<D: DatasetView + ?Sized> RdfFrame<'_, D> {
    fn metadata(&self) -> Metadata<'_, D> {
        Metadata {
            dataset: self.dataset,
            graph: self
                .source_graph
                .map_or(GraphMatch::Default, GraphMatch::Named),
        }
    }

    /// Exact subject footprint shared by source identity and immutable admission.
    pub(super) fn journal_basis_subjects(&self) -> BTreeSet<D::Id> {
        let metadata = self.metadata();
        let mut subjects = BTreeSet::new();
        for coordinates in self.contexts.values() {
            if let Some((identity, _)) = &coordinates.journal_position {
                if let Some(subject) = iri_id(metadata.dataset, identity) {
                    subjects.insert(subject);
                    subjects.extend(objects(
                        &metadata,
                        subject,
                        &format!("{LOGIC_NAMESPACE}journalEntry"),
                    ));
                }
                if let Some(owner) = coordinates
                    .enactment
                    .as_deref()
                    .and_then(|identity| iri_id(metadata.dataset, identity))
                {
                    subjects.insert(owner);
                }
            }
        }
        subjects
    }

    /// Bind the supplied journal records before evaluation, including on a stop
    /// before lazy admission. Unrelated journals in the source graph are not read.
    pub(super) fn journal_basis(&self, records: &mut BTreeSet<Vec<u8>>) {
        let metadata = self.metadata();
        for subject in self.journal_basis_subjects() {
            for quad in gmeow_logic_compile::frontend::selected_source_statements(
                metadata.dataset,
                Some(subject),
                None,
                None,
                metadata.graph,
            ) {
                records.insert(
                    serde_json::to_vec(&(
                        "journal-source",
                        crate::reason::dataset::native(metadata.dataset, quad.s)
                            .to_canonical_bytes(),
                        crate::reason::dataset::native(metadata.dataset, quad.p)
                            .to_canonical_bytes(),
                        crate::reason::dataset::native(metadata.dataset, quad.o)
                            .to_canonical_bytes(),
                    ))
                    .expect("resolved journal record serializes"),
                );
            }
        }
    }

    pub(super) fn selected_journal(&self, context: &str) -> Result<&FiniteJournal, AdmissionError> {
        let coordinates = self.context(context)?;
        let (identity, _) = coordinates.journal_position.as_ref().ok_or_else(|| {
            malformed("finite temporal evaluation requires a selected journal position")
        })?;
        let enactment = coordinates
            .enactment
            .as_deref()
            .ok_or_else(|| malformed("finite temporal evaluation requires an enactment"))?;
        let journal = self
            .journals
            .get(identity)
            .ok_or_else(|| malformed("selected journal was not indexed"))?
            .get_or_init(|| admit(&self.metadata(), identity, enactment))
            .as_ref()
            .map_err(Clone::clone)?;
        if journal.enactment() != enactment {
            return Err(malformed(
                "one journal cannot belong to different selected enactments",
            ));
        }
        Ok(journal)
    }

    pub(super) fn temporal_trace(&self, context: &str) -> Result<TemporalTrace, AdmissionError> {
        let journal = self.selected_journal(context)?;
        let coordinates = self.context(context)?;
        let (identity, position) = coordinates
            .journal_position
            .as_ref()
            .expect("selected journal position");
        let enactment = journal.enactment();
        let position = usize::try_from(*position)
            .map_err(|_| malformed("journal position exceeds the admitted range"))?;
        let entry = journal.entries().get(position).ok_or_else(|| {
            malformed("context position is outside the selected committed prefix")
        })?;
        let prefix = TemporalPrefix {
            identity: format!("urn:gmeow:temporal-prefix:{}", journal.prefix_identity()),
            journal: journal.identity().to_owned(),
            enactment: enactment.into(),
            head: journal
                .entries()
                .last()
                .expect("admitted nonempty journal")
                .identity
                .clone(),
            finalized: journal.boundary() == JournalBoundary::Finalized,
            initial_head: journal.initial_head().to_owned(),
            head_hash: journal.head_hash().to_owned(),
        };
        let end = (position + 2).min(journal.entries().len());
        let mut points = vec![TemporalPoint {
            context: context.into(),
            witnesses: vec![entry.identity.clone()],
        }];
        let axis = Accessibility::parse(super::super::TYPED_ACCESSIBILITY[3])
            .expect("typed temporal axis");
        for index in position + 1..end {
            let current = &points.last().expect("nonempty trace").context;
            let successors = self.successors(current, axis)?;
            let closure = successors.closure_witness.ok_or_else(|| {
                malformed(
                    "observed temporal adjacency requires an explicitly closed successor inventory",
                )
            })?;
            let [next] = successors.transitions.as_slice() else {
                return Err(malformed(
                    "observed temporal adjacency requires exactly one successor context",
                ));
            };
            let next_coordinates = self.context(&next.destination)?;
            self.context(current)?
                .validate_transition(axis, next_coordinates)?;
            if next_coordinates.journal_position.as_ref() != Some(&(identity.clone(), index as u64))
            {
                return Err(malformed(
                    "temporal successor must select the immediately next committed position",
                ));
            }
            points.push(TemporalPoint {
                context: next.destination.clone(),
                witnesses: vec![
                    journal.entries()[index].identity.clone(),
                    next.witness.clone(),
                    closure,
                ],
            });
        }
        Ok(TemporalTrace {
            points,
            prefix: TemporalBasis::Journal(prefix),
        })
    }
}
