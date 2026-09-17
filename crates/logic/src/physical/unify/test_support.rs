// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only support retained outside the production source inventory.

use super::*;

impl Subst {
    /// The number of metavariables that currently have a binding.
    pub(crate) fn bound_count(&self) -> usize {
        self.bindings.iter().filter(|b| b.is_some()).count()
    }
}
