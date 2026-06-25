// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::path::Path;

use gmeow_slice::lint_projection;

const ALIGNMENT_CHECKS: &[&str] = &[
    "inverse-direction",
    "domain-range",
    "property-character",
    "equivalence-collapse",
    "dc-refinement",
    "dc-hand-authored",
];

#[test]
fn alignment_direction_parity_snapshot() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let findings = lint_projection(root, false).expect("lint_projection should succeed");
    let alignment_findings: Vec<_> = findings
        .into_iter()
        .filter(|f| ALIGNMENT_CHECKS.contains(&f.check.as_str()))
        .collect();
    insta::assert_json_snapshot!(alignment_findings);
}
