// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Elementary Transaction-Logic updates over world snapshots.
//!
//! `WorldStore` remains append-only: a transition never deletes quads from an
//! existing state. Instead it materializes a fresh successor named graph from the
//! base state, omitting supports retired by `del` and adding supports asserted by
//! `ins`. Retired supports are recorded with state-transition provenance so the
//! before/after path is explicit and the historical support remains auditable.

use std::collections::{BTreeMap, BTreeSet};

use oxigraph::model::{NamedNode, NamedOrBlankNode, Quad, Term};

use crate::entrenchment::{Entrenchment, LeastEntrenched};
use crate::provenance::{mint_reifier, term_n3};
use crate::store::WorldStore;

/// `logic:activeInState` — support was active in the named state.
pub const ACTIVE_IN_STATE: &str = "https://blackcatinformatics.ca/logic/activeInState";
/// `logic:validUntilState` — support stops holding at the successor state.
pub const VALID_UNTIL_STATE: &str = "https://blackcatinformatics.ca/logic/validUntilState";
/// `logic:retiredByTransaction` — transaction/update responsible for retiring a support.
pub const RETIRED_BY_TRANSACTION: &str =
    "https://blackcatinformatics.ca/logic/retiredByTransaction";
/// Canonical GMEOW supersession relation; reused rather than duplicated in `logic:`.
pub const SUPERSEDED_BY: &str = "https://blackcatinformatics.ca/gmeow/supersededBy";

/// One RDF fact/support addressed by an elementary update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionFact {
    pub subject: NamedOrBlankNode,
    pub predicate: NamedNode,
    pub object: Term,
}

impl TransitionFact {
    /// Build an IRI-only fact.
    ///
    /// This convenience constructor covers the common conformance fixtures. Use
    /// [`Self::from_quad`] when preserving literal or blank-node terms from a
    /// loaded store.
    pub fn iri(subject: &str, predicate: &str, object: &str) -> Result<Self, String> {
        let subject = NamedNode::new(subject)
            .map_err(|e| format!("invalid transition subject IRI {subject:?}: {e}"))?;
        let predicate = NamedNode::new(predicate)
            .map_err(|e| format!("invalid transition predicate IRI {predicate:?}: {e}"))?;
        let object = NamedNode::new(object)
            .map_err(|e| format!("invalid transition object IRI {object:?}: {e}"))?;
        Ok(Self {
            subject: NamedOrBlankNode::NamedNode(subject),
            predicate,
            object: Term::NamedNode(object),
        })
    }

    /// Build a fact from an existing quad, discarding only its graph component.
    pub fn from_quad(quad: &Quad) -> Self {
        Self {
            subject: quad.subject.clone(),
            predicate: quad.predicate.clone(),
            object: quad.object.clone(),
        }
    }

    /// Deterministic content key for equality, grouping, and sorting.
    pub fn key(&self) -> Result<TransitionFactKey, String> {
        Ok(TransitionFactKey {
            subject_n3: subject_n3(&self.subject),
            predicate_iri: self.predicate.as_str().to_owned(),
            object_n3: term_n3(&self.object)?,
        })
    }

    /// Content-addressed support/reifier IRI for this fact.
    pub fn reifier(&self) -> Result<String, String> {
        mint_reifier(
            &subject_as_term(&self.subject),
            &self.predicate,
            &self.object,
        )
    }
}

/// Sortable, graph-independent key for a transition fact.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TransitionFactKey {
    subject_n3: String,
    predicate_iri: String,
    object_n3: String,
}

/// The elementary update primitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementaryUpdateKind {
    /// `ins(F)` — F holds in the successor state.
    Insert,
    /// `del(F)` — F's support is retired in the successor state.
    Delete,
}

/// A single elementary update with explicit precedence and provenance identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementaryUpdate {
    /// IRI of this update operation; used for entrenchment conflict arbitration.
    pub update_iri: String,
    /// IRI of the transaction/path step recorded as the retiring transaction.
    pub transaction_iri: String,
    pub kind: ElementaryUpdateKind,
    pub fact: TransitionFact,
}

impl ElementaryUpdate {
    pub fn insert(
        update_iri: impl Into<String>,
        transaction_iri: impl Into<String>,
        fact: TransitionFact,
    ) -> Self {
        Self {
            update_iri: update_iri.into(),
            transaction_iri: transaction_iri.into(),
            kind: ElementaryUpdateKind::Insert,
            fact,
        }
    }

    pub fn delete(
        update_iri: impl Into<String>,
        transaction_iri: impl Into<String>,
        fact: TransitionFact,
    ) -> Self {
        Self {
            update_iri: update_iri.into(),
            transaction_iri: transaction_iri.into(),
            kind: ElementaryUpdateKind::Delete,
            fact,
        }
    }

    fn sort_key(&self) -> Result<(TransitionFactKey, String, u8), String> {
        let kind = match self.kind {
            ElementaryUpdateKind::Insert => 0,
            ElementaryUpdateKind::Delete => 1,
        };
        Ok((self.fact.key()?, self.update_iri.clone(), kind))
    }
}

/// Summary of one applied elementary transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionReport {
    pub base_state: String,
    pub successor_state: String,
    pub carried_supports: usize,
    pub inserted_supports: Vec<String>,
    pub retired_supports: Vec<String>,
}

/// Apply one elementary transition step from `base_state` to `successor_state`.
///
/// The successor state must be a fresh named graph. Conflicting same-fact updates
/// (`ins(F)` and `del(F)` in the same step) are resolved by the unique
/// most-entrenched update IRI in `entrenchment_state`; incomparable maxima are a
/// hard error.
pub fn apply_elementary_transition(
    store: &WorldStore,
    base_state: &str,
    successor_state: &str,
    updates: &[ElementaryUpdate],
    entrenchment_state: &str,
) -> Result<TransitionReport, String> {
    if base_state == successor_state {
        return Err("elementary transition requires distinct base and successor states".to_owned());
    }
    let base_state_node = named_node("base state", base_state)?;
    let successor_state_node = named_node("successor state", successor_state)?;
    validate_iri("entrenchment state", entrenchment_state)?;
    let predicates = MetadataPredicates::new()?;

    if !store
        .quads_for_pattern_in_world(successor_state, None, None, None)
        .is_empty()
    {
        return Err(format!(
            "successor state {successor_state:?} already contains quads; \
             elementary transitions materialize into a fresh state"
        ));
    }

    let entrenchment = Entrenchment::read_from_world(store, entrenchment_state)?;
    let effective = resolve_updates(updates, &entrenchment)?;

    let base_quads = sorted_world_quads(store, base_state)?;
    let base_keys: BTreeSet<TransitionFactKey> = base_quads
        .iter()
        .map(|q| TransitionFact::from_quad(q).key())
        .collect::<Result<_, _>>()?;

    let retired_by_key: BTreeMap<TransitionFactKey, ElementaryUpdate> = effective
        .iter()
        .filter(|u| u.kind == ElementaryUpdateKind::Delete)
        .map(|u| Ok((u.fact.key()?, u.clone())))
        .collect::<Result<_, String>>()?;

    for key in retired_by_key.keys() {
        if !base_keys.contains(key) {
            return Err(format!(
                "illegal del: support {:?} does not hold in base state {base_state:?}",
                key
            ));
        }
    }

    let mut carried_supports = 0usize;
    let mut retired_supports = Vec::new();
    for quad in &base_quads {
        let fact = TransitionFact::from_quad(quad);
        let key = fact.key()?;
        if retired_by_key.contains_key(&key) {
            retired_supports.push(fact.reifier()?);
            continue;
        }
        store.insert_quad_terms(
            successor_state,
            quad.subject.clone(),
            quad.predicate.clone(),
            quad.object.clone(),
        )?;
        carried_supports += 1;
    }

    let mut inserted_supports = Vec::new();
    let mut inserts: Vec<&ElementaryUpdate> = effective
        .iter()
        .filter(|u| u.kind == ElementaryUpdateKind::Insert)
        .collect();
    inserts.sort_by_key(|u| u.sort_key().expect("validated transition fact"));
    for update in inserts {
        store.insert_quad_terms(
            successor_state,
            update.fact.subject.clone(),
            update.fact.predicate.clone(),
            update.fact.object.clone(),
        )?;
        let support = update.fact.reifier()?;
        let support_node = named_node("metadata subject IRI", &support)?;
        store.insert_quad_terms(
            successor_state,
            NamedOrBlankNode::NamedNode(support_node),
            predicates.active_in_state.clone(),
            Term::NamedNode(successor_state_node.clone()),
        )?;
        inserted_supports.push(support);
    }

    let mut deletes: Vec<&ElementaryUpdate> = retired_by_key.values().collect();
    deletes.sort_by_key(|u| u.sort_key().expect("validated transition fact"));
    for update in deletes {
        let support = update.fact.reifier()?;
        let support_node = named_node("metadata subject IRI", &support)?;
        let support_subject = NamedOrBlankNode::NamedNode(support_node);
        let update_iri_node = named_node("update IRI", &update.update_iri)?;
        let transaction_iri_node = named_node("transaction IRI", &update.transaction_iri)?;

        store.insert_quad_terms(
            successor_state,
            support_subject.clone(),
            predicates.active_in_state.clone(),
            Term::NamedNode(base_state_node.clone()),
        )?;
        store.insert_quad_terms(
            successor_state,
            support_subject.clone(),
            predicates.valid_until_state.clone(),
            Term::NamedNode(successor_state_node.clone()),
        )?;
        store.insert_quad_terms(
            successor_state,
            support_subject.clone(),
            predicates.superseded_by.clone(),
            Term::NamedNode(update_iri_node),
        )?;
        store.insert_quad_terms(
            successor_state,
            support_subject,
            predicates.retired_by_transaction.clone(),
            Term::NamedNode(transaction_iri_node),
        )?;
    }

    inserted_supports.sort();
    inserted_supports.dedup();
    retired_supports.sort();
    retired_supports.dedup();

    Ok(TransitionReport {
        base_state: base_state.to_owned(),
        successor_state: successor_state.to_owned(),
        carried_supports,
        inserted_supports,
        retired_supports,
    })
}

fn resolve_updates(
    updates: &[ElementaryUpdate],
    entrenchment: &Entrenchment,
) -> Result<Vec<ElementaryUpdate>, String> {
    let mut by_fact: BTreeMap<TransitionFactKey, Vec<&ElementaryUpdate>> = BTreeMap::new();
    for update in updates {
        validate_iri("update IRI", &update.update_iri)?;
        validate_iri("transaction IRI", &update.transaction_iri)?;
        by_fact.entry(update.fact.key()?).or_default().push(update);
    }

    let mut out = Vec::with_capacity(by_fact.len());
    for (fact, group) in by_fact {
        let has_insert = group.iter().any(|u| u.kind == ElementaryUpdateKind::Insert);
        let has_delete = group.iter().any(|u| u.kind == ElementaryUpdateKind::Delete);
        if has_insert && has_delete {
            let mut candidates: Vec<String> = group.iter().map(|u| u.update_iri.clone()).collect();
            candidates.sort();
            candidates.dedup();
            match entrenchment.most_entrenched(&candidates) {
                LeastEntrenched::Unique(winner) => {
                    let chosen =
                        group
                            .iter()
                            .find(|u| u.update_iri == winner)
                            .ok_or_else(|| {
                                format!("entrenchment selected missing update {winner:?}")
                            })?;
                    out.push((*chosen).clone());
                }
                LeastEntrenched::Tie(tie) => {
                    return Err(format!(
                        "ambiguous elementary updates for {:?}: incomparable most-entrenched \
                         candidates {:?}",
                        fact, tie
                    ));
                }
                LeastEntrenched::Empty => {
                    return Err(format!(
                        "ambiguous elementary updates for {:?}: no candidates",
                        fact
                    ));
                }
            }
        } else {
            let mut sorted = group;
            sorted.sort_by_key(|u| u.sort_key().expect("validated transition fact"));
            out.push((*sorted[0]).clone());
        }
    }
    out.sort_by_key(|u| u.sort_key().expect("validated transition fact"));
    Ok(out)
}

fn sorted_world_quads(store: &WorldStore, world: &str) -> Result<Vec<Quad>, String> {
    let mut quads = store.quads_for_pattern_in_world(world, None, None, None);
    let mut keyed = Vec::with_capacity(quads.len());
    for quad in quads.drain(..) {
        keyed.push((TransitionFact::from_quad(&quad).key()?, quad));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(keyed.into_iter().map(|(_, quad)| quad).collect())
}

#[derive(Debug, Clone)]
struct MetadataPredicates {
    active_in_state: NamedNode,
    valid_until_state: NamedNode,
    retired_by_transaction: NamedNode,
    superseded_by: NamedNode,
}

impl MetadataPredicates {
    fn new() -> Result<Self, String> {
        Ok(Self {
            active_in_state: named_node("metadata predicate IRI", ACTIVE_IN_STATE)?,
            valid_until_state: named_node("metadata predicate IRI", VALID_UNTIL_STATE)?,
            retired_by_transaction: named_node("metadata predicate IRI", RETIRED_BY_TRANSACTION)?,
            superseded_by: named_node("metadata predicate IRI", SUPERSEDED_BY)?,
        })
    }
}

fn validate_iri(label: &str, iri: &str) -> Result<(), String> {
    named_node(label, iri).map(|_| ())
}

fn named_node(label: &str, iri: &str) -> Result<NamedNode, String> {
    NamedNode::new(iri).map_err(|e| format!("invalid {label} {iri:?}: {e}"))
}

fn subject_as_term(subject: &NamedOrBlankNode) -> Term {
    match subject {
        NamedOrBlankNode::NamedNode(node) => Term::NamedNode(node.clone()),
        NamedOrBlankNode::BlankNode(node) => Term::BlankNode(node.clone()),
    }
}

fn subject_n3(subject: &NamedOrBlankNode) -> String {
    match subject {
        NamedOrBlankNode::NamedNode(node) => format!("<{}>", node.as_str()),
        NamedOrBlankNode::BlankNode(node) => format!("_:{}", node.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entrenchment::OVERRIDES;
    use crate::provenance::{LOGIC_NAMESPACE, NAMESPACE};

    const BASE: &str = "https://example.org/state/base";
    const NEXT: &str = "https://example.org/state/next";
    const NEXT2: &str = "https://example.org/state/next2";
    const TX: &str = "https://example.org/tx/1";
    const UPDATE_INSERT: &str = "https://example.org/update/insert";
    const UPDATE_DELETE: &str = "https://example.org/update/delete";
    const S: &str = "https://example.org/s";
    const P: &str = "https://example.org/p";
    const O: &str = "https://example.org/o";

    fn fact() -> TransitionFact {
        TransitionFact::iri(S, P, O).expect("valid fact")
    }

    #[test]
    fn ins_materializes_fact_only_in_successor_state() {
        let store = WorldStore::new();
        let update = ElementaryUpdate::insert(UPDATE_INSERT, TX, fact());

        let report =
            apply_elementary_transition(&store, BASE, NEXT, &[update], BASE).expect("transition");

        assert!(store
            .quads_for_pattern_in_world(BASE, Some(S), Some(P), Some(O))
            .is_empty());
        assert_eq!(
            store
                .quads_for_pattern_in_world(NEXT, Some(S), Some(P), Some(O))
                .len(),
            1
        );
        assert_eq!(report.carried_supports, 0);
        assert_eq!(report.inserted_supports, vec![fact().reifier().unwrap()]);
    }

    #[test]
    fn del_retires_support_without_erasing_base_state() {
        let store = WorldStore::new();
        store.insert_quad(BASE, S, P, O);
        let support = fact().reifier().unwrap();
        let update = ElementaryUpdate::delete(UPDATE_DELETE, TX, fact());

        let report =
            apply_elementary_transition(&store, BASE, NEXT, &[update], BASE).expect("transition");

        assert_eq!(
            store
                .quads_for_pattern_in_world(BASE, Some(S), Some(P), Some(O))
                .len(),
            1,
            "base state remains append-only"
        );
        assert!(store
            .quads_for_pattern_in_world(NEXT, Some(S), Some(P), Some(O))
            .is_empty());
        assert_eq!(report.retired_supports, vec![support.clone()]);

        assert_eq!(
            store
                .quads_for_pattern_in_world(NEXT, Some(&support), Some(ACTIVE_IN_STATE), Some(BASE))
                .len(),
            1
        );
        assert_eq!(
            store
                .quads_for_pattern_in_world(
                    NEXT,
                    Some(&support),
                    Some(VALID_UNTIL_STATE),
                    Some(NEXT)
                )
                .len(),
            1
        );
        assert_eq!(
            store
                .quads_for_pattern_in_world(
                    NEXT,
                    Some(&support),
                    Some(RETIRED_BY_TRANSACTION),
                    Some(TX)
                )
                .len(),
            1
        );
        assert_eq!(
            store
                .quads_for_pattern_in_world(
                    NEXT,
                    Some(&support),
                    Some(SUPERSEDED_BY),
                    Some(UPDATE_DELETE)
                )
                .len(),
            1
        );
    }

    #[test]
    fn same_fact_conflict_uses_most_entrenched_update() {
        let store = WorldStore::new();
        store.insert_quad(BASE, S, P, O);
        store.insert_quad(BASE, UPDATE_INSERT, OVERRIDES, UPDATE_DELETE);
        let insert = ElementaryUpdate::insert(UPDATE_INSERT, TX, fact());
        let delete = ElementaryUpdate::delete(UPDATE_DELETE, TX, fact());

        apply_elementary_transition(&store, BASE, NEXT, &[delete, insert], BASE)
            .expect("insert update is more entrenched");

        assert_eq!(
            store
                .quads_for_pattern_in_world(NEXT, Some(S), Some(P), Some(O))
                .len(),
            1
        );
    }

    #[test]
    fn duplicate_conflict_candidates_do_not_create_false_ties() {
        let store = WorldStore::new();
        store.insert_quad(BASE, S, P, O);
        store.insert_quad(BASE, UPDATE_INSERT, OVERRIDES, UPDATE_DELETE);
        let insert = ElementaryUpdate::insert(UPDATE_INSERT, TX, fact());
        let delete = ElementaryUpdate::delete(UPDATE_DELETE, TX, fact());

        apply_elementary_transition(&store, BASE, NEXT, &[delete, insert.clone(), insert], BASE)
            .expect("duplicated winning update IRI is still a unique winner");

        assert_eq!(
            store
                .quads_for_pattern_in_world(NEXT, Some(S), Some(P), Some(O))
                .len(),
            1
        );
    }

    #[test]
    fn incomparable_conflicting_updates_are_illegal() {
        let store = WorldStore::new();
        store.insert_quad(BASE, S, P, O);
        let insert = ElementaryUpdate::insert(UPDATE_INSERT, TX, fact());
        let delete = ElementaryUpdate::delete(UPDATE_DELETE, TX, fact());

        let err = apply_elementary_transition(&store, BASE, NEXT, &[delete, insert], BASE)
            .expect_err("incomparable updates must fail");
        assert!(err.contains("ambiguous elementary updates"), "got: {err}");
        assert!(store
            .quads_for_pattern_in_world(NEXT, None, None, None)
            .is_empty());

        store.insert_quad(BASE, UPDATE_DELETE, OVERRIDES, UPDATE_INSERT);
        apply_elementary_transition(
            &store,
            BASE,
            NEXT2,
            &[
                ElementaryUpdate::delete(UPDATE_DELETE, TX, fact()),
                ElementaryUpdate::insert(UPDATE_INSERT, TX, fact()),
            ],
            BASE,
        )
        .expect("delete update is now more entrenched");
        assert!(store
            .quads_for_pattern_in_world(NEXT2, Some(S), Some(P), Some(O))
            .is_empty());
    }

    #[test]
    fn del_of_absent_support_is_illegal() {
        let store = WorldStore::new();
        let update = ElementaryUpdate::delete(UPDATE_DELETE, TX, fact());
        let err = apply_elementary_transition(&store, BASE, NEXT, &[update], BASE)
            .expect_err("absent support cannot be retired");
        assert!(err.contains("illegal del"), "got: {err}");
        assert!(store
            .quads_for_pattern_in_world(NEXT, None, None, None)
            .is_empty());
    }

    #[test]
    fn successor_state_must_be_fresh() {
        let store = WorldStore::new();
        store.insert_quad(NEXT, S, P, O);
        let update = ElementaryUpdate::insert(UPDATE_INSERT, TX, fact());
        let err = apply_elementary_transition(&store, BASE, NEXT, &[update], BASE)
            .expect_err("successor is not fresh");
        assert!(err.contains("already contains quads"), "got: {err}");
    }

    #[test]
    fn metadata_constants_stay_in_expected_namespaces() {
        assert!(ACTIVE_IN_STATE.starts_with(LOGIC_NAMESPACE));
        assert!(VALID_UNTIL_STATE.starts_with(LOGIC_NAMESPACE));
        assert!(RETIRED_BY_TRANSACTION.starts_with(LOGIC_NAMESPACE));
        assert!(SUPERSEDED_BY.starts_with(NAMESPACE));
    }
}
