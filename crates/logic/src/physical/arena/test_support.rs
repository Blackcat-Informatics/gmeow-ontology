// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only support retained outside the production source inventory.

use super::*;

impl RowArena {
    /// The number of [`TermRef`]s currently held in the backing buffer (test / cost
    /// probe — inline tuples are not counted here, they never touch the buffer).
    pub(crate) fn backing_len(&self) -> usize {
        self.backing.len()
    }
}
