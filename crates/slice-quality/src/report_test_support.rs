// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers for the owning production module.

use super::*;

#[cfg(test)]
impl SliceReport {
    /// Test-only constructor: assemble a [`SliceReport`] from already-computed
    /// parts, bypassing [`score_slice_with_standard`]'s scoring pass entirely.
    /// Used by `crate::lint`'s unit tests to build synthetic reports (a
    /// declared tier ratchet, a graded advisory, a degenerate empty-grade
    /// slice, …) without a real slice directory or rubric dataset.
    /// `advisory_axes` must be index-parallel to `advisories`; a caller that does
    /// not care about axis provenance passes an empty vector and gets none.
    pub(crate) fn for_test(
        standard: MeasurementStandard,
        assessment: SliceAssessment,
        advisories: Vec<Finding>,
        advisory_axes: Vec<String>,
        axis_weight: std::collections::HashMap<String, f64>,
    ) -> Self {
        assert!(
            advisory_axes.is_empty() || advisory_axes.len() == advisories.len(),
            "advisory_axes must be index-parallel to advisories (or empty)"
        );
        Self {
            standard,
            assessment,
            advisories,
            advisory_axes,
            axis_weight,
            source_files: SliceSourceFiles {
                manifest: MANIFEST_KEY.to_owned(),
                module: Some(MODULE_KEY.to_owned()),
            },
        }
    }

    /// Remove the optional module identity for the manifest-fallback negative control.
    pub(crate) fn remove_module_source_for_test(&mut self) {
        self.source_files.module = None;
    }
}
