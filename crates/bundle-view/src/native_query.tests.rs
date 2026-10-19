// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn query_failure_retains_native_cause_and_registered_class() {
    let dataset = purrdf::RdfDatasetBuilder::new().freeze().unwrap();
    let failure = select(&dataset, "SELECT WHERE {").unwrap_err();
    assert!(failure.downcast_ref::<purrdf::RdfDiagnostic>().is_some());
    assert_eq!(failure.code(), crate::error::Parse::register());
    assert_eq!(
        failure.inner().failure_class.as_deref(),
        crate::error::Parse::FAILURE_CLASS
    );
}
