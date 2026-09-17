// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only support retained outside the production source inventory.

use super::*;

impl DenseInterner {
    /// Whether nothing has been interned yet.
    pub(crate) fn is_empty(&self) -> bool {
        self.reverse.is_empty()
    }
}
