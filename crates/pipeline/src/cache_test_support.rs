// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only access to the same cache owner used by production.
use super::*;

impl PipelineCache {
    pub(super) fn with_limits(mut self, max_entries: usize, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self.store = self.store.with_limits(StoreLimits {
            max_entry_bytes: MAX_ENTRY_BYTES,
            max_receipt_bytes: MAX_RECEIPT_BYTES,
            max_entries,
            max_total_bytes: max_bytes,
        });
        self
    }

    pub(super) fn receipt_path(&self, key: &str) -> PathBuf {
        let key = ActionKey::from_hex(key).expect("pipeline stage keys are SHA-256 hex");
        self.store.receipt_path(&key)
    }
}
