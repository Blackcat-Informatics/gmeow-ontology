// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared source leaves and native proof addresses carried by tableau labels.

use super::super::{NativeProofId, RefutationPremise};
use std::{collections::BTreeSet, sync::Arc};

/// Reuse immutable operands when their union already has an owned representation.
fn union<T: Ord + Clone>(left: &Arc<BTreeSet<T>>, right: &Arc<BTreeSet<T>>) -> Arc<BTreeSet<T>> {
    if Arc::ptr_eq(left, right) || right.is_subset(left) {
        Arc::clone(left)
    } else if left.is_subset(right) {
        Arc::clone(right)
    } else {
        Arc::new(left.union(right).cloned().collect())
    }
}

/// One class preparation shares immutable original leaves across native supports.
/// Complete premise values remain the lookup and proof-ordering authority.
#[derive(Default)]
pub(super) struct SourcePool {
    leaves: BTreeSet<Arc<RefutationPremise>>,
}

impl SourcePool {
    /// Retain existing source allocations before replacing their support wrapper.
    pub(super) fn seed(&mut self, support: &Support) {
        self.leaves.extend(support.sources.iter().cloned());
    }

    fn intern(&mut self, premise: RefutationPremise) -> Arc<RefutationPremise> {
        if let Some(existing) = self.leaves.get(&premise) {
            return Arc::clone(existing);
        }
        let premise = Arc::new(premise);
        self.leaves.insert(Arc::clone(&premise));
        premise
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Support {
    sources: Arc<BTreeSet<Arc<RefutationPremise>>>,
    native: Arc<BTreeSet<NativeProofId>>,
}

impl Support {
    pub(super) fn is_empty(&self) -> bool {
        self.sources.is_empty() && self.native.is_empty()
    }
    pub(super) fn one(premise: RefutationPremise) -> Self {
        Self {
            sources: Arc::new(BTreeSet::from([Arc::new(premise)])),
            native: Arc::default(),
        }
    }
    pub(super) fn from_native(
        sources: Vec<RefutationPremise>,
        native: Vec<NativeProofId>,
        pool: &mut SourcePool,
    ) -> Self {
        let sources = sources
            .into_iter()
            .map(|source| pool.intern(source))
            .collect();
        Self {
            sources: Arc::new(sources),
            native: Arc::new(native.into_iter().collect()),
        }
    }
    pub(super) fn merge(&self, other: &Self) -> Self {
        if other.is_empty() {
            return self.clone();
        }
        if self.is_empty() {
            return other.clone();
        }
        Self {
            sources: union(&self.sources, &other.sources),
            native: union(&self.native, &other.native),
        }
    }
    pub(super) fn extend(&mut self, other: &Self) {
        *self = self.merge(other);
    }
    pub(super) fn rows(&self) -> Vec<RefutationPremise> {
        self.sources
            .iter()
            .map(|source| source.as_ref().clone())
            .collect()
    }
    pub(super) fn native(&self) -> Vec<NativeProofId> {
        self.native.iter().copied().collect()
    }
}
