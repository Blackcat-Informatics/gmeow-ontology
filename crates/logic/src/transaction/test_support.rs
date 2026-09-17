// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only support retained outside the production source inventory.

use super::*;

impl ExecOutcome {
    /// The executed states (start..=end) — non-empty only on `Succeeded`; an empty slice for a
    /// `Pending` / `Failed` run, which committed no path. A test-only inspector: the production
    /// emission path destructures the `Succeeded` variant directly.
    pub(crate) fn path(&self) -> &[String] {
        match self {
            Self::Succeeded { path, .. } => path,
            _ => &[],
        }
    }

    /// The elementary steps applied along the executed path — empty for a `Pending` / `Failed`
    /// run, which applied no committed step. A test-only inspector (see [`Self::path`]).
    pub(crate) fn steps(&self) -> &[PlannedStep] {
        match self {
            Self::Succeeded { steps, .. } => steps,
            _ => &[],
        }
    }
}
