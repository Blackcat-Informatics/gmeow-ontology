// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The formal-concept lattice over the distribution catalog's surface × capability
//! incidence.
//!
//! [`read_concept_lattice`] is the READER half of a producer/reader pair whose EMITTER is
//! a separate concern: the lattice nodes it reads back are `gmeow:FormalConcept` subjects
//! carrying `gmeow:conceptExtent` (the surfaces in the concept's extent) and
//! `gmeow:conceptIntent` (the capabilities in its intent), emitted into the same
//! meta-level distribution-catalog named graph the distribution matrix rides in.
//!
//! # Why an empty result is correct, and why this is not a `DistributionRow`
//!
//! A bundle whose catalog graph declares no `gmeow:FormalConcept` yields an EMPTY row set.
//! That is the honest reading of "this catalog carries no lattice", not a degradation: a
//! concept is a derived node, and a catalog is complete without one. Non-emptiness is
//! therefore deliberately NOT gated here — gating it would make this reader fail on every
//! bundle materialized before the emitter exists, which is a reader bug, not a bundle
//! defect. What IS gated is a lattice that is present but unreadable; see below.
//!
//! A [`DistributionRow`](crate::DistributionRow) cannot stand in for a
//! [`ConceptRow`]: it carries no extent/intent fields, and its reader hard-fails on any
//! subject that is not a `gmeow:DocumentationDistribution` — which every concept node is
//! not. The two are separate readers over one graph, exactly as their two shapes are
//! separate.
//!
//! # The lattice bounds are legitimately one-sided
//!
//! In formal concept analysis the top concept is `(G, G′)` and the bottom is `(M′, M)`;
//! over a non-trivial context one of those has an EMPTY intent and the other an empty
//! extent. An empty `extent` or `intent` on a row is therefore normal lattice structure
//! and is never a failure. The failure this module does raise is a node that carries
//! concept facets WITHOUT the `gmeow:FormalConcept` type: such a node would be silently
//! dropped by the type filter, which is precisely the silent degradation no-optionality
//! forbids.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_errors::Diag;
use gmeow_ns::GMEOW_NS;

use crate::catalog_graph::{RDF_TYPE, catalog_triples};
use crate::error::ConceptLattice;
use crate::identity::{iri, local_name};

fn err(message: impl Into<String>) -> Diag {
    Diag::of_kind(ConceptLattice {
        message: message.into(),
    })
}

/// One node of the formal-concept lattice derived over the distribution catalog's
/// surface × capability incidence: a `gmeow:FormalConcept` with its extent (the surfaces
/// it covers) and its intent (the capabilities they share).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConceptRow {
    /// The concept node's subject IRI — the stable identity a caller cites.
    pub concept: String,
    /// Sorted, deduped extent member local names (`gmeow:conceptExtent`). Empty at the
    /// lattice's bottom, which is normal FCA structure and not a defect.
    pub extent: Vec<String>,
    /// Sorted, deduped intent member local names (`gmeow:conceptIntent`). Empty at the
    /// lattice's top, which is normal FCA structure and not a defect.
    pub intent: Vec<String>,
}

/// Read the formal-concept lattice back out of the meta-level distribution-catalog named
/// graph shipped inside `gts_bytes`, sorted by concept IRI.
///
/// A catalog that declares no `gmeow:FormalConcept` returns an EMPTY vector — see the
/// module docs for why that is the correct reading rather than a failure.
///
/// # Errors
///
/// [`ConceptLattice`] when the snapshot will not fold, when it carries no
/// distribution-catalog named graph at all (that IS a bundle defect), or when a subject
/// carries `gmeow:conceptExtent` / `gmeow:conceptIntent` without being typed
/// `gmeow:FormalConcept` — a node the type filter would otherwise drop in silence.
pub fn read_concept_lattice(gts_bytes: &[u8]) -> Result<Vec<ConceptRow>, Diag> {
    let catalog = catalog_triples(gts_bytes, &err)?;

    let concept_type = iri(GMEOW_NS, "FormalConcept");
    let pred_extent = iri(GMEOW_NS, "conceptExtent");
    let pred_intent = iri(GMEOW_NS, "conceptIntent");

    let concepts: BTreeSet<&str> = catalog
        .iter()
        .filter(|(_, p, o)| *p == RDF_TYPE && *o == concept_type)
        .map(|(s, _, _)| s.as_str())
        .collect();

    // A facet-bearing subject that is not typed `gmeow:FormalConcept` would be dropped by
    // the type filter above without a word. Refuse instead, naming the node.
    for (subject, predicate, _) in &catalog {
        if (*predicate == pred_extent || *predicate == pred_intent)
            && !concepts.contains(subject.as_str())
        {
            return Err(err(format!(
                "{subject} carries <{predicate}> but is not typed gmeow:FormalConcept — the \
                 lattice reader would silently drop it; type the node or drop the facet"
            )));
        }
    }

    // One pass over the catalog collects both facet sets for every concept, so the reader
    // stays linear in the catalog size rather than re-scanning per concept.
    let mut extents: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut intents: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for (subject, predicate, object) in &catalog {
        let Some(&concept) = concepts.get(subject.as_str()) else {
            continue;
        };
        if *predicate == pred_extent {
            extents
                .entry(concept)
                .or_default()
                .insert(local_name(object));
        } else if *predicate == pred_intent {
            intents
                .entry(concept)
                .or_default()
                .insert(local_name(object));
        }
    }

    Ok(concepts
        .into_iter()
        .map(|concept| ConceptRow {
            concept: concept.to_owned(),
            extent: extents
                .get(concept)
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_default(),
            intent: intents
                .get(concept)
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_default(),
        })
        .collect())
}
