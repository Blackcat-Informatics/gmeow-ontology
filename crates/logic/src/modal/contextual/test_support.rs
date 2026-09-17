// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only default-source construction of an attributed contextual frame.

use purrdf::DatasetView;

use super::{AdmissionError, RdfFrame};

impl<'a, D: DatasetView + ?Sized> RdfFrame<'a, D> {
    pub(in crate::modal) fn load(dataset: &'a D) -> Result<Self, AdmissionError> {
        Self::load_with_closure(dataset, &[])
    }

    pub(super) fn load_with_closure(
        dataset: &'a D,
        closure: &[crate::reason::InferredAxiom],
    ) -> Result<Self, AdmissionError> {
        Self::load_in_graph(dataset, closure, None)
    }
}
