// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `BudgetReport` — the flat-vs-reified statement budget (#360 made checkable).
//!
//! A faithful port of the Python dataclass: three string-keyed counters
//! (`flat`, `reified`, `skipped`) plus an `as_text()` renderer that sorts each
//! section by key. `BTreeMap` gives the sorted iteration `sorted(self.flat.items())`
//! produces in Python.

use std::collections::BTreeMap;

/// Flat-vs-reified statement budget.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BudgetReport {
    /// Links emitted as a single flat quad each.
    pub flat: BTreeMap<String, u64>,
    /// Constructs whose vantage/score/mode is data, reified.
    pub reified: BTreeMap<String, u64>,
    /// Deliberately-not-imported items (no silent caps).
    pub skipped: BTreeMap<String, u64>,
}

impl BudgetReport {
    /// Create an empty budget report.
    pub fn new() -> Self {
        Self::default()
    }

    /// Increment the `flat` counter for `key` by `n`.
    pub fn add_flat(&mut self, key: &str, n: u64) {
        *self.flat.entry(key.to_string()).or_insert(0) += n;
    }

    /// Increment the `reified` counter for `key` by `n`.
    pub fn add_reified(&mut self, key: &str, n: u64) {
        *self.reified.entry(key.to_string()).or_insert(0) += n;
    }

    /// Increment the `skipped` counter for `key` by `n`.
    pub fn add_skipped(&mut self, key: &str, n: u64) {
        *self.skipped.entry(key.to_string()).or_insert(0) += n;
    }

    /// Render the human-readable budget table (matches the Python `as_text`).
    ///
    /// Note: Python's `Counter` omits keys whose value is never incremented, and
    /// `sorted(...)` orders by key. `BTreeMap` only contains incremented keys and
    /// iterates in key order, so the two render identically.
    pub fn as_text(&self) -> String {
        let mut lines: Vec<String> = vec![
            "FOUNDATION IMPORT BUDGET".to_string(),
            "== flat links (1 quad each) ==".to_string(),
        ];
        for (k, v) in &self.flat {
            lines.push(format!("  {k}: {v}"));
        }
        lines.push("== reified constructs (vantage/score/mode is data) ==".to_string());
        for (k, v) in &self.reified {
            lines.push(format!("  {k}: {v}"));
        }
        lines.push("== deliberately not imported (no silent caps) ==".to_string());
        for (k, v) in &self.skipped {
            lines.push(format!("  {k}: {v}"));
        }
        lines.join("\n")
    }
}
