// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared source leaves and native proof addresses carried by tableau labels.

use super::super::{NativeProofId, RefutationPremise};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Support {
    sources: Arc<BTreeSet<RefutationPremise>>,
    native: Arc<BTreeSet<NativeProofId>>,
}

impl Support {
    pub(super) fn is_empty(&self) -> bool {
        self.sources.is_empty() && self.native.is_empty()
    }
    pub(super) fn one(premise: RefutationPremise) -> Self {
        Self {
            sources: Arc::new(BTreeSet::from([premise])),
            native: Arc::default(),
        }
    }
    pub(super) fn from_native(sources: Vec<RefutationPremise>, native: Vec<NativeProofId>) -> Self {
        Self {
            sources: Arc::new(sources.into_iter().collect()),
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
            sources: Arc::new(self.sources.union(&other.sources).cloned().collect()),
            native: Arc::new(self.native.union(&other.native).copied().collect()),
        }
    }
    pub(super) fn extend(&mut self, other: &Self) {
        *self = self.merge(other);
    }
    pub(super) fn rows(&self) -> Vec<RefutationPremise> {
        self.sources.iter().cloned().collect()
    }
    pub(super) fn native(&self) -> Vec<NativeProofId> {
        self.native.iter().copied().collect()
    }
}
