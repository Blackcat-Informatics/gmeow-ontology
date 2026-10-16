// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact source leaves through one immutable batch lookup.

use super::{
    NativeFamilyLedger, NativeProofId, NativeProofNode, NativeProofOrigin, RefutationPremise,
    Result, invalid,
};
use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};

/// The ledger is immutably borrowed for the entire validation batch, so its
/// proof positions and original source occurrences cannot change under a query.
pub(crate) struct NativeProofLookup<'a> {
    ledger: &'a NativeFamilyLedger,
    nodes: OnceCell<BTreeMap<NativeProofId, &'a NativeProofNode>>,
}

impl<'a> NativeProofLookup<'a> {
    pub(super) fn new(ledger: &'a NativeFamilyLedger) -> Self {
        Self {
            ledger,
            nodes: OnceCell::new(),
        }
    }

    pub(crate) fn ledger(&self) -> &NativeFamilyLedger {
        self.ledger
    }

    pub(crate) fn source_leaves(&self, roots: &[NativeProofId]) -> Result<Vec<RefutationPremise>> {
        if roots.is_empty() {
            return Ok(Vec::new());
        }
        let nodes = self.nodes.get_or_init(|| {
            self.ledger
                .proofs
                .iter()
                .map(|node| (node.id, node))
                .collect()
        });
        source_leaves(roots, |id| nodes.get(id).copied())
    }
}

/// One traversal policy serves immutable validation and the native evidence
/// owner's checked append-only proof positions. Only actual assertion leaves
/// belong to this world; other evidence keeps its separate authority boundary.
pub(super) fn source_leaves<'a>(
    roots: &[NativeProofId],
    lookup: impl Fn(&NativeProofId) -> Option<&'a NativeProofNode>,
) -> Result<Vec<RefutationPremise>> {
    let mut pending = roots.to_vec();
    let mut visited = BTreeSet::new();
    let mut sources = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }
        let node = lookup(&id)
            .ok_or_else(|| invalid("source support references an absent native proof"))?;
        match &node.origin {
            NativeProofOrigin::Asserted {
                sources: leaves, ..
            } => {
                sources.extend(leaves.iter().cloned());
            }
            NativeProofOrigin::Derived { premises, .. } => pending.extend(premises),
            NativeProofOrigin::Modal { .. }
            | NativeProofOrigin::Contextual { .. }
            | NativeProofOrigin::Intrinsic { .. } => {}
        }
    }
    Ok(sources.into_iter().collect())
}
