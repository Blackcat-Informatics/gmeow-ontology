// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers kept outside the selected production source closure.

use super::*;

impl ObservedTerm {
    #[cfg(test)]
    pub(in crate::stages::conformance) fn iri(value: String) -> Self {
        Self::Iri(value)
    }
}
