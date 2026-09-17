// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_errors::intern_code;
use std::collections::HashSet;

#[test]
fn every_pipeline_code_interns_with_no_collision() {
    let handles = register_all();
    // register_all() and the catalog enumerate the same kinds in the same order.
    assert_eq!(
        handles.len(),
        PIPELINE_DIAG_CODES.len(),
        "register_all() and PIPELINE_DIAG_CODES must enumerate the same kinds"
    );

    // Every catalogued code interns (register_all seeded the registry).
    for code in PIPELINE_DIAG_CODES {
        assert!(
            intern_code(code).is_ok(),
            "pipeline code `{code}` did not intern after register_all()"
        );
    }

    // No two kinds may share a code literal: distinct strings AND distinct
    // interned handles. A duplicate `code = "..."` would fail loudly here.
    let distinct_strings: HashSet<&&str> = PIPELINE_DIAG_CODES.iter().collect();
    assert_eq!(
        distinct_strings.len(),
        PIPELINE_DIAG_CODES.len(),
        "duplicate pipeline diagnostic code string detected"
    );
    let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
    assert_eq!(
        distinct_handles.len(),
        handles.len(),
        "two pipeline diagnostic kinds interned to the same code handle"
    );
}

/// The seven medium-axis kinds carry their ontology failure-class IRI on the
/// GENERATED constant AND through the `DiagKind` accessor — the link the
/// repo-static bijection gate reads statically must be the same one a live
/// `Diag` producer exposes at run time, or the gate would be proving something
/// about a string the code never uses.
#[test]
fn every_medium_kind_carries_its_ontology_failure_class() {
    use gmeow_errors::DiagKind;

    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
    let bound: [(&str, Option<&'static str>, Option<&'static str>); 7] = [
        (
            "MediumUndeclaredDictionary",
            MediumUndeclaredDictionary::FAILURE_CLASS,
            MediumUndeclaredDictionary {
                detail: String::new(),
            }
            .failure_class(),
        ),
        (
            "MediumUnknownDictionary",
            MediumUnknownDictionary::FAILURE_CLASS,
            MediumUnknownDictionary {
                detail: String::new(),
            }
            .failure_class(),
        ),
        (
            "MediumUnknownSchema",
            MediumUnknownSchema::FAILURE_CLASS,
            MediumUnknownSchema {
                detail: String::new(),
            }
            .failure_class(),
        ),
        (
            "MediumDigestMismatch",
            MediumDigestMismatch::FAILURE_CLASS,
            MediumDigestMismatch {
                detail: String::new(),
            }
            .failure_class(),
        ),
        (
            "MediumOpaqueFrame",
            MediumOpaqueFrame::FAILURE_CLASS,
            MediumOpaqueFrame {
                detail: String::new(),
            }
            .failure_class(),
        ),
        (
            "MediumDictionaryRegression",
            MediumDictionaryRegression::FAILURE_CLASS,
            MediumDictionaryRegression {
                detail: String::new(),
            }
            .failure_class(),
        ),
        (
            "MediumCorpusDrift",
            MediumCorpusDrift::FAILURE_CLASS,
            MediumCorpusDrift {
                detail: String::new(),
            }
            .failure_class(),
        ),
    ];
    for (local, constant, accessor) in bound {
        let expected = format!("{GMEOW}{local}");
        assert_eq!(
            constant,
            Some(expected.as_str()),
            "{local}: FAILURE_CLASS must name its ontology individual"
        );
        assert_eq!(
            accessor, constant,
            "{local}: the DiagKind accessor must agree with the constant"
        );
    }
}
