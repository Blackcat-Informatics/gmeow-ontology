// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn query_failure_retains_native_cause_and_registered_class() {
    let dataset = purrdf::RdfDatasetBuilder::new().freeze().unwrap();
    let failure = select(&dataset, "SELECT WHERE {").unwrap_err();
    let native = failure.downcast_ref::<purrdf::RdfDiagnostic>().unwrap();
    if let Some(location) = &native.location {
        assert_eq!(
            failure.inner().source_ctx.location,
            gmeow_errors::Location::from_rdf(location)
        );
        assert_eq!(
            failure
                .inner()
                .source_ctx
                .focus
                .as_ref()
                .map(|focus| focus.0.as_str()),
            location.subject.as_deref()
        );
    }
    assert_eq!(failure.code(), crate::error::Parse::register());
    assert_eq!(
        failure.inner().failure_class.as_deref(),
        crate::error::Parse::FAILURE_CLASS
    );
}
