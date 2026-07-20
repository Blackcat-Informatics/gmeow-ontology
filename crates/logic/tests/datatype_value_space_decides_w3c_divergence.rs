// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Family 5 acceptance: the native datatype value-space refutation sub-decider
//! DECIDES the committed W3C OWL 2 Full divergence slugs it now covers, matching
//! the W3C published verdict EXACTLY.
//!
//! Each slug's `input.nq` is run through the SAME `dl_consistency` path the
//! grader/runner uses. The native token — `incomplete` when a construct is
//! undecided (a non-empty `gaps`), otherwise the consistency boolean — must equal
//! the W3C ground truth. These cases were `native_verdict = "incomplete"` before
//! Family 5; the subsolver now decides them soundly and completely (an empty
//! `gaps` plus the correct consistency), so the token is the W3C verdict.
//!
//! Cases the subsolver leaves WITHHELD (an unbounded/undecidable facet — e.g. an
//! `xsd:pattern` value space) stay `incomplete` and are deliberately NOT listed
//! here: soundness over coverage.

use purrdf::{NativeRdfFormat, dataset_from_bytes};

/// The committed W3C-divergence slugs Family 5 now DECIDES, with the W3C published
/// verdict each must reproduce.
const DECIDED: &[(&str, &str)] = &[
    ("datatype-datacomplementof-001", "consistent"),
    ("datatype-float-discrete-001", "inconsistent"),
    ("webont-i5-8-001", "inconsistent"),
    ("webont-i5-8-002", "consistent"),
    ("new-feature-rational-001", "consistent"),
    ("new-feature-rational-002", "inconsistent"),
    ("new-feature-rational-003", "consistent"),
];

fn native_token(slug: &str) -> String {
    let path = format!(
        "{}/../../conformance/logic/cases/external/w3c-owl2-full-divergence/{slug}/input.nq",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let dataset = dataset_from_bytes(&bytes, NativeRdfFormat::NQuads)
        .unwrap_or_else(|e| panic!("parse {path}: {e}"));
    let verdict = gmeow_logic::reason::dl_consistency(dataset.as_ref())
        .unwrap_or_else(|e| panic!("dl_consistency on {slug}: {e}"));
    if !verdict.gaps.is_empty() {
        "incomplete".to_owned()
    } else if verdict.consistent {
        "consistent".to_owned()
    } else {
        "inconsistent".to_owned()
    }
}

#[test]
fn family5_decides_the_datatype_value_space_divergence_slugs_matching_w3c() {
    let mut failures = Vec::new();
    for (slug, expected) in DECIDED {
        let token = native_token(slug);
        if token != *expected {
            failures.push(format!(
                "{slug}: native decided {token:?}, W3C published {expected:?}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "datatype value-space acceptance failure(s):\n  • {}",
        failures.join("\n  • ")
    );
}
