// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compact potential-writer inventories; no source facts or analysis state escape.

mod execution_cbor;

use std::collections::BTreeSet;

use super::{StatementPattern, value_flow::ValueFlow};

/// Every examined producer and the conservative union of its possible predicates.
/// An open predicate envelope is never evidence of absence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct WriterInventory {
    pub(crate) theory: [u8; 32],
    pub(crate) producers: Vec<String>,
    pub(crate) predicates: BTreeSet<String>,
    pub(crate) unbounded: bool,
    /// Original source operators require their actual grammar admission, even
    /// when an execution budget prevents the observer from reaching them.
    pub(crate) requires_source_admission: bool,
}

impl WriterInventory {
    pub(crate) fn new(theory: [u8; 32]) -> Self {
        Self {
            theory,
            producers: Vec::new(),
            predicates: BTreeSet::new(),
            unbounded: false,
            requires_source_admission: false,
        }
    }

    pub(crate) fn include(&mut self, flow: &ValueFlow, pattern: &StatementPattern) {
        let (predicates, unbounded) = flow.writer_predicates(pattern);
        self.predicates.extend(predicates);
        self.unbounded |= unbounded;
    }

    pub(crate) fn may_write(&self, predicate: &str) -> bool {
        self.unbounded || self.predicates.contains(predicate)
    }
}
