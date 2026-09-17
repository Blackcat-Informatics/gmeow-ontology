// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn standalone_compilation_shares_exact_selection_and_bounds_retention() {
    let catalog = SourceCatalog::from_sources(ParsedAuthoredSources::synthetic(
        "<urn:test:A> <https://blackcatinformatics.ca/logic/subClassOf> <urn:test:B> .",
    ))
    .unwrap();
    let first = catalog.compiled_document("synthetic.nq", None).unwrap();
    let again = catalog.compiled_document("synthetic.nq", None).unwrap();
    assert!(Arc::ptr_eq(&first, &again));
    let scoped = catalog
        .compiled_document("synthetic.nq", Some("urn:source:other".into()))
        .unwrap();
    assert!(!Arc::ptr_eq(&first, &scoped));
    assert_eq!(
        scoped.program().source_iri.as_deref(),
        Some("urn:source:other")
    );
    assert!(catalog.compiled_document("absent.nq", None).is_err());
    for index in 0..20 {
        catalog
            .compiled_document("synthetic.nq", Some(format!("urn:source:{index}")))
            .unwrap();
    }
    assert_eq!(catalog.documents.lock().unwrap().len(), 16);
    let overflow = catalog
        .compiled_document("synthetic.nq", Some("urn:source:19".into()))
        .unwrap();
    let recomputed = catalog
        .compiled_document("synthetic.nq", Some("urn:source:19".into()))
        .unwrap();
    assert!(!Arc::ptr_eq(&overflow, &recomputed));
    assert_eq!(overflow.program(), recomputed.program());
}
